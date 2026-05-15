//! Authentication & authorization.
//!
//!  * Email + password (Argon2 hashing)
//!  * Google OAuth2 (auth code flow)
//!  * JWT access tokens + opaque refresh tokens (stored hashed)
//!  * Permission checks against `users.role` and `user_permissions`

pub mod jwt;
pub mod oauth;
pub mod password;
pub mod permissions;
pub mod tokens;

pub use jwt::{Claims, JwtConfig, JwtCodec};
pub use password::{hash_password, verify_password};
pub use permissions::Permission;
pub use tokens::{generate_refresh_token, hash_refresh_token};
