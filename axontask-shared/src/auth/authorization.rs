/// Authorization helpers and permission checks
///
/// This module provides utilities for role-based access control (RBAC) and
/// resource-level authorization in AxonTask.
///
/// # Permission Model
///
/// AxonTask uses a hierarchical permission model:
///
/// 1. **Tenant Membership**: User must be a member of the tenant
/// 2. **Role-Based Permissions**: Defined by MembershipRole (Owner, Admin, Member, Viewer)
/// 3. **Resource-Level Permissions**: Additional checks for specific resources
/// 4. **Scope-Based Permissions**: For API keys with limited scopes
///
/// # Example
///
/// ```no_run
/// use axontask_shared::auth::authorization::{require_role, require_scope, ResourcePermission};
/// use axontask_shared::auth::middleware::AuthContext;
/// use axontask_shared::models::membership::MembershipRole;
/// use sqlx::PgPool;
/// use uuid::Uuid;
///
/// async fn check_permissions(
///     pool: &PgPool,
///     auth: &AuthContext,
///     task_id: Uuid,
/// ) -> Result<(), String> {
///     // Check user has admin role in tenant
///     require_role(pool, auth.tenant_id, auth.user_id, MembershipRole::Admin).await?;
///
///     // Check user can write tasks (for API keys)
///     require_scope(auth, "tasks:write")?;
///
///     Ok(())
/// }
/// ```

use sqlx::PgPool;
use uuid::Uuid;

use super::middleware::AuthContext;
use crate::models::membership::{Membership, MembershipRole};

/// Error type for authorization checks
#[derive(Debug, thiserror::Error)]
pub enum AuthzError {
    /// User is not a member of the tenant
    #[error("Not a member of tenant {0}")]
    NotMember(Uuid),

    /// User doesn't have required role
    #[error("Insufficient permissions: requires {required:?}, has {actual:?}")]
    InsufficientRole {
        required: MembershipRole,
        actual: MembershipRole,
    },

    /// User doesn't have required scope (for API keys)
    #[error("Missing required scope: {0}")]
    MissingScope(String),

    /// User doesn't own the resource
    #[error("Not authorized to access this resource")]
    NotAuthorized,

    /// Database error
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
}

/// Permission types for authorization checks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourcePermission {
    /// Read permission (Viewer+)
    Read,

    /// Write permission (Member+)
    Write,

    /// Manage permission (Admin+)
    Manage,

    /// Owner permission (Owner only)
    Own,
}

impl ResourcePermission {
    /// Gets the minimum role required for this permission
    pub fn min_role(&self) -> MembershipRole {
        match self {
            ResourcePermission::Read => MembershipRole::Viewer,
            ResourcePermission::Write => MembershipRole::Member,
            ResourcePermission::Manage => MembershipRole::Admin,
            ResourcePermission::Own => MembershipRole::Owner,
        }
    }
}

/// Checks if a user is a member of a tenant
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `tenant_id` - Tenant ID
/// * `user_id` - User ID
///
/// # Returns
///
/// `Ok(())` if user is a member, error otherwise
///
/// # Errors
///
/// Returns `AuthzError::NotMember` if user is not a member
///
/// # Example
///
/// ```no_run
/// # use axontask_shared::auth::authorization::require_membership;
/// # use sqlx::PgPool;
/// # use uuid::Uuid;
/// # async fn example(pool: PgPool, tenant_id: Uuid, user_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
/// require_membership(&pool, tenant_id, user_id).await?;
/// # Ok(())
/// # }
/// ```
pub async fn require_membership(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), AuthzError> {
    let has_access = Membership::has_access(pool, tenant_id, user_id).await?;

    if !has_access {
        return Err(AuthzError::NotMember(tenant_id));
    }

    Ok(())
}

