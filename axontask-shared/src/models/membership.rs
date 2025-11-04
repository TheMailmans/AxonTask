/// Membership model and database operations
///
/// This module provides the Membership model for user-tenant relationships with RBAC.
/// It implements a many-to-many relationship between users and tenants with role-based access control.
///
/// # Schema
///
/// ```sql
/// CREATE TYPE membership_role AS ENUM ('owner', 'admin', 'member', 'viewer');
///
/// CREATE TABLE memberships (
///     tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
///     user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
///     role membership_role NOT NULL DEFAULT 'member',
///     created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
///     PRIMARY KEY (tenant_id, user_id)
/// );
/// ```
///
/// # Roles
///
/// - **owner**: Full control, billing, delete tenant
/// - **admin**: Manage users, API keys, tasks
/// - **member**: Create and manage own tasks
/// - **viewer**: Read-only access
///
/// # Example
///
/// ```no_run
/// use axontask_shared::models::membership::{Membership, CreateMembership, MembershipRole};
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
/// use uuid::Uuid;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let pool = create_pool(DatabaseConfig::default()).await?;
///
/// let tenant_id = Uuid::new_v4();
/// let user_id = Uuid::new_v4();
///
/// // Add a user to a tenant as an admin
/// let membership = Membership::create(&pool, CreateMembership {
///     tenant_id,
///     user_id,
///     role: MembershipRole::Admin,
/// }).await?;
///
/// // Check if user has access
/// let has_access = Membership::has_access(&pool, tenant_id, user_id).await?;
/// # Ok(())
/// # }
/// ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// RBAC roles for tenant memberships
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "membership_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum MembershipRole {
    /// Full control: billing, delete tenant, manage all users
    Owner,

    /// Can manage users, API keys, and all tasks
    Admin,

    /// Can create and manage own tasks
    Member,

    /// Read-only access to tasks and data
    Viewer,
}

impl MembershipRole {
    /// Converts role to string for display
    pub fn as_str(&self) -> &'static str {
        match self {
            MembershipRole::Owner => "owner",
            MembershipRole::Admin => "admin",
            MembershipRole::Member => "member",
            MembershipRole::Viewer => "viewer",
        }
    }

    /// Checks if this role has permission to perform an action
    ///
    /// # Permission Hierarchy
    ///
    /// - Owner: Can do everything
    /// - Admin: Can do everything except billing and delete tenant
    /// - Member: Can create/read/update own tasks, read tenant info
    /// - Viewer: Read-only access
    pub fn can_manage_users(&self) -> bool {
        matches!(self, MembershipRole::Owner | MembershipRole::Admin)
    }

    /// Can manage API keys
    pub fn can_manage_api_keys(&self) -> bool {
        matches!(self, MembershipRole::Owner | MembershipRole::Admin)
    }

    /// Can manage billing
    pub fn can_manage_billing(&self) -> bool {
        matches!(self, MembershipRole::Owner)
    }

    /// Can delete tenant
    pub fn can_delete_tenant(&self) -> bool {
        matches!(self, MembershipRole::Owner)
    }

    /// Can create tasks
    pub fn can_create_tasks(&self) -> bool {
        !matches!(self, MembershipRole::Viewer)
    }

    /// Can view all tasks (not just own)
    pub fn can_view_all_tasks(&self) -> bool {
        matches!(self, MembershipRole::Owner | MembershipRole::Admin)
    }

    /// Checks if this role has permission level of the required role
    ///
    /// Hierarchy: Owner > Admin > Member > Viewer
    pub fn has_permission(&self, required: &MembershipRole) -> bool {
        let self_level = self.permission_level();
        let required_level = required.permission_level();
        self_level >= required_level
    }

    /// Returns numeric permission level for comparison
    fn permission_level(&self) -> u8 {
        match self {
            MembershipRole::Owner => 4,
            MembershipRole::Admin => 3,
            MembershipRole::Member => 2,
            MembershipRole::Viewer => 1,
        }
    }
}

/// Membership model representing user-tenant relationship with role
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Membership {
    /// Tenant ID
    pub tenant_id: Uuid,

    /// User ID
    pub user_id: Uuid,

    /// Role within the tenant
    pub role: MembershipRole,

    /// When the membership was created
    pub created_at: DateTime<Utc>,
}

/// Input for creating a new membership
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMembership {
    /// Tenant ID
    pub tenant_id: Uuid,

    /// User ID
    pub user_id: Uuid,

    /// Role to assign (defaults to Member)
    #[serde(default = "default_role")]
    pub role: MembershipRole,
}

