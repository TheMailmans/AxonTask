/// Integration tests for AxonTask API
///
/// These tests verify the full system works end-to-end:
/// - API endpoints with authentication
/// - Task lifecycle (create → execute → complete)
/// - Event streaming via SSE
/// - Cancellation flow
/// - Timeout enforcement
/// - Rate limiting and quotas

mod common;

use axontask_shared::models::task::{Task, TaskState};
use axontask_worker::orchestrator::{OrchestratorConfig, WorkerOrchestrator};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::TestContext;
use serde_json::json;
use tower::Service as _;

/// Test that we can create a task via the API
#[tokio::test]
async fn test_create_task() {
    let ctx = TestContext::new().await.unwrap();

    let request = Request::builder()
        .method("POST")
        .uri("/v1/mcp/start_task")
        .header("authorization", ctx.auth_header())
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "test-task",
                "adapter": "mock",
                "args": {
                    "duration_ms": 1000,
                    "should_fail": false
                },
                "timeout_s": 60
            })
            .to_string(),
        ))
        .unwrap();

    let response = ctx.app.clone().call(request).await.unwrap();

    // Debug: print response body if not OK
    let status = response.status();
    if status != StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8_lossy(&body);
        panic!("Expected 200 OK, got {}: {}", status, body_str);
    }

    assert_eq!(status, StatusCode::OK);

    // Parse response
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(response_json["task_id"].is_string());
    assert_eq!(response_json["status"], "pending");

    ctx.cleanup().await.unwrap();
}

/// Test full task lifecycle with worker
#[tokio::test]
async fn test_full_task_lifecycle() {
    let ctx = TestContext::new().await.unwrap();

    // Start worker in background
    let orchestrator = WorkerOrchestrator::with_config(
        ctx.db.clone(),
        ctx.redis.clone(),
        OrchestratorConfig {
            poll_interval_secs: 1,
            max_concurrent_tasks: 5,
            batch_size: 5,
        },
    );

    let shutdown_token = orchestrator.shutdown_token();
    let worker_handle = tokio::spawn(async move {
        orchestrator.run().await
    });

    // Create task via API
    let task_id = common::create_test_task(
        &ctx,
        "lifecycle-test",
        "mock",
        json!({
            "duration_ms": 500,
            "should_fail": false
        }),
    )
    .await
    .unwrap();

    // Wait for task to be claimed and start executing
    common::wait_for(
        || async {
            let task = Task::find_by_id(&ctx.db, task_id).await.unwrap().unwrap();
            task.state == TaskState::Running || task.state == TaskState::Succeeded
        },
        10,
    )
    .await
    .unwrap();

    // Wait for task to complete
    common::wait_for(
        || async {
            let task = Task::find_by_id(&ctx.db, task_id).await.unwrap().unwrap();
            task.state == TaskState::Succeeded
        },
        10,
    )
    .await
    .unwrap();

    // Verify final state
    let task = Task::find_by_id(&ctx.db, task_id).await.unwrap().unwrap();
    assert_eq!(task.state, TaskState::Succeeded);
    assert!(task.started_at.is_some());
    assert!(task.ended_at.is_some());

    // Shutdown worker
    shutdown_token.cancel();
    let _ = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        worker_handle,
    )
    .await;

    ctx.cleanup().await.unwrap();
}

/// Test task cancellation flow
#[tokio::test]
async fn test_task_cancellation() {
    let ctx = TestContext::new().await.unwrap();

    // Start worker
    let orchestrator = WorkerOrchestrator::new(ctx.db.clone(), ctx.redis.clone());
    let shutdown_token = orchestrator.shutdown_token();
    let worker_handle = tokio::spawn(async move {
        orchestrator.run().await
    });

    // Create long-running task
    let task_id = common::create_test_task(
        &ctx,
        "cancel-test",
        "mock",
        json!({
            "duration_ms": 10000, // 10 seconds
            "should_fail": false
        }),
    )
    .await
    .unwrap();

    // Wait for task to start
    common::wait_for(
        || async {
            let task = Task::find_by_id(&ctx.db, task_id).await.unwrap().unwrap();
            task.state == TaskState::Running
        },
        10,
    )
    .await
    .unwrap();

    // Cancel task via API
    let request = Request::builder()
        .method("POST")
        .uri(format!("/v1/mcp/tasks/{}/cancel", task_id))
        .header("authorization", ctx.auth_header())
        .body(Body::empty())
        .unwrap();

    let response = ctx.app.clone().call(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Wait for task to be cancelled
    common::wait_for(
        || async {
            let task = Task::find_by_id(&ctx.db, task_id).await.unwrap().unwrap();
            task.state == TaskState::Canceled
        },
        10,
    )
    .await
    .unwrap();

    // Verify cancelled state
    let task = Task::find_by_id(&ctx.db, task_id).await.unwrap().unwrap();
    assert_eq!(task.state, TaskState::Canceled);

    // Shutdown worker
    shutdown_token.cancel();
    let _ = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        worker_handle,
    )
    .await;

    ctx.cleanup().await.unwrap();
}