/// Checks if a user has a specific role in a tenant
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `tenant_id` - Tenant ID
/// * `user_id` - User ID
/// * `required_role` - Minimum required role
///
/// # Returns
///
/// `Ok(())` if user has the required role or higher, error otherwise
///
/// # Errors
///
/// Returns error if:
/// - User is not a member
/// - User's role is insufficient
///
/// # Example
///
/// ```no_run
/// # use axontask_shared::auth::authorization::require_role;
/// # use axontask_shared::models::membership::MembershipRole;
/// # use sqlx::PgPool;
/// # use uuid::Uuid;
/// # async fn example(pool: PgPool, tenant_id: Uuid, user_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
/// // Require admin or higher
/// require_role(&pool, tenant_id, user_id, MembershipRole::Admin).await?;
/// # Ok(())
/// # }
/// ```
pub async fn require_role(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    required_role: MembershipRole,
) -> Result<(), AuthzError> {
    let user_role = Membership::get_role(pool, tenant_id, user_id)
        .await?
        .ok_or(AuthzError::NotMember(tenant_id))?;

    if !user_role.has_permission(&required_role) {
        return Err(AuthzError::InsufficientRole {
            required: required_role,
            actual: user_role,
        });
    }

    Ok(())
}

/// Checks if auth context has a required scope
///
/// For JWT authentication, this always passes (full access).
/// For API key authentication, checks the scopes list.
///
/// # Arguments
///
/// * `auth` - Authentication context
/// * `required_scope` - Required scope (e.g., "tasks:write")
///
/// # Returns
///
/// `Ok(())` if scope is present, error otherwise
///
/// # Example
///
/// ```no_run
/// # use axontask_shared::auth::authorization::require_scope;
/// # use axontask_shared::auth::middleware::AuthContext;
/// # use uuid::Uuid;
/// # fn example(auth: AuthContext) -> Result<(), Box<dyn std::error::Error>> {
/// require_scope(&auth, "tasks:write")?;
/// # Ok(())
/// # }
/// ```
pub fn require_scope(auth: &AuthContext, required_scope: &str) -> Result<(), AuthzError> {
    if !auth.has_scope(required_scope) {
        return Err(AuthzError::MissingScope(required_scope.to_string()));
    }

    Ok(())
}

/// Checks if auth context has permission for a resource
///
/// Combines role and scope checks:
/// 1. Verifies user has required role in tenant
/// 2. Verifies scope (for API keys)
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `auth` - Authentication context
/// * `permission` - Required permission level
/// * `scope` - Required scope (for API keys)
///
/// # Returns
///
/// `Ok(())` if authorized, error otherwise
///
/// # Example
///
/// ```no_run
/// # use axontask_shared::auth::authorization::{require_permission, ResourcePermission};
/// # use axontask_shared::auth::middleware::AuthContext;
/// # use sqlx::PgPool;
/// # async fn example(pool: PgPool, auth: AuthContext) -> Result<(), Box<dyn std::error::Error>> {
/// require_permission(&pool, &auth, ResourcePermission::Write, "tasks:write").await?;
/// # Ok(())
/// # }
/// ```
pub async fn require_permission(
    pool: &PgPool,
    auth: &AuthContext,
    permission: ResourcePermission,
    scope: &str,
) -> Result<(), AuthzError> {
    // Check role
    require_role(pool, auth.tenant_id, auth.user_id, permission.min_role()).await?;

    // Check scope (for API keys)
    require_scope(auth, scope)?;

    Ok(())
}

/// Checks if user owns a resource
///
/// Verifies that the resource's owner_id matches the authenticated user.
///
/// # Arguments
///
/// * `auth` - Authentication context
/// * `resource_owner_id` - Owner ID of the resource
///
/// # Returns
///
/// `Ok(())` if user owns the resource, error otherwise
///
/// # Example
///
/// ```no_run
/// # use axontask_shared::auth::authorization::require_ownership;
/// # use axontask_shared::auth::middleware::AuthContext;
/// # use uuid::Uuid;
/// # fn example(auth: AuthContext, task_owner_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
/// require_ownership(&auth, task_owner_id)?;
/// # Ok(())
/// # }
/// ```
pub fn require_ownership(auth: &AuthContext, resource_owner_id: Uuid) -> Result<(), AuthzError> {
    if auth.user_id != resource_owner_id {
        return Err(AuthzError::NotAuthorized);
    }

    Ok(())
}

