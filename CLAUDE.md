# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

---

## Project Overview

**AxonTask** is a production-ready, open-source system for persistent background tasks with real-time streaming, designed for AI agents. It provides MCP-native tools that allow agents to start long-running tasks, stream progress in real-time via SSE, and resume reliably across session interruptions.

**Status**: 🚧 Phase 2 (Authentication System) - Partial Complete (Shared library done, API endpoints pending)

**Key Features**:
- Persistent task execution (survives restarts/crashes)
- Real-time streaming via Server-Sent Events (SSE)
- Resumable from any point with Redis Streams
- Hash-chained events for integrity verification
- Production-grade security, rate limiting, and quotas
- Self-hostable with optional Stripe billing

---

## Tech Stack

### Backend (All Rust)
- **API Framework**: Axum 0.7+ (async web framework on Tokio)
- **Async Runtime**: Tokio 1.x (multi-threaded)
- **Database**: PostgreSQL 15+ with `sqlx` (compile-time query checking)
- **Message Queue**: Redis 7+ with Redis Streams (XADD/XREAD)
- **Authentication**: JWT (`jsonwebtoken`) + Argon2 password hashing
- **Serialization**: `serde` + `serde_json`

### Infrastructure
- **Database**: PostgreSQL (tasks, users, events)
- **Cache/Queue**: Redis (streams, rate limiting, pub/sub)
- **Containers**: Docker (for docker adapter and deployment)

### Frontend
- **TBD**: Decision pending (Leptos vs Next.js) - Phase 10

---

## Development Commands

### Prerequisites
- Rust 1.75+ (`rustup install stable`)
- Docker & Docker Compose (for Postgres and Redis)
- sqlx-cli (`cargo install sqlx-cli --no-default-features --features postgres`)

### Initial Setup
```bash
# Clone and enter repository
cd /Users/tylermailman/Documents/Projects/AxonTask

# Start development services (Postgres + Redis)
docker-compose up -d

# Set up environment
cp .env.example .env
# Edit .env with your database credentials

# Run migrations
sqlx database create
sqlx migrate run

# Build all crates
cargo build
```

### Common Commands
```bash
# Run all tests
cargo test

# Run specific crate tests
cargo test -p axontask-api
cargo test -p axontask-worker
cargo test -p axontask-shared

# Run linter (strict mode enforced)
cargo clippy --all-targets --all-features -- -D warnings

# Format code
cargo fmt

# Generate documentation
cargo doc --open --no-deps

# Run API server (development)
cargo run -p axontask-api

# Run worker (development)
cargo run -p axontask-worker

# Run single test
cargo test test_name -- --exact --nocapture

# Check compile-time SQL queries
cargo sqlx prepare

# Database migrations
sqlx migrate add <migration_name>
sqlx migrate run
sqlx migrate revert
```

### Load Testing
```bash
# Install k6
brew install k6  # macOS

# Run load tests
k6 run tests/load/k6-scripts/start_task.js
```

---

## Architecture Overview

### High-Level Design

```
┌──────────────┐
│   Clients    │  (Claude, GPT, agents via MCP)
│  (MCP Tools) │
└──────┬───────┘
       │ HTTP/WS
       ▼
┌──────────────────────────────────────────────┐
│         Axum API Server (Rust)               │
│  - MCP endpoints (start/stream/status/cancel)│
│  - Auth (JWT + API keys)                     │
│  - Rate limiting (Redis token buckets)       │
│  - SSE streaming with backfill + live tail   │
└──────┬───────────────────────────────────────┘
       │
       ├─────► PostgreSQL (tasks, events, users)
       │
       ├─────► Redis Streams (event fanout, replay)
       │
       ▼
┌──────────────────────────────────────────────┐
│       Tokio Workers (Rust)                   │
│  - Poll task queue                           │
│  - Execute via adapters (shell, docker, fly) │
│  - Emit events to Redis Streams              │
│  - Heartbeats every 30s                      │
└──────────────────────────────────────────────┘
```

