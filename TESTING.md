# AxonTask Testing Guide

This document describes how to run and write tests for AxonTask.

## Test Types

### Unit Tests

Unit tests are located in `src/` alongside the code they test. They verify individual components in isolation.

```bash
# Run all unit tests
cargo test --lib

# Run unit tests for a specific crate
cargo test -p axontask-api --lib
cargo test -p axontask-worker --lib
cargo test -p axontask-shared --lib
```

**Current Coverage:** 74 unit tests across all crates

### Integration Tests

Integration tests verify the full system works end-to-end:
- API endpoints with authentication
- Worker task execution
- Event streaming via SSE
- Cancellation and timeout enforcement
- Rate limiting and quotas

#### Running Integration Tests

**Prerequisites:**
- Docker installed and running
- `sqlx-cli` installed (`cargo install sqlx-cli`)

**Quick Start:**

```bash
# Run all integration tests (automatic Docker setup/teardown)
./run-tests.sh

# Keep containers running for debugging
./run-tests.sh --keep

# Clean up containers manually
docker-compose -f docker-compose.test.yml down
```

**Manual Setup:**

```bash
# 1. Start test containers
docker-compose -f docker-compose.test.yml up -d

# 2. Load test environment
export $(cat .env.test | grep -v '^#' | xargs)

# 3. Run migrations
sqlx migrate run --source ./migrations

# 4. Run tests
cargo test --test integration_test

# 5. Clean up
docker-compose -f docker-compose.test.yml down
```

## Test Infrastructure

### Test Database

- **Image**: PostgreSQL 15 Alpine
- **Port**: 5433 (to avoid conflicts with dev database)
- **Database**: `axontask_test`
- **User**: `axontask_test`
- **Password**: `test_password`

### Test Redis

- **Image**: Redis 7 Alpine
- **Port**: 6380 (to avoid conflicts with dev Redis)

### Test Configuration

Test configuration is in `.env.test`:
- Separate DATABASE_URL pointing to port 5433
- Separate REDIS_URL pointing to port 6380
- Test JWT secret (not for production)

## Writing Integration Tests

Integration tests are in `axontask-api/tests/integration_test.rs`.

### Test Template

```rust
#[tokio::test]
async fn test_my_feature() {
    // 1. Create test context (sets up DB, Redis, API)
    let ctx = TestContext::new().await.unwrap();

    // 2. Perform test operations
    // ... your test code ...

    // 3. Clean up
    ctx.cleanup().await.unwrap();
}
```

### Helper Functions

**`TestContext`** - Provides:
- Database connection
- Redis client
- Axum app router
- Test tenant and user
- JWT token for authentication

**`create_test_task()`** - Creates a task directly in database

**`wait_for()`** - Polls condition with timeout

### Example Test

```rust
#[tokio::test]
async fn test_full_task_lifecycle() {
    let ctx = TestContext::new().await.unwrap();

    // Start worker
    let orchestrator = WorkerOrchestrator::new(ctx.db.clone(), ctx.redis.clone());
    let shutdown_token = orchestrator.shutdown_token();
    let worker_handle = tokio::spawn(async move {
        orchestrator.run().await
    });

    // Create task
    let task_id = create_test_task(
        &ctx,
        "test-task",
        "mock",
        json!({"duration_ms": 500}),
    )
    .await
    .unwrap();

    // Wait for completion
    wait_for(
        || async {
            let task = Task::find_by_id(&ctx.db, task_id).await.unwrap().unwrap();
            task.state == TaskState::Succeeded
        },
        10,
    )
    .await
    .unwrap();

    // Verify
    let task = Task::find_by_id(&ctx.db, task_id).await.unwrap().unwrap();
    assert_eq!(task.state, TaskState::Succeeded);

    // Cleanup
    shutdown_token.cancel();
    worker_handle.await.unwrap();
    ctx.cleanup().await.unwrap();
}
```

## Current Test Coverage

### Unit Tests (74 total)

- **axontask-api**: 37 tests
  - Rate limiting: 4 tests
  - Quota enforcement: 12 tests
  - MCP endpoints: 21 tests

