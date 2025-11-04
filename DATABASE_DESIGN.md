# AxonTask Database Design

**Version**: 1.0
**Last Updated**: November 04, 2025
**Database**: PostgreSQL 15+
**Status**: Complete Specification

---

## Table of Contents

1. [Overview](#overview)
2. [Entity Relationship Diagram](#entity-relationship-diagram)
3. [Tables](#tables)
4. [Indexes](#indexes)
5. [Constraints](#constraints)
6. [Migrations](#migrations)
7. [Query Patterns](#query-patterns)
8. [Optimization](#optimization)
9. [Backup & Recovery](#backup--recovery)

---

## Overview

The AxonTask database uses PostgreSQL as the primary data store for all persistent data. The schema is designed for:

- **Multi-tenancy**: Every table includes `tenant_id` for isolation
- **Audit trail**: Append-only event log with hash chaining
- **Performance**: Optimized indexes for common queries
- **Scalability**: Partitioning ready for large tables
- **Compliance**: GDPR-compliant data retention and deletion

### Database Naming Conventions

- **Tables**: Plural, snake_case (e.g., `tasks`, `task_events`)
- **Columns**: snake_case (e.g., `created_at`, `tenant_id`)
- **Indexes**: `idx_{table}_{columns}` (e.g., `idx_tasks_tenant_started`)
- **Foreign Keys**: `fk_{table}_{ref_table}` (e.g., `fk_tasks_tenants`)
- **Primary Keys**: `{table}_pkey` (default)

---

## Entity Relationship Diagram

```
┌──────────────────┐
│     tenants      │
│                  │
│ • id (PK)        │
│ • name           │
│ • plan           │
│ • created_at     │
│ • settings       │
└────────┬─────────┘
         │
         │ 1:N
         │
    ┌────┴────────────────────────────────────┐
    │                                         │
    ▼                                         ▼
┌──────────────────┐                  ┌──────────────────┐
│  memberships     │                  │    api_keys      │
│                  │                  │                  │
│ • tenant_id (PK) │                  │ • id (PK)        │
│ • user_id (PK)   │                  │ • tenant_id (FK) │
│ • role           │                  │ • name           │
│ • created_at     │                  │ • hash           │
└────────┬─────────┘                  │ • scopes         │
         │                            │ • created_at     │
         │                            │ • last_used_at   │
         │                            │ • revoked        │
         │                            └──────────────────┘
         │
         │ N:1
         │
         ▼
┌──────────────────┐
│      users       │
│                  │
│ • id (PK)        │
│ • email (UNIQUE) │
│ • password_hash  │
│ • created_at     │
│ • updated_at     │
└──────────────────┘


┌──────────────────┐
│     tenants      │
└────────┬─────────┘
         │
         │ 1:N
         │
         ▼
┌──────────────────────────────────────────┐
│                  tasks                    │
│                                           │
│ • id (PK)                                 │
│ • tenant_id (FK)                          │
│ • created_by (FK → users.id)              │
│ • name                                    │
│ • adapter                                 │
│ • args (JSONB)                            │
│ • state (enum)                            │
│ • started_at                              │
│ • ended_at                                │
│ • cursor (last event seq for streaming)   │
│ • bytes_streamed                          │
│ • minutes_used                            │
│ • timeout_seconds                         │
│ • created_at                              │
└────────┬─────────────────────────────────┘
         │
         │ 1:N
         │
    ┌────┴──────────────────────┐
    │                           │
    ▼                           ▼
┌──────────────────┐    ┌──────────────────┐
│   task_events    │    │ task_snapshots   │
│                  │    │                  │
│ • task_id (PK)   │    │ • task_id (PK)   │
│ • seq (PK)       │    │ • seq (PK)       │
│ • ts             │    │ • ts             │
│ • kind           │    │ • summary (JSON) │
│ • payload (JSON) │    │ • stdout_bytes   │
│ • hash_prev      │    │ • stderr_bytes   │
│ • hash_curr      │    └──────────────────┘
└──────────────────┘
         │
         │ 1:N
         │
         ▼
┌──────────────────┐
│ task_heartbeats  │
│                  │
│ • task_id (PK)   │
│ • ts (PK)        │
│ • worker_id      │
└──────────────────┘


┌──────────────────┐
│     tenants      │
└────────┬─────────┘
         │
         │ 1:N
         │
    ┌────┴──────────────────────┐
    │                           │
    ▼                           ▼
┌──────────────────┐    ┌──────────────────┐
│    webhooks      │    │ usage_counters   │
│                  │    │                  │
│ • id (PK)        │    │ • tenant_id (PK) │
│ • tenant_id (FK) │    │ • period (PK)    │
│ • url            │    │ • task_minutes   │
│ • secret         │    │ • streams        │
│ • active         │    │ • bytes          │
│ • created_at     │    │ • tasks_created  │
└────────┬─────────┘    └──────────────────┘
         │
         │ 1:N
         │
         ▼
┌──────────────────┐
│webhook_deliveries│
│                  │
│ • id (PK)        │
│ • webhook_id (FK)│
│ • task_id (FK)   │
│ • event_type     │
│ • status         │
│ • sent_at        │
│ • response_code  │
│ • response_body  │
│ • signature      │
│ • retries        │
└──────────────────┘
```

---

## Tables

### 1. `tenants`

Multi-tenant isolation. Every user belongs to one or more tenants.

```sql
CREATE TABLE tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    plan VARCHAR(50) NOT NULL DEFAULT 'trial',
    stripe_customer_id VARCHAR(255),
    stripe_subscription_id VARCHAR(255),
    settings JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT tenants_plan_check CHECK (
        plan IN ('trial', 'entry', 'pro', 'enterprise')
    )
);

COMMENT ON TABLE tenants IS 'Organizations/accounts (multi-tenant isolation)';
COMMENT ON COLUMN tenants.plan IS 'Billing plan: trial, entry, pro, enterprise';
COMMENT ON COLUMN tenants.settings IS 'Tenant-specific configuration (JSON)';
COMMENT ON COLUMN tenants.stripe_customer_id IS 'Stripe customer ID (if billing enabled)';
```

**Sample Data**:
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "Acme Corp",
  "plan": "pro",
  "settings": {
    "quotas": {
      "concurrent_tasks": 100,
      "tasks_per_day": 100000
    },
    "retention_days": 30,
    "timezone": "America/New_York"
  }
}
```

---

### 2. `users`

Individual user accounts. Users can belong to multiple tenants via `memberships`.

```sql
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email CITEXT NOT NULL UNIQUE,
    email_verified BOOLEAN NOT NULL DEFAULT FALSE,
    password_hash VARCHAR(255) NOT NULL,
    name VARCHAR(255),
    avatar_url VARCHAR(512),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMPTZ
);

COMMENT ON TABLE users IS 'User accounts (can belong to multiple tenants)';
COMMENT ON COLUMN users.email IS 'Email address (case-insensitive via CITEXT)';
COMMENT ON COLUMN users.password_hash IS 'Argon2id hash of password';

CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_created_at ON users(created_at DESC);
```

**Notes**:
- `CITEXT` extension required for case-insensitive email
- `password_hash` stores Argon2id hash (never plaintext)
- Email verification flow optional (for production)

---

### 3. `memberships`

Join table for users ↔ tenants many-to-many relationship with roles.

```sql
CREATE TYPE membership_role AS ENUM ('owner', 'admin', 'member', 'viewer');

CREATE TABLE memberships (
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role membership_role NOT NULL DEFAULT 'member',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (tenant_id, user_id)
);

COMMENT ON TABLE memberships IS 'User-tenant relationships with roles (RBAC)';
COMMENT ON COLUMN memberships.role IS 'owner: full control, admin: manage users/keys, member: create tasks, viewer: read-only';

CREATE INDEX idx_memberships_user_id ON memberships(user_id);
```

**Roles**:
- `owner`: Full control, billing, delete tenant
- `admin`: Manage users, API keys, tasks
- `member`: Create and manage own tasks
- `viewer`: Read-only access

---

### 4. `api_keys`

API keys for programmatic access (alternative to JWT).

```sql
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    key_prefix VARCHAR(10) NOT NULL,
    key_hash VARCHAR(64) NOT NULL UNIQUE,
    scopes TEXT[] NOT NULL DEFAULT ARRAY['read:task', 'write:task'],
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ,
    revoked BOOLEAN NOT NULL DEFAULT FALSE,
    revoked_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ
);

COMMENT ON TABLE api_keys IS 'API keys for programmatic access';
COMMENT ON COLUMN api_keys.key_prefix IS 'First 10 chars of key (e.g., "axon_abc12") for display';
COMMENT ON COLUMN api_keys.key_hash IS 'SHA-256 hash of full key (never store plaintext)';
COMMENT ON COLUMN api_keys.scopes IS 'Array of permissions (e.g., ["read:task", "write:task", "admin"])';

CREATE INDEX idx_api_keys_tenant_id ON api_keys(tenant_id);
CREATE INDEX idx_api_keys_hash ON api_keys(key_hash) WHERE NOT revoked;
CREATE INDEX idx_api_keys_last_used ON api_keys(last_used_at DESC NULLS LAST);
```

**Key Format**: `axon_<32-byte-base62>` (e.g., `axon_abc123...xyz789`)

**Scopes**:
- `read:task`: Get task status, stream events
- `write:task`: Create, cancel tasks
- `read:webhook`: List webhooks
- `write:webhook`: Create, delete webhooks
- `admin`: Full access (manage users, billing)

---

### 5. `tasks`

Core table for all background tasks.

```sql
CREATE TYPE task_state AS ENUM (
    'pending',
    'running',
    'succeeded',
    'failed',
    'canceled',
    'timeout'
);

CREATE TABLE tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE SET NULL,
    name VARCHAR(255) NOT NULL,
    adapter VARCHAR(50) NOT NULL,
    args JSONB NOT NULL DEFAULT '{}',
    state task_state NOT NULL DEFAULT 'pending',
    started_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ,
    cursor BIGINT NOT NULL DEFAULT 0,
    bytes_streamed BIGINT NOT NULL DEFAULT 0,
    minutes_used INTEGER NOT NULL DEFAULT 0,
    timeout_seconds INTEGER NOT NULL DEFAULT 3600,
    error_message TEXT,
    exit_code INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT tasks_adapter_check CHECK (
        adapter IN ('mock', 'shell', 'docker', 'fly', 'supabase')
    ),
    CONSTRAINT tasks_timeout_check CHECK (timeout_seconds BETWEEN 1 AND 86400),
    CONSTRAINT tasks_ended_after_started CHECK (
        ended_at IS NULL OR started_at IS NULL OR ended_at >= started_at
    )
);

COMMENT ON TABLE tasks IS 'Background tasks executed by workers';
COMMENT ON COLUMN tasks.adapter IS 'Execution adapter: mock, shell, docker, fly, supabase';
COMMENT ON COLUMN tasks.args IS 'Adapter-specific arguments (JSON)';
COMMENT ON COLUMN tasks.cursor IS 'Last event seq number (for resumable streaming)';
COMMENT ON COLUMN tasks.bytes_streamed IS 'Total bytes sent via SSE (for usage tracking)';
COMMENT ON COLUMN tasks.minutes_used IS 'Task execution time in minutes (rounded up, for billing)';

CREATE INDEX idx_tasks_tenant_id ON tasks(tenant_id);
CREATE INDEX idx_tasks_tenant_state ON tasks(tenant_id, state);
CREATE INDEX idx_tasks_tenant_created ON tasks(tenant_id, created_at DESC);
CREATE INDEX idx_tasks_state_started ON tasks(state, started_at) WHERE state = 'running';
CREATE INDEX idx_tasks_created_by ON tasks(created_by);
```

**State Transitions**:
```
pending → running → succeeded
                 → failed
                 → timeout
pending → canceled
running → canceled
```

**Adapter Args Examples**:

**Shell**:
```json
{
  "command": "ls -la /tmp",
  "env": {"PATH": "/usr/bin:/bin"}
}
```

**Docker**:
```json
{
  "image": "node:18-alpine",
  "command": ["npm", "test"],
  "env": {"NODE_ENV": "test"}
}
```

**Fly**:
```json
{
  "app": "myapp",
  "region": "iad"
}
```

---

### 6. `task_events`

Append-only event log for task execution. Forms a hash chain for integrity.

```sql
CREATE TABLE task_events (
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    seq BIGINT NOT NULL,
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    kind VARCHAR(50) NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}',
    hash_prev BYTEA,
    hash_curr BYTEA NOT NULL,

    PRIMARY KEY (task_id, seq),

    CONSTRAINT task_events_kind_check CHECK (
        kind IN ('started', 'progress', 'stdout', 'stderr', 'success', 'error', 'canceled', 'timeout', 'digest')
    )
);

COMMENT ON TABLE task_events IS 'Append-only event log with hash chaining for integrity';
COMMENT ON COLUMN task_events.seq IS 'Monotonic sequence number within task (0, 1, 2, ...)';
COMMENT ON COLUMN task_events.kind IS 'Event type: started, progress, stdout, stderr, success, error, canceled, timeout, digest';
COMMENT ON COLUMN task_events.payload IS 'Event data (JSON, adapter-specific)';
COMMENT ON COLUMN task_events.hash_prev IS 'SHA-256 hash of previous event (NULL for seq=0)';
COMMENT ON COLUMN task_events.hash_curr IS 'SHA-256 hash of (hash_prev || payload)';

CREATE INDEX idx_task_events_task_id_seq ON task_events(task_id, seq);
CREATE INDEX idx_task_events_ts ON task_events(ts DESC);
```

**Event Payload Examples**:

**started**:
```json
{
  "adapter": "shell",
  "args": {"command": "ls -la"}
}
```

**progress**:
```json
{
  "message": "Processing file 5 of 10",
  "percent": 50
}
```

**stdout**:
```json
{
  "data": "total 48\ndrwxr-xr-x  6 user  staff  192 Jan  3 10:00 .\n"
}
```

**success**:
```json
{
  "exit_code": 0,
  "duration_ms": 5432
}
```

**error**:
```json
{
  "message": "Command failed with exit code 1",
  "exit_code": 1,
  "stderr": "Error: file not found"
}
```

---

### 7. `task_snapshots`

Compacted summaries of old task events to reduce storage and speed up backfill.

```sql
CREATE TABLE task_snapshots (
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    seq BIGINT NOT NULL,
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    summary JSONB NOT NULL,
    stdout_bytes BIGINT NOT NULL DEFAULT 0,
    stderr_bytes BIGINT NOT NULL DEFAULT 0,

    PRIMARY KEY (task_id, seq)
);

COMMENT ON TABLE task_snapshots IS 'Compacted event summaries (created by compaction job)';
COMMENT ON COLUMN task_snapshots.seq IS 'Sequence number this snapshot covers up to';
COMMENT ON COLUMN task_snapshots.summary IS 'Aggregated event data (e.g., total progress, key milestones)';

CREATE INDEX idx_task_snapshots_task_id ON task_snapshots(task_id);
```

**Snapshot Summary Example**:
```json
{
  "events_compacted": 1000,
  "seq_range": [0, 999],
  "state_transitions": [
    {"seq": 0, "kind": "started"},
    {"seq": 500, "kind": "progress", "percent": 50},
    {"seq": 999, "kind": "progress", "percent": 99}
  ],
  "stdout_lines": 850,
  "stderr_lines": 10,
  "errors": []
}
```

---

### 8. `task_heartbeats`

Worker heartbeats to detect failures and enable orphaned task reclamation.

```sql
CREATE TABLE task_heartbeats (
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    worker_id UUID NOT NULL,

    PRIMARY KEY (task_id, ts)
);

COMMENT ON TABLE task_heartbeats IS 'Worker heartbeats (every 30s) to detect failures';
COMMENT ON COLUMN task_heartbeats.worker_id IS 'Unique ID of the worker instance';

CREATE INDEX idx_task_heartbeats_task_id ON task_heartbeats(task_id, ts DESC);
```

**Usage**:
- Workers send heartbeat every 30 seconds while task is running
- Watchdog queries tasks with `state = 'running'` and checks last heartbeat
- If heartbeat > 90 seconds old → task is orphaned → re-queue

---

### 9. `webhooks`

Webhook configurations for event notifications.

```sql
CREATE TABLE webhooks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    url VARCHAR(2048) NOT NULL,
    secret BYTEA NOT NULL,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    events TEXT[] NOT NULL DEFAULT ARRAY['task.succeeded', 'task.failed'],
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT webhooks_url_check CHECK (url ~* '^https?://.*')
);

COMMENT ON TABLE webhooks IS 'Webhook endpoints for event notifications';
COMMENT ON COLUMN webhooks.secret IS 'HMAC secret for signature generation (encrypted)';
COMMENT ON COLUMN webhooks.events IS 'Event types to trigger webhook (e.g., task.succeeded, task.failed)';

CREATE INDEX idx_webhooks_tenant_id ON webhooks(tenant_id);
CREATE INDEX idx_webhooks_active ON webhooks(active) WHERE active = TRUE;
```

**Event Types**:
- `task.started`
- `task.succeeded`
- `task.failed`
- `task.canceled`
- `task.timeout`

---

### 10. `webhook_deliveries`

Log of all webhook delivery attempts.

```sql
CREATE TABLE webhook_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_id UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    event_type VARCHAR(50) NOT NULL,
    status INTEGER NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    response_code INTEGER,
    response_body TEXT,
    signature VARCHAR(128) NOT NULL,
    retries INTEGER NOT NULL DEFAULT 0,
    next_retry_at TIMESTAMPTZ
);

COMMENT ON TABLE webhook_deliveries IS 'Webhook delivery attempts and results';
COMMENT ON COLUMN webhook_deliveries.status IS 'HTTP status code (or 0 if request failed)';
COMMENT ON COLUMN webhook_deliveries.signature IS 'HMAC-SHA256 signature sent in header';
COMMENT ON COLUMN webhook_deliveries.retries IS 'Number of retry attempts (max 5)';

CREATE INDEX idx_webhook_deliveries_webhook_id ON webhook_deliveries(webhook_id);
CREATE INDEX idx_webhook_deliveries_task_id ON webhook_deliveries(task_id);
CREATE INDEX idx_webhook_deliveries_sent_at ON webhook_deliveries(sent_at DESC);
CREATE INDEX idx_webhook_deliveries_retry ON webhook_deliveries(next_retry_at)
    WHERE next_retry_at IS NOT NULL;
```

---

### 11. `usage_counters`

Track usage per tenant per period for billing and quota enforcement.

```sql
CREATE TABLE usage_counters (
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    period DATE NOT NULL,
    task_minutes INTEGER NOT NULL DEFAULT 0,
    streams INTEGER NOT NULL DEFAULT 0,
    bytes BIGINT NOT NULL DEFAULT 0,
    tasks_created INTEGER NOT NULL DEFAULT 0,

    PRIMARY KEY (tenant_id, period)
);

COMMENT ON TABLE usage_counters IS 'Usage tracking per tenant per day (for billing and quotas)';
COMMENT ON COLUMN usage_counters.period IS 'Date (YYYY-MM-DD) of usage period';
COMMENT ON COLUMN usage_counters.task_minutes IS 'Total task execution minutes (rounded up)';
COMMENT ON COLUMN usage_counters.streams IS 'Total SSE stream connections';
COMMENT ON COLUMN usage_counters.bytes IS 'Total bytes streamed via SSE';
COMMENT ON COLUMN usage_counters.tasks_created IS 'Total tasks created';

CREATE INDEX idx_usage_counters_tenant_period ON usage_counters(tenant_id, period DESC);
```

**Usage**:
- Incremented by workers after task completion
- Queried by API for quota enforcement
- Reported to Stripe for metered billing (if enabled)

---

## Indexes

### Primary Indexes (Auto-created)

All primary keys automatically create unique indexes:
- `tenants_pkey` on `tenants(id)`
- `users_pkey` on `users(id)`
- `memberships_pkey` on `memberships(tenant_id, user_id)`
- `api_keys_pkey` on `api_keys(id)`
- `tasks_pkey` on `tasks(id)`
- `task_events_pkey` on `task_events(task_id, seq)`
- `webhooks_pkey` on `webhooks(id)`
- `webhook_deliveries_pkey` on `webhook_deliveries(id)`
- `usage_counters_pkey` on `usage_counters(tenant_id, period)`

### Secondary Indexes

**For tenant isolation and multi-tenancy**:
```sql
CREATE INDEX idx_tasks_tenant_id ON tasks(tenant_id);
CREATE INDEX idx_api_keys_tenant_id ON api_keys(tenant_id);
CREATE INDEX idx_webhooks_tenant_id ON webhooks(tenant_id);
CREATE INDEX idx_memberships_user_id ON memberships(user_id);
```

**For common queries**:
```sql
-- List recent tasks for a tenant
CREATE INDEX idx_tasks_tenant_created ON tasks(tenant_id, created_at DESC);

-- List running tasks (for watchdog)
CREATE INDEX idx_tasks_state_started ON tasks(state, started_at) WHERE state = 'running';

-- Task events in order (for streaming)
CREATE INDEX idx_task_events_task_id_seq ON task_events(task_id, seq);

-- API key lookup (only non-revoked)
CREATE INDEX idx_api_keys_hash ON api_keys(key_hash) WHERE NOT revoked;

-- Webhook delivery retries
CREATE INDEX idx_webhook_deliveries_retry ON webhook_deliveries(next_retry_at)
    WHERE next_retry_at IS NOT NULL;
```

**For performance monitoring**:
```sql
-- Recent events for debugging
CREATE INDEX idx_task_events_ts ON task_events(ts DESC);

-- API key usage tracking
CREATE INDEX idx_api_keys_last_used ON api_keys(last_used_at DESC NULLS LAST);
```

---

## Constraints

### Foreign Key Constraints

All foreign keys use `ON DELETE CASCADE` or `ON DELETE SET NULL` for referential integrity:

```sql
-- Memberships cascade delete when tenant or user is deleted
ALTER TABLE memberships
    ADD CONSTRAINT fk_memberships_tenant
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;

ALTER TABLE memberships
    ADD CONSTRAINT fk_memberships_user
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

-- Tasks set created_by to NULL if user is deleted (keep task history)
ALTER TABLE tasks
    ADD CONSTRAINT fk_tasks_created_by
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL;

-- Task events cascade delete with task
ALTER TABLE task_events
    ADD CONSTRAINT fk_task_events_task
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE;
```

### Check Constraints

```sql
-- Ensure plan is valid
ALTER TABLE tenants
    ADD CONSTRAINT tenants_plan_check
    CHECK (plan IN ('trial', 'entry', 'pro', 'enterprise'));

-- Ensure task timeout is reasonable
ALTER TABLE tasks
    ADD CONSTRAINT tasks_timeout_check
    CHECK (timeout_seconds BETWEEN 1 AND 86400);

-- Ensure ended_at is after started_at
ALTER TABLE tasks
    ADD CONSTRAINT tasks_ended_after_started
    CHECK (ended_at IS NULL OR started_at IS NULL OR ended_at >= started_at);

-- Ensure webhook URL is valid
ALTER TABLE webhooks
    ADD CONSTRAINT webhooks_url_check
    CHECK (url ~* '^https?://.*');
```

### Unique Constraints

```sql
-- Email must be unique
ALTER TABLE users ADD CONSTRAINT users_email_unique UNIQUE (email);

-- API key hash must be unique
ALTER TABLE api_keys ADD CONSTRAINT api_keys_hash_unique UNIQUE (key_hash);
```

---

## Migrations

### Migration Strategy

We use **sqlx** for database migrations with the following principles:

1. **Forward-only**: No down migrations (database is append-only)
2. **Versioned**: Migrations are numbered sequentially (`YYYYMMDDHHMMSS_name.sql`)
3. **Idempotent**: Migrations should be safe to re-run
4. **Tested**: All migrations have rollback plan (manual if needed)

### Migration Files

```
migrations/
├── 00001_init_extensions.sql
├── 00002_create_tenants.sql
├── 00003_create_users.sql
├── 00004_create_memberships.sql
├── 00005_create_api_keys.sql
├── 00006_create_tasks.sql
├── 00007_create_task_events.sql
├── 00008_create_task_snapshots.sql
├── 00009_create_task_heartbeats.sql
├── 00010_create_webhooks.sql
├── 00011_create_webhook_deliveries.sql
├── 00012_create_usage_counters.sql
└── 00013_create_indexes.sql
```

### Running Migrations

```bash
# Create database
sqlx database create

# Run all pending migrations
sqlx migrate run

# Check migration status
sqlx migrate info

# Revert last migration (if needed)
sqlx migrate revert
```

---

## Query Patterns

### Common Queries

**1. List tasks for a tenant (with pagination)**:
```sql
SELECT id, name, state, created_at, started_at, ended_at
FROM tasks
WHERE tenant_id = $1
ORDER BY created_at DESC
LIMIT $2 OFFSET $3;
```

**2. Get task with tenant isolation**:
```sql
SELECT *
FROM tasks
WHERE id = $1 AND tenant_id = $2;
```

**3. Stream events from sequence (backfill)**:
```sql
SELECT seq, ts, kind, payload, hash_prev, hash_curr
FROM task_events
WHERE task_id = $1 AND seq >= $2
ORDER BY seq ASC
LIMIT 1000;
```

**4. Find running tasks with stale heartbeats (watchdog)**:
```sql
SELECT t.id, t.tenant_id, MAX(h.ts) as last_heartbeat
FROM tasks t
LEFT JOIN task_heartbeats h ON h.task_id = t.id
WHERE t.state = 'running'
GROUP BY t.id
HAVING MAX(h.ts) < NOW() - INTERVAL '90 seconds' OR MAX(h.ts) IS NULL;
```

**5. Get current usage for a tenant**:
```sql
SELECT
    COALESCE(SUM(task_minutes), 0) as total_minutes,
    COALESCE(SUM(tasks_created), 0) as total_tasks
FROM usage_counters
WHERE tenant_id = $1
  AND period >= DATE_TRUNC('month', CURRENT_DATE);
```

**6. Validate API key**:
```sql
SELECT ak.id, ak.tenant_id, ak.scopes, t.plan
FROM api_keys ak
JOIN tenants t ON t.id = ak.tenant_id
WHERE ak.key_hash = $1
  AND ak.revoked = FALSE
  AND (ak.expires_at IS NULL OR ak.expires_at > NOW());
```

### Optimization Tips

1. **Always include tenant_id**: Enables partition-pruning in future
2. **Use LIMIT**: Prevent accidental full table scans
3. **Index coverage**: Ensure queries use indexes (check with EXPLAIN ANALYZE)
4. **Batch inserts**: Use `INSERT INTO ... VALUES (...), (...)` for events
5. **Prepared statements**: Use sqlx `query!()` macro for compile-time checking

---

## Optimization

### Partitioning (Future)

For large-scale deployments, partition `task_events` by date:

```sql
CREATE TABLE task_events_2025_01 PARTITION OF task_events
    FOR VALUES FROM ('2025-01-01') TO ('2025-02-01');

CREATE TABLE task_events_2025_02 PARTITION OF task_events
    FOR VALUES FROM ('2025-02-01') TO ('2025-03-01');
```

**Benefits**:
- Faster queries (partition pruning)
- Easier archival (drop old partitions)
- Better index performance

### Archival Strategy

**Archive old task events after retention period**:

```sql
-- Move to archive table
INSERT INTO task_events_archive
SELECT * FROM task_events
WHERE ts < NOW() - INTERVAL '90 days';

-- Delete from main table
DELETE FROM task_events
WHERE ts < NOW() - INTERVAL '90 days';
```

**Or use partitioning and simply drop**:
```sql
DROP TABLE task_events_2024_01;
```

### Vacuum & Analyze

```sql
-- Regular maintenance (run weekly)
VACUUM ANALYZE tasks;
VACUUM ANALYZE task_events;

-- Full vacuum (run monthly, during low traffic)
VACUUM FULL task_events;
```

---

## Backup & Recovery

### Backup Strategy

**Daily Backups**:
```bash
# Full backup
pg_dump -h localhost -U axontask axontask > backup_$(date +%Y%m%d).sql

# Compressed
pg_dump -h localhost -U axontask axontask | gzip > backup_$(date +%Y%m%d).sql.gz

# Custom format (for pg_restore)
pg_dump -h localhost -U axontask -Fc axontask > backup_$(date +%Y%m%d).dump
```

**Point-in-Time Recovery (PITR)**:
- Enable WAL archiving in `postgresql.conf`
- Archive WAL files to S3/GCS
- Restore to any point in time

**Retention**:
- Daily backups: Keep 7 days
- Weekly backups: Keep 4 weeks
- Monthly backups: Keep 12 months

### Recovery

**From SQL backup**:
```bash
psql -h localhost -U axontask axontask < backup_20250103.sql
```

**From custom format**:
```bash
pg_restore -h localhost -U axontask -d axontask backup_20250103.dump
```

**Point-in-Time Recovery**:
```bash
# Restore base backup
pg_restore -d axontask base_backup.dump

# Replay WAL files up to specific timestamp
recovery_target_time = '2025-01-03 14:30:00'
```

---

## Data Retention & GDPR Compliance

### User Data Deletion

**Delete user and all associated data**:
```sql
-- 1. Delete user (cascades to memberships)
DELETE FROM users WHERE id = $1;

-- 2. Anonymize tasks created by user (already SET NULL via FK constraint)
-- No action needed

-- 3. Remove from audit logs (if applicable)
DELETE FROM audit_log WHERE user_id = $1;
```

### Task Data Retention

**Per plan**:
- Trial: 24 hours
- Entry: 7 days
- Pro: 30 days
- Enterprise: 90 days

**Automatic cleanup (run daily)**:
```sql
DELETE FROM task_events
WHERE task_id IN (
    SELECT id FROM tasks
    WHERE ended_at < NOW() - INTERVAL '7 days'
      AND tenant_id IN (
          SELECT id FROM tenants WHERE plan = 'entry'
      )
);
```

---

## Conclusion

This database design provides:
- ✅ **Complete schema** for all AxonTask features
- ✅ **Multi-tenancy** with proper isolation
- ✅ **Performance** with optimized indexes
- ✅ **Integrity** with hash-chained events
- ✅ **Scalability** with partitioning strategy
- ✅ **Compliance** with GDPR-ready deletion

**Next**: See [API_DESIGN.md](API_DESIGN.md) for how the API interacts with this schema.

---

**Document Version**: 1.0
**Last Updated**: November 04, 2025
**Maintained By**: Tyler Mailman (tyler@axonhub.io)
