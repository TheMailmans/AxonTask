-- AxonTask Initial Schema Rollback
-- Migration: 20250103000000_init_schema (DOWN)
-- Description: Drop all tables and types in reverse dependency order
-- Author: Tyler Mailman
-- Date: 2025-01-03

-- ==============================================================================
-- DROP TABLES (reverse dependency order)
-- ==============================================================================

DROP TABLE IF EXISTS usage_counters CASCADE;
DROP TABLE IF EXISTS webhook_deliveries CASCADE;
DROP TABLE IF EXISTS webhooks CASCADE;
DROP TABLE IF EXISTS task_heartbeats CASCADE;
DROP TABLE IF EXISTS task_snapshots CASCADE;
DROP TABLE IF EXISTS task_events CASCADE;
DROP TABLE IF EXISTS tasks CASCADE;
DROP TABLE IF EXISTS api_keys CASCADE;
DROP TABLE IF EXISTS memberships CASCADE;
DROP TABLE IF EXISTS users CASCADE;
DROP TABLE IF EXISTS tenants CASCADE;

-- ==============================================================================
-- DROP TYPES
-- ==============================================================================

DROP TYPE IF EXISTS task_state CASCADE;
DROP TYPE IF EXISTS membership_role CASCADE;

-- Note: We intentionally do NOT drop extensions (uuid-ossp, citext)
-- as they may be used by other schemas or databases