### Data Flow

1. **Task Start**: Client → API → Postgres (task record) → Redis (task queue) → Worker picks up
2. **Event Emission**: Worker → Redis Streams (events:{task_id}) → Hash chain computed
3. **Event Streaming**: Client → API SSE endpoint → Backfill from Redis → Live tail (XREAD BLOCK)
4. **Task Resume**: Client → API (with last_seq) → Backfill from last_seq → Continue live tail

### Crate Structure

```
axontask/
├── axontask-api/         # Axum web server (MCP endpoints, auth, streaming)
├── axontask-worker/      # Tokio worker (task execution, adapters)
├── axontask-shared/      # Shared types, models, utilities
├── axontask-sdk/         # (Future) Client SDK for MCP tools
└── axontask-cli/         # (Future) CLI tool for task management
```

---

## Key Design Decisions

### 1. **Rust-First Architecture**
- **Why**: Single language for API + workers = better type safety, performance, and deployment simplicity
- **Tradeoff**: Smaller ecosystem vs Node.js, but acceptable for this project

### 2. **Redis Streams for Event Replay**
- **Why**: Durable, ordered, resumable streams with XREAD; better than pub/sub for replay
- **Alternatives Considered**: Kafka (too heavy), RabbitMQ (harder replay), plain Redis lists (less robust)
- **Key Commands**: XADD (write), XREAD (backfill), XREAD BLOCK (live tail), XTRIM (compaction)

### 3. **Hash-Chained Events**
- **Why**: Tamper-evident audit trail; each event hash includes previous event hash
- **Implementation**: SHA-256 hash chain; optional Ed25519 signed receipts for Pro/Ent
- **Stored In**: Postgres `task_events` table (hash_prev, hash_curr columns)

### 4. **Custom JWT Auth (No Supabase)**
- **Why**: Fully self-contained for open source; no external dependencies for self-hosting
- **Implementation**: Argon2id password hashing + HS256 JWT tokens + API keys (hashed)
- **Future**: May add OAuth providers as optional feature

### 5. **Adapter Pattern for Task Execution**
- **Why**: Extensible; new task types just implement `Adapter` trait
- **Adapters Included**: Mock, Shell (sandboxed), Docker, Fly.io
- **Trait**: `async fn start(&self, args: Value) -> Result<impl Stream<Item = Event>>`

### 6. **SSE (Not WebSocket) for Streaming**
- **Why**: Simpler protocol, auto-reconnect in browsers, unidirectional (sufficient), built-in Last-Event-ID
- **Tradeoff**: No bidirectional, but not needed for this use case

### 7. **Stripe Integration (Optional)**
- **Why**: Industry-standard billing; config-disabled for self-hosting
- **Plans**: Trial (7d), Entry ($9.99), Pro ($29), Ent ($199+)
- **Metering**: Task-minutes, stream connections, bytes streamed

---

## Code Organization

### API Server (`axontask-api/`)
```
src/
├── main.rs              # Entry point, server setup
├── app.rs               # App state, router builder
├── routes/              # Route handlers
│   ├── auth.rs          # Register, login, refresh
│   ├── api_keys.rs      # API key CRUD
│   ├── mcp/             # MCP tool endpoints
│   │   ├── start_task.rs
│   │   ├── stream_task.rs
│   │   ├── get_status.rs
│   │   ├── cancel_task.rs
│   │   └── resume_task.rs
│   └── webhooks.rs      # Webhook management
├── middleware/          # Axum middleware
│   ├── auth.rs          # JWT + API key validation
│   ├── rate_limit.rs    # Token bucket rate limiting
│   ├── quotas.rs        # Quota enforcement
│   ├── logging.rs       # Structured logging
│   └── security.rs      # Security headers
└── error.rs             # Error types and HTTP mapping
```

