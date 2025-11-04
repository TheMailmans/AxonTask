-- AxonTask Initial Schema
-- Migration: 20250103000000_init_schema
-- Description: Complete database schema with 8 tables for multi-tenant task execution system
-- Author: Tyler Mailman
-- Date: 2025-01-03
--
-- This migration creates all necessary tables, enums, indexes, and constraints
-- based on the Rust model definitions in axontask-shared/src/models/

-- ==============================================================================
-- EXTENSIONS
-- ==============================================================================

-- Enable UUID generation functions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Enable case-insensitive text for emails
CREATE EXTENSION IF NOT EXISTS "citext";

-- ==============================================================================
-- ENUMS
-- ==============================================================================

-- Membership roles for RBAC (role-based access control)
-- Used in: memberships table
CREATE TYPE membership_role AS ENUM (
    'owner',   -- Full control: billing, delete tenant, manage all users
    'admin',   -- Can manage users, API keys, and all tasks
    'member',  -- Can create and manage own tasks
    'viewer'   -- Read-only access to tasks and data
);

-- Task execution states
-- Used in: tasks table
CREATE TYPE task_state AS ENUM (
    'pending',   -- Task is queued, waiting for a worker
    'running',   -- Task is currently being executed by a worker
    'succeeded', -- Task completed successfully
    'failed',    -- Task failed with an error
    'canceled',  -- Task was canceled by user or system
    'timeout'    -- Task exceeded timeout limit
);

-- ==============================================================================
-- TABLE 1: tenants
-- Multi-tenant isolation. Every user belongs to one or more tenants.
-- All resources (tasks, API keys, etc.) belong to a tenant.
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

    -- Ensure plan is one of the valid values
    CONSTRAINT tenants_plan_check CHECK (
        plan IN ('trial', 'entry', 'pro', 'enterprise')
    )
);

COMMENT ON TABLE tenants IS 'Organizations/accounts (multi-tenant isolation)';
COMMENT ON COLUMN tenants.id IS 'Unique tenant ID (UUID v4)';
COMMENT ON COLUMN tenants.name IS 'Organization/account name';
COMMENT ON COLUMN tenants.plan IS 'Billing plan: trial (7 days, limited), entry ($9.99/mo, 1000 tasks/day), pro ($29/mo, unlimited), enterprise (custom)';
COMMENT ON COLUMN tenants.stripe_customer_id IS 'Stripe customer ID (if billing enabled)';
COMMENT ON COLUMN tenants.stripe_subscription_id IS 'Stripe subscription ID (if billing enabled)';
COMMENT ON COLUMN tenants.settings IS 'Tenant-specific configuration (JSONB). Example: {"quotas": {"concurrent_tasks": 100}, "retention_days": 30}';

-- Index for listing tenants
CREATE INDEX idx_tenants_created_at ON tenants(created_at DESC);

-- Index for filtering by plan
CREATE INDEX idx_tenants_plan ON tenants(plan);

-- ==============================================================================
-- TABLE 2: users
-- Individual user accounts. Users can belong to multiple tenants via memberships.
-- Passwords are stored as Argon2id hashes, never in plaintext.
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

COMMENT ON TABLE users IS 'User accounts (can belong to multiple tenants via memberships)';
COMMENT ON COLUMN users.id IS 'Unique user ID (UUID v4)';
COMMENT ON COLUMN users.email IS 'Email address (case-insensitive via CITEXT). Must be unique across all users.';
COMMENT ON COLUMN users.email_verified IS 'Whether the email address has been verified. Set to true after email verification flow completes.';
COMMENT ON COLUMN users.password_hash IS 'Argon2id password hash. Never store plaintext passwords!';
COMMENT ON COLUMN users.name IS 'Optional display name';
COMMENT ON COLUMN users.avatar_url IS 'Optional avatar/profile picture URL';
COMMENT ON COLUMN users.last_login_at IS 'When the user last logged in (NULL if never logged in)';

-- Index for email lookups (case-insensitive)
CREATE INDEX idx_users_email ON users(email);

-- Index for listing users
CREATE INDEX idx_users_created_at ON users(created_at DESC);

-- ==============================================================================
-- TABLE 3: memberships
-- Join table for users ↔ tenants many-to-many relationship with roles (RBAC).
-- Implements role-based access control for tenant resources.
-- ==============================================================================

