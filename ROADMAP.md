# AxonTask Development Roadmap

**Status**: 🚧 In Progress
**Version**: 1.0
**Last Updated**: November 04, 2025
**Architecture**: Rust-first (Axum API + Tokio Workers)
**Scope**: Enhanced MVP (Core + Differentiators)

---

## Table of Contents
- [Project Vision](#project-vision)
- [Development Principles](#development-principles)
- [Technology Stack](#technology-stack)
- [Phase Overview](#phase-overview)
- [Detailed Phases](#detailed-phases)
- [Progress Tracking](#progress-tracking)

---

## Project Vision

AxonTask is a production-ready, open-source system for **persistent background tasks with streaming for AI agents**. It provides MCP-native tools that allow agents to start long-running tasks, stream progress in real-time, and resume reliably across session interruptions.

**Key Features:**
- 🔄 Persistent task execution (survives restarts)
- 📡 Real-time streaming via SSE/WebSocket
- 🔁 Resumable from any point (durable replay)
- 🔐 Hash-chained events for integrity
- 🛡️ Production-grade security & rate limiting
- 🎯 Self-hostable with optional SaaS billing

---

## Development Principles

### Zero Technical Debt
- **No shortcuts**: Every feature is production-ready from day one
- **No "TODO" comments**: If something needs work, create a tracked issue
- **No placeholder code**: Implement fully or defer to later phase

### Self-Documenting Code
- **Clear naming**: Functions, types, and variables explain their purpose
- **Doc comments**: All public APIs have `///` documentation
- **Examples**: Complex logic includes usage examples in docs
- **Architecture Decision Records (ADRs)**: Document significant decisions in `docs/adr/`

### Test Coverage Requirements
- **Unit tests**: All business logic (target: 80%+ coverage)
- **Integration tests**: All API endpoints and worker flows
- **Documentation tests**: All code examples in docs must compile and run
- **Performance tests**: Critical paths have benchmark tests

### Documentation Standards
- **Living documentation**: Update CLAUDE.md with every architectural change
- **API documentation**: OpenAPI spec kept in sync with code
- **Self-hosting guide**: Updated with every new configuration option
- **Runbooks**: Operational procedures documented as they're created

---

## Technology Stack

### Backend (Rust)
- **API Framework**: Axum 0.7+ (tokio-based async web framework)
- **Async Runtime**: Tokio 1.x (multi-threaded runtime)
- **Database**: PostgreSQL 15+ with `sqlx` (compile-time query checking)
- **Message Queue**: Redis 7+ with Redis Streams
- **Authentication**: JWT with `jsonwebtoken` + Argon2 password hashing
- **HTTP Client**: `reqwest` for adapter integrations
- **Serialization**: `serde` + `serde_json`

### Infrastructure
- **Database**: PostgreSQL (primary data store)
- **Cache/Queue**: Redis (streams, rate limiting, pub/sub)
- **Container Runtime**: Docker (for docker adapter)
- **Reverse Proxy**: Nginx or Caddy (recommended for self-hosting)

### Frontend (TBD - Phase 10)
- **Option A**: Leptos (Rust full-stack, WASM)
- **Option B**: Next.js + TypeScript (wider ecosystem)
- **Decision point**: End of Phase 9

### Development Tools
- **Build**: Cargo with workspace
- **Migrations**: sqlx-cli for database migrations
- **Testing**: cargo test + cargo-nextest (optional)
- **Linting**: clippy (strict mode)
- **Formatting**: rustfmt
- **Documentation**: cargo doc
- **Load Testing**: k6 or drill

### External Services
- **Payment Processing**: Stripe (optional, config-disabled for self-hosting)
- **Deployment**: Docker + Docker Compose (local), Fly.io or self-hosted (production)

---

## Phase Overview

| Phase | Name | Status | Estimated Tasks |
|-------|------|--------|----------------|
| 0 | Project Foundation | ⬜ Not Started | 8 tasks |
| 1 | Core Data Layer | ⬜ Not Started | 10 tasks |
| 2 | Authentication System | ⬜ Not Started | 12 tasks |
| 3 | API Framework | ⬜ Not Started | 9 tasks |
| 4 | Redis Streams Infrastructure | ⬜ Not Started | 11 tasks |
| 5 | MCP Tool Endpoints | ⬜ Not Started | 13 tasks |
| 6 | Worker System & Adapters | ⬜ Not Started | 15 tasks |
| 7 | Hash Chain & Integrity | ⬜ Not Started | 8 tasks |
| 8 | Rate Limiting & Quotas | ⬜ Not Started | 10 tasks |
| 9 | Webhook System | ⬜ Not Started | 9 tasks |
| 10 | Dashboard Frontend | ⬜ Not Started | 18 tasks |
| 11 | Stripe Integration | ⬜ Not Started | 12 tasks |
| 12 | Testing Suite | ⬜ Not Started | 14 tasks |
| 13 | Documentation | ⬜ Not Started | 16 tasks |
| 14 | Deployment & DevOps | ⬜ Not Started | 11 tasks |
| 15 | Polish & Launch Prep | ⬜ Not Started | 10 tasks |

**Total**: ~166 tracked tasks

---

## Detailed Phases

---

## Phase 0: Project Foundation

**Goal**: Set up repository structure, tooling, and development environment.

**Status**: ⬜ Not Started

### Tasks

- [ ] **0.1**: Create Cargo workspace structure
  - Files: `Cargo.toml` (workspace root)
  - Crates: `axontask-api`, `axontask-worker`, `axontask-shared`
  - Documentation: Workspace layout in CLAUDE.md

- [ ] **0.2**: Initialize Git repository and configure
  - Files: `.gitignore`, `.gitattributes`
  - Configure: LFS for large files if needed
  - Documentation: Git workflow in CONTRIBUTING.md

- [ ] **0.3**: Set up development Docker Compose
  - Files: `docker-compose.yml`, `docker-compose.dev.yml`
  - Services: Postgres 15, Redis 7
  - Volumes: Persistent data for development
  - Documentation: Getting started in README.md

- [ ] **0.4**: Create database schema SQL
  - Files: `migrations/00000_init_schema.sql`
  - Tables: users, tenants, memberships, api_keys, tasks, task_events, task_snapshots, task_heartbeats, webhooks, webhook_deliveries, usage_counters
  - Documentation: Schema design in `docs/database.md`

- [ ] **0.5**: Configure sqlx for compile-time query checking
  - Files: `.env.example`, `sqlx-data.json`
  - Setup: Database URL, migration runner
  - Documentation: Database setup in CLAUDE.md

- [ ] **0.6**: Set up CI/CD pipeline
  - Files: `.github/workflows/ci.yml`
  - Jobs: lint (clippy), test, format check, build
  - Documentation: CI process in CONTRIBUTING.md

- [ ] **0.7**: Create logging and observability setup
  - Dependencies: `tracing`, `tracing-subscriber`
  - Configuration: Structured JSON logs, log levels
  - Documentation: Logging standards in CLAUDE.md

- [ ] **0.8**: Write initial CONTRIBUTING.md
  - Sections: Code style, PR process, testing requirements, documentation standards
  - Include: Commit message format, branch naming

### Acceptance Criteria
✅ `cargo build` succeeds for all crates
✅ `docker-compose up` starts Postgres and Redis
✅ Database migrations run successfully
✅ CI pipeline runs and passes
✅ CLAUDE.md has project structure documented
✅ CONTRIBUTING.md has clear guidelines

### Documentation Updates
- CLAUDE.md: Project structure, build commands, database setup
- README.md: Quick start guide, prerequisites
- CONTRIBUTING.md: Complete contribution guidelines

---

## Phase 1: Core Data Layer

**Goal**: Implement database models, migrations, and connection pooling.

**Status**: ⬜ Not Started
**Dependencies**: Phase 0

### Tasks

- [ ] **1.1**: Implement database connection pool
  - Files: `axontask-shared/src/db/pool.rs`
  - Features: Connection pooling with `sqlx::PgPool`, health checks
  - Tests: Connection establishment, pool exhaustion handling
  - Documentation: Database connection configuration

- [ ] **1.2**: Create migration runner
  - Files: `axontask-shared/src/db/migrations.rs`
  - Features: Auto-run migrations on startup (configurable), migration status checking
  - Tests: Up/down migrations, idempotency
  - Documentation: Migration workflow in CLAUDE.md

- [ ] **1.3**: Implement User model
  - Files: `axontask-shared/src/models/user.rs`
  - Fields: id, email, password_hash, created_at, updated_at
  - Methods: CRUD operations, find_by_email
  - Tests: All CRUD operations, constraint violations
  - Documentation: User model schema

- [ ] **1.4**: Implement Tenant model
  - Files: `axontask-shared/src/models/tenant.rs`
  - Fields: id, name, plan, created_at, settings (jsonb)
  - Methods: CRUD operations, plan management
  - Tests: All operations, plan transitions
  - Documentation: Tenant model and multi-tenancy design

- [ ] **1.5**: Implement Membership model
  - Files: `axontask-shared/src/models/membership.rs`
  - Fields: tenant_id, user_id, role, created_at
  - Methods: Add/remove members, check permissions
  - Tests: Role management, permission checks
  - Documentation: RBAC design

- [ ] **1.6**: Implement ApiKey model
  - Files: `axontask-shared/src/models/api_key.rs`
  - Fields: id, tenant_id, name, hash, scopes, created_at, last_used_at, revoked
  - Methods: Create (with plaintext return), validate, revoke, update last_used
  - Tests: Key generation, validation, revocation
  - Documentation: API key security model

- [ ] **1.7**: Implement Task model
  - Files: `axontask-shared/src/models/task.rs`
  - Fields: id, tenant_id, name, adapter, args, state, started_at, ended_at, cursor, bytes_streamed, minutes_used, created_by
  - Methods: CRUD, state transitions, statistics
  - Tests: State machine, metrics tracking
  - Documentation: Task lifecycle

- [ ] **1.8**: Implement TaskEvent model
  - Files: `axontask-shared/src/models/task_event.rs`
  - Fields: task_id, seq, ts, kind, payload, hash_prev, hash_curr
  - Methods: Append event, query by range, verify chain
  - Tests: Append-only enforcement, hash chain integrity
  - Documentation: Event sourcing design

- [ ] **1.9**: Implement Webhook model
  - Files: `axontask-shared/src/models/webhook.rs`
  - Fields: id, tenant_id, url, secret, active
  - Methods: CRUD, signature generation
  - Tests: CRUD operations, signature generation
  - Documentation: Webhook security

- [ ] **1.10**: Implement UsageCounter model
  - Files: `axontask-shared/src/models/usage.rs`
  - Fields: tenant_id, period, task_minutes, streams, bytes
  - Methods: Increment counters, get current usage, reset periods
  - Tests: Concurrent increments, period rollover
  - Documentation: Usage tracking and billing

### Acceptance Criteria
✅ All models have complete CRUD operations
✅ All models have >80% test coverage
✅ Database queries use sqlx compile-time checking
✅ All foreign key relationships are enforced
✅ All unique constraints are tested
✅ Documentation includes ER diagram

### Documentation Updates
- CLAUDE.md: Data models and relationships
- `docs/database.md`: Complete schema documentation with ER diagram
- `docs/api/models.md`: Model API reference

---

## Phase 2: Authentication System

**Goal**: Implement JWT-based authentication, password hashing, and API key validation.

**Status**: ⬜ Not Started
**Dependencies**: Phase 1

### Tasks

- [ ] **2.1**: Implement password hashing module
  - Files: `axontask-shared/src/auth/password.rs`
  - Features: Argon2id hashing, verification, secure parameters
  - Tests: Hash/verify roundtrip, timing attack resistance
  - Documentation: Password security policy

- [ ] **2.2**: Implement JWT token generation
  - Files: `axontask-shared/src/auth/jwt.rs`
  - Features: Sign tokens with HS256, configurable expiry, claims (user_id, tenant_id, roles)
  - Tests: Token generation, expiry, invalid signatures
  - Documentation: JWT structure and claims

- [ ] **2.3**: Implement JWT token validation
  - Files: `axontask-shared/src/auth/jwt.rs`
  - Features: Verify signature, check expiry, extract claims
  - Tests: Valid tokens, expired tokens, tampered tokens
  - Documentation: Token validation flow

- [ ] **2.4**: Implement API key generation
  - Files: `axontask-shared/src/auth/api_keys.rs`
  - Features: Generate cryptographically secure keys, prefix format (axon_), hash storage
  - Tests: Key uniqueness, hash verification
  - Documentation: API key format and security

- [ ] **2.5**: Implement API key validation
  - Files: `axontask-shared/src/auth/api_keys.rs`
  - Features: Constant-time comparison, scope checking, revocation checking
  - Tests: Valid keys, revoked keys, incorrect keys
  - Documentation: API key validation flow

- [ ] **2.6**: Create authentication middleware
  - Files: `axontask-api/src/middleware/auth.rs`
  - Features: Extract JWT from Authorization header, validate token, inject user context
  - Tests: All authentication scenarios
  - Documentation: Middleware usage

- [ ] **2.7**: Create API key middleware
  - Files: `axontask-api/src/middleware/api_key.rs`
  - Features: Extract key from header, validate, check scopes, update last_used
  - Tests: All API key scenarios
  - Documentation: API key header format

- [ ] **2.8**: Implement user registration endpoint
  - Files: `axontask-api/src/routes/auth.rs`
  - Endpoint: `POST /auth/register`
  - Features: Email validation, password strength check, create user + default tenant
  - Tests: Valid registration, duplicate email, weak password
  - Documentation: Registration API spec

- [ ] **2.9**: Implement login endpoint
  - Files: `axontask-api/src/routes/auth.rs`
  - Endpoint: `POST /auth/login`
  - Features: Email/password validation, return JWT + refresh token
  - Tests: Valid login, invalid credentials, account lockout (future)
  - Documentation: Login API spec

- [ ] **2.10**: Implement token refresh endpoint
  - Files: `axontask-api/src/routes/auth.rs`
  - Endpoint: `POST /auth/refresh`
  - Features: Validate refresh token, issue new access token
  - Tests: Valid refresh, expired refresh token
  - Documentation: Token refresh flow

- [ ] **2.11**: Implement API key CRUD endpoints
  - Files: `axontask-api/src/routes/api_keys.rs`
  - Endpoints: `POST /api-keys`, `GET /api-keys`, `DELETE /api-keys/:id`
  - Features: Create, list (masked), revoke
  - Tests: All CRUD operations, tenant isolation
  - Documentation: API key management API spec

- [ ] **2.12**: Implement authorization helper
  - Files: `axontask-shared/src/auth/authz.rs`
  - Features: Check tenant membership, role-based permissions, resource ownership
  - Tests: All permission scenarios
  - Documentation: Authorization model

### Acceptance Criteria
✅ All authentication endpoints return proper status codes
✅ JWT tokens are properly signed and validated
✅ API keys are stored securely (hashed)
✅ All endpoints enforce tenant isolation
✅ Rate limiting is applied to auth endpoints
✅ All tests pass with >80% coverage

### Documentation Updates
- CLAUDE.md: Authentication flow and patterns
- `docs/api/authentication.md`: Complete auth API documentation
- `docs/security.md`: Security model and best practices

---

## Phase 3: API Framework

**Goal**: Set up Axum web server with routing, error handling, and middleware.

**Status**: ⬜ Not Started
**Dependencies**: Phase 2

### Tasks

- [ ] **3.1**: Create Axum application struct
  - Files: `axontask-api/src/app.rs`
  - Features: App state (DB pool, Redis, config), router builder, middleware stack
  - Tests: App initialization, graceful shutdown
  - Documentation: Application structure

- [ ] **3.2**: Implement error handling
  - Files: `axontask-api/src/error.rs`
  - Features: Custom error types, HTTP status mapping, error response format
  - Tests: All error scenarios, error serialization
  - Documentation: Error handling patterns

- [ ] **3.3**: Implement request/response types
  - Files: `axontask-shared/src/types/`
  - Features: Common request/response structs, validation derives
  - Tests: Serialization/deserialization
  - Documentation: API type reference

- [ ] **3.4**: Create router structure
  - Files: `axontask-api/src/routes/mod.rs`
  - Features: Modular route organization, versioned API (/v1)
  - Tests: Route registration
  - Documentation: Routing architecture

- [ ] **3.5**: Implement CORS middleware
  - Files: `axontask-api/src/middleware/cors.rs`
  - Features: Configurable origins, credentials support, preflight handling
  - Tests: CORS headers, preflight requests
  - Documentation: CORS configuration

- [ ] **3.6**: Implement security headers middleware
  - Files: `axontask-api/src/middleware/security.rs`
  - Features: HSTS, X-Content-Type-Options, X-Frame-Options, CSP
  - Tests: All security headers present
  - Documentation: Security headers policy

- [ ] **3.7**: Implement request logging middleware
  - Files: `axontask-api/src/middleware/logging.rs`
  - Features: Structured request/response logging, timing, request IDs
  - Tests: Log output format
  - Documentation: Logging format

- [ ] **3.8**: Create health check endpoint
  - Files: `axontask-api/src/routes/health.rs`
  - Endpoint: `GET /health`
  - Features: Database ping, Redis ping, service status
  - Tests: All health states
  - Documentation: Health check spec

- [ ] **3.9**: Implement OpenAPI documentation generation
  - Files: `axontask-api/src/docs.rs`
  - Features: Auto-generate OpenAPI spec from routes
  - Dependencies: `utoipa` or similar
  - Tests: Spec validity
  - Documentation: API documentation workflow

### Acceptance Criteria
✅ API server starts and binds to configured port
✅ All middleware layers function correctly
✅ Error responses follow consistent format
✅ Health check returns proper status
✅ OpenAPI spec is generated and accurate
✅ All tests pass

### Documentation Updates
- CLAUDE.md: API server architecture and middleware stack
- `docs/api/README.md`: API overview and conventions
- `docs/development.md`: Local development guide

---

## Phase 4: Redis Streams Infrastructure

**Goal**: Implement durable event streaming with Redis Streams, backfill, and compaction.

**Status**: ⬜ Not Started
**Dependencies**: Phase 3

### Tasks

- [ ] **4.1**: Create Redis client wrapper
  - Files: `axontask-shared/src/redis/client.rs`
  - Features: Connection pooling, automatic reconnection, health checks
  - Tests: Connection management, failover
  - Documentation: Redis setup and configuration

- [ ] **4.2**: Implement event serialization
  - Files: `axontask-shared/src/events/serialization.rs`
  - Features: Serialize/deserialize task events to Redis Stream format
  - Tests: All event types, roundtrip serialization
  - Documentation: Event format specification

- [ ] **4.3**: Implement stream writer
  - Files: `axontask-shared/src/redis/stream_writer.rs`
  - Features: XADD to events:{task_id}, error handling, retry logic
  - Tests: Write events, error scenarios
  - Documentation: Event publishing patterns

- [ ] **4.4**: Implement stream reader (backfill)
  - Files: `axontask-shared/src/redis/stream_reader.rs`
  - Features: XREAD with COUNT for backfill, cursor management
  - Tests: Read events, pagination, empty streams
  - Documentation: Backfill logic

- [ ] **4.5**: Implement stream reader (live tail)
  - Files: `axontask-shared/src/redis/stream_reader.rs`
  - Features: XREAD BLOCK for real-time events, timeout handling
  - Tests: Live streaming, disconnection, reconnection
  - Documentation: Live streaming patterns

- [ ] **4.6**: Implement heartbeat system
  - Files: `axontask-shared/src/redis/heartbeat.rs`
  - Features: Worker heartbeats to hb:{task_id} every 30s
  - Tests: Heartbeat sending, expiry
  - Documentation: Heartbeat protocol

- [ ] **4.7**: Implement heartbeat watchdog
  - Files: `axontask-worker/src/watchdog.rs`
  - Features: Monitor heartbeats, reclaim orphaned tasks (>2 missed)
  - Tests: Reclaim logic, false positives
  - Documentation: Failure recovery

- [ ] **4.8**: Implement stream compaction
  - Files: `axontask-worker/src/compaction.rs`
  - Features: Roll up old events into snapshots, XTRIM based on plan retention
  - Tests: Compaction logic, snapshot creation
  - Documentation: Compaction policy

- [ ] **4.9**: Create compaction scheduler
  - Files: `axontask-worker/src/compaction_scheduler.rs`
  - Features: Hourly job to compact old streams
  - Tests: Schedule execution, missed runs
  - Documentation: Compaction schedule

- [ ] **4.10**: Implement gap detection
  - Files: `axontask-shared/src/redis/gap_detection.rs`
  - Features: Detect client too far behind, emit compacted summary event
  - Tests: Gap scenarios, summary generation
  - Documentation: Gap handling

- [ ] **4.11**: Create stream metrics
  - Files: `axontask-shared/src/redis/metrics.rs`
  - Features: Track stream lag, event rate, consumer positions
  - Tests: Metrics accuracy
  - Documentation: Metrics and monitoring

### Acceptance Criteria
✅ Events are durably written to Redis Streams
✅ Backfill works for arbitrary cursor positions
✅ Live streaming delivers events in real-time (<200ms)
✅ Heartbeats prevent orphaned tasks
✅ Compaction reduces storage while maintaining integrity
✅ All tests pass

### Documentation Updates
- CLAUDE.md: Redis Streams architecture
- `docs/architecture/streaming.md`: Detailed streaming design
- `docs/operations/redis.md`: Redis operational guide

---

## Phase 5: MCP Tool Endpoints

**Goal**: Implement all MCP tool endpoints (start, stream, status, resume, cancel).

**Status**: ⬜ Not Started
**Dependencies**: Phase 4

### Tasks

- [ ] **5.1**: Implement start_task endpoint
  - Files: `axontask-api/src/routes/mcp/start_task.rs`
  - Endpoint: `POST /mcp/start_task`
  - Features: Validate input, create task record, enqueue to worker, return task_id + stream_url
  - Tests: All input scenarios, tenant quotas, invalid adapters
  - Documentation: start_task API spec with examples

- [ ] **5.2**: Implement get_task_status endpoint
  - Files: `axontask-api/src/routes/mcp/get_status.rs`
  - Endpoint: `GET /mcp/tasks/:task_id/status`
  - Features: Return task state, timestamps, last_seq, metrics
  - Tests: All states, non-existent tasks, tenant isolation
  - Documentation: get_task_status API spec

- [ ] **5.3**: Implement cancel_task endpoint
  - Files: `axontask-api/src/routes/mcp/cancel_task.rs`
  - Endpoint: `POST /mcp/tasks/:task_id/cancel`
  - Features: Signal worker via control stream, update state, emit canceled event
  - Tests: Cancel running task, already completed task
  - Documentation: cancel_task API spec

- [ ] **5.4**: Implement stream_task endpoint (SSE setup)
  - Files: `axontask-api/src/routes/mcp/stream_task.rs`
  - Endpoint: `GET /mcp/tasks/:task_id/stream`
  - Features: SSE connection setup, headers (Content-Type: text/event-stream)
  - Tests: Connection establishment, headers
  - Documentation: SSE connection details

- [ ] **5.5**: Implement stream_task backfill logic
  - Files: `axontask-api/src/routes/mcp/stream_task.rs`
  - Features: Read since_seq parameter, fetch historical events from Redis, send as SSE
  - Tests: Backfill from start, middle, end
  - Documentation: Resume/replay mechanism

- [ ] **5.6**: Implement stream_task live streaming
  - Files: `axontask-api/src/routes/mcp/stream_task.rs`
  - Features: After backfill, subscribe to live events, forward to SSE, heartbeat every 25s
  - Tests: Live events, connection timeout, client disconnect
  - Documentation: Live streaming behavior

- [ ] **5.7**: Implement stream_task error handling
  - Files: `axontask-api/src/routes/mcp/stream_task.rs`
  - Features: Handle gaps (compacted event), stream errors, graceful disconnection
  - Tests: All error scenarios
  - Documentation: Error handling

- [ ] **5.8**: Implement resume_task endpoint
  - Files: `axontask-api/src/routes/mcp/resume_task.rs`
  - Endpoint: `POST /mcp/tasks/:task_id/resume`
  - Features: Same as stream_task but with explicit resume semantics
  - Tests: Resume from various points
  - Documentation: resume_task vs stream_task

- [ ] **5.9**: Implement rate limiting for MCP endpoints
  - Files: `axontask-api/src/routes/mcp/rate_limit.rs`
  - Features: Per-tenant, per-key, per-route token buckets
  - Tests: Rate limit enforcement, burst handling
  - Documentation: Rate limits per plan

- [ ] **5.10**: Implement quota enforcement
  - Files: `axontask-api/src/routes/mcp/quotas.rs`
  - Features: Check concurrent tasks, daily tasks, stream connections
  - Tests: Quota exceeded scenarios
  - Documentation: Quota limits per plan

- [ ] **5.11**: Implement MCP error taxonomy
  - Files: `axontask-shared/src/mcp/errors.rs`
  - Features: Standardized error codes (ADAPTER_TIMEOUT, RATE_LIMIT, etc.)
  - Tests: Error code generation
  - Documentation: MCP error reference

- [ ] **5.12**: Create MCP client SDK (Rust)
  - Files: `axontask-sdk/src/client.rs`
  - Features: Easy-to-use client for MCP tools, auto-retry, auto-resume
  - Tests: All MCP operations
  - Documentation: SDK usage guide

- [ ] **5.13**: Write MCP integration tests
  - Files: `axontask-api/tests/mcp_integration.rs`
  - Features: End-to-end tests for all MCP flows
  - Tests: Start→stream→complete, start→cancel, resume scenarios
  - Documentation: Integration test guide

### Acceptance Criteria
✅ All MCP endpoints implemented and tested
✅ SSE streaming works reliably with resume
✅ Rate limits and quotas are enforced
✅ Error responses follow MCP taxonomy
✅ SDK provides clean developer experience
✅ Integration tests cover all workflows

### Documentation Updates
- CLAUDE.md: MCP endpoint patterns
- `docs/api/mcp.md`: Complete MCP API reference
- `docs/sdk/rust.md`: Rust SDK documentation

---

## Phase 6: Worker System & Adapters

**Goal**: Implement Tokio-based worker system with adapter trait and 4 core adapters.

**Status**: ⬜ Not Started
**Dependencies**: Phase 5

### Tasks

- [ ] **6.1**: Define Adapter trait
  - Files: `axontask-worker/src/adapters/trait.rs`
  - Features: Async trait with start, cancel, methods; stream output
  - Tests: Mock adapter implementation
  - Documentation: Adapter contract

- [ ] **6.2**: Create worker orchestrator
  - Files: `axontask-worker/src/orchestrator.rs`
  - Features: Listen to task queue, dispatch to adapters, manage lifecycle
  - Tests: Task dispatching, cancellation
  - Documentation: Worker architecture

- [ ] **6.3**: Implement task queue reader
  - Files: `axontask-worker/src/queue.rs`
  - Features: Poll Redis for pending tasks, priority handling
  - Tests: Queue polling, prioritization
  - Documentation: Queue mechanism

- [ ] **6.4**: Implement event emitter
  - Files: `axontask-worker/src/events.rs`
  - Features: Emit events to Redis Streams with proper formatting
  - Tests: Event emission, error handling
  - Documentation: Event emission patterns

- [ ] **6.5**: Implement Mock adapter
  - Files: `axontask-worker/src/adapters/mock.rs`
  - Features: Emit deterministic fake events for testing/demo
  - Tests: Event sequence, timing
  - Documentation: Mock adapter usage

- [ ] **6.6**: Implement Shell adapter (sandboxed)
  - Files: `axontask-worker/src/adapters/shell.rs`
  - Features: Execute commands in restricted environment, stream stdout/stderr
  - Sandbox: Read-only FS, no network, resource limits
  - Tests: Command execution, sandbox enforcement, timeout
  - Documentation: Shell adapter security model

- [ ] **6.7**: Implement sandbox enforcement
  - Files: `axontask-worker/src/sandbox.rs`
  - Features: seccomp filters, cgroup limits (CPU, memory), namespace isolation
  - Tests: Escape attempts (should fail)
  - Documentation: Sandboxing approach

- [ ] **6.8**: Implement Docker adapter
  - Files: `axontask-worker/src/adapters/docker.rs`
  - Features: Build/run containers, stream logs, cleanup
  - Tests: Container lifecycle, log streaming, cleanup on cancel
  - Documentation: Docker adapter usage

- [ ] **6.9**: Implement Fly.io adapter
  - Files: `axontask-worker/src/adapters/fly.rs`
  - Features: Call Fly API for deployments, stream status events
  - Tests: Mock Fly API responses
  - Documentation: Fly adapter configuration

- [ ] **6.10**: Implement adapter registry
  - Files: `axontask-worker/src/adapters/registry.rs`
  - Features: Register adapters, lookup by name, versioning
  - Tests: Registration, lookup
  - Documentation: Custom adapter guide

- [ ] **6.11**: Implement timeout handling
  - Files: `axontask-worker/src/timeout.rs`
  - Features: Per-task timeout, graceful termination, force kill
  - Tests: Timeout scenarios, cleanup
  - Documentation: Timeout behavior

- [ ] **6.12**: Implement resource tracking
  - Files: `axontask-worker/src/metrics.rs`
  - Features: Track task_minutes, bytes_streamed, update DB
  - Tests: Metric accuracy
  - Documentation: Resource tracking

- [ ] **6.13**: Implement control stream listener
  - Files: `axontask-worker/src/control.rs`
  - Features: Listen to ctrl:{task_id} for cancellation signals
  - Tests: Cancel propagation
  - Documentation: Control protocol

- [ ] **6.14**: Implement worker shutdown
  - Files: `axontask-worker/src/shutdown.rs`
  - Features: Graceful shutdown, finish running tasks or save state
  - Tests: Shutdown scenarios
  - Documentation: Shutdown behavior

- [ ] **6.15**: Write adapter integration tests
  - Files: `axontask-worker/tests/adapters.rs`
  - Features: End-to-end tests for each adapter
  - Tests: Success, failure, timeout, cancellation for each adapter
  - Documentation: Adapter testing guide

### Acceptance Criteria
✅ Worker successfully executes tasks from queue
✅ All 4 adapters implemented and tested
✅ Sandbox prevents escapes (tested)
✅ Task cancellation works reliably
✅ Resource metrics are accurate
✅ All tests pass

### Documentation Updates
- CLAUDE.md: Worker architecture and adapter system
- `docs/architecture/workers.md`: Worker system design
- `docs/adapters/README.md`: Adapter development guide
- `docs/adapters/shell.md`: Shell adapter documentation
- `docs/adapters/docker.md`: Docker adapter documentation
- `docs/adapters/fly.md`: Fly adapter documentation
- `docs/security.md`: Update with sandbox details

---

## Phase 7: Hash Chain & Integrity

**Goal**: Implement hash-chained events and optional signed receipts for task integrity.

**Status**: ⬜ Not Started
**Dependencies**: Phase 6

### Tasks

- [ ] **7.1**: Implement hash chain builder
  - Files: `axontask-shared/src/integrity/hash_chain.rs`
  - Features: Compute hash_curr from (hash_prev || event_data), SHA-256
  - Tests: Chain building, verification
  - Documentation: Hash chain algorithm

- [ ] **7.2**: Update event emission to include hashes
  - Files: `axontask-worker/src/events.rs`
  - Features: Calculate and include hash_prev, hash_curr in every event
  - Tests: Hash chain integrity across events
  - Documentation: Event hash fields

- [ ] **7.3**: Implement hash chain verification
  - Files: `axontask-shared/src/integrity/verify.rs`
  - Features: Verify chain integrity for a sequence of events
  - Tests: Valid chain, broken chain, missing event
  - Documentation: Verification process

- [ ] **7.4**: Implement receipt generation
  - Files: `axontask-shared/src/integrity/receipt.rs`
  - Features: Generate signed receipt (Ed25519) for completed tasks
  - Tests: Receipt generation, signature verification
  - Documentation: Receipt format

- [ ] **7.5**: Implement receipt signing
  - Files: `axontask-shared/src/integrity/signer.rs`
  - Features: Ed25519 keypair management, signing
  - Tests: Sign/verify roundtrip
  - Documentation: Key management

- [ ] **7.6**: Implement get_task_receipt endpoint
  - Files: `axontask-api/src/routes/mcp/get_receipt.rs`
  - Endpoint: `GET /mcp/tasks/:task_id/receipt`
  - Features: Return signed receipt with chain_root, signature, range
  - Tests: Receipt retrieval, verification
  - Documentation: get_task_receipt API spec

- [ ] **7.7**: Implement receipt verification CLI tool
  - Files: `axontask-cli/src/commands/verify.rs`
  - Features: Verify downloaded receipt independently
  - Tests: Verify valid/invalid receipts
  - Documentation: Verification instructions

- [ ] **7.8**: Add periodic digest events
  - Files: `axontask-worker/src/events.rs`
  - Features: Emit {type:'digest', hash, upto_seq} every N events
  - Tests: Digest event generation
  - Documentation: Digest event purpose

### Acceptance Criteria
✅ All events include valid hash chain
✅ Chain verification detects tampering
✅ Receipts are properly signed and verifiable
✅ CLI tool can verify receipts offline
✅ All tests pass

### Documentation Updates
- CLAUDE.md: Integrity system overview
- `docs/integrity.md`: Complete integrity documentation
- `docs/api/receipts.md`: Receipt API and verification guide

---

## Phase 8: Rate Limiting & Quotas

**Goal**: Implement token bucket rate limiting and execution credit quotas.

**Status**: ⬜ Not Started
**Dependencies**: Phase 5

### Tasks

- [ ] **8.1**: Implement token bucket in Redis
  - Files: `axontask-shared/src/rate_limit/token_bucket.rs`
  - Features: Token bucket algorithm with Redis, refill rate, burst capacity
  - Tests: Rate limit enforcement, refill, burst
  - Documentation: Token bucket algorithm

- [ ] **8.2**: Implement multi-key rate limiter
  - Files: `axontask-shared/src/rate_limit/multi_key.rs`
  - Features: Composite keys {tenant}:{apiKey}:{ip}:{route}
  - Tests: Multi-dimension limiting
  - Documentation: Rate limit dimensions

- [ ] **8.3**: Implement execution credits system
  - Files: `axontask-shared/src/quotas/credits.rs`
  - Features: Credit balance per tenant, hourly refill, task cost calculation
  - Tests: Credit consumption, refill, exhaustion
  - Documentation: Credit system design

- [ ] **8.4**: Implement plan configuration
  - Files: `axontask-shared/src/config/plans.rs`
  - Features: Define limits per plan (Trial, Entry, Pro, Ent)
  - Tests: Plan lookup, limit enforcement
  - Documentation: Plan specifications

- [ ] **8.5**: Implement quota checker middleware
  - Files: `axontask-api/src/middleware/quotas.rs`
  - Features: Check quotas before task start, return 429 if exceeded
  - Tests: Quota enforcement, proper error messages
  - Documentation: Quota error handling

- [ ] **8.6**: Implement rate limit middleware
  - Files: `axontask-api/src/middleware/rate_limit.rs`
  - Features: Apply rate limits per route, return 429 with Retry-After
  - Tests: Rate limit enforcement, headers
  - Documentation: Rate limit headers

- [ ] **8.7**: Implement concurrent task counter
  - Files: `axontask-shared/src/quotas/concurrency.rs`
  - Features: Track concurrent running tasks per tenant
  - Tests: Counter increment/decrement, limit enforcement
  - Documentation: Concurrency limits

- [ ] **8.8**: Implement usage tracking
  - Files: `axontask-shared/src/quotas/usage.rs`
  - Features: Increment usage_counters, daily rollover
  - Tests: Usage accumulation, period transitions
  - Documentation: Usage tracking

- [ ] **8.9**: Create priority lanes
  - Files: `axontask-worker/src/priority.rs`
  - Features: Separate queues for Pro/Ent, priority processing
  - Tests: Priority enforcement
  - Documentation: Priority queue system

- [ ] **8.10**: Write rate limiting tests
  - Files: `axontask-api/tests/rate_limiting.rs`
  - Features: Integration tests for all rate limit scenarios
  - Tests: Per-route limits, burst, refill, multiple clients
  - Documentation: Rate limit testing

### Acceptance Criteria
✅ Rate limits are enforced per plan
✅ Execution credits prevent burst abuse
✅ Concurrent task limits work correctly
✅ 429 responses include proper headers
✅ Priority lanes give preference to paid plans
✅ All tests pass

### Documentation Updates
- CLAUDE.md: Rate limiting architecture
- `docs/rate-limiting.md`: Complete rate limiting documentation
- `docs/api/errors.md`: Update with 429 error details
- `docs/plans.md`: Plan limits reference

---

## Phase 9: Webhook System

**Goal**: Implement webhook registration, HMAC signing, and reliable delivery.

**Status**: ⬜ Not Started
**Dependencies**: Phase 5

### Tasks

- [ ] **9.1**: Implement webhook registration endpoint
  - Files: `axontask-api/src/routes/webhooks.rs`
  - Endpoint: `POST /webhooks`
  - Features: Register webhook URL, generate shared secret
  - Tests: Registration, validation
  - Documentation: Webhook registration API

- [ ] **9.2**: Implement webhook CRUD endpoints
  - Files: `axontask-api/src/routes/webhooks.rs`
  - Endpoints: `GET /webhooks`, `DELETE /webhooks/:id`
  - Features: List, delete webhooks
  - Tests: CRUD operations, tenant isolation
  - Documentation: Webhook management API

- [ ] **9.3**: Implement HMAC signature generation
  - Files: `axontask-shared/src/webhooks/signature.rs`
  - Features: HMAC-SHA256 signature with timestamp, header format
  - Tests: Signature generation, verification
  - Documentation: Webhook signature scheme

- [ ] **9.4**: Implement webhook dispatcher
  - Files: `axontask-worker/src/webhooks/dispatcher.rs`
  - Features: Queue webhook deliveries, async sending
  - Tests: Delivery, queueing
  - Documentation: Webhook delivery architecture

- [ ] **9.5**: Implement retry logic
  - Files: `axontask-worker/src/webhooks/retry.rs`
  - Features: Exponential backoff with jitter, max retries (5)
  - Tests: Retry behavior, backoff timing
  - Documentation: Retry policy

- [ ] **9.6**: Implement delivery tracking
  - Files: `axontask-shared/src/models/webhook_delivery.rs`
  - Features: Record delivery attempts, status, response
  - Tests: Delivery logging
  - Documentation: Delivery logs

- [ ] **9.7**: Implement webhook rate limiting
  - Files: `axontask-worker/src/webhooks/rate_limit.rs`
  - Features: Per-tenant webhook rate limits (10/min Entry, 60/min Pro)
  - Tests: Rate limit enforcement
  - Documentation: Webhook rate limits

- [ ] **9.8**: Implement webhook test endpoint
  - Files: `axontask-api/src/routes/webhooks.rs`
  - Endpoint: `POST /webhooks/:id/test`
  - Features: Send test payload to webhook
  - Tests: Test delivery
  - Documentation: Webhook testing

- [ ] **9.9**: Write webhook integration tests
  - Files: `axontask-worker/tests/webhooks.rs`
  - Features: End-to-end webhook delivery tests
  - Tests: Success, failure, retry, signature validation
  - Documentation: Webhook testing guide

### Acceptance Criteria
✅ Webhooks can be registered and managed
✅ Signatures are properly generated and verifiable
✅ Delivery is reliable with retry
✅ Rate limits prevent abuse
✅ Delivery logs are complete
✅ All tests pass

### Documentation Updates
- CLAUDE.md: Webhook system overview
- `docs/webhooks.md`: Complete webhook documentation
- `docs/api/webhooks.md`: Webhook API reference
- `docs/integrations/webhooks.md`: Integration guide with examples

---

## Phase 10: Dashboard Frontend

**Goal**: Build web dashboard for task management, streaming, and settings.

**Status**: ⬜ Not Started
**Dependencies**: Phase 9

**Note**: Tech stack decision required (Leptos vs Next.js)

### Tasks

- [ ] **10.1**: Choose frontend framework
  - Options: Leptos (Rust WASM) or Next.js (TypeScript)
  - Criteria: Developer experience, ecosystem, performance
  - Documentation: Framework decision rationale

- [ ] **10.2**: Set up frontend project
  - Files: Frontend project structure
  - Features: Build system, dev server, asset pipeline
  - Tests: Build succeeds
  - Documentation: Frontend setup guide

- [ ] **10.3**: Implement authentication flow
  - Pages: Login, Register
  - Features: JWT storage, automatic token refresh
  - Tests: Auth flows, token expiry handling
  - Documentation: Frontend auth patterns

- [ ] **10.4**: Implement API client
  - Files: `src/api/client.ts` or `src/api/client.rs`
  - Features: Typed API client, error handling, auth injection
  - Tests: All API calls
  - Documentation: API client usage

- [ ] **10.5**: Implement dashboard layout
  - Components: Header, sidebar, main content
  - Features: Navigation, user menu, responsive design
  - Tests: Layout rendering
  - Documentation: Layout components

- [ ] **10.6**: Implement task list page
  - Route: `/tasks`
  - Features: List tasks, filter by status, sort, pagination
  - Tests: List rendering, filtering, sorting
  - Documentation: Task list usage

- [ ] **10.7**: Implement task detail page
  - Route: `/tasks/:id`
  - Features: Show task metadata, status, timeline
  - Tests: Detail rendering
  - Documentation: Task detail view

- [ ] **10.8**: Implement live streaming view
  - Component: StreamViewer
  - Features: SSE connection, event display, auto-scroll, replay controls
  - Tests: Streaming, reconnection, resume
  - Documentation: Streaming UI

- [ ] **10.9**: Implement task creation modal
  - Component: CreateTaskModal
  - Features: Select adapter, input args, validate, submit
  - Tests: Validation, submission
  - Documentation: Task creation

- [ ] **10.10**: Implement API key management page
  - Route: `/settings/api-keys`
  - Features: List keys (masked), create, revoke, copy to clipboard
  - Tests: Key management operations
  - Documentation: API key UI

- [ ] **10.11**: Implement settings page
  - Route: `/settings`
  - Features: Profile, password change, preferences
  - Tests: Settings updates
  - Documentation: Settings page

- [ ] **10.12**: Implement usage metrics page
  - Route: `/usage`
  - Features: Display current usage, quotas, historical charts
  - Tests: Metrics display
  - Documentation: Usage monitoring

- [ ] **10.13**: Implement webhook management page
  - Route: `/webhooks`
  - Features: Register, list, delete, test webhooks
  - Tests: Webhook operations
  - Documentation: Webhook UI

- [ ] **10.14**: Implement receipt viewer
  - Component: ReceiptViewer
  - Features: Display receipt, verify signature, download
  - Tests: Receipt rendering, verification
  - Documentation: Receipt verification UI

- [ ] **10.15**: Implement error handling UI
  - Components: ErrorBoundary, Toast notifications
  - Features: User-friendly error messages, retry actions
  - Tests: Error scenarios
  - Documentation: Error handling patterns

- [ ] **10.16**: Implement loading states
  - Components: Skeleton screens, spinners
  - Features: Graceful loading UX
  - Tests: Loading states
  - Documentation: Loading patterns

- [ ] **10.17**: Add responsive design
  - Features: Mobile-friendly layouts, touch interactions
  - Tests: Responsive breakpoints
  - Documentation: Responsive design system

- [ ] **10.18**: Write frontend E2E tests
  - Files: `tests/e2e/`
  - Features: Playwright or Cypress tests for critical flows
  - Tests: Login, create task, stream, manage keys
  - Documentation: E2E testing guide

### Acceptance Criteria
✅ Dashboard is fully functional
✅ All pages are responsive
✅ Live streaming works reliably
✅ Auth flows are secure
✅ All E2E tests pass
✅ UI is accessible (WCAG 2.1 AA)

### Documentation Updates
- CLAUDE.md: Frontend architecture
- `docs/frontend/README.md`: Frontend documentation
- `docs/frontend/components.md`: Component guide
- `docs/development.md`: Update with frontend setup

---

## Phase 11: Stripe Integration

**Goal**: Implement Stripe for subscriptions, usage metering, and billing.

**Status**: ⬜ Not Started
**Dependencies**: Phase 10

### Tasks

- [ ] **11.1**: Set up Stripe account and products
  - Features: Create products (Trial, Entry, Pro, Ent), configure metering
  - Documentation: Stripe setup guide

- [ ] **11.2**: Implement Stripe client
  - Files: `axontask-shared/src/billing/stripe_client.rs`
  - Features: Stripe API wrapper, error handling
  - Tests: Mock Stripe API calls
  - Documentation: Stripe client usage

- [ ] **11.3**: Implement subscription creation
  - Files: `axontask-api/src/routes/billing.rs`
  - Endpoint: `POST /billing/subscribe`
  - Features: Create Stripe customer, subscription, payment method
  - Tests: Subscription creation, errors
  - Documentation: Subscription API

- [ ] **11.4**: Implement usage reporting
  - Files: `axontask-worker/src/billing/usage_reporter.rs`
  - Features: Hourly job to report usage to Stripe
  - Tests: Usage reporting accuracy
  - Documentation: Usage metering

- [ ] **11.5**: Implement Stripe webhook handler
  - Files: `axontask-api/src/routes/billing/webhooks.rs`
  - Endpoint: `POST /billing/webhooks/stripe`
  - Features: Handle subscription events (created, updated, canceled, payment_failed)
  - Tests: All webhook events
  - Documentation: Stripe webhook handling

- [ ] **11.6**: Implement subscription management endpoints
  - Files: `axontask-api/src/routes/billing.rs`
  - Endpoints: `GET /billing/subscription`, `PUT /billing/subscription`, `DELETE /billing/subscription`
  - Features: View, upgrade/downgrade, cancel subscription
  - Tests: All operations
  - Documentation: Subscription management API

- [ ] **11.7**: Implement invoice management
  - Files: `axontask-api/src/routes/billing.rs`
  - Endpoint: `GET /billing/invoices`
  - Features: List invoices, download PDF
  - Tests: Invoice retrieval
  - Documentation: Invoice API

- [ ] **11.8**: Implement payment method management
  - Files: `axontask-api/src/routes/billing.rs`
  - Endpoints: `POST /billing/payment-methods`, `DELETE /billing/payment-methods/:id`
  - Features: Add, remove payment methods
  - Tests: Payment method operations
  - Documentation: Payment method API

- [ ] **11.9**: Implement plan upgrade/downgrade logic
  - Files: `axontask-shared/src/billing/plan_changes.rs`
  - Features: Proration, immediate vs end-of-period changes
  - Tests: All plan transitions
  - Documentation: Plan change behavior

- [ ] **11.10**: Implement billing configuration
  - Files: `axontask-shared/src/config/billing.rs`
  - Features: Environment variables to enable/disable billing, test mode
  - Tests: Config parsing
  - Documentation: Billing configuration

- [ ] **11.11**: Add billing UI to dashboard
  - Pages: `/billing`, `/billing/invoices`
  - Features: Subscription status, upgrade/downgrade, payment methods, invoices
  - Tests: Billing UI flows
  - Documentation: Billing UI guide

- [ ] **11.12**: Write billing integration tests
  - Files: `axontask-api/tests/billing.rs`
  - Features: End-to-end tests with Stripe test mode
  - Tests: Subscribe, upgrade, cancel, webhook handling
  - Documentation: Billing testing guide

### Acceptance Criteria
✅ Subscriptions can be created and managed
✅ Usage is accurately reported to Stripe
✅ Webhooks update subscription status
✅ Invoices are generated and accessible
✅ Billing is config-disabled for self-hosting
✅ All tests pass with Stripe test mode

### Documentation Updates
- CLAUDE.md: Billing system overview
- `docs/billing.md`: Complete billing documentation
- `docs/self-hosting.md`: Update with billing disable instructions
- `docs/api/billing.md`: Billing API reference

---

## Phase 12: Testing Suite

**Goal**: Comprehensive test coverage (unit, integration, load, E2E).

**Status**: ⬜ Not Started
**Dependencies**: Phase 11

### Tasks

- [ ] **12.1**: Set up test infrastructure
  - Files: Test database, test Redis, test fixtures
  - Features: Isolated test environment, seed data
  - Documentation: Testing setup guide

- [ ] **12.2**: Write unit tests for all models
  - Files: `axontask-shared/tests/models/`
  - Features: Test all CRUD operations, constraints, edge cases
  - Tests: >80% coverage for all models
  - Documentation: Model testing patterns

- [ ] **12.3**: Write unit tests for authentication
  - Files: `axontask-shared/tests/auth/`
  - Features: Test password hashing, JWT, API keys
  - Tests: All auth scenarios
  - Documentation: Auth testing patterns

- [ ] **12.4**: Write integration tests for API endpoints
  - Files: `axontask-api/tests/integration/`
  - Features: Test all endpoints with real DB/Redis
  - Tests: Happy paths, error cases, edge cases
  - Documentation: API testing guide

- [ ] **12.5**: Write integration tests for worker system
  - Files: `axontask-worker/tests/integration/`
  - Features: Test task execution, adapters, lifecycle
  - Tests: All adapter flows, cancellation, timeout
  - Documentation: Worker testing guide

- [ ] **12.6**: Write integration tests for streaming
  - Files: `axontask-api/tests/streaming/`
  - Features: Test SSE connection, backfill, live tail, resume
  - Tests: All streaming scenarios
  - Documentation: Streaming testing guide

- [ ] **12.7**: Write integration tests for webhooks
  - Files: `axontask-worker/tests/webhooks/`
  - Features: Test delivery, retry, signatures
  - Tests: Success, failure, retry exhaustion
  - Documentation: Webhook testing guide

- [ ] **12.8**: Write integration tests for billing
  - Files: `axontask-api/tests/billing/`
  - Features: Test Stripe integration with test mode
  - Tests: Subscribe, upgrade, cancel, webhooks
  - Documentation: Billing testing guide

- [ ] **12.9**: Set up load testing with k6
  - Files: `tests/load/k6-scripts/`
  - Features: Load test scripts for critical paths
  - Tests: Start task, stream events, concurrent tasks
  - Documentation: Load testing guide

- [ ] **12.10**: Run load tests and optimize
  - Features: Identify bottlenecks, optimize hot paths
  - Tests: Meet SLOs (p95 stream attach <500ms, event latency <200ms)
  - Documentation: Performance optimization notes

- [ ] **12.11**: Write durability tests
  - Files: `axontask-worker/tests/durability/`
  - Features: Kill worker mid-task, verify reclaim and resume
  - Tests: No event loss, idempotency
  - Documentation: Durability testing guide

- [ ] **12.12**: Write security tests
  - Files: `tests/security/`
  - Features: Test sandbox escapes, injection attacks, auth bypass attempts
  - Tests: All security controls
  - Documentation: Security testing guide

- [ ] **12.13**: Set up continuous integration
  - Files: `.github/workflows/test.yml`
  - Features: Run all tests on PR, block merge on failure
  - Tests: CI passes for all test types
  - Documentation: CI/CD documentation

- [ ] **12.14**: Generate test coverage reports
  - Files: `.github/workflows/coverage.yml`
  - Features: Generate coverage with tarpaulin or cargo-llvm-cov
  - Tests: >80% overall coverage
  - Documentation: Coverage reporting

### Acceptance Criteria
✅ >80% test coverage across all crates
✅ All integration tests pass
✅ Load tests meet SLOs
✅ Security tests pass (no escapes)
✅ Durability tests pass (no data loss)
✅ CI/CD pipeline runs all tests

### Documentation Updates
- CLAUDE.md: Testing standards and commands
- `docs/testing/README.md`: Complete testing guide
- `docs/testing/load.md`: Load testing documentation
- `docs/testing/security.md`: Security testing guide

---

## Phase 13: Documentation

**Goal**: Complete, production-ready documentation for all audiences.

**Status**: ⬜ Not Started
**Dependencies**: Phase 12

### Tasks

- [ ] **13.1**: Write comprehensive README.md
  - Files: `README.md`
  - Sections: Overview, features, quick start, architecture, links
  - Documentation: High-level project introduction

- [ ] **13.2**: Write ARCHITECTURE.md
  - Files: `docs/ARCHITECTURE.md`
  - Sections: System design, components, data flow, decisions
  - Documentation: Architectural overview with diagrams

- [ ] **13.3**: Write API reference documentation
  - Files: `docs/api/README.md`, per-endpoint docs
  - Sections: All endpoints, request/response examples, error codes
  - Documentation: Complete API reference

- [ ] **13.4**: Write MCP contracts documentation
  - Files: `docs/mcp/README.md`
  - Sections: Tool specifications, examples, error handling
  - Documentation: MCP integration guide

- [ ] **13.5**: Write self-hosting guide
  - Files: `docs/self-hosting/README.md`
  - Sections: Requirements, installation, configuration, first run
  - Documentation: Step-by-step deployment guide

- [ ] **13.6**: Write configuration reference
  - Files: `docs/configuration.md`
  - Sections: All environment variables, defaults, examples
  - Documentation: Complete config reference

- [ ] **13.7**: Write adapter development guide
  - Files: `docs/adapters/development.md`
  - Sections: Adapter trait, implementation, testing, registration
  - Documentation: Custom adapter guide

- [ ] **13.8**: Write security documentation
  - Files: `docs/security.md`
  - Sections: Threat model, authentication, authorization, sandboxing, secrets
  - Documentation: Security best practices

- [ ] **13.9**: Write operational runbooks
  - Files: `docs/operations/runbooks/`
  - Sections: Common incidents, monitoring, backup/restore, scaling
  - Documentation: Operations guide

- [ ] **13.10**: Write database documentation
  - Files: `docs/database.md`
  - Sections: Schema, migrations, ER diagram, indexes, queries
  - Documentation: Database guide

- [ ] **13.11**: Write monitoring and observability guide
  - Files: `docs/operations/monitoring.md`
  - Sections: Metrics, logs, traces, alerting, dashboards
  - Documentation: Observability setup

- [ ] **13.12**: Write backup and recovery guide
  - Files: `docs/operations/backup.md`
  - Sections: Backup strategy, restore procedures, disaster recovery
  - Documentation: Backup guide

- [ ] **13.13**: Write scaling guide
  - Files: `docs/operations/scaling.md`
  - Sections: Horizontal scaling, database scaling, Redis scaling, bottlenecks
  - Documentation: Scaling strategies

- [ ] **13.14**: Write troubleshooting guide
  - Files: `docs/troubleshooting.md`
  - Sections: Common issues, debugging tips, logs, support
  - Documentation: Troubleshooting reference

- [ ] **13.15**: Generate and publish API documentation
  - Features: OpenAPI spec, interactive docs (Swagger/ReDoc)
  - Documentation: Published API docs

- [ ] **13.16**: Create architecture diagrams
  - Files: `docs/diagrams/`
  - Sections: System architecture, data flow, deployment, security boundaries
  - Tools: Mermaid, PlantUML, or draw.io
  - Documentation: Visual architecture guide

### Acceptance Criteria
✅ All documentation is complete and accurate
✅ Self-hosting guide works end-to-end
✅ API reference has examples for all endpoints
✅ Architecture diagrams clearly show system design
✅ Runbooks cover common operational scenarios
✅ Documentation is accessible via GitHub Pages or similar

### Documentation Updates
- CLAUDE.md: Final comprehensive update
- All docs: Final review and polish

---

## Phase 14: Deployment & DevOps

**Goal**: Production-ready deployment, monitoring, and operational tooling.

**Status**: ⬜ Not Started
**Dependencies**: Phase 13

### Tasks

- [ ] **14.1**: Create production Dockerfile for API
  - Files: `Dockerfile.api`
  - Features: Multi-stage build, minimal image, security hardening
  - Tests: Image builds and runs
  - Documentation: Docker image documentation

- [ ] **14.2**: Create production Dockerfile for Worker
  - Files: `Dockerfile.worker`
  - Features: Multi-stage build, minimal image, security hardening
  - Tests: Image builds and runs
  - Documentation: Worker image documentation

- [ ] **14.3**: Create Docker Compose for production
  - Files: `docker-compose.prod.yml`
  - Features: API, worker, Postgres, Redis, Nginx
  - Tests: Full stack deployment
  - Documentation: Docker Compose production guide

- [ ] **14.4**: Set up database backup automation
  - Files: `scripts/backup.sh`, cron config
  - Features: Automated daily backups, retention policy, offsite storage
  - Tests: Backup and restore procedures
  - Documentation: Backup automation guide

- [ ] **14.5**: Set up monitoring with Prometheus
  - Files: `prometheus.yml`, metric exporters
  - Features: Scrape API and worker metrics, retention
  - Tests: Metrics collection
  - Documentation: Prometheus setup guide

- [ ] **14.6**: Set up alerting with Alertmanager
  - Files: `alertmanager.yml`, alert rules
  - Features: Alert on critical issues, notification channels
  - Tests: Alert firing
  - Documentation: Alerting guide

- [ ] **14.7**: Set up log aggregation
  - Files: Logging configuration
  - Options: Loki, ELK, or CloudWatch
  - Features: Centralized logs, search, retention
  - Tests: Log ingestion
  - Documentation: Logging setup guide

- [ ] **14.8**: Create deployment automation scripts
  - Files: `scripts/deploy.sh`, CI/CD workflows
  - Features: Zero-downtime deployment, rollback procedures
  - Tests: Deployment and rollback
  - Documentation: Deployment guide

- [ ] **14.9**: Set up health checks and auto-healing
  - Features: Liveness/readiness probes, automatic restart
  - Tests: Health check responses, restart behavior
  - Documentation: Health check configuration

- [ ] **14.10**: Create database migration guide
  - Files: `docs/operations/migrations.md`
  - Features: Safe migration procedures, rollback steps
  - Tests: Migration and rollback
  - Documentation: Migration guide

- [ ] **14.11**: Set up SSL/TLS certificates
  - Features: Let's Encrypt automation or cert management
  - Tests: HTTPS working, cert renewal
  - Documentation: SSL/TLS setup guide

### Acceptance Criteria
✅ Production Docker images are optimized and secure
✅ Full stack deploys successfully
✅ Monitoring captures all key metrics
✅ Alerts fire on critical issues
✅ Logs are centralized and searchable
✅ Deployment is automated and safe
✅ Backups are automated and tested

### Documentation Updates
- CLAUDE.md: Deployment and operations section
- `docs/deployment/README.md`: Complete deployment guide
- `docs/operations/README.md`: Operations overview

---

## Phase 15: Polish & Launch Prep

**Goal**: Final polish, security audit, and launch preparation.

**Status**: ⬜ Not Started
**Dependencies**: Phase 14

### Tasks

- [ ] **15.1**: Security audit - authentication
  - Features: Review auth implementation, test for vulnerabilities
  - Tests: Penetration testing
  - Documentation: Security audit report

- [ ] **15.2**: Security audit - API endpoints
  - Features: Review all endpoints for injection, IDOR, etc.
  - Tests: OWASP Top 10 checks
  - Documentation: Security findings and fixes

- [ ] **15.3**: Security audit - worker sandboxing
  - Features: Review sandbox implementation, test escapes
  - Tests: Escape attempts
  - Documentation: Sandbox security report

- [ ] **15.4**: Performance audit and optimization
  - Features: Profile hot paths, optimize database queries, tune Redis
  - Tests: Load tests before/after
  - Documentation: Performance optimization notes

- [ ] **15.5**: Accessibility audit (WCAG 2.1 AA)
  - Features: Review dashboard for accessibility
  - Tests: Screen reader testing, keyboard navigation
  - Documentation: Accessibility report

- [ ] **15.6**: Create demo environment
  - Features: Public demo instance with sample data
  - Tests: Demo functionality
  - Documentation: Demo setup guide

- [ ] **15.7**: Create quick start tutorial
  - Files: `docs/tutorial.md`
  - Features: Step-by-step first task creation and streaming
  - Documentation: Interactive tutorial

- [ ] **15.8**: Create CLI tool
  - Files: `axontask-cli/`
  - Features: CLI for task management (start, logs, cancel, resume, verify)
  - Tests: All CLI commands
  - Documentation: CLI documentation

- [ ] **15.9**: Polish error messages
  - Features: Review all error messages for clarity and actionability
  - Tests: Error message usability
  - Documentation: Error message guidelines

- [ ] **15.10**: Final documentation review
  - Features: Review all docs for accuracy, completeness, clarity
  - Documentation: Documentation quality report

### Acceptance Criteria
✅ Security audit passed with no critical issues
✅ Performance meets all SLOs
✅ Dashboard is WCAG 2.1 AA compliant
✅ Demo environment is live and functional
✅ CLI tool is complete and documented
✅ All documentation is polished

### Documentation Updates
- CLAUDE.md: Final update with launch checklist
- README.md: Polish and finalize
- All docs: Final review and polish

---

## Progress Tracking

### Overall Progress
- **Phases Completed**: 0 / 16
- **Tasks Completed**: 0 / 166
- **Estimated Progress**: 0%

### Current Phase
**Phase 0: Project Foundation** - ⬜ Not Started

### Next Steps
1. Complete Phase 0 tasks
2. Update CLAUDE.md with initial project structure
3. Begin Phase 1 (Core Data Layer)

---

## Development Rules (Always Enforce)

### Code Quality
1. **No TODO comments** - Create tracked issues instead
2. **No placeholder implementations** - Complete or defer
3. **All public APIs documented** - Use `///` doc comments
4. **All code formatted** - Run `cargo fmt` before commit
5. **All code linted** - Run `cargo clippy` before commit
6. **All tests passing** - Run `cargo test` before commit

### Documentation
1. **Update CLAUDE.md** - After every architectural change
2. **Update ROADMAP.md** - Check off completed tasks
3. **Add examples** - For all complex features
4. **Keep sync** - Code and docs always match

### Testing
1. **Write tests first** - TDD when possible
2. **>80% coverage** - All modules
3. **Test error paths** - Not just happy paths
4. **Integration tests** - For all API endpoints and workers

### Security
1. **Never log secrets** - Redact sensitive data
2. **Validate all input** - Never trust user input
3. **Enforce isolation** - Tenant boundaries everywhere
4. **Use parameterized queries** - Prevent SQL injection

### Performance
1. **Avoid N+1 queries** - Use joins or batch loading
2. **Index frequently queried columns** - Database performance
3. **Use connection pooling** - Don't create connections per request
4. **Profile before optimizing** - Measure, don't guess

---

## Notes

- This roadmap is living document - update as the project evolves
- Mark tasks complete with `[x]` as they're finished
- Add new phases or tasks as needed
- Keep CLAUDE.md in sync with major changes
- Review and update estimates based on actual progress

---

**Last Updated**: November 04, 2025
**Next Review**: After Phase 0 completion