fn default_role() -> MembershipRole {
    MembershipRole::Member
}

impl Membership {
    /// Creates a new membership (adds user to tenant)
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `data` - Membership creation data
    ///
    /// # Returns
    ///
    /// The newly created membership
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Membership already exists (unique constraint violation)
    /// - Tenant or user doesn't exist (foreign key violation)
    /// - Database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::membership::{Membership, CreateMembership, MembershipRole};
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
    /// let membership = Membership::create(&pool, CreateMembership {
    ///     tenant_id,
    ///     user_id,
    ///     role: MembershipRole::Member,
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create(pool: &PgPool, data: CreateMembership) -> Result<Self, sqlx::Error> {
        let membership = sqlx::query_as::<_, Membership>(
            r#"
            INSERT INTO memberships (tenant_id, user_id, role)
            VALUES ($1, $2, $3)
            RETURNING tenant_id, user_id, role, created_at
            "#,
        )
        .bind(data.tenant_id)
        .bind(data.user_id)
        .bind(data.role)
        .fetch_one(pool)
        .await?;

        Ok(membership)
    }

    /// Finds a specific membership by tenant and user
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `tenant_id` - Tenant ID
    /// * `user_id` - User ID
    ///
    /// # Returns
    ///
    /// The membership if found, None otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::membership::Membership;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
    /// if let Some(membership) = Membership::find(&pool, tenant_id, user_id).await? {
    ///     println!("User role: {:?}", membership.role);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn find(
        pool: &PgPool,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        let membership = sqlx::query_as::<_, Membership>(
            r#"
            SELECT tenant_id, user_id, role, created_at
            FROM memberships
            WHERE tenant_id = $1 AND user_id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

        Ok(membership)
    }