CREATE TABLE memberships (
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role membership_role NOT NULL DEFAULT 'member',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (tenant_id, user_id)
);

COMMENT ON TABLE memberships IS 'User-tenant relationships with roles (RBAC)';
COMMENT ON COLUMN memberships.tenant_id IS 'Tenant ID';
COMMENT ON COLUMN memberships.user_id IS 'User ID';
COMMENT ON COLUMN memberships.role IS 'Role within the tenant. owner: full control (billing, delete tenant, manage all users), admin: manage users/API keys/tasks, member: create and manage own tasks, viewer: read-only access';
COMMENT ON COLUMN memberships.created_at IS 'When the membership was created';

-- Index for listing all tenants a user belongs to
CREATE INDEX idx_memberships_user_id ON memberships(user_id);

-- Index for filtering by role
CREATE INDEX idx_memberships_role ON memberships(tenant_id, role);

-- ==============================================================================
-- TABLE 4: api_keys
-- API keys for programmatic access (alternative to JWT tokens).
-- Keys are stored as SHA-256 hashes, never plaintext.
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

COMMENT ON TABLE api_keys IS 'API keys for programmatic access (server-to-server communication)';
COMMENT ON COLUMN api_keys.id IS 'Unique API key ID';
COMMENT ON COLUMN api_keys.tenant_id IS 'Tenant this key belongs to';
COMMENT ON COLUMN api_keys.name IS 'Human-readable name for the key';
COMMENT ON COLUMN api_keys.key_prefix IS 'First 10 characters of the key for display (e.g., "axon_abc12...")';
COMMENT ON COLUMN api_keys.key_hash IS 'SHA-256 hash of the full key. Never store plaintext! Full key is only returned on creation.';
COMMENT ON COLUMN api_keys.scopes IS 'Permission scopes (e.g., ["read:task", "write:task", "admin"])';
COMMENT ON COLUMN api_keys.created_at IS 'When the key was created';
COMMENT ON COLUMN api_keys.last_used_at IS 'When the key was last used (updated on each validation)';
COMMENT ON COLUMN api_keys.revoked IS 'Whether the key has been revoked';
COMMENT ON COLUMN api_keys.revoked_at IS 'When the key was revoked (if applicable)';
COMMENT ON COLUMN api_keys.expires_at IS 'Optional expiration date';

-- Index for listing keys by tenant
CREATE INDEX idx_api_keys_tenant_id ON api_keys(tenant_id);

-- Index for key validation (only include non-revoked, non-expired keys)
CREATE INDEX idx_api_keys_hash ON api_keys(key_hash)
    WHERE revoked = FALSE;

-- Index for tracking usage
CREATE INDEX idx_api_keys_last_used ON api_keys(last_used_at DESC NULLS LAST);

-- ==============================================================================
-- TABLE 5: tasks
-- Core table for all background tasks. Tasks are the main entity of AxonTask.
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

    -- Ensure timeout is reasonable (1 second to 24 hours)
    CONSTRAINT tasks_timeout_check CHECK (timeout_seconds BETWEEN 1 AND 86400),

    -- Ensure ended_at is after started_at if both are set
    CONSTRAINT tasks_ended_after_started CHECK (
        ended_at IS NULL OR started_at IS NULL OR ended_at >= started_at
    )
);

COMMENT ON TABLE tasks IS 'Background tasks executed by workers';
COMMENT ON COLUMN tasks.id IS 'Unique task ID';
COMMENT ON COLUMN tasks.tenant_id IS 'Tenant this task belongs to';
COMMENT ON COLUMN tasks.created_by IS 'User who created the task (nullable if user deleted)';
COMMENT ON COLUMN tasks.name IS 'Human-readable task name';
COMMENT ON COLUMN tasks.adapter IS 'Adapter to execute the task (e.g., "shell", "docker", "fly")';
COMMENT ON COLUMN tasks.args IS 'Adapter-specific arguments (JSON)';
COMMENT ON COLUMN tasks.state IS 'Current execution state. State machine: pending → running → (succeeded|failed|timeout|canceled)';
COMMENT ON COLUMN tasks.started_at IS 'When the task started executing (NULL if not started)';
COMMENT ON COLUMN tasks.ended_at IS 'When the task finished (NULL if not finished)';
COMMENT ON COLUMN tasks.cursor IS 'Last event sequence number (for resumable streaming). Updated as events are processed.';
COMMENT ON COLUMN tasks.bytes_streamed IS 'Total bytes streamed via SSE (for usage tracking)';
COMMENT ON COLUMN tasks.minutes_used IS 'Task execution time in minutes (rounded up, for billing)';
COMMENT ON COLUMN tasks.timeout_seconds IS 'Timeout in seconds (default 3600 = 1 hour, max 86400 = 24 hours)';
COMMENT ON COLUMN tasks.error_message IS 'Error message (if state is "failed" or "timeout")';
COMMENT ON COLUMN tasks.exit_code IS 'Exit code (if applicable)';

