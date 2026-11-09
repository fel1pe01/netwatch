use axum::http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// User roles for RBAC
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Administrator,
    Operator,
    Viewer,
}

impl Role {
    /// Check if this role has at least the required role level
    pub fn has_permission(&self, required: Role) -> bool {
        matches!(
            (self, required),
            (Role::Administrator, _)
                | (Role::Operator, Role::Operator)
                | (Role::Operator, Role::Viewer)
                | (Role::Viewer, Role::Viewer)
        )
    }

    /// Get all permissions for this role
    pub fn permissions(&self) -> Vec<Permission> {
        match self {
            Role::Administrator => vec![
                Permission::Read,
                Permission::Write,
                Permission::Configure,
                Permission::Admin,
            ],
            Role::Operator => vec![Permission::Read, Permission::Write],
            Role::Viewer => vec![Permission::Read],
        }
    }
}

/// Permission levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    Read,
    Write,
    Configure,
    Admin,
}

/// Authenticated user context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub username: String,
    pub role: Role,
    pub permissions: Vec<Permission>,
}

impl AuthUser {
    pub fn new(username: String, role: Role) -> Self {
        let permissions = role.permissions();
        Self {
            username,
            role,
            permissions,
        }
    }

    pub fn has_permission(&self, perm: Permission) -> bool {
        self.permissions.contains(&perm)
    }

    pub fn can_access(&self, required_role: Role) -> bool {
        self.role.has_permission(required_role)
    }
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Enable authentication (default: true in production)
    pub enabled: bool,
    /// API tokens mapped to users (in production, use secure storage)
    #[serde(default)]
    pub tokens: HashMap<String, AuthUser>,
    /// Paths that don't require authentication
    #[serde(default = "default_public_paths")]
    pub public_paths: Vec<String>,
    /// Token header name (default: Authorization)
    #[serde(default = "default_token_header")]
    pub token_header: String,
}

fn default_public_paths() -> Vec<String> {
    vec!["/health".to_string(), "/metrics".to_string()]
}

fn default_token_header() -> String {
    "Authorization".to_string()
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tokens: HashMap::new(),
            public_paths: default_public_paths(),
            token_header: default_token_header(),
        }
    }
}

/// Authentication state shared across the application
#[derive(Clone)]
pub struct AuthState {
    config: Arc<RwLock<AuthConfig>>,
}

impl AuthState {
    pub fn new(config: AuthConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Validate a token and return the associated user
    pub async fn validate_token(&self, token: &str) -> Option<AuthUser> {
        let config = self.config.read().await;
        config.tokens.get(token).cloned()
    }

    /// Check if a path is public (doesn't require auth)
    pub async fn is_public_path(&self, path: &str) -> bool {
        let config = self.config.read().await;
        config.public_paths.iter().any(|p| path.starts_with(p))
    }

    /// Check if auth is enabled
    pub async fn is_enabled(&self) -> bool {
        let config = self.config.read().await;
        config.enabled
    }

    /// Get the token header name
    pub async fn token_header(&self) -> String {
        let config = self.config.read().await;
        config.token_header.clone()
    }

    /// Add or update a token
    pub async fn set_token(&self, token: String, user: AuthUser) {
        let mut config = self.config.write().await;
        config.tokens.insert(token, user);
    }

    /// Remove a token
    pub async fn remove_token(&self, token: &str) {
        let mut config = self.config.write().await;
        config.tokens.remove(token);
    }
}

/// Extract bearer token from Authorization header
pub fn extract_bearer_token(headers: &HeaderMap, header_name: &str) -> Option<String> {
    let auth_header = headers.get(header_name)?;
    let auth_str = auth_header.to_str().ok()?;
    auth_str.strip_prefix("Bearer ").map(|s| s.to_string())
}

/// Authentication error responses
#[derive(Debug, Serialize)]
pub struct AuthError {
    pub error: String,
    pub message: String,
}

impl AuthError {
    pub fn unauthorized(message: impl Into<String>) -> (StatusCode, axum::Json<Self>) {
        (
            StatusCode::UNAUTHORIZED,
            axum::Json(Self {
                error: "unauthorized".to_string(),
                message: message.into(),
            }),
        )
    }

    pub fn forbidden(message: impl Into<String>) -> (StatusCode, axum::Json<Self>) {
        (
            StatusCode::FORBIDDEN,
            axum::Json(Self {
                error: "forbidden".to_string(),
                message: message.into(),
            }),
        )
    }
}

/// Token generation utilities
pub mod token {
    use rand::Rng;

    /// Generate a secure random token
    pub fn generate_token(length: usize) -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = rand::thread_rng();
        (0..length)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    /// Generate a token with prefix for identification
    pub fn generate_prefixed_token(prefix: &str, length: usize) -> String {
        format!("{}_{}", prefix, generate_token(length))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_permissions() {
        assert!(Role::Administrator.has_permission(Role::Viewer));
        assert!(Role::Administrator.has_permission(Role::Operator));
        assert!(Role::Administrator.has_permission(Role::Administrator));
        assert!(Role::Operator.has_permission(Role::Viewer));
        assert!(Role::Operator.has_permission(Role::Operator));
        assert!(!Role::Operator.has_permission(Role::Administrator));
        assert!(Role::Viewer.has_permission(Role::Viewer));
        assert!(!Role::Viewer.has_permission(Role::Operator));
        assert!(!Role::Viewer.has_permission(Role::Administrator));
    }

    #[test]
    fn test_auth_user_permissions() {
        let admin = AuthUser::new("admin".to_string(), Role::Administrator);
        let operator = AuthUser::new("operator".to_string(), Role::Operator);
        let viewer = AuthUser::new("viewer".to_string(), Role::Viewer);

        assert!(admin.has_permission(Permission::Admin));
        assert!(admin.has_permission(Permission::Configure));
        assert!(admin.has_permission(Permission::Write));
        assert!(admin.has_permission(Permission::Read));

        assert!(operator.has_permission(Permission::Write));
        assert!(operator.has_permission(Permission::Read));
        assert!(!operator.has_permission(Permission::Configure));
        assert!(!operator.has_permission(Permission::Admin));

        assert!(viewer.has_permission(Permission::Read));
        assert!(!viewer.has_permission(Permission::Write));
        assert!(!viewer.has_permission(Permission::Configure));
        assert!(!viewer.has_permission(Permission::Admin));
    }

    #[test]
    fn test_token_generation() {
        let token = token::generate_token(32);
        assert_eq!(token.len(), 32);
        assert!(token.chars().all(|c| c.is_ascii_alphanumeric()));

        let prefixed = token::generate_prefixed_token("nw", 24);
        assert!(prefixed.starts_with("nw_"));
        assert_eq!(prefixed.len(), 3 + 24);
    }
}