### Worker (`axontask-worker/`)
```
src/
├── main.rs              # Entry point, worker loop
├── orchestrator.rs      # Task queue polling and dispatch
├── adapters/            # Task execution adapters
│   ├── trait.rs         # Adapter trait definition
│   ├── mock.rs          # Mock adapter (testing)
│   ├── shell.rs         # Shell command execution
│   ├── docker.rs        # Docker container management
│   └── fly.rs           # Fly.io deployment monitoring
├── sandbox.rs           # Sandboxing (seccomp, cgroups)
├── events.rs            # Event emission to Redis Streams
├── heartbeat.rs         # Worker heartbeat system
├── watchdog.rs          # Orphaned task reclamation
└── compaction.rs        # Stream compaction (snapshots)
```

### Shared (`axontask-shared/`)
```
src/
├── lib.rs               # Crate entry point
├── db/                  # Database layer (Phase 1) ✅
│   ├── mod.rs
│   ├── pool.rs          # Connection pooling with health checks
│   └── migrations.rs    # Migration runner and utilities
├── models/              # Database models (sqlx) (Phase 1) 🚧
│   ├── mod.rs
│   ├── user.rs          # User accounts ✅
│   ├── tenant.rs        # Organizations (Task 1.4)
│   ├── membership.rs    # User-tenant relationships (Task 1.5)
│   ├── api_key.rs       # API keys (Task 1.6)
│   ├── task.rs          # Tasks (Task 1.7)
│   ├── task_event.rs    # Events (Task 1.8)
│   ├── webhook.rs       # Webhooks (Task 1.9)
│   └── usage.rs         # Usage counters (Task 1.10)
├── auth/                # Auth utilities (Phase 2) ✅
│   ├── password.rs      # Argon2id hashing (Task 2.1) ✅
│   ├── jwt.rs           # JWT generation/validation (Tasks 2.2-2.3) ✅
│   ├── api_key.rs       # API key utilities (Tasks 2.4-2.5) ✅
│   ├── middleware.rs    # Auth middleware (Tasks 2.6-2.7) ✅
│   └── authorization.rs # RBAC helpers (Task 2.12) ✅
├── redis/               # Redis clients (Phase 4)
│   ├── client.rs        # Connection pooling
│   ├── stream_writer.rs # XADD wrapper
│   └── stream_reader.rs # XREAD wrapper
├── integrity/           # Hash chain and receipts (Phase 7)
│   ├── hash_chain.rs
│   ├── receipt.rs
│   └── signer.rs        # Ed25519 signing
└── config/              # Configuration (Phase 2)
    ├── plans.rs         # Plan definitions and limits
    └── env.rs           # Environment variable parsing

tests/                   # Integration tests
├── db_pool_tests.rs     # Pool tests (15 tests) ✅
├── db_migrations_tests.rs # Migration tests (8 tests) ✅
└── models/              # Model integration tests
    ├── user_tests.rs    # User CRUD tests
    ├── tenant_tests.rs
    └── ...
```

---

## Development Guidelines

### Zero Technical Debt Policy
- **No TODO comments**: Create GitHub issues instead
- **No placeholder code**: Implement fully or defer to later phase
- **No hardcoded values**: Use config or constants
- **No copy-paste code**: Extract to functions/modules

### Self-Documenting Code
- **All public items**: Must have `///` doc comments
- **Complex logic**: Add explanatory comments
- **Examples**: Include usage examples in doc comments for public APIs
- **Error messages**: Must be actionable (what went wrong + how to fix)

### Testing Standards
- **Target**: >80% code coverage
- **All models**: Unit tests for CRUD, constraints, edge cases
- **All endpoints**: Integration tests with real DB/Redis
- **All adapters**: Integration tests (success, failure, timeout, cancel)
- **Error paths**: Test failures, not just happy paths
- **Documentation examples**: Must compile and pass as doc tests

### Database Patterns
- **Use sqlx macros**: `query!()` and `query_as!()` for compile-time checking
- **Always filter by tenant_id**: Enforce tenant isolation in all queries
- **Use transactions**: For multi-step operations
- **Parameterized queries**: Never string concatenation (prevent SQL injection)
- **Indexes**: Add indexes for frequently queried columns

