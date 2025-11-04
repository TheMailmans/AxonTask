-- AxonTask Initial Schema Rollback
-- Migration: 20250103000000_init_schema (DOWN)
-- Description: Drop all tables and types in reverse dependency order
-- Author: Tyler Mailman
-- Date: 2025-01-03
--
-- This migration reverses the initial schema creation.
-- Tables are dropped in reverse dependency order to avoid foreign key violations.

-- ==============================================================================
-- DROP TABLES (in reverse dependency order)
-- ==============================================================================

-- Drop child tables first (those with foreign keys)
DROP TABLE IF EXISTS usage_counters CASCADE;
DROP TABLE IF EXISTS webhooks CASCADE;
DROP TABLE IF EXISTS task_events CASCADE;
DROP TABLE IF EXISTS tasks CASCADE;
DROP TABLE IF EXISTS api_keys CASCADE;
DROP TABLE IF EXISTS memberships CASCADE;

-- Drop parent tables (no foreign keys referencing them)
DROP TABLE IF EXISTS users CASCADE;
DROP TABLE IF EXISTS tenants CASCADE;

-- ==============================================================================
-- DROP ENUMS
-- ==============================================================================

DROP TYPE IF EXISTS task_state CASCADE;
DROP TYPE IF EXISTS membership_role CASCADE;

-- ==============================================================================
-- EXTENSIONS
-- ==============================================================================

-- Note: We intentionally do NOT drop extensions (uuid-ossp, citext)
-- as they may be used by other schemas or databases.
-- If you need to drop them manually, run:
--   DROP EXTENSION IF EXISTS "citext" CASCADE;
--   DROP EXTENSION IF EXISTS "uuid-ossp" CASCADE;

-- ==============================================================================
-- SUMMARY
-- ==============================================================================
--
-- Dropped 8 tables:
--   1. usage_counters
--   2. webhooks
--   3. task_events
--   4. tasks
--   5. api_keys
--   6. memberships
--   7. users
--   8. tenants
--
-- Dropped 2 ENUMs:
--   1. task_state
--   2. membership_role
--
-- All indexes, constraints, and comments are automatically dropped with CASCADE.
--
