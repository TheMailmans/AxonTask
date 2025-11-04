# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

---

## Project Overview

**AxonTask** is a production-ready, open-source system for persistent background tasks with real-time streaming, designed for AI agents. It provides MCP-native tools that allow agents to start long-running tasks, stream progress in real-time via SSE, and resume reliably across session interruptions.

**Status**: 🚧 Phase 0 (Foundation) - Initial setup in progress

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
├── models/              # Database models (sqlx)
│   ├── user.rs
│   ├── tenant.rs
│   ├── task.rs
│   ├── task_event.rs
│   ├── api_key.rs
│   └── webhook.rs
├── auth/                # Auth utilities
│   ├── password.rs      # Argon2 hashing
│   ├── jwt.rs           # JWT generation/validation
│   └── api_keys.rs      # API key generation/validation
├── redis/               # Redis clients
│   ├── client.rs        # Connection pooling
│   ├── stream_writer.rs # XADD wrapper
│   └── stream_reader.rs # XREAD wrapper
├── integrity/           # Hash chain and receipts
│   ├── hash_chain.rs
│   ├── receipt.rs
│   └── signer.rs        # Ed25519 signing
└── config/              # Configuration
    ├── plans.rs         # Plan definitions and limits
    └── env.rs           # Environment variable parsing
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

**Phase 0: Project Foundation** (In Progress)
- [x] ROADMAP.md created
- [x] CLAUDE.md created
- [ ] Cargo workspace structure
- [ ] Docker Compose setup
- [ ] Database schema
- [ ] CI/CD pipeline
- [ ] CONTRIBUTING.md

Next: Complete Phase 0, then begin Phase 1 (Core Data Layer)

---

**Last Updated**: 2025-11-03