### Redis Patterns
- **Stream naming**: `events:{task_id}`, `ctrl:{task_id}`, `hb:{task_id}`
- **Consumer groups**: Use for multi-worker scenarios
- **Trim policy**: XTRIM based on plan retention (24h Trial, 7d Entry, 30d Pro, 90d Ent)
- **Error handling**: Retry with exponential backoff; log failures

### Error Handling
- **Use Result<T, E>**: All fallible operations
- **Custom error types**: Implement `thiserror` for error types
- **Context**: Add context with `.context()` (anyhow or custom)
- **User-facing errors**: Map internal errors to user-friendly messages
- **Log errors**: Always log errors with context before returning

### Security Guidelines
- **Never log secrets**: Redact API keys, tokens, passwords
- **Hash API keys**: Store hashed with prefix; return plaintext only on creation
- **Validate input**: Use validation derives (e.g., `validator` crate)
- **Enforce tenant isolation**: Every query must filter by tenant_id
- **Rate limit**: Apply rate limits to all public endpoints
- **Sandbox**: Execute untrusted code in restricted environment (seccomp, cgroups)

### Performance Guidelines
- **Avoid N+1 queries**: Use joins or batch loading
- **Connection pooling**: Use sqlx PgPool and Redis connection pool
- **Index queries**: Ensure queries use indexes (EXPLAIN ANALYZE)
- **Async all the way**: Avoid blocking operations in async functions
- **Stream large responses**: Don't load entire result set into memory

---

## Important Patterns

### 1. Authentication Middleware Pattern
```rust
// Extract JWT or API key from request, inject into extensions
pub async fn auth_middleware(
    State(pool): State<PgPool>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, AuthError> {
    let token = extract_token(&req)?;
    let user = validate_token(token, &pool).await?;
    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}
```

### 2. Tenant Isolation Pattern
```rust
// Always include tenant_id in queries
let tasks = sqlx::query_as!(
    Task,
    "SELECT * FROM tasks WHERE tenant_id = $1 AND id = $2",
    user.tenant_id,
    task_id
)
.fetch_one(&pool)
.await?;
```

### 3. Redis Streams Backfill + Tail Pattern
```rust
// Backfill historical events
let backfill: Vec<Event> = xread(&redis, "events:{task_id}", since_id, COUNT=1000).await?;
for event in backfill {
    send_sse(event).await?;
}

// Then switch to live tail
loop {
    let events: Vec<Event> = xread_block(&redis, "events:{task_id}", last_id, BLOCK=5000).await?;
    for event in events {
        send_sse(event).await?;
        last_id = event.id;
    }
}
```

### 4. Hash Chain Pattern
```rust
// Each event includes hash of previous event
let hash_curr = sha256(&[hash_prev, &event.data]);
let event = TaskEvent {
    seq,
    data,
    hash_prev,
    hash_curr,
};
// Verification: recompute hash chain and compare
```

### 5. Adapter Pattern
```rust
#[async_trait]
pub trait Adapter: Send + Sync {
    async fn start(&self, args: Value) -> Result<Pin<Box<dyn Stream<Item = Event> + Send>>>;
    async fn cancel(&self, task_id: Uuid) -> Result<()>;
}

// Usage in worker
let adapter: Box<dyn Adapter> = match task.adapter {
    "shell" => Box::new(ShellAdapter::new()),
    "docker" => Box::new(DockerAdapter::new()),
    _ => return Err("Unknown adapter"),
};
let mut stream = adapter.start(task.args).await?;
while let Some(event) = stream.next().await {
    emit_event(task.id, event).await?;
}
```

---

## Database Schema (Overview)

