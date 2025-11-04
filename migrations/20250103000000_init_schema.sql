-- AxonTask Initial Schema
-- Migration: 20250103000000_init_schema
-- Description: Complete database schema with 11 tables for multi-tenant task execution system
-- Author: Tyler Mailman
-- Date: 2025-01-03

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "citext";

-- ==============================================================================
-- ENUMS
-- ==============================================================================

-- Membership roles for RBAC
CREATE TYPE membership_role AS ENUM ('owner', 'admin', 'member', 'viewer');

-- Task execution states
CREATE TYPE task_state AS ENUM (
    'pending',
    'running',
    'succeeded',
    'failed',
    'canceled',
    'timeout'
);

-- ==============================================================================
-- TABLE 1: tenants
-- Multi-tenant isolation. Every user belongs to one or more tenants.
-- ==============================================================================

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

-- ==============================================================================
-- TABLE 2: users
-- Individual user accounts. Users can belong to multiple tenants via memberships.
-- ==============================================================================

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

-- ==============================================================================
-- TABLE 3: memberships
-- Join table for users ↔ tenants many-to-many relationship with roles.
-- ==============================================================================

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

-- ==============================================================================
-- TABLE 4: api_keys
-- API keys for programmatic access (alternative to JWT).
-- ==============================================================================

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

-- ==============================================================================
-- TABLE 5: tasks
-- Core table for all background tasks.
-- ==============================================================================

CREATE TABLE tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
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

-- ==============================================================================
-- TABLE 6: task_events
-- Append-only event log for task execution. Forms a hash chain for integrity.
-- ==============================================================================

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

-- ==============================================================================
-- TABLE 7: task_snapshots
-- Compacted summaries of old task events to reduce storage and speed up backfill.
-- ==============================================================================

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

-- ==============================================================================
-- TABLE 8: task_heartbeats
-- Worker heartbeats to detect failures and enable orphaned task reclamation.
-- ==============================================================================

CREATE TABLE task_heartbeats (
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    worker_id UUID NOT NULL,

    PRIMARY KEY (task_id, ts)
);

COMMENT ON TABLE task_heartbeats IS 'Worker heartbeats (every 30s) to detect failures';
COMMENT ON COLUMN task_heartbeats.worker_id IS 'Unique ID of the worker instance';

CREATE INDEX idx_task_heartbeats_task_id ON task_heartbeats(task_id, ts DESC);

-- ==============================================================================
-- TABLE 9: webhooks
-- Webhook configurations for event notifications.
-- ==============================================================================

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

-- ==============================================================================
-- TABLE 10: webhook_deliveries
-- Log of all webhook delivery attempts.
-- ==============================================================================

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

-- ==============================================================================
-- TABLE 11: usage_counters
-- Track usage per tenant per period for billing and quota enforcement.
-- ==============================================================================

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