    /// Checks if a user has access to a tenant (any role)
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `tenant_id` - Tenant ID to check
    /// * `user_id` - User ID to check
    ///
    /// # Returns
    ///
    /// True if user is a member of the tenant, false otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::membership::Membership;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
    /// if Membership::has_access(&pool, tenant_id, user_id).await? {
    ///     println!("User has access to tenant");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn has_access(
        pool: &PgPool,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM memberships
                WHERE tenant_id = $1 AND user_id = $2
            )
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_one(pool)
        .await?;

        Ok(exists)
    }

    /// Gets user's role in a tenant
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `tenant_id` - Tenant ID
    /// * `user_id` - User ID
    ///
    /// # Returns
    ///
    /// The user's role if they are a member, None otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::membership::Membership;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
    /// if let Some(role) = Membership::get_role(&pool, tenant_id, user_id).await? {
    ///     println!("User role: {}", role.as_str());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_role(
        pool: &PgPool,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<MembershipRole>, sqlx::Error> {
        let role: Option<MembershipRole> = sqlx::query_scalar(
            r#"
            SELECT role FROM memberships
            WHERE tenant_id = $1 AND user_id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

        Ok(role)
    }

    /// Updates a user's role in a tenant
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `tenant_id` - Tenant ID
    /// * `user_id` - User ID
    /// * `role` - New role
    ///
    /// # Returns
    ///
    /// The updated membership if found, None if membership doesn't exist
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::membership::{Membership, MembershipRole};
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
    /// Membership::update_role(&pool, tenant_id, user_id, MembershipRole::Admin).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update_role(
        pool: &PgPool,
        tenant_id: Uuid,
        user_id: Uuid,
        role: MembershipRole,
    ) -> Result<Option<Self>, sqlx::Error> {
        let membership = sqlx::query_as::<_, Membership>(
            r#"
            UPDATE memberships
            SET role = $3
            WHERE tenant_id = $1 AND user_id = $2
            RETURNING tenant_id, user_id, role, created_at
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .fetch_optional(pool)
        .await?;

        Ok(membership)
    }

    /// Deletes a membership (removes user from tenant)
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `tenant_id` - Tenant ID
    /// * `user_id` - User ID
    ///
    /// # Returns
    ///
    /// True if membership was deleted, false if membership didn't exist
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::membership::Membership;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
    /// let removed = Membership::delete(&pool, tenant_id, user_id).await?;
    /// if removed {
    ///     println!("User removed from tenant");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete(pool: &PgPool, tenant_id: Uuid, user_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM memberships WHERE tenant_id = $1 AND user_id = $2"
        )
        .bind(tenant_id)
        .bind(user_id)
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Lists all members of a tenant
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `tenant_id` - Tenant ID
    ///
    /// # Returns
    ///
    /// Vector of memberships for the tenant
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::membership::Membership;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), sqlx::Error> {
    /// let members = Membership::list_by_tenant(&pool, tenant_id).await?;
    /// println!("Tenant has {} members", members.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_by_tenant(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        let memberships = sqlx::query_as::<_, Membership>(
            r#"
            SELECT tenant_id, user_id, role, created_at
            FROM memberships
            WHERE tenant_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        Ok(memberships)
    }

    /// Lists all tenants a user belongs to
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `user_id` - User ID
    ///
    /// # Returns
    ///
    /// Vector of memberships for the user
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::membership::Membership;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    /// let memberships = Membership::list_by_user(&pool, user_id).await?;
    /// println!("User belongs to {} tenants", memberships.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_by_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        let memberships = sqlx::query_as::<_, Membership>(
            r#"
            SELECT tenant_id, user_id, role, created_at
            FROM memberships
            WHERE user_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?;

        Ok(memberships)
    }

    /// Lists members by role within a tenant
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `tenant_id` - Tenant ID
    /// * `role` - Role to filter by
    ///
    /// # Returns
    ///
    /// Vector of memberships with the specified role
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::membership::{Membership, MembershipRole};
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), sqlx::Error> {
    /// let admins = Membership::list_by_role(&pool, tenant_id, MembershipRole::Admin).await?;
    /// println!("Tenant has {} admins", admins.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_by_role(
        pool: &PgPool,
        tenant_id: Uuid,
        role: MembershipRole,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let memberships = sqlx::query_as::<_, Membership>(
            r#"
            SELECT tenant_id, user_id, role, created_at
            FROM memberships
            WHERE tenant_id = $1 AND role = $2
            ORDER BY created_at ASC
            "#,
        )
        .bind(tenant_id)
        .bind(role)
        .fetch_all(pool)
        .await?;

        Ok(memberships)
    }

    /// Counts members in a tenant
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `tenant_id` - Tenant ID
    ///
    /// # Returns
    ///
    /// Number of members in the tenant
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::membership::Membership;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), sqlx::Error> {
    /// let count = Membership::count_by_tenant(&pool, tenant_id).await?;
    /// println!("Tenant has {} members", count);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn count_by_tenant(pool: &PgPool, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM memberships WHERE tenant_id = $1"
        )
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_membership_role_as_str() {
        assert_eq!(MembershipRole::Owner.as_str(), "owner");
        assert_eq!(MembershipRole::Admin.as_str(), "admin");
        assert_eq!(MembershipRole::Member.as_str(), "member");
        assert_eq!(MembershipRole::Viewer.as_str(), "viewer");
    }

    #[test]
    fn test_role_permissions() {
        // Owner can do everything
        assert!(MembershipRole::Owner.can_manage_users());
        assert!(MembershipRole::Owner.can_manage_api_keys());
        assert!(MembershipRole::Owner.can_manage_billing());
        assert!(MembershipRole::Owner.can_delete_tenant());
        assert!(MembershipRole::Owner.can_create_tasks());
        assert!(MembershipRole::Owner.can_view_all_tasks());

        // Admin can do most things except billing and delete
        assert!(MembershipRole::Admin.can_manage_users());
        assert!(MembershipRole::Admin.can_manage_api_keys());
        assert!(!MembershipRole::Admin.can_manage_billing());
        assert!(!MembershipRole::Admin.can_delete_tenant());
        assert!(MembershipRole::Admin.can_create_tasks());
        assert!(MembershipRole::Admin.can_view_all_tasks());

        // Member can create tasks but not manage users
        assert!(!MembershipRole::Member.can_manage_users());
        assert!(!MembershipRole::Member.can_manage_api_keys());
        assert!(!MembershipRole::Member.can_manage_billing());
        assert!(!MembershipRole::Member.can_delete_tenant());
        assert!(MembershipRole::Member.can_create_tasks());
        assert!(!MembershipRole::Member.can_view_all_tasks());

        // Viewer can only read
        assert!(!MembershipRole::Viewer.can_manage_users());
        assert!(!MembershipRole::Viewer.can_manage_api_keys());
        assert!(!MembershipRole::Viewer.can_manage_billing());
        assert!(!MembershipRole::Viewer.can_delete_tenant());
        assert!(!MembershipRole::Viewer.can_create_tasks());
        assert!(!MembershipRole::Viewer.can_view_all_tasks());
    }

    #[test]
    fn test_create_membership_default_role() {
        assert_eq!(default_role(), MembershipRole::Member);
    }

    // Integration tests for database operations are in tests/models/membership_tests.rs
}