### Core Tables
- **users**: id, email, password_hash, created_at
- **tenants**: id, name, plan, created_at, settings
- **memberships**: tenant_id, user_id, role
- **api_keys**: id, tenant_id, name, hash, scopes, created_at, last_used_at, revoked
- **tasks**: id, tenant_id, name, adapter, args, state, started_at, ended_at, cursor, bytes_streamed, minutes_used
- **task_events**: task_id, seq, ts, kind, payload, hash_prev, hash_curr (append-only)
- **task_snapshots**: task_id, seq, ts, summary, stdout_bytes, stderr_bytes (for compaction)
- **task_heartbeats**: task_id, ts, worker_id
- **webhooks**: id, tenant_id, url, secret, active
- **webhook_deliveries**: id, webhook_id, task_id, status, sent_at, signature, body
- **usage_counters**: tenant_id, period, task_minutes, streams, bytes

### Key Indexes
- `tasks(tenant_id, started_at)` - Listing recent tasks
- `task_events(task_id, seq)` - Event replay
- `api_keys(hash)` - API key lookup
- `usage_counters(tenant_id, period)` - Usage queries

### Migrations
Located in `migrations/` directory. Use sqlx-cli to manage.

---

## MCP Tool Contracts

### start_task
```json
POST /mcp/start_task
{
  "name": "deploy-app",
  "adapter": "fly",
  "args": { "app": "myapp" },
  "timeout_s": 900
}
→ {
  "task_id": "uuid",
  "stream_url": "/mcp/tasks/{id}/stream",
  "resume_token": "token"
}
```

### stream_task
```
GET /mcp/tasks/{task_id}/stream?since_seq=0
Headers: Accept: text/event-stream
→ SSE stream of events
```

### get_task_status
```
GET /mcp/tasks/{task_id}/status
→ {
  "state": "running",
  "started_at": "iso8601",
  "ended_at": null,
  "last_seq": 42
}
```

### cancel_task
```
POST /mcp/tasks/{task_id}/cancel
→ { "canceled": true }
```

### resume_task
```
POST /mcp/tasks/{task_id}/resume
{ "last_seq": 10 }
→ (same as stream_task with backfill)
```

---

## Configuration

### Environment Variables

See `.env.example` for full list. Key variables:

```bash
# Database
DATABASE_URL=postgresql://user:pass@localhost/axontask

# Redis
REDIS_URL=redis://localhost:6379

# API Server
API_PORT=8080
API_HOST=0.0.0.0

# JWT
JWT_SECRET=<generate-with-openssl-rand>
JWT_EXPIRY_SECONDS=3600

# Stripe (optional)
STRIPE_SECRET_KEY=sk_test_...
STRIPE_WEBHOOK_SECRET=whsec_...
BILLING_ENABLED=false  # Set to true for SaaS

# Rate Limits (per plan)
ENTRY_TASKS_PER_DAY=1000
ENTRY_CONCURRENCY=20
PRO_CONCURRENCY=100

# Stream Retention (hours/days)
STREAM_TRIM_TRIAL_HOURS=24
STREAM_TRIM_ENTRY_DAYS=7
STREAM_TRIM_PRO_DAYS=30
STREAM_TRIM_ENT_DAYS=90

# Security
HMAC_SECRET=<generate-with-openssl-rand>
RECEIPT_SIGNING_KEY_ED25519=<generate-with-openssl>
```

---

## Testing

### Run All Tests
```bash
cargo test
```

### Run Specific Tests
```bash
# Unit tests for models
cargo test -p axontask-shared --lib models

# Integration tests for API
cargo test -p axontask-api --test integration

# Single test
cargo test test_create_task -- --exact --nocapture
```

### Load Testing
```bash
k6 run tests/load/k6-scripts/start_task.js
```

### Coverage
```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html --output-dir coverage
open coverage/index.html
```

---

## Deployment

### Local Development
```bash
docker-compose up -d       # Start Postgres + Redis
cargo run -p axontask-api  # Run API server
cargo run -p axontask-worker  # Run worker (separate terminal)
```

### Production (Docker)
```bash
docker-compose -f docker-compose.prod.yml up -d
```

### Self-Hosting Guide
See `docs/self-hosting/README.md` for complete guide.