-- Index for listing tasks by tenant
CREATE INDEX idx_tasks_tenant_id ON tasks(tenant_id, created_at DESC);

-- Index for filtering by state
CREATE INDEX idx_tasks_state ON tasks(state);

-- Composite index for tenant + state queries
CREATE INDEX idx_tasks_tenant_state ON tasks(tenant_id, state);

-- Index for finding running tasks (for timeout detection)
CREATE INDEX idx_tasks_running ON tasks(state, started_at)
    WHERE state = 'running';

-- Index for finding pending tasks (for worker queue)
CREATE INDEX idx_tasks_pending ON tasks(created_at)
    WHERE state = 'pending';

-- Index for user's tasks
CREATE INDEX idx_tasks_created_by ON tasks(created_by, created_at DESC)
    WHERE created_by IS NOT NULL;

-- ==============================================================================
-- TABLE 6: task_events
-- Append-only event log for task execution. Forms a hash chain for integrity.
-- Each event includes hashes that create a cryptographic audit trail.
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

    -- Ensure kind is a valid event type
    CONSTRAINT task_events_kind_check CHECK (
        kind IN ('started', 'progress', 'stdout', 'stderr', 'success', 'error', 'canceled', 'timeout', 'digest')
    ),

    -- Ensure sequence starts at 0
    CONSTRAINT task_events_seq_non_negative CHECK (seq >= 0),

    -- First event (seq=0) should not have a previous hash
    CONSTRAINT task_events_first_no_prev CHECK (
        (seq = 0 AND hash_prev IS NULL) OR (seq > 0)
    )
);

COMMENT ON TABLE task_events IS 'Append-only event log with hash chaining for integrity. Each event includes a hash that links to the previous event, creating a tamper-evident audit trail.';
COMMENT ON COLUMN task_events.task_id IS 'Task this event belongs to';
COMMENT ON COLUMN task_events.seq IS 'Sequence number (monotonic within task, starting at 0)';
COMMENT ON COLUMN task_events.ts IS 'Event timestamp';
COMMENT ON COLUMN task_events.kind IS 'Event type: started (execution started), progress (progress update), stdout (standard output), stderr (standard error), success (completed successfully), error (failed), canceled (canceled by user), timeout (exceeded timeout), digest (checkpoint for compaction)';
COMMENT ON COLUMN task_events.payload IS 'Event data (JSON, adapter-specific)';
COMMENT ON COLUMN task_events.hash_prev IS 'SHA-256 hash of previous event (NULL for seq=0). This links events into a chain.';
COMMENT ON COLUMN task_events.hash_curr IS 'SHA-256 hash of this event. Computed as: SHA256(hash_prev || seq || kind || payload)';

-- Index for querying event ranges (for SSE streaming)
CREATE INDEX idx_task_events_task_seq ON task_events(task_id, seq);

-- Index for time-based queries
CREATE INDEX idx_task_events_ts ON task_events(ts DESC);

-- Index for finding latest event per task
CREATE INDEX idx_task_events_latest ON task_events(task_id, seq DESC);

-- ==============================================================================
-- TABLE 7: webhooks
-- Webhook configurations for event notifications.
-- Webhooks allow tenants to receive real-time HTTP callbacks when events occur.
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

    -- Ensure URL is valid HTTP/HTTPS
    CONSTRAINT webhooks_url_check CHECK (url ~* '^https?://.*')
);