/// Checks if user can access a resource
///
/// Allows access if:
/// - User owns the resource, OR
/// - User has the required permission level
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `auth` - Authentication context
/// * `resource_owner_id` - Owner ID of the resource
/// * `permission` - Required permission level (if not owner)
/// * `scope` - Required scope (for API keys)
///
/// # Returns
///
/// `Ok(())` if authorized, error otherwise
///
/// # Example
///
/// ```no_run
/// # use axontask_shared::auth::authorization::{require_access, ResourcePermission};
/// # use axontask_shared::auth::middleware::AuthContext;
/// # use sqlx::PgPool;
/// # use uuid::Uuid;
/// # async fn example(pool: PgPool, auth: AuthContext, task_owner_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
/// // User can access if they own it OR have Write permission
/// require_access(&pool, &auth, task_owner_id, ResourcePermission::Write, "tasks:read").await?;
/// # Ok(())
/// # }
/// ```
pub async fn require_access(
    pool: &PgPool,
    auth: &AuthContext,
    resource_owner_id: Uuid,
    permission: ResourcePermission,
    scope: &str,
) -> Result<(), AuthzError> {
    // Owner always has access
    if auth.user_id == resource_owner_id {
        require_scope(auth, scope)?;
        return Ok(());
    }

    // Otherwise check permission
    require_permission(pool, auth, permission, scope).await
}

/// Checks if user can manage billing
///
/// Only owners can manage billing (change plans, update payment methods, etc.)
///
/// # Example
///
/// ```no_run
/// # use axontask_shared::auth::authorization::require_billing_access;
/// # use axontask_shared::auth::middleware::AuthContext;
/// # use sqlx::PgPool;
/// # async fn example(pool: PgPool, auth: AuthContext) -> Result<(), Box<dyn std::error::Error>> {
/// require_billing_access(&pool, &auth).await?;
/// # Ok(())
/// # }
/// ```
pub async fn require_billing_access(pool: &PgPool, auth: &AuthContext) -> Result<(), AuthzError> {
    require_role(pool, auth.tenant_id, auth.user_id, MembershipRole::Owner).await
}

/// Checks if user can manage users
///
/// Admins and owners can manage users (invite, remove, change roles).
///
/// # Example
///
/// ```no_run
/// # use axontask_shared::auth::authorization::require_user_management;
/// # use axontask_shared::auth::middleware::AuthContext;
/// # use sqlx::PgPool;
/// # async fn example(pool: PgPool, auth: AuthContext) -> Result<(), Box<dyn std::error::Error>> {
/// require_user_management(&pool, &auth).await?;
/// # Ok(())
/// # }
/// ```
pub async fn require_user_management(pool: &PgPool, auth: &AuthContext) -> Result<(), AuthzError> {
    require_role(pool, auth.tenant_id, auth.user_id, MembershipRole::Admin).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_permission_min_role() {
        assert_eq!(ResourcePermission::Read.min_role(), MembershipRole::Viewer);
        assert_eq!(ResourcePermission::Write.min_role(), MembershipRole::Member);
        assert_eq!(ResourcePermission::Manage.min_role(), MembershipRole::Admin);
        assert_eq!(ResourcePermission::Own.min_role(), MembershipRole::Owner);
    }

    #[test]
    fn test_require_scope() {
        let mut auth = AuthContext::from_jwt(Uuid::new_v4(), Uuid::new_v4());

        // JWT always has scope
        assert!(require_scope(&auth, "tasks:read").is_ok());
        assert!(require_scope(&auth, "anything").is_ok());

        // API key with scopes
        auth.scopes = Some(vec!["tasks:read".to_string(), "tasks:write".to_string()]);
        auth.method = super::super::middleware::AuthMethod::ApiKey;

        assert!(require_scope(&auth, "tasks:read").is_ok());
        assert!(require_scope(&auth, "tasks:write").is_ok());
        assert!(require_scope(&auth, "tasks:delete").is_err());
    }

    #[test]
    fn test_require_ownership() {
        let user_id = Uuid::new_v4();
        let auth = AuthContext::from_jwt(user_id, Uuid::new_v4());

        // Same user
        assert!(require_ownership(&auth, user_id).is_ok());

        // Different user
        assert!(require_ownership(&auth, Uuid::new_v4()).is_err());
    }

    #[test]
    fn test_authz_error_display() {
        let err = AuthzError::NotMember(Uuid::new_v4());
        assert!(err.to_string().contains("Not a member"));

        let err = AuthzError::MissingScope("tasks:write".to_string());
        assert!(err.to_string().contains("tasks:write"));

        let err = AuthzError::NotAuthorized;
        assert!(err.to_string().contains("Not authorized"));
    }
}
