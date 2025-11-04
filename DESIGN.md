# AxonTask System Design

**Version**: 1.0
**Last Updated**: November 04, 2025
**Status**: Complete Specification

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [System Overview](#system-overview)
3. [Architecture](#architecture)
4. [Components](#components)
5. [Data Flow](#data-flow)
6. [Technology Stack](#technology-stack)
7. [Security Architecture](#security-architecture)
8. [Scalability & Performance](#scalability--performance)
9. [Deployment Architecture](#deployment-architecture)
10. [Monitoring & Observability](#monitoring--observability)

---

## Executive Summary

**AxonTask** is a distributed system for persistent background task execution with real-time streaming, designed for AI agents. The system ensures tasks survive failures, provides resumable event streams, and maintains tamper-evident audit trails.

###Key Characteristics

- **Distributed**: API servers and workers scale independently
- **Durable**: Tasks and events survive crashes and restarts
- **Resumable**: Clients can reconnect and continue from any point
- **Secure**: Multi-tenant with isolation, rate limiting, and sandboxing
- **Observable**: Comprehensive metrics, logs, and tracing
- **Production-Ready**: Zero technical debt, full test coverage

---

## System Overview

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         CLIENT LAYER                             │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐      │
│  │  Claude  │  │   GPT    │  │  Custom  │  │   Web    │      │
│  │  Agent   │  │  Agent   │  │  Agents  │  │Dashboard │      │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘      │
│       │             │              │             │              │
│       └─────────────┴──────────────┴─────────────┘              │
│                         │                                        │
│                    MCP Tools / HTTP                             │
└─────────────────────────┼──────────────────────────────────────┘
                          │
┌─────────────────────────┼──────────────────────────────────────┐
│                         ▼                                        │
│                   API GATEWAY                                    │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │           Axum API Server (Rust)                           │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  │ │
│  │  │   Auth   │  │   MCP    │  │   Rate   │  │  Stream  │  │ │
│  │  │Middleware│  │Endpoints │  │ Limiting │  │   SSE    │  │ │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘  │ │
│  └────────────────────────────────────────────────────────────┘ │
└────────────┬───────────────────┬──────────────────────┬────────┘
             │                   │                      │
             │                   │                      │
┌────────────▼──────┐  ┌─────────▼──────┐  ┌──────────▼────────┐
│   PostgreSQL      │  │  Redis Streams  │  │  Redis Cache      │
│                   │  │                 │  │                   │
│  ┌─────────────┐  │  │  ┌───────────┐ │  │  ┌─────────────┐ │
│  │   Tasks     │  │  │  │  events:  │ │  │  │Rate Buckets │ │
│  │   Events    │  │  │  │ {task_id} │ │  │  │  Quotas     │ │
│  │   Users     │  │  │  │           │ │  │  │  Sessions   │ │
│  │   Tenants   │  │  │  │  ctrl:    │ │  │  └─────────────┘ │
│  └─────────────┘  │  │  │ {task_id} │ │  └───────────────────┘
└───────────────────┘  │  └───────────┘ │
                       └────────────────┘
                                │
                                │ Poll queue
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                        WORKER LAYER                              │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │          Tokio Workers (Rust) - Multiple Instances         │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  │ │
│  │  │Orchestr- │  │ Adapter  │  │  Event   │  │Heartbeat │  │ │
│  │  │  ator    │  │ Registry │  │ Emitter  │  │  System  │  │ │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘  │ │
│  │                                                            │ │
│  │  ┌──────────────── ADAPTERS ─────────────────┐           │ │
│  │  │  ┌────────┐  ┌────────┐  ┌────────┐      │           │ │
│  │  │  │ Shell  │  │ Docker │  │  Fly   │ ...  │           │ │
│  │  │  │(sandbox)│  │        │  │        │      │           │ │
│  │  │  └────────┘  └────────┘  └────────┘      │           │ │
│  │  └──────────────────────────────────────────┘           │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### Core Principles

1. **Separation of Concerns**: API and workers are independent, communicating via Redis/DB
2. **Event Sourcing**: All task events are append-only and form a hash chain
3. **Durable Replay**: Redis Streams enable backfill + live tail for any cursor position
4. **Tenant Isolation**: Every query/operation includes tenant_id filter
5. **Fail-Safe**: Heartbeats + watchdog reclaim orphaned tasks
6. **Observable**: Structured logging, metrics, and distributed tracing

---

## Architecture

### Layers

#### 1. Client Layer
- **AI Agents**: Claude, GPT, custom agents via MCP protocol
- **Web Dashboard**: Next.js/Leptos frontend for human operators
- **CLI Tool**: Command-line interface for task management

#### 2. API Layer (Axum)
- **MCP Endpoints**: start_task, stream_task, get_status, cancel_task, resume_task
- **Authentication**: JWT + API key middleware
- **Authorization**: Role-based access control (RBAC)
- **Rate Limiting**: Token bucket per tenant/key/route
- **SSE Streaming**: Server-Sent Events with backfill + live tail

#### 3. Data Layer
- **PostgreSQL**: Persistent storage (tasks, events, users, tenants)
- **Redis Streams**: Durable event replay (XADD/XREAD)
- **Redis Cache**: Rate limits, quotas, sessions, temporary data

#### 4. Worker Layer (Tokio)
- **Orchestrator**: Poll task queue, dispatch to adapters
- **Adapters**: Execute tasks (shell, docker, fly, custom)
- **Event Emitter**: Publish events to Redis Streams with hash chain
- **Heartbeat**: Every 30s, signals worker liveness
- **Watchdog**: Reclaim tasks from dead workers

---

## Components

### 1. API Server (`axontask-api`)

**Responsibilities**:
- Accept HTTP requests from clients
- Authenticate and authorize users
- Enforce rate limits and quotas
- Create task records in PostgreSQL
- Stream events via SSE from Redis Streams
- Handle webhooks

**Key Modules**:

```
axontask-api/
├── src/
│   ├── main.rs                  # Entry point, server startup
│   ├── app.rs                   # App state, router
│   ├── routes/
│   │   ├── auth.rs              # POST /auth/register, /login, /refresh
│   │   ├── api_keys.rs          # CRUD for API keys
│   │   ├── tasks.rs             # GET /tasks (list with filters)
│   │   ├── mcp/
│   │   │   ├── start_task.rs    # POST /mcp/start_task
│   │   │   ├── stream_task.rs   # GET /mcp/tasks/:id/stream (SSE)
│   │   │   ├── get_status.rs    # GET /mcp/tasks/:id/status
│   │   │   ├── cancel_task.rs   # POST /mcp/tasks/:id/cancel
│   │   │   ├── resume_task.rs   # POST /mcp/tasks/:id/resume
│   │   │   └── get_receipt.rs   # GET /mcp/tasks/:id/receipt
│   │   └── webhooks.rs          # CRUD for webhooks
│   ├── middleware/
│   │   ├── auth.rs              # JWT + API key validation
│   │   ├── rate_limit.rs        # Token bucket rate limiting
│   │   ├── quotas.rs            # Quota enforcement
│   │   ├── logging.rs           # Request/response logging
│   │   ├── cors.rs              # CORS headers
│   │   └── security.rs          # Security headers (HSTS, CSP, etc.)
│   ├── services/
│   │   ├── sse.rs               # SSE streaming logic
│   │   ├── queue.rs             # Enqueue tasks to Redis
│   │   └── webhooks.rs          # Webhook delivery
│   └── error.rs                 # Error types and HTTP mapping
```

**Technologies**:
- **Axum**: Web framework
- **Tower**: Middleware stack
- **sqlx**: PostgreSQL client
- **redis**: Redis client
- **tokio**: Async runtime

---

### 2. Worker (`axontask-worker`)

**Responsibilities**:
- Poll task queue for pending tasks
- Dispatch tasks to appropriate adapters
- Execute tasks via adapters
- Emit events to Redis Streams with hash chain
- Send heartbeats every 30s
- Handle task cancellation
- Reclaim orphaned tasks (watchdog)

**Key Modules**:

```
axontask-worker/
├── src/
│   ├── main.rs                  # Entry point, worker startup
│   ├── orchestrator.rs          # Main worker loop, task dispatch
│   ├── queue.rs                 # Poll Redis for pending tasks
│   ├── adapters/
│   │   ├── trait.rs             # Adapter trait definition
│   │   ├── registry.rs          # Register and lookup adapters
│   │   ├── mock.rs              # Mock adapter (testing)
│   │   ├── shell.rs             # Shell command execution (sandboxed)
│   │   ├── docker.rs            # Docker container management
│   │   └── fly.rs               # Fly.io deployment monitoring
│   ├── sandbox.rs               # Sandboxing (seccomp, cgroups)
│   ├── events.rs                # Event emission to Redis Streams
│   ├── hash_chain.rs            # Hash chain computation
│   ├── heartbeat.rs             # Worker heartbeat system
│   ├── watchdog.rs              # Orphaned task reclamation
│   ├── compaction.rs            # Stream compaction (snapshots)
│   └── metrics.rs               # Resource tracking (CPU, memory, bytes)
```

**Technologies**:
- **Tokio**: Async runtime
- **sqlx**: PostgreSQL client
- **redis**: Redis client (Streams)
- **nix**: Sandboxing (seccomp, cgroups)
- **bollard**: Docker client

---

### 3. Shared Library (`axontask-shared`)

**Responsibilities**:
- Shared types and utilities
- Database models
- Authentication logic
- Redis Stream utilities
- Configuration management

**Key Modules**:

```
axontask-shared/
├── src/
│   ├── models/
│   │   ├── user.rs              # User model
│   │   ├── tenant.rs            # Tenant model
│   │   ├── membership.rs        # Membership model
│   │   ├── api_key.rs           # API key model
│   │   ├── task.rs              # Task model
│   │   ├── task_event.rs        # Task event model
│   │   ├── task_snapshot.rs     # Task snapshot model
│   │   ├── webhook.rs           # Webhook model
│   │   └── usage.rs             # Usage counter model
│   ├── auth/
│   │   ├── password.rs          # Argon2 password hashing
│   │   ├── jwt.rs               # JWT generation/validation
│   │   └── api_keys.rs          # API key generation/validation
│   ├── redis/
│   │   ├── client.rs            # Redis client wrapper
│   │   ├── stream_writer.rs    # XADD wrapper
│   │   └── stream_reader.rs    # XREAD wrapper
│   ├── integrity/
│   │   ├── hash_chain.rs        # Hash chain builder/verifier
│   │   ├── receipt.rs           # Receipt generation
│   │   └── signer.rs            # Ed25519 signing
│   ├── config/
│   │   ├── plans.rs             # Plan definitions and limits
│   │   └── env.rs               # Environment variable parsing
│   ├── db/
│   │   ├── pool.rs              # Database connection pool
│   │   └── migrations.rs        # Migration runner
│   └── error.rs                 # Common error types
```

---

## Data Flow

### 1. Task Start Flow

```
┌────────┐
│ Client │
└───┬────┘
    │ POST /mcp/start_task
    │ { name, adapter, args }
    ▼
┌────────────┐
│ API Server │
└─────┬──────┘
      │
      │ 1. Validate JWT/API key
      │ 2. Check rate limits
      │ 3. Check quotas (concurrency, daily tasks)
      │ 4. Deduct execution credits
      │
      ▼
┌──────────────┐
│  PostgreSQL  │  INSERT INTO tasks (...)
└──────┬───────┘  state = 'pending'
       │
       │ 5. Enqueue to Redis
       ▼
┌──────────────┐
│ Redis (List) │  LPUSH task_queue {task_id}
└──────┬───────┘
       │
       │ 6. Return response
       ▼
┌────────┐
│ Client │  { task_id, stream_url, resume_token }
└────────┘
```

### 2. Task Execution Flow

```
┌────────────┐
│   Worker   │  (polling task_queue)
└─────┬──────┘
      │ BRPOP task_queue
      │
      │ 1. Fetch task from PostgreSQL
      ▼
┌──────────────┐
│  PostgreSQL  │  SELECT * FROM tasks WHERE id = ...
└──────┬───────┘
       │
       │ 2. Update state to 'running'
       │
       │ 3. Lookup adapter
       ▼
┌──────────────┐
│   Adapter    │  shell / docker / fly / ...
│   (Trait)    │
└──────┬───────┘
       │
       │ 4. Execute task (async stream)
       │    Emit events as they happen
       │
       ▼
┌──────────────┐
│ Event Stream │  progress, stdout, stderr, ...
└──────┬───────┘
       │
       │ 5. For each event:
       │    - Compute hash_curr = SHA256(hash_prev || event_data)
       │    - Emit to Redis Streams
       ▼
┌──────────────┐
│Redis Streams │  XADD events:{task_id} * data={...} hash_prev={...} hash_curr={...}
└──────┬───────┘
       │
       │ 6. Also append to PostgreSQL
       ▼
┌──────────────┐
│  PostgreSQL  │  INSERT INTO task_events (task_id, seq, ...)
└──────┬───────┘
       │
       │ 7. On completion/failure:
       │    - Update task state
       │    - Emit final event
       │
       ▼
┌────────────┐
│   Done     │
└────────────┘
```

### 3. Event Streaming Flow (SSE)

```
┌────────┐
│ Client │
└───┬────┘
    │ GET /mcp/tasks/{id}/stream?since_seq=0
    │
    ▼
┌────────────┐
│ API Server │
└─────┬──────┘
      │
      │ 1. Validate auth
      │ 2. Check tenant ownership
      │ 3. Establish SSE connection
      │
      │ 4. BACKFILL from Redis Streams
      ▼
┌──────────────┐
│Redis Streams │  XREAD COUNT 1000 STREAMS events:{task_id} {since_seq}
└──────┬───────┘
       │
       │ 5. Send events as SSE
       │    data: {event JSON}\n\n
       ▼
┌────────┐
│ Client │  Receive events
└───┬────┘
    │
    │ 6. After backfill complete, switch to LIVE TAIL
    │
    ▼
┌──────────────┐
│Redis Streams │  XREAD BLOCK 5000 STREAMS events:{task_id} {last_id}
└──────┬───────┘
       │
       │ 7. Send new events as they arrive
       ▼
┌────────┐
│ Client │  Receive live updates
└────────┘

(Every 25s, send heartbeat comment to keep connection alive)
```

### 4. Heartbeat & Watchdog Flow

```
┌────────────┐
│   Worker   │
└─────┬──────┘
      │
      │ Every 30 seconds:
      ▼
┌──────────────┐
│Redis Streams │  XADD hb:{task_id} * ts={now} worker_id={worker_id}
└──────────────┘

┌────────────┐
│  Watchdog  │  (separate process or thread)
└─────┬──────┘
      │
      │ Every 60 seconds:
      │ 1. Query tasks with state = 'running'
      │ 2. Check last heartbeat timestamp
      │
      │ If heartbeat > 90s old:
      ▼
┌──────────────┐
│  PostgreSQL  │  UPDATE tasks SET state = 'pending' WHERE id = ...
└──────┬───────┘
       │
       │ 3. Re-enqueue task
       ▼
┌──────────────┐
│ Redis (List) │  LPUSH task_queue {task_id}
└──────────────┘

(Another worker will pick it up)
```

---

## Technology Stack

### Backend

| Component | Technology | Purpose |
|-----------|------------|---------|
| **Language** | Rust 1.75+ | Systems programming, safety, performance |
| **API Framework** | Axum 0.7 | Async web framework built on Tokio |
| **Async Runtime** | Tokio 1.x | Multi-threaded async runtime |
| **Database** | PostgreSQL 15+ | Primary data store |
| **Database Client** | sqlx 0.7 | Compile-time checked SQL queries |
| **Message Queue** | Redis 7+ | Streams for events, Lists for task queue |
| **Redis Client** | redis-rs 0.24 | Redis client with Streams support |
| **Authentication** | jsonwebtoken 9.2 | JWT generation and validation |
| **Password Hashing** | argon2 0.5 | Secure password hashing (Argon2id) |
| **Cryptography** | sha2, ed25519-dalek | Hash chains, digital signatures |
| **Serialization** | serde, serde_json | JSON serialization/deserialization |
| **Error Handling** | thiserror, anyhow | Ergonomic error handling |
| **HTTP Client** | reqwest 0.11 | For adapter integrations (Fly.io API, etc.) |
| **Logging** | tracing, tracing-subscriber | Structured logging and distributed tracing |
| **Docker Client** | bollard | Docker API client (for docker adapter) |

### Frontend (To Be Implemented - Phase 10)

**Option A: Leptos** (Rust full-stack WASM)
- **Leptos**: Reactive UI framework
- **TailwindCSS**: Styling
- **Leptos Router**: Client-side routing
- **leptos_sse**: SSE support

**Option B: Next.js** (TypeScript)
- **Next.js 14**: React framework with App Router
- **TypeScript**: Type safety
- **TailwindCSS**: Styling
- **SWR/React Query**: Data fetching
- **EventSource API**: SSE client

### Infrastructure

| Component | Technology | Purpose |
|-----------|------------|---------|
| **Container Runtime** | Docker | For docker adapter and deployment |
| **Orchestration** | Docker Compose | Local dev, self-hosted deployment |
| **Reverse Proxy** | Caddy or Nginx | HTTPS termination, load balancing |
| **Monitoring** | Prometheus + Grafana | Metrics collection and visualization |
| **Log Aggregation** | Loki or ELK | Centralized log storage |
| **Tracing** | Jaeger | Distributed tracing |
| **CI/CD** | GitHub Actions | Automated testing and deployment |

### Deployment Platforms

- **Fly.io**: Rust API + Workers (recommended for production)
- **Railway/Render**: Alternative managed deployment
- **Vercel**: Frontend (Next.js option)
- **DigitalOcean/Hetzner**: Self-hosted VPS option

---

## Security Architecture

### 1. Authentication

**JWT (JSON Web Tokens)**:
- **Algorithm**: HS256 (HMAC with SHA-256)
- **Expiry**: 1 hour (access token), 7 days (refresh token)
- **Claims**: user_id, tenant_id, roles, exp, iat
- **Storage**: HTTP-only cookies (web) or local storage (dashboard)

**API Keys**:
- **Format**: `axon_` prefix + 32-byte random (base62 encoded)
- **Storage**: SHA-256 hash in database (never plaintext)
- **Scopes**: read:task, write:task, admin
- **Metadata**: last_used_at, created_at, revoked flag

### 2. Authorization

**Tenant Isolation**:
- Every query includes `tenant_id` filter
- Row-Level Security (RLS) enforced at query level
- Cross-tenant access blocked at middleware layer

**Role-Based Access Control (RBAC)**:
- **Roles**: owner, admin, member, viewer
- **Permissions**:
  - owner: all operations
  - admin: manage users, API keys, tasks
  - member: create/view tasks
  - viewer: view tasks only

### 3. Rate Limiting

**Token Bucket Algorithm**:
- **Dimensions**: per-tenant, per-key, per-IP, per-route
- **Storage**: Redis with TTL
- **Lua Scripts**: Atomic refill + consume operations

**Limits**:
- Trial: 300 tasks total, 5 concurrent
- Entry: 1000/day, 20 concurrent
- Pro: 100k/month, 100 concurrent
- Ent: Custom

### 4. Sandboxing (Shell Adapter)

**Defense in Depth**:
1. **Namespace Isolation**: Separate mount, PID, network namespaces
2. **seccomp**: Syscall filtering (allow only safe syscalls)
3. **cgroups**: CPU and memory limits
4. **Read-Only FS**: Temporary writable directories only
5. **No Network**: Block all egress by default (allowlist for adapters needing network)
6. **Timeouts**: Kill processes exceeding time limit

### 5. Data Protection

**At Rest**:
- PostgreSQL: Volume encryption (LUKS or cloud-native)
- Redis: Persistence with AOF (Append-Only File)
- Secrets: Encrypted with AES-256-GCM (KMS or app-level)

**In Transit**:
- TLS 1.3 for all HTTP connections
- Certificate pinning for critical integrations
- HSTS headers enforced

**Secrets Management**:
- Environment variables for config
- Never log secrets (redact in logs)
- API keys hashed with SHA-256
- Passwords hashed with Argon2id

### 6. Audit Logging

**Append-Only Event Log**:
- All admin actions logged (user CRUD, plan changes, etc.)
- Hash-chained for tamper detection
- Optional signed audit export (Enterprise)

---

## Scalability & Performance

### Horizontal Scaling

**API Servers**:
- Stateless: Can add/remove instances dynamically
- Load balanced: Nginx/Caddy round-robin or least-connections
- Session storage: Redis for shared sessions
- Auto-scaling: Based on CPU/memory or request rate

**Workers**:
- Queue-based: Add workers to process more tasks concurrently
- Isolated: Each worker is independent
- Auto-scaling: Based on queue depth (# pending tasks)
- Priority lanes: Pro/Ent tasks in separate queue

### Vertical Scaling

**PostgreSQL**:
- Connection pooling: sqlx PgPool with max_connections
- Read replicas: For read-heavy workloads (optional)
- Indexing: All frequently queried columns indexed
- Partitioning: `task_events` table by date (future optimization)

**Redis**:
- Streams: Trimmed based on retention policy (XTRIM)
- Memory limits: Eviction policy for cache (LRU)
- Clustering: Redis Cluster for horizontal scaling (future)

### Performance Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| **API Response Time** | p95 < 100ms | Prometheus histogram |
| **SSE First Byte** | < 2s | Time to first event after connection |
| **Event Latency** | p95 < 200ms | Worker emit → Client receive |
| **Stream Attach** | p95 < 500ms | SSE connection establishment + backfill |
| **Task Throughput** | 1000+ tasks/sec | With 10 workers |
| **Concurrent Streams** | 10,000+ | Per API instance |

### Caching Strategy

**Redis Cache**:
- User sessions (TTL: 1 hour)
- Rate limit buckets (TTL: based on refill period)
- Tenant plan metadata (TTL: 5 minutes, invalidate on change)
- API key validation cache (TTL: 1 minute)

**Application-Level**:
- Adapter registry (in-memory, initialized at startup)
- Plan configuration (in-memory, reloaded on config change)

---

## Deployment Architecture

### Production Deployment (Fly.io)

```
┌─────────────────────────────────────────────────────────────┐
│                        Fly.io Edge                           │
│                  (Global Anycast, DDoS Protection)           │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                    API Instances                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│  │  API 1   │  │  API 2   │  │  API N   │  (auto-scaled)   │
│  │  (Axum)  │  │  (Axum)  │  │  (Axum)  │                  │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘                  │
└───────┼─────────────┼─────────────┼────────────────────────┘
        │             │             │
        └─────────────┴─────────────┘
                      │
        ┌─────────────┴─────────────┐
        │                           │
        ▼                           ▼
┌──────────────────┐      ┌──────────────────┐
│   PostgreSQL     │      │  Redis (Upstash) │
│   (Fly Postgres) │      │   or Fly Redis   │
│                  │      │                  │
│  - Primary       │      │  - Streams       │
│  - Replica (opt) │      │  - Cache         │
└──────────────────┘      └──────────────────┘
                                   │
                                   │ Poll queue
                                   ▼
┌─────────────────────────────────────────────────────────────┐
│                   Worker Instances                           │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│  │ Worker 1 │  │ Worker 2 │  │ Worker N │  (auto-scaled)   │
│  │ (Tokio)  │  │ (Tokio)  │  │ (Tokio)  │                  │
│  └──────────┘  └──────────┘  └──────────┘                  │
└─────────────────────────────────────────────────────────────┘
```

### Self-Hosted Deployment (Docker Compose)

```
┌─────────────────────────────────────────────────────────────┐
│                         Host Server                          │
│                   (DigitalOcean, Hetzner, etc.)              │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │                   Docker Compose                        │ │
│  │                                                         │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐            │ │
│  │  │  Caddy   │  │   API    │  │  Worker  │            │ │
│  │  │(Reverse  │  │ (Axum)   │  │ (Tokio)  │            │ │
│  │  │  Proxy)  │  │          │  │          │            │ │
│  │  └────┬─────┘  └────┬─────┘  └────┬─────┘            │ │
│  │       │             │             │                   │ │
│  │       │             └─────┬───────┘                   │ │
│  │       │                   │                           │ │
│  │  ┌────▼─────┐  ┌──────────▼────┐  ┌────────────────┐ │ │
│  │  │PostgreSQL│  │     Redis      │  │   Dashboard    │ │ │
│  │  │   (DB)   │  │  (Streams)     │  │  (Next.js)     │ │ │
│  │  └──────────┘  └────────────────┘  └────────────────┘ │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

---

## Monitoring & Observability

### Metrics (Prometheus)

**API Metrics**:
- `http_requests_total` (counter): Total requests by method, path, status
- `http_request_duration_seconds` (histogram): Request latency
- `sse_connections_active` (gauge): Current SSE connections
- `rate_limit_hits_total` (counter): Rate limit violations
- `auth_failures_total` (counter): Authentication failures

**Worker Metrics**:
- `tasks_processed_total` (counter): Tasks by adapter, status
- `task_duration_seconds` (histogram): Task execution time
- `queue_depth` (gauge): Pending tasks in queue
- `heartbeats_sent_total` (counter): Heartbeats by worker
- `orphaned_tasks_reclaimed_total` (counter): Watchdog reclaims

**Database Metrics**:
- `db_connections_active` (gauge): Active connections
- `db_query_duration_seconds` (histogram): Query latency

**Redis Metrics**:
- `redis_stream_length` (gauge): Stream length by task_id
- `redis_commands_total` (counter): Commands by type

### Logging (Structured JSON)

**Log Levels**:
- **ERROR**: Failures, exceptions
- **WARN**: Degraded performance, retries
- **INFO**: Key operations (task started, completed)
- **DEBUG**: Detailed execution flow
- **TRACE**: Extremely verbose (development only)

**Log Fields**:
- `timestamp`: ISO8601
- `level`: error/warn/info/debug/trace
- `message`: Human-readable message
- `task_id`: Associated task (if applicable)
- `tenant_id`: Associated tenant
- `user_id`: Associated user (if applicable)
- `span_id`, `trace_id`: Distributed tracing IDs

### Tracing (Jaeger)

**Distributed Tracing**:
- Span per HTTP request
- Span per task execution
- Span per database query
- Span per Redis operation

**Trace Propagation**:
- W3C Trace Context headers
- Context injected into all async operations

### Alerts (Alertmanager)

**Critical Alerts** (Page immediately):
- API server down
- Worker pool exhausted
- Database connection failures
- Redis connection failures
- Disk usage > 90%

**Warning Alerts** (Slack notification):
- Error rate > 5% for 10 minutes
- p95 latency > 500ms for 10 minutes
- Queue depth > 1000 for 5 minutes
- Orphaned tasks detected

---

## Conclusion

This design provides a complete blueprint for building AxonTask as a production-ready, scalable, and secure system. Every component is specified in detail, enabling independent implementation by multiple developers or teams.

**Next Steps**:
1. Review [DATABASE_DESIGN.md](DATABASE_DESIGN.md) for complete schema
2. Review [API_DESIGN.md](API_DESIGN.md) for all endpoint specifications
3. Review [FRONTEND_DESIGN.md](FRONTEND_DESIGN.md) for UI components and flows
4. Follow [DEPLOYMENT.md](DEPLOYMENT.md) for deployment instructions
5. Follow [SETUP.md](SETUP.md) for step-by-step setup

---

**Document Version**: 1.0
**Last Updated**: November 04, 2025
**Maintained By**: Tyler Mailman (tyler@axonhub.io)