---

## Common Tasks

### Add a New Database Migration
```bash
sqlx migrate add create_new_table
# Edit migrations/XXXXXX_create_new_table.sql
sqlx migrate run
cargo sqlx prepare  # Update sqlx-data.json for compile-time checking
```

### Add a New API Endpoint
1. Define route handler in `axontask-api/src/routes/`
2. Add route in `axontask-api/src/app.rs`
3. Add authentication middleware if needed
4. Add rate limiting if needed
5. Write integration test in `axontask-api/tests/`
6. Update OpenAPI spec (auto-generated or manual)
7. Document in `docs/api/`

### Add a New Adapter
1. Implement `Adapter` trait in `axontask-worker/src/adapters/your_adapter.rs`
2. Register in `axontask-worker/src/adapters/registry.rs`
3. Write integration tests in `axontask-worker/tests/adapters/`
4. Document in `docs/adapters/your_adapter.md`
5. Update README.md with adapter list

### Debug SSE Streaming Issues
```bash
# Test SSE endpoint with curl
curl -N -H "Authorization: Bearer <token>" \
  http://localhost:8080/mcp/tasks/{task_id}/stream

# Check Redis Streams
redis-cli XREAD COUNT 10 STREAMS events:{task_id} 0

# Check worker logs
docker logs axontask-worker -f
```

---

## Troubleshooting

### Tests Failing
1. Ensure Docker services are running: `docker-compose up -d`
2. Reset test database: `sqlx database drop && sqlx database create && sqlx migrate run`
3. Clear Redis: `redis-cli FLUSHALL`
4. Re-run: `cargo test`

### Compile-Time SQL Errors
1. Ensure DATABASE_URL in .env is correct
2. Run migrations: `sqlx migrate run`
3. Update sqlx-data.json: `cargo sqlx prepare`

### Worker Not Picking Up Tasks
1. Check Redis connection: `redis-cli PING`
2. Verify task is queued: `redis-cli LRANGE task_queue 0 -1`
3. Check worker logs for errors
4. Ensure worker is running: `cargo run -p axontask-worker`

---

## Resources

- **Roadmap**: See `ROADMAP.md` for detailed development plan
- **API Docs**: See `docs/api/` for complete API reference
- **Architecture**: See `docs/ARCHITECTURE.md` for system design
- **Self-Hosting**: See `docs/self-hosting/README.md` for deployment guide
- **Contributing**: See `CONTRIBUTING.md` for contribution guidelines

---

## Current Development Status

### Phase 0: Project Foundation ✅ COMPLETED

- [x] **0.1** Cargo workspace structure (`axontask-api`, `axontask-worker`, `axontask-shared`)
- [x] **0.2** Git repository initialized with .gitignore
- [x] **0.3** Docker Compose setup (PostgreSQL 15 + Redis 7)
- [x] **0.4** Database schema SQL migrations (11 tables, enums, indexes)
- [x] **0.5** sqlx configured for compile-time query checking
- [x] **0.6** CI/CD pipeline (GitHub Actions: test, lint, build, security, docs, coverage)
- [x] **0.7** Logging setup (tracing + tracing-subscriber in API/Worker)
- [x] **0.8** CONTRIBUTING.md with code standards
- [x] ROADMAP.md (16 phases, 166 tasks)
- [x] CLAUDE.md (this file)
- [x] LICENSE (BSL 1.1 + Commercial)
- [x] CLA.md (Contributor License Agreement)
- [x] Complete documentation (DESIGN.md, DATABASE_DESIGN.md, API_DESIGN.md, etc.)

**Status**: ✅ Complete (2025-01-03)

---

### Phase 1: Core Data Layer ✅ COMPLETED

**All 10 Tasks Completed:**

- [x] **1.1** Database connection pool (`axontask-shared/src/db/pool.rs` - 320 lines)
  - Production-grade PgPool with configurable timeouts, health checks, statistics
  - 15 integration tests (pool exhaustion, concurrent queries, transactions)