/// Test timeout enforcement
#[tokio::test]
async fn test_timeout_enforcement() {
    let ctx = TestContext::new().await.unwrap();

    // Start worker
    let orchestrator = WorkerOrchestrator::new(ctx.db.clone(), ctx.redis.clone());
    let shutdown_token = orchestrator.shutdown_token();
    let worker_handle = tokio::spawn(async move {
        orchestrator.run().await
    });

    // Create task with short timeout
    let task = Task::create(
        &ctx.db,
        axontask_shared::models::task::CreateTask {
            tenant_id: ctx.tenant.id,
            created_by: Some(ctx.user.id),
            name: "timeout-test".to_string(),
            adapter: "mock".to_string(),
            args: json!({
                "duration_ms": 10000, // 10 seconds
                "should_fail": false
            }),
            timeout_seconds: 2, // 2 second timeout
        },
    )
    .await
    .unwrap();

    // Wait for task to start
    common::wait_for(
        || async {
            let t = Task::find_by_id(&ctx.db, task.id).await.unwrap().unwrap();
            t.state == TaskState::Running
        },
        10,
    )
    .await
    .unwrap();

    // Wait for timeout to trigger
    common::wait_for(
        || async {
            let t = Task::find_by_id(&ctx.db, task.id).await.unwrap().unwrap();
            t.state == TaskState::Timeout || t.state == TaskState::Canceled
        },
        15,
    )
    .await
    .unwrap();

    // Verify timeout/cancelled state
    let task = Task::find_by_id(&ctx.db, task.id).await.unwrap().unwrap();
    assert!(
        task.state == TaskState::Timeout || task.state == TaskState::Canceled,
        "Expected timeout or cancelled, got {:?}",
        task.state
    );

    // Shutdown worker
    shutdown_token.cancel();
    let _ = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        worker_handle,
    )
    .await;

    ctx.cleanup().await.unwrap();
}

/// Test rate limiting
#[tokio::test]
async fn test_rate_limiting() {
    let ctx = TestContext::new().await.unwrap();

    // Make requests up to limit (Pro plan: 300/min)
    // For testing, we'll just verify rate limit headers are present
    let request = Request::builder()
        .method("POST")
        .uri("/v1/mcp/start_task")
        .header("authorization", ctx.auth_header())
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "rate-test",
                "adapter": "mock",
                "args": {},
                "timeout_s": 60
            })
            .to_string(),
        ))
        .unwrap();

    let response = ctx.app.clone().call(request).await.unwrap();

    // Verify rate limit headers are present
    assert!(response.headers().contains_key("x-ratelimit-limit"));
    assert!(response.headers().contains_key("x-ratelimit-remaining"));
    assert!(response.headers().contains_key("x-ratelimit-reset"));

    ctx.cleanup().await.unwrap();
}

/// Test quota enforcement
#[tokio::test]
async fn test_quota_enforcement() {
    let ctx = TestContext::new().await.unwrap();

    // Pro plan has high limits, so this should succeed
    let request = Request::builder()
        .method("POST")
        .uri("/v1/mcp/start_task")
        .header("authorization", ctx.auth_header())
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "quota-test",
                "adapter": "mock",
                "args": {},
                "timeout_s": 60
            })
            .to_string(),
        ))
        .unwrap();

    let response = ctx.app.clone().call(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    ctx.cleanup().await.unwrap();
}

/// Test authentication requirement
#[tokio::test]
async fn test_authentication_required() {
    let ctx = TestContext::new().await.unwrap();

    // Request without auth header
    let request = Request::builder()
        .method("POST")
        .uri("/v1/mcp/start_task")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "test",
                "adapter": "mock",
                "args": {}
            })
            .to_string(),
        ))
        .unwrap();

    let response = ctx.app.clone().call(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    ctx.cleanup().await.unwrap();
}

/// Test get task status endpoint
#[tokio::test]
async fn test_get_task_status() {
    let ctx = TestContext::new().await.unwrap();

    // Create task
    let task_id = common::create_test_task(
        &ctx,
        "status-test",
        "mock",
        json!({}),
    )
    .await
    .unwrap();

    // Get status
    let request = Request::builder()
        .method("GET")
        .uri(format!("/v1/mcp/tasks/{}/status", task_id))
        .header("authorization", ctx.auth_header())
        .body(Body::empty())
        .unwrap();

    let response = ctx.app.clone().call(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Parse response
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(response_json["task_id"], task_id.to_string());
    assert_eq!(response_json["state"], "pending");

    ctx.cleanup().await.unwrap();
}
