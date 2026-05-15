# Generic RBAC Auth Service

Standalone, reusable authentication and authorization service written in Rust. Provides user registration, login, JWT tokens, OAuth (Google), email verification, password reset, and role-based access control (RBAC).

**Zero business logic** — this is pure auth infrastructure you can embed in any application.

## Features

- **User management**: Register, login, email verification, password reset, account disable
- **Token management**: JWT access + refresh tokens with rotation and revocation
- **OAuth**: Google OAuth integration with automatic user creation
- **RBAC**: Role-based access control with granular permissions
- **Admin API**: User listing, role assignment, permission grants
- **Email**: Async email notifications (optional; logs to stdout in dev mode)
- **Database**: PostgreSQL with migrations, stateless API design

## Architecture

Cargo workspace:
```
crates/
├── api/         Axum HTTP server + routes
├── auth/        JWT, Argon2, OAuth logic
├── core/        Domain types (User, Role, Permission)
├── db/          SQLx + Postgres repository layer
```

Zero external services needed except PostgreSQL.

## Quick Start

### Prerequisites

- Rust 1.78+ (`rust-toolchain.toml` pins version)
- PostgreSQL 14+ (local or Docker)
- Docker (optional, for postgres container)

### Setup (Local Dev)

```powershell
# 1. Clone and copy env
cp .env.example .env
# Edit .env if needed (defaults assume localhost postgres)

# 2. Start Postgres (if using Docker)
cd docker
docker compose up -d postgres
cd ..

# 3. Run migrations
$env:DATABASE_URL = "postgres://auth_user:auth_password@localhost:5434/auth_db"
sqlx migrate run

# 4. Start the API
cargo run -p generic-auth-api
```

API listens on `http://localhost:8080`. Health check: `GET /health`.

### Setup (Docker)

```powershell
cd docker
docker compose up -d
```

This runs postgres + API in containers. Postgres: 5432, API: 8080.

## API Endpoints

All auth endpoints return `(access_token, refresh_token)` on success.

### Authentication

| Method | Path | Auth | Body | Description |
|--------|------|------|------|-------------|
| POST | `/api/v1/auth/register` | — | email, password | Register; auto-sends verification email |
| POST | `/api/v1/auth/login` | — | email, password | Login; returns tokens |
| POST | `/api/v1/auth/refresh` | — | refresh_token | Rotate tokens (old token revoked) |
| POST | `/api/v1/auth/logout` | Bearer | — | Revoke current refresh token |
| GET | `/api/v1/auth/verify-email` | — | ?token= | Verify email address |
| POST | `/api/v1/auth/resend-verification` | Bearer | — | Re-send verification email |
| POST | `/api/v1/auth/forgot-password` | — | email | Send password reset email |
| POST | `/api/v1/auth/reset-password` | — | token, password | Reset password |
| GET | `/api/v1/auth/oauth/google` | — | ?redirect_uri= | Redirect to Google OAuth |
| GET | `/api/v1/auth/oauth/google/callback` | — | ?code= | OAuth callback; exchanges code for tokens |

### Users

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/v1/users/me` | Bearer | Current user profile |
| GET | `/api/v1/users/me/permissions` | Bearer | Current user's effective permissions |

### Admin

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/v1/admin/users` | Bearer | List users (paginated: `?limit=&offset=`); requires `users.read` |
| PATCH | `/api/v1/admin/users/{id}/role` | Bearer | Change user role; requires `users.assign_roles` |
| PATCH | `/api/v1/admin/users/{id}/active` | Bearer | Enable/disable account; requires `users.manage` |
| POST | `/api/v1/admin/users/{id}/permissions` | Bearer | Grant/revoke permission; requires `users.assign_roles` |

### Health

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Liveness check |

## Database Schema

Auth-only tables (no business logic):