COMMENT ON TABLE webhooks IS 'Webhook endpoints for event notifications via HTTP callbacks';
COMMENT ON COLUMN webhooks.id IS 'Unique webhook ID';
COMMENT ON COLUMN webhooks.tenant_id IS 'Tenant this webhook belongs to';
COMMENT ON COLUMN webhooks.url IS 'Webhook URL (must be http:// or https://)';
COMMENT ON COLUMN webhooks.secret IS 'HMAC secret for signature generation (stored as bytes). Each delivery includes an HMAC-SHA256 signature sent in X-AxonTask-Signature header.';
COMMENT ON COLUMN webhooks.active IS 'Whether webhook is active';
COMMENT ON COLUMN webhooks.events IS 'Event types to trigger webhook. Examples: "task.started", "task.succeeded", "task.failed", "task.canceled", "task.timeout"';
COMMENT ON COLUMN webhooks.created_at IS 'When webhook was created';
COMMENT ON COLUMN webhooks.updated_at IS 'When webhook was last updated';

-- Index for listing webhooks by tenant
CREATE INDEX idx_webhooks_tenant_id ON webhooks(tenant_id);

-- Index for finding active webhooks
CREATE INDEX idx_webhooks_active ON webhooks(tenant_id, active)
    WHERE active = TRUE;

-- Index for event type lookups (using GIN for array containment)
CREATE INDEX idx_webhooks_events ON webhooks USING GIN(events);

-- ==============================================================================
-- TABLE 8: usage_counters
-- Track usage per tenant per period for billing and quota enforcement.
-- Counters are incremented as resources are consumed.
-- ==============================================================================

CREATE TABLE usage_counters (
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    period DATE NOT NULL,
    task_minutes INTEGER NOT NULL DEFAULT 0,
    streams INTEGER NOT NULL DEFAULT 0,
    bytes BIGINT NOT NULL DEFAULT 0,
    tasks_created INTEGER NOT NULL DEFAULT 0,

    PRIMARY KEY (tenant_id, period),

    -- Ensure counters are non-negative
    CONSTRAINT usage_counters_non_negative CHECK (
        task_minutes >= 0 AND
        streams >= 0 AND
        bytes >= 0 AND
        tasks_created >= 0
    )
);

COMMENT ON TABLE usage_counters IS 'Usage tracking per tenant per day (for billing, quota enforcement, and analytics)';
COMMENT ON COLUMN usage_counters.tenant_id IS 'Tenant ID';
COMMENT ON COLUMN usage_counters.period IS 'Usage period (date in YYYY-MM-DD format)';
COMMENT ON COLUMN usage_counters.task_minutes IS 'Total task execution minutes (rounded up, for billing)';
COMMENT ON COLUMN usage_counters.streams IS 'Total SSE stream connections';
COMMENT ON COLUMN usage_counters.bytes IS 'Total bytes streamed via SSE';
COMMENT ON COLUMN usage_counters.tasks_created IS 'Total tasks created';

-- Index for querying usage by tenant and period
CREATE INDEX idx_usage_counters_tenant_period ON usage_counters(tenant_id, period DESC);

-- Index for current period lookups
CREATE INDEX idx_usage_counters_current ON usage_counters(tenant_id, period)
    WHERE period = CURRENT_DATE;

-- ==============================================================================
-- SUMMARY
-- ==============================================================================
--
-- Tables created: 8
--   1. tenants         - Organizations/accounts (multi-tenant isolation)
--   2. users           - User accounts
--   3. memberships     - User-tenant relationships with roles (RBAC)
--   4. api_keys        - API keys for programmatic access
--   5. tasks           - Background tasks
--   6. task_events     - Append-only event log with hash chaining
--   7. webhooks        - Webhook configurations
--   8. usage_counters  - Usage tracking for billing
--
-- ENUMs created: 2
--   - membership_role (owner, admin, member, viewer)
--   - task_state (pending, running, succeeded, failed, canceled, timeout)
--
-- Indexes created: 29
--   - Performance indexes on tenant_id, foreign keys, common queries
--   - Partial indexes for filtered queries (active webhooks, running tasks, etc.)
--   - GIN index for array containment (webhook events)
--
-- Foreign key relationships:
--   - tenants (top-level, no dependencies)
--   - users (top-level, no dependencies)
--   - memberships → tenants, users (ON DELETE CASCADE)
--   - api_keys → tenants (ON DELETE CASCADE)
--   - tasks → tenants (ON DELETE CASCADE), users (ON DELETE SET NULL)
--   - task_events → tasks (ON DELETE CASCADE)
--   - webhooks → tenants (ON DELETE CASCADE)
--   - usage_counters → tenants (ON DELETE CASCADE)
--