- [x] **1.2** Migration runner (`axontask-shared/src/db/migrations.rs` - 280 lines)
  - Auto-run migrations, status checking, idempotency verification
  - Database create/drop utilities, 8 integration tests

- [x] **1.3** User model (`axontask-shared/src/models/user.rs` - 568 lines)
  - Full CRUD, find_by_email, pagination, last_login tracking
  - Fields: id, email (CITEXT), email_verified, password_hash, name, avatar_url

- [x] **1.4** Tenant model (`axontask-shared/src/models/tenant.rs` - 674 lines)
  - Plan management (Trial/Entry/Pro/Enterprise), Stripe integration
  - Settings merge (JSONB), list_by_plan, cascading deletes

- [x] **1.5** Membership model (`axontask-shared/src/models/membership.rs` - 677 lines)
  - RBAC with 4 roles (Owner/Admin/Member/Viewer)
  - Permission checks, list_by_tenant/user/role, access verification

- [x] **1.6** ApiKey model (`axontask-shared/src/models/api_key.rs` - 417 lines)
  - Secure key generation (axon_ prefix), SHA-256 hashing
  - Scopes, revocation, expiration, last_used tracking

- [x] **1.7** Task model (`axontask-shared/src/models/task.rs` - 660 lines)
  - State machine (pending→running→succeeded/failed/timeout/canceled)
  - State transition methods, statistics tracking, tenant isolation

- [x] **1.8** TaskEvent model (`axontask-shared/src/models/task_event.rs` - 500 lines)
  - Append-only event log with SHA-256 hash chaining
  - Hash chain verification, query by range, event kinds enum

- [x] **1.9** Webhook model (`axontask-shared/src/models/webhook.rs` - 480 lines)
  - HMAC-SHA256 signature generation, event subscriptions
  - Active/inactive toggling, find_by_event_type

- [x] **1.10** UsageCounter model (`axontask-shared/src/models/usage.rs` - 380 lines)
  - Daily usage tracking (task_minutes, streams, bytes, tasks_created)
  - Increment methods, history queries, aggregation, period management

**Migrations:**

- `migrations/20250103000000_init_schema.sql` (350 lines)
  - All 11 tables with indexes, constraints, comments
  - Enums: membership_role, task_state

- `migrations/20250103000000_init_schema.down.sql` (30 lines)
  - Complete rollback in reverse dependency order

**Code Quality Metrics:**

- **Total Lines**: 5,425 lines (models + db + migrations)
- **Models**: 10 complete models (~4,400 lines)
- **Database**: Pool + migrations (~600 lines)
- **Migrations**: UP + DOWN SQL (~380 lines)
- **Tests**: 23 unit tests + integration test infrastructure
- **Documentation**: 100% of public APIs with examples
- **Clippy**: Passes with `-D warnings` (zero warnings)
- **Technical Debt**: ZERO (no TODOs, no placeholders, no shortcuts)

**Status**: ✅ Complete (2025-01-03)

---

### Phase 2: Authentication System ✅ PARTIALLY COMPLETE (Shared Library)

**Tasks 2.1-2.7 and 2.12 Completed (Shared Library)**

Tasks 2.8-2.11 (API endpoints) require `axontask-api` crate (Phase 3).

#### Implemented Modules:

- [x] **2.1** Password Hashing (`axontask-shared/src/auth/password.rs` - 406 lines)
  - Algorithm: Argon2id (Password Hashing Competition winner)
  - Parameters: 64 MB memory, 3 iterations, 4 parallelism
  - Functions: `hash_password()`, `verify_password()`, `validate_password_strength()`
  - Security: Random salt, constant-time verification, timing attack resistance
  - Tests: 14 comprehensive tests

