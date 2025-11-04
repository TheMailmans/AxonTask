# AxonTask API Design

**Version**: 1.0
**Last Updated**: November 04, 2025
**Base URL**: `https://api.axontask.dev` (production) or `http://localhost:8080` (development)
**Status**: Complete Specification

---

## Table of Contents

1. [API Overview](#api-overview)
2. [Authentication](#authentication)
3. [Rate Limiting](#rate-limiting)
4. [Error Handling](#error-handling)
5. [Pagination](#pagination)
6. [Endpoints](#endpoints)
   - [Authentication](#authentication-endpoints)
   - [MCP Tools](#mcp-tool-endpoints)
   - [Tasks](#task-endpoints)
   - [API Keys](#api-key-endpoints)
   - [Webhooks](#webhook-endpoints)
   - [Usage & Billing](#usage--billing-endpoints)
7. [Webhooks](#webhook-delivery)
8. [OpenAPI Specification](#openapi-specification)

---

## API Overview

### Versioning

- **Current Version**: v1
- **URL Format**: `/v1/{resource}`
- **Versioning Strategy**: URL-based (not header-based)
- **Deprecation Policy**: 6 months notice before removal

### Content Type

- **Request**: `application/json`
- **Response**: `application/json`
- **Streaming**: `text/event-stream` (SSE endpoints only)

### CORS

- **Allowed Origins**: Configured per deployment
- **Credentials**: Allowed (`Access-Control-Allow-Credentials: true`)
- **Methods**: GET, POST, PUT, DELETE, OPTIONS
- **Headers**: `Authorization`, `Content-Type`, `X-API-Key`

---

## Authentication

### JWT (JSON Web Tokens)

**Header Format**:
```
Authorization: Bearer <jwt_token>
```

**Token Structure**:
```json
{
  "sub": "user_id",
  "tenant_id": "tenant_id",
  "roles": ["admin"],
  "exp": 1704312000,
  "iat": 1704308400
}
```

**Expiry**:
- Access Token: 1 hour
- Refresh Token: 7 days

### API Keys

**Header Format**:
```
X-API-Key: axon_abc123...xyz789
```

**Key Format**: `axon_<32-byte-base62>`

**Scopes**: Embedded in key, checked server-side

### Error Responses

**401 Unauthorized**:
```json
{
  "error": {
    "code": "UNAUTHORIZED",
    "message": "Invalid or expired token",
    "details": {}
  }
}
```

**403 Forbidden**:
```json
{
  "error": {
    "code": "FORBIDDEN",
    "message": "Insufficient permissions",
    "details": {
      "required_scope": "write:task",
      "your_scopes": ["read:task"]
    }
  }
}
```

---

## Rate Limiting

### Headers

Every response includes rate limit headers:

```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 999
X-RateLimit-Reset: 1704312000
```

### Limits by Plan

| Plan | Requests/Minute | Concurrent Tasks | Streams |
|------|-----------------|------------------|---------|
| Trial | 30 | 5 | 3 |
| Entry | 60 | 20 | 10 |
| Pro | 300 | 100 | 100 |
| Enterprise | Custom | Custom | Custom |

### Error Response

**429 Too Many Requests**:
```json
{
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Rate limit exceeded. Retry after 60 seconds.",
    "details": {
      "retry_after": 60,
      "limit": 30,
      "window": "minute"
    }
  }
}
```

---

## Error Handling

### Error Response Format

All error responses follow this structure:

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable error message",
    "details": {},
    "request_id": "req_abc123"
  }
}
```

### Error Codes

| HTTP Status | Error Code | Description |
|-------------|------------|-------------|
| 400 | `BAD_REQUEST` | Invalid request format or parameters |
| 400 | `VALIDATION_ERROR` | Input validation failed |
| 401 | `UNAUTHORIZED` | Missing or invalid authentication |
| 403 | `FORBIDDEN` | Insufficient permissions |
| 404 | `NOT_FOUND` | Resource not found |
| 409 | `CONFLICT` | Resource conflict (e.g., duplicate email) |
| 422 | `UNPROCESSABLE_ENTITY` | Semantic errors in request |
| 429 | `RATE_LIMIT_EXCEEDED` | Too many requests |
| 500 | `INTERNAL_SERVER_ERROR` | Server error |
| 503 | `SERVICE_UNAVAILABLE` | Service temporarily unavailable |

---

## Pagination

### Request Parameters

```
GET /v1/tasks?page=1&per_page=20&sort_by=created_at&sort_order=desc
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `page` | integer | 1 | Page number (1-indexed) |
| `per_page` | integer | 20 | Items per page (max 100) |
| `sort_by` | string | `created_at` | Field to sort by |
| `sort_order` | string | `desc` | `asc` or `desc` |

### Response Format

```json
{
  "data": [...],
  "pagination": {
    "page": 1,
    "per_page": 20,
    "total": 150,
    "total_pages": 8,
    "has_next": true,
    "has_prev": false
  }
}
```

---

## Endpoints

---

## Authentication Endpoints

### POST /v1/auth/register

Register a new user account.

**Request**:
```json
{
  "email": "user@example.com",
  "password": "SecurePassword123!",
  "name": "John Doe",
  "tenant_name": "Acme Corp"
}
```

**Response (201 Created)**:
```json
{
  "user": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "email": "user@example.com",
    "name": "John Doe",
    "created_at": "2025-01-03T10:00:00Z"
  },
  "tenant": {
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "name": "Acme Corp",
    "plan": "trial"
  },
  "tokens": {
    "access_token": "eyJhbGciOiJIUzI1NiIs...",
    "refresh_token": "eyJhbGciOiJIUzI1NiIs...",
    "expires_in": 3600
  }
}
```

**Errors**:
- `409 CONFLICT`: Email already exists
- `400 VALIDATION_ERROR`: Weak password or invalid email

---

### POST /v1/auth/login

Login with email and password.

**Request**:
```json
{
  "email": "user@example.com",
  "password": "SecurePassword123!"
}
```

**Response (200 OK)**:
```json
{
  "user": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "email": "user@example.com",
    "name": "John Doe"
  },
  "tenant": {
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "name": "Acme Corp",
    "plan": "pro"
  },
  "tokens": {
    "access_token": "eyJhbGciOiJIUzI1NiIs...",
    "refresh_token": "eyJhbGciOiJIUzI1NiIs...",
    "expires_in": 3600
  }
}
```

**Errors**:
- `401 UNAUTHORIZED`: Invalid credentials
- `429 RATE_LIMIT_EXCEEDED`: Too many login attempts

---

### POST /v1/auth/refresh

Refresh access token using refresh token.

**Request**:
```json
{
  "refresh_token": "eyJhbGciOiJIUzI1NiIs..."
}
```

**Response (200 OK)**:
```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIs...",
  "refresh_token": "eyJhbGciOiJIUzI1NiIs...",
  "expires_in": 3600
}
```

**Errors**:
- `401 UNAUTHORIZED`: Invalid or expired refresh token

---

### POST /v1/auth/logout

Logout and invalidate tokens.

**Request**: Empty body

**Response (204 No Content)**

---

## MCP Tool Endpoints

### POST /v1/mcp/start_task

Start a new background task.

**Authentication**: Required (JWT or API key)
**Scope**: `write:task`

**Request**:
```json
{
  "name": "deploy-app",
  "adapter": "fly",
  "args": {
    "app": "myapp",
    "region": "iad"
  },
  "timeout_seconds": 900
}
```

**Response (201 Created)**:
```json
{
  "task_id": "770e8400-e29b-41d4-a716-446655440000",
  "name": "deploy-app",
  "adapter": "fly",
  "state": "pending",
  "stream_url": "/v1/mcp/tasks/770e8400-e29b-41d4-a716-446655440000/stream",
  "resume_token": "task_770e8400_seq_0",
  "created_at": "2025-01-03T10:00:00Z"
}
```

**Errors**:
- `400 BAD_REQUEST`: Invalid adapter or args
- `403 FORBIDDEN`: Quota exceeded (concurrent tasks or daily limit)
- `429 RATE_LIMIT_EXCEEDED`: Too many task creations

---

### GET /v1/mcp/tasks/:task_id/stream

Stream task events via Server-Sent Events (SSE).

**Authentication**: Required (JWT or API key)
**Scope**: `read:task`
**Content-Type**: `text/event-stream`

**Query Parameters**:
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `since_seq` | integer | 0 | Start from sequence number (for resume) |

**Request**:
```
GET /v1/mcp/tasks/770e8400-e29b-41d4-a716-446655440000/stream?since_seq=0
Accept: text/event-stream
```

**Response (200 OK)**:
```
Content-Type: text/event-stream
Cache-Control: no-cache
Connection: keep-alive

data: {"seq":0,"ts":"2025-01-03T10:00:00Z","kind":"started","payload":{"adapter":"fly","args":{"app":"myapp"}}}

data: {"seq":1,"ts":"2025-01-03T10:00:05Z","kind":"progress","payload":{"message":"Deploying...","percent":10}}

data: {"seq":2,"ts":"2025-01-03T10:00:10Z","kind":"stdout","payload":{"data":"==> Building image\n"}}

: heartbeat

data: {"seq":3,"ts":"2025-01-03T10:01:00Z","kind":"success","payload":{"exit_code":0,"duration_ms":60000}}
```

**Event Format**:
```json
{
  "seq": 0,
  "ts": "2025-01-03T10:00:00Z",
  "kind": "started|progress|stdout|stderr|success|error|canceled",
  "payload": {}
}
```

**Heartbeat**: `: heartbeat\n` every 25 seconds to keep connection alive

**Errors**:
- `404 NOT_FOUND`: Task not found
- `403 FORBIDDEN`: Not authorized to access this task

---

### GET /v1/mcp/tasks/:task_id/status

Get current task status.

**Authentication**: Required (JWT or API key)
**Scope**: `read:task`

**Response (200 OK)**:
```json
{
  "task_id": "770e8400-e29b-41d4-a716-446655440000",
  "name": "deploy-app",
  "adapter": "fly",
  "state": "succeeded",
  "started_at": "2025-01-03T10:00:00Z",
  "ended_at": "2025-01-03T10:01:00Z",
  "last_seq": 3,
  "bytes_streamed": 1024,
  "minutes_used": 1,
  "created_at": "2025-01-03T09:59:50Z"
}
```

**Errors**:
- `404 NOT_FOUND`: Task not found
- `403 FORBIDDEN`: Not authorized to access this task

---

### POST /v1/mcp/tasks/:task_id/cancel

Cancel a running task.

**Authentication**: Required (JWT or API key)
**Scope**: `write:task`

**Response (200 OK)**:
```json
{
  "task_id": "770e8400-e29b-41d4-a716-446655440000",
  "state": "canceled",
  "canceled_at": "2025-01-03T10:00:30Z"
}
```

**Errors**:
- `404 NOT_FOUND`: Task not found
- `400 BAD_REQUEST`: Task already completed
- `403 FORBIDDEN`: Not authorized to cancel this task

---

### POST /v1/mcp/tasks/:task_id/resume

Resume streaming from last position (alias for stream with since_seq).

**Authentication**: Required (JWT or API key)
**Scope**: `read:task`

**Request**:
```json
{
  "last_seq": 10
}
```

**Response**: Same as `/stream` endpoint (SSE)

---

### GET /v1/mcp/tasks/:task_id/receipt

Get signed integrity receipt for completed task.

**Authentication**: Required (JWT or API key)
**Scope**: `read:task`
**Plan**: Pro or Enterprise only

**Response (200 OK)**:
```json
{
  "task_id": "770e8400-e29b-41d4-a716-446655440000",
  "chain_root": "a3f5b8c9d2e1...",
  "signature": "ed25519:abc123...",
  "range": {
    "from_seq": 0,
    "to_seq": 50
  },
  "events_included": 51,
  "generated_at": "2025-01-03T10:02:00Z",
  "receipt": "base64-encoded-receipt"
}
```

**Errors**:
- `404 NOT_FOUND`: Task not found
- `400 BAD_REQUEST`: Task not completed
- `403 FORBIDDEN`: Feature not available on current plan

---

## Task Endpoints

### GET /v1/tasks

List tasks for current tenant.

**Authentication**: Required (JWT or API key)
**Scope**: `read:task`

**Query Parameters**:
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `state` | string | (all) | Filter by state: pending, running, succeeded, failed, canceled |
| `adapter` | string | (all) | Filter by adapter |
| `created_after` | ISO8601 | (none) | Filter tasks created after date |
| `created_before` | ISO8601 | (none) | Filter tasks created before date |
| `page` | integer | 1 | Page number |
| `per_page` | integer | 20 | Items per page (max 100) |

**Response (200 OK)**:
```json
{
  "data": [
    {
      "id": "770e8400-e29b-41d4-a716-446655440000",
      "name": "deploy-app",
      "adapter": "fly",
      "state": "succeeded",
      "started_at": "2025-01-03T10:00:00Z",
      "ended_at": "2025-01-03T10:01:00Z",
      "minutes_used": 1,
      "created_at": "2025-01-03T09:59:50Z"
    }
  ],
  "pagination": {
    "page": 1,
    "per_page": 20,
    "total": 150,
    "total_pages": 8,
    "has_next": true,
    "has_prev": false
  }
}
```

---

### GET /v1/tasks/:task_id

Get detailed task information.

**Authentication**: Required (JWT or API key)
**Scope**: `read:task`

**Response (200 OK)**:
```json
{
  "id": "770e8400-e29b-41d4-a716-446655440000",
  "tenant_id": "660e8400-e29b-41d4-a716-446655440000",
  "created_by": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "email": "user@example.com",
    "name": "John Doe"
  },
  "name": "deploy-app",
  "adapter": "fly",
  "args": {
    "app": "myapp",
    "region": "iad"
  },
  "state": "succeeded",
  "started_at": "2025-01-03T10:00:00Z",
  "ended_at": "2025-01-03T10:01:00Z",
  "cursor": 50,
  "bytes_streamed": 10240,
  "minutes_used": 1,
  "timeout_seconds": 900,
  "created_at": "2025-01-03T09:59:50Z",
  "updated_at": "2025-01-03T10:01:00Z"
}
```

**Errors**:
- `404 NOT_FOUND`: Task not found
- `403 FORBIDDEN`: Not authorized

---

## API Key Endpoints

### POST /v1/api-keys

Create a new API key.

**Authentication**: Required (JWT only, not API key)
**Scope**: `admin` or owner/admin role

**Request**:
```json
{
  "name": "CI/CD Pipeline",
  "scopes": ["read:task", "write:task"],
  "expires_at": "2026-01-03T00:00:00Z"
}
```

**Response (201 Created)**:
```json
{
  "id": "880e8400-e29b-41d4-a716-446655440000",
  "name": "CI/CD Pipeline",
  "key": "axon_abc123xyz789...",
  "key_prefix": "axon_abc12",
  "scopes": ["read:task", "write:task"],
  "created_at": "2025-01-03T10:00:00Z",
  "expires_at": "2026-01-03T00:00:00Z"
}
```

**⚠️ Important**: The full `key` is only returned ONCE. Store it securely.

**Errors**:
- `403 FORBIDDEN`: Insufficient permissions
- `400 BAD_REQUEST`: Invalid scopes

---

### GET /v1/api-keys

List API keys for current tenant.

**Authentication**: Required (JWT only)
**Scope**: `admin` or owner/admin role

**Response (200 OK)**:
```json
{
  "data": [
    {
      "id": "880e8400-e29b-41d4-a716-446655440000",
      "name": "CI/CD Pipeline",
      "key_prefix": "axon_abc12",
      "scopes": ["read:task", "write:task"],
      "created_at": "2025-01-03T10:00:00Z",
      "last_used_at": "2025-01-03T12:00:00Z",
      "expires_at": "2026-01-03T00:00:00Z",
      "revoked": false
    }
  ]
}
```

---

### DELETE /v1/api-keys/:key_id

Revoke an API key.

**Authentication**: Required (JWT only)
**Scope**: `admin` or owner/admin role

**Response (204 No Content)**

**Errors**:
- `404 NOT_FOUND`: Key not found
- `403 FORBIDDEN`: Insufficient permissions

---

## Webhook Endpoints

### POST /v1/webhooks

Register a new webhook.

**Authentication**: Required (JWT or API key)
**Scope**: `write:webhook` or admin

**Request**:
```json
{
  "url": "https://myapp.com/webhooks/axontask",
  "events": ["task.succeeded", "task.failed"],
  "active": true
}
```

**Response (201 Created)**:
```json
{
  "id": "990e8400-e29b-41d4-a716-446655440000",
  "url": "https://myapp.com/webhooks/axontask",
  "secret": "whsec_abc123...",
  "events": ["task.succeeded", "task.failed"],
  "active": true,
  "created_at": "2025-01-03T10:00:00Z"
}
```

**⚠️ Important**: The `secret` is only returned ONCE. Use it to verify webhook signatures.

**Errors**:
- `400 BAD_REQUEST`: Invalid URL or events
- `403 FORBIDDEN`: Webhook limit exceeded for plan

---

### GET /v1/webhooks

List webhooks for current tenant.

**Authentication**: Required (JWT or API key)
**Scope**: `read:webhook` or admin

**Response (200 OK)**:
```json
{
  "data": [
    {
      "id": "990e8400-e29b-41d4-a716-446655440000",
      "url": "https://myapp.com/webhooks/axontask",
      "events": ["task.succeeded", "task.failed"],
      "active": true,
      "created_at": "2025-01-03T10:00:00Z",
      "updated_at": "2025-01-03T10:00:00Z"
    }
  ]
}
```

---

### DELETE /v1/webhooks/:webhook_id

Delete a webhook.

**Authentication**: Required (JWT or API key)
**Scope**: `write:webhook` or admin

**Response (204 No Content)**

---

### POST /v1/webhooks/:webhook_id/test

Send a test webhook.

**Authentication**: Required (JWT or API key)
**Scope**: `write:webhook` or admin

**Response (200 OK)**:
```json
{
  "webhook_id": "990e8400-e29b-41d4-a716-446655440000",
  "test_sent": true,
  "status_code": 200,
  "response_time_ms": 123
}
```

---

## Usage & Billing Endpoints

### GET /v1/usage

Get current usage for tenant.

**Authentication**: Required (JWT only)

**Query Parameters**:
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `period` | string | `month` | Period: day, week, month, year |

**Response (200 OK)**:
```json
{
  "period": "month",
  "period_start": "2025-01-01",
  "period_end": "2025-01-31",
  "usage": {
    "task_minutes": 5432,
    "tasks_created": 1250,
    "streams": 450,
    "bytes_streamed": 1073741824
  },
  "quotas": {
    "task_minutes_limit": 100000,
    "tasks_per_day_limit": 1000,
    "concurrent_tasks_limit": 100,
    "streams_limit": 100
  },
  "current": {
    "concurrent_tasks": 5,
    "active_streams": 2
  }
}
```

---

### GET /v1/billing/subscription

Get current subscription details.

**Authentication**: Required (JWT only)
**Scope**: owner or admin

**Response (200 OK)**:
```json
{
  "plan": "pro",
  "status": "active",
  "current_period_start": "2025-01-01",
  "current_period_end": "2025-02-01",
  "cancel_at_period_end": false,
  "stripe_subscription_id": "sub_abc123"
}
```

---

### GET /v1/billing/invoices

List invoices for tenant.

**Authentication**: Required (JWT only)
**Scope**: owner or admin

**Response (200 OK)**:
```json
{
  "data": [
    {
      "id": "in_abc123",
      "amount": 2900,
      "currency": "usd",
      "status": "paid",
      "period_start": "2025-01-01",
      "period_end": "2025-02-01",
      "invoice_pdf": "https://stripe.com/invoices/..."
    }
  ]
}
```

---

## Webhook Delivery

### Payload Format

When a webhook event occurs, AxonTask sends a POST request to your registered URL:

**Headers**:
```
Content-Type: application/json
X-AxonTask-Signature: sha256=abc123...
X-AxonTask-Event: task.succeeded
X-AxonTask-Delivery: 990e8400-e29b-41d4-a716-446655440000
```

**Body**:
```json
{
  "event": "task.succeeded",
  "timestamp": "2025-01-03T10:01:00Z",
  "task": {
    "id": "770e8400-e29b-41d4-a716-446655440000",
    "tenant_id": "660e8400-e29b-41d4-a716-446655440000",
    "name": "deploy-app",
    "adapter": "fly",
    "state": "succeeded",
    "started_at": "2025-01-03T10:00:00Z",
    "ended_at": "2025-01-03T10:01:00Z",
    "minutes_used": 1
  }
}
```

### Signature Verification

Verify the signature to ensure the webhook is from AxonTask:

```python
import hmac
import hashlib

def verify_signature(payload, signature, secret):
    expected = hmac.new(
        secret.encode(),
        payload.encode(),
        hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(f"sha256={expected}", signature)
```

### Retry Policy

- Max retries: 5
- Backoff: Exponential with jitter (1s, 2s, 4s, 8s, 16s)
- Success: HTTP 200-299
- Failure: HTTP 400-599 or connection error

---

## OpenAPI Specification

Full OpenAPI 3.0 specification available at:
- **JSON**: `/v1/openapi.json`
- **YAML**: `/v1/openapi.yaml`
- **Interactive Docs**: `/v1/docs` (Swagger UI)

---

## Conclusion

This API design provides a complete, production-ready specification for all AxonTask endpoints. Every endpoint includes:
- ✅ Complete request/response schemas
- ✅ Error handling
- ✅ Authentication and authorization
- ✅ Rate limiting
- ✅ Real-world examples

**Next**: See [FRONTEND_DESIGN.md](FRONTEND_DESIGN.md) for dashboard UI specifications.

---

**Document Version**: 1.0
**Last Updated**: November 04, 2025
**Maintained By**: Tyler Mailman (tyler@axonhub.io)