- **axontask-worker**: 37 tests
  - Adapter trait: 7 tests
  - Mock adapter: 10 tests
  - Event emitter: 4 tests
  - Queue: 2 tests
  - Timeout: 10 tests
  - Control: 3 tests
  - Orchestrator: 1 test

### Integration Tests (8 total)

- ✅ Task creation via API
- ✅ Full task lifecycle (create → execute → complete)
- ✅ Task cancellation flow
- ✅ Timeout enforcement
- ✅ Rate limiting headers
- ✅ Quota enforcement
- ✅ Authentication requirement
- ✅ Get task status

## Test Guidelines

### DO:
- ✅ Use `TestContext` for integration tests
- ✅ Clean up resources in tests (`ctx.cleanup()`)
- ✅ Use `wait_for()` for async conditions
- ✅ Test both success and failure cases
- ✅ Verify state transitions in database
- ✅ Test with realistic data

### DON'T:
- ❌ Hard-code delays (use `wait_for()` instead)
- ❌ Share state between tests
- ❌ Leave containers running (cleanup)
- ❌ Use production credentials in tests
- ❌ Skip cleanup on test failure

## Debugging Tests

### View Container Logs

```bash
# All logs
docker-compose -f docker-compose.test.yml logs -f

# PostgreSQL only
docker-compose -f docker-compose.test.yml logs -f postgres-test

# Redis only
docker-compose -f docker-compose.test.yml logs -f redis-test
```

### Connect to Test Database

```bash
# Using Docker exec
docker-compose -f docker-compose.test.yml exec postgres-test psql -U axontask_test

# Using psql directly
psql postgresql://axontask_test:test_password@localhost:5433/axontask_test
```

### Connect to Test Redis

```bash
# Using Docker exec
docker-compose -f docker-compose.test.yml exec redis-test redis-cli

# Using redis-cli directly
redis-cli -p 6380
```

### Keep Containers Running

```bash
# Run tests but keep containers
./run-tests.sh --keep

# Then debug as needed...

# Clean up when done
docker-compose -f docker-compose.test.yml down
```

## CI/CD Integration

Tests are designed to run in CI environments:

```yaml
# Example GitHub Actions
- name: Start test containers
  run: docker-compose -f docker-compose.test.yml up -d

- name: Run migrations
  run: sqlx migrate run --source ./migrations
  env:
    DATABASE_URL: postgresql://axontask_test:test_password@localhost:5433/axontask_test

- name: Run tests
  run: cargo test
  env:
    DATABASE_URL: postgresql://axontask_test:test_password@localhost:5433/axontask_test
    REDIS_URL: redis://localhost:6380

- name: Cleanup
  run: docker-compose -f docker-compose.test.yml down
```

## Performance Testing

For load testing, see the separate `LOAD_TESTING.md` guide (coming soon).

## Test Data

Tests use randomized data to avoid collisions:
- Tenant names: `Test Tenant {UUID}`
- User emails: `test-{UUID}@example.com`
- Task names: `{test-name}-{UUID}`

This allows parallel test execution without conflicts.

## Troubleshooting

### "Database already exists"
```bash
docker-compose -f docker-compose.test.yml down -v
docker-compose -f docker-compose.test.yml up -d
```

### "Port already in use"
Check if dev containers are running on 5432/6379:
```bash
docker ps
```

### "Tests timeout"
- Check container health: `docker-compose -f docker-compose.test.yml ps`
- View logs: `docker-compose -f docker-compose.test.yml logs`
- Increase wait timeouts in tests

### "Migration failed"
```bash
# Reset database
docker-compose -f docker-compose.test.yml down -v
docker-compose -f docker-compose.test.yml up -d
sleep 5
sqlx migrate run --source ./migrations
```

## Next Steps

- [ ] Add performance benchmarks
- [ ] Add load testing suite
- [ ] Add chaos engineering tests
- [ ] Measure test coverage with `cargo-tarpaulin`
- [ ] Add mutation testing with `cargo-mutants`