```sql
-- Roles (admin, moderator, user)
roles (id, name, description)

-- Permissions (users.read, users.manage, users.assign_roles)
permissions (id, name, description)

-- Role → permission mappings
role_permissions (role_id, permission_id)

-- Users
users (id, email, password_hash, display_name, role_id, is_active, email_verified, created_at, updated_at)

-- OAuth identities (one user can have multiple)
oauth_identities (id, user_id, provider, subject, email, raw_profile, created_at)

-- Refresh tokens (rotated, revocable)
refresh_tokens (id, user_id, token_hash, issued_at, expires_at, revoked_at, user_agent, ip)

-- Email verification tokens
email_verification_tokens (id, user_id, token_hash, expires_at, used_at, created_at)

-- Password reset tokens
password_reset_tokens (id, user_id, token_hash, expires_at, used_at, created_at)

-- Per-user permission overrides (optional)
user_permissions (user_id, permission_id, granted)
```

## Configuration

Config hierarchy (later overrides earlier):

1. `config/default.toml`
2. `config/<APP_ENV>.toml` (e.g., `production.toml`)
3. Environment variables: `APP__SECTION__KEY` (double underscores = nesting)
4. `DATABASE_URL` (overrides `db.url`)

See `.env.example` for all variables.

### Key Sections

**[auth]**
- `jwt_secret`: Secret for signing JWT tokens (set in production!)
- `jwt_access_ttl_min`: Access token lifetime (default 30 min)
- `jwt_refresh_ttl_days`: Refresh token lifetime (default 14 days)
- `password_min_length`: Minimum password length (default 8)

**[auth.google]** (optional)
- `client_id`, `client_secret`: From Google Cloud Console
- `redirect_url`: Where OAuth callback lands

**[email]** (optional)
- Leave `smtp_host` blank for dev mode → logs links to stdout
- Set for production: `smtp_host`, `smtp_port`, `smtp_username`, `smtp_password`

**[db]**
- `url`: Postgres connection string
- `max_connections`: Pool size (default 20)
- `min_connections`: Min idle connections (default 2)
- `run_migrations_on_start`: Auto-migrate on startup (default true)

## Admin Setup

No admin users are seeded by migrations. Create one via CLI:

```powershell
$env:DATABASE_URL = "postgres://auth_user:auth_password@localhost:5432/auth_db"
cargo run -p generic-auth-api -- create-admin --email=you@example.com --password=yourpassword
```

Or with compiled binary:
```powershell
./generic-auth-api create-admin --email=you@example.com --password=yourpassword
```

Then login at `POST /api/v1/auth/login` with those credentials.

## Testing

```powershell
# Unit tests (no DB needed)
cargo test -p generic-auth-api routes::auth

# All tests
cargo test --workspace
```

Integration tests that touch the DB require:
```powershell
$env:DATABASE_URL = "postgres://auth_user:auth_password@localhost:5432/auth_db_test"
sqlx migrate run --database-url $env:DATABASE_URL
cargo test --workspace
```

## Troubleshooting

**`error: relation "..." does not exist`** when running tests
- Migrations haven't been applied. Run `sqlx migrate run` against the test DB.

**`password authentication failed`** at startup
- Check `DATABASE_URL` env var is set correctly.
- Postgres isn't running or credentials are wrong.

**`SQLX_OFFLINE=true` but no cached data** during Docker build
- For offline compilation, run locally: `cargo sqlx prepare --workspace`
- Commit the `.sqlx/` directory.

## Integration Guide

To embed this in your application:

1. **Add auth as a dependency**:
   ```toml
   generic-auth-auth = { path = "path/to/crates/auth" }
   generic-auth-db = { path = "path/to/crates/db" }
   ```

2. **Initialize auth state** (see `crates/api/src/state.rs`):
   ```rust
   let db = generic_auth_db::connect(&db_config).await?;
   let jwt = JwtCodec::new(jwt_config);
   ```

3. **Protect endpoints** with `#[require_auth]` middleware or JWT validation:
   ```rust
   let user = extract_user_from_token(&request, &jwt)?;
   ```

4. **Check permissions** as needed:
   ```rust
   if !user.has_permission(&db, "resource.write").await? {
       return Err(Unauthorized);
   }
   ```

## API Documentation

Live Swagger UI: `http://localhost:8080/api/docs`

Generated from `#[utoipa::path]` annotations on route handlers.

## License

MIT OR Apache-2.0