- [x] **2.2-2.3** JWT Authentication (`axontask-shared/src/auth/jwt.rs` - 500 lines)
  - Algorithm: HS256 (HMAC-SHA256)
  - Token Types: Access (24h), Refresh (30d)
  - Claims: user_id (sub), tenant_id, token_type, iss, iat, exp, nbf
  - Functions:
    - `create_token()`: Generate signed JWT
    - `validate_token()`: Verify signature and claims
    - `validate_access_token()`: Type-safe access validation
    - `validate_refresh_token()`: Type-safe refresh validation
    - `refresh_access_token()`: Exchange refresh for new access token
  - Tests: 12 tests (valid/expired/tampered tokens)

- [x] **2.4-2.5** API Key Utilities (`axontask-shared/src/auth/api_key.rs` - 580 lines)
  - Format: `axon_{32_random_chars}` (base62, 37 chars total)
  - Security: SHA-256 hashing, constant-time comparison
  - Functions:
    - `generate_api_key()`: Cryptographically random generation
    - `hash_api_key()`: SHA-256 for storage
    - `verify_api_key()`: Constant-time validation
    - `validate_api_key_format()`: Format validation
    - `parse_scopes()`: Parse comma-separated scopes
    - `has_scope()`: Wildcard matching (supports `tasks:*`, `*`)
  - Tests: 15 tests including timing attack resistance

- [x] **2.6-2.7** Authentication Middleware (`axontask-shared/src/auth/middleware.rs` - 480 lines)
  - JWT Middleware: Validates Bearer tokens from `Authorization` header
  - API Key Middleware: Validates keys from `X-Api-Key` header with DB lookup
  - AuthContext: Injected into request extensions
    - Fields: user_id, tenant_id, method (JWT/ApiKey), scopes, api_key_id
    - Methods: `has_scope()` for permission checking
  - Error Handling: 401 (Unauthorized), 400 (Bad Request), 500 (Internal Server Error)
  - Integration: Axum tower middleware with helper functions
  - Tests: 4 unit tests

- [x] **2.12** Authorization Helpers (`axontask-shared/src/auth/authorization.rs` - 520 lines)
  - Permission Model: Role-based + Scope-based access control
  - ResourcePermission: Read, Write, Manage, Own (maps to RBAC roles)
  - Functions:
    - `require_membership()`: Verify tenant membership
    - `require_role()`: Check RBAC role (Owner/Admin/Member/Viewer)
    - `require_scope()`: Validate API key scopes
    - `require_permission()`: Combined role + scope check
    - `require_ownership()`: Verify resource ownership
    - `require_access()`: Owner OR permission check
    - `require_billing_access()`: Owner-only operations
    - `require_user_management()`: Admin+ operations
  - Tests: 4 unit tests

#### Pending Tasks (Require API Server):

- [ ] **2.8** User registration endpoint (`POST /auth/register`)
- [ ] **2.9** Login endpoint (`POST /auth/login`)
- [ ] **2.10** Token refresh endpoint (`POST /auth/refresh`)
- [ ] **2.11** API key CRUD endpoints (`/api/keys/*`)

**Code Quality Metrics:**

- **Total Lines**: ~2,500 lines (including tests and documentation)
- **Modules**: 5 complete auth modules
- **Tests**: 49 unit tests across all modules
- **Documentation**: 100% of public APIs with examples
- **Clippy**: Passes with `-D warnings` (zero warnings)
- **Security**:
  - Argon2id with 64 MB memory (GPU-resistant)
  - SHA-256 hashing for API keys
  - Constant-time comparisons
  - Timing attack resistance verified in tests
- **Technical Debt**: ZERO (no TODOs, no placeholders)

**Dependencies Used:**

- `argon2` - Password hashing (already in workspace)
- `jsonwebtoken` - JWT implementation
- `sha2` - SHA-256 hashing
- `rand` - Cryptographic randomness
- `axum` - Web framework integration
- `sqlx` - Database operations

**Next**: Phase 3 - API Server Setup (create `axontask-api` crate and implement endpoints 2.8-2.11)

**Status**: ✅ Shared library complete (2025-01-03), API endpoints pending

---

**Last Updated**: 2025-01-03
