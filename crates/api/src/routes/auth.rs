//! Email/password registration & login, OAuth (Google),
//! refresh-token rotation, and logout.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Json, Router,
};
use generic_auth_auth::{
    generate_refresh_token, hash_password, hash_refresh_token, verify_password,
};
use generic_auth_core::{models::{Role, User}, AppError};
use generic_auth_db::users as user_db;
use serde::{Deserialize, Serialize};
use time::{Duration as TDuration, OffsetDateTime};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::email::{send_password_reset_email, send_verification_email};
use crate::error::{ApiError, ApiResult};
use crate::middleware::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register",              post(register))
        .route("/login",                 post(login))
        .route("/refresh",               post(refresh))
        .route("/logout",                post(logout))
        .route("/verify-email",          get(verify_email))
        .route("/resend-verification",   post(resend_verification))
        .route("/forgot-password",       post(forgot_password))
        .route("/reset-password",        post(reset_password))
        .route("/oauth/google",          get(google_start))
        .route("/oauth/google/callback", get(google_callback))
}

// -------- email/password ---------------------------------------------------

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RegisterReq {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8))]
    pub password: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_in: i64,
}

/// Operations the `register_user` flow needs. Lets us mock the DB + JWT
/// codec in unit tests without a Postgres connection.
#[async_trait::async_trait]
pub trait RegisterPort: Send + Sync {
    async fn create_user(
        &self,
        email: &str,
        password_hash: &str,
        display_name: Option<&str>,
    ) -> Result<User, AppError>;

    async fn store_refresh_token(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: time::OffsetDateTime,
    ) -> Result<(), AppError>;

    fn issue_access_token(&self, user_id: Uuid, role: Role) -> Result<String, AppError>;
}

#[async_trait::async_trait]
impl RegisterPort for AppState {
    async fn create_user(
        &self, email: &str, password_hash: &str, display_name: Option<&str>,
    ) -> Result<User, AppError> {
        user_db::create_user(&self.db, user_db::NewUser {
            email: Some(email),
            password_hash: Some(password_hash),
            display_name,
            role: Role::User,
            email_verified: false,
        }).await.map_err(Into::into)
    }

    async fn store_refresh_token(
        &self, user_id: Uuid, token_hash: &str, expires_at: time::OffsetDateTime,
    ) -> Result<(), AppError> {
        user_db::store_refresh_token(&self.db, user_id, token_hash, expires_at, None)
            .await.map(|_| ()).map_err(Into::into)
    }

    fn issue_access_token(&self, user_id: Uuid, role: Role) -> Result<String, AppError> {
        self.jwt.issue(user_id, role)
    }
}

/// Pure registration logic — testable without a DB.
pub async fn register_user(
    port: &dyn RegisterPort,
    body: RegisterReq,
    access_ttl_min: i64,
    refresh_ttl_days: i64,
) -> Result<TokenPair, AppError> {
    body.validate().map_err(AppError::from_validation_errors)?;
    let phc = hash_password(&body.password)?;

    let user = port.create_user(&body.email, &phc, body.display_name.as_deref()).await?;
    let access = port.issue_access_token(user.id, user.role)?;

    let refresh = generate_refresh_token();
    let hash = hash_refresh_token(&refresh);
    let expires_at = OffsetDateTime::now_utc() + TDuration::days(refresh_ttl_days);
    port.store_refresh_token(user.id, &hash, expires_at).await?;

    Ok(TokenPair {
        access_token: access,
        refresh_token: refresh,
        access_expires_in: access_ttl_min * 60,
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    request_body = RegisterReq,
    responses(
        (status = 201, description = "Registered", body = TokenPair),
        (status = 409, description = "Email already taken"),
        (status = 422, description = "Validation error"),
    ),
    tag = "auth",
)]
pub(crate) async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterReq>,
) -> ApiResult<(StatusCode, Json<TokenPair>)> {
    let email = body.email.clone();
    let pair = register_user(
        &state,
        body,
        state.settings.auth.jwt_access_ttl_min,
        state.settings.auth.jwt_refresh_ttl_days,
    ).await.map_err(ApiError)?;

    // Issue verification token and send email (non-blocking).
    if let Ok(user) = user_db::find_by_email(&state.db, &email).await {
        if let Some(u) = user {
            let _ = spawn_verification_email(state.clone(), u.user.id, email).await;
        }
    }

    Ok((StatusCode::CREATED, Json(pair)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginReq {
    pub email: String,
    pub password: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    request_body = LoginReq,
    responses(
        (status = 200, description = "Logged in", body = TokenPair),
        (status = 401, description = "Invalid credentials"),
    ),
    tag = "auth",
)]
pub(crate) async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginReq>,
) -> ApiResult<Json<TokenPair>> {
    let row = user_db::find_by_email(&state.db, &body.email)
        .await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::Auth("Invalid email or password".into())))?;

    let phc = row.password_hash.as_deref()
        .ok_or_else(|| ApiError(AppError::Auth("password login disabled".into())))?;

    if !verify_password(&body.password, phc).map_err(ApiError)? {
        return Err(ApiError(AppError::Auth("Invalid email or password".into())));
    }
    if !row.user.is_active {
        return Err(ApiError(AppError::Forbidden));
    }

    let pair = issue_tokens(&state, row.user.id, row.user.role, None).await?;
    Ok(Json(pair))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RefreshReq { pub refresh_token: String }

#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    request_body = RefreshReq,
    responses(
        (status = 200, description = "Tokens rotated", body = TokenPair),
        (status = 401, description = "Token invalid or expired"),
    ),
    tag = "auth",
)]
pub(crate) async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshReq>,
) -> ApiResult<Json<TokenPair>> {
    let hash = hash_refresh_token(&body.refresh_token);
    let (user_id, expires_at, revoked_at) = user_db::lookup_refresh_token(&state.db, &hash)
        .await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::Unauthorized))?;
    if revoked_at.is_some() || expires_at <= OffsetDateTime::now_utc() {
        return Err(ApiError(AppError::Unauthorized));
    }
    // Rotate.
    user_db::revoke_refresh_token(&state.db, &hash)
        .await.map_err(|e| ApiError(e.into()))?;
    let user = user_db::find_by_id(&state.db, user_id)
        .await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::Unauthorized))?;

    Ok(Json(issue_tokens(&state, user.id, user.role, None).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LogoutReq { pub refresh_token: String }

#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    request_body = LogoutReq,
    responses(
        (status = 204, description = "Logged out"),
        (status = 401),
    ),
    tag = "auth",
)]
pub(crate) async fn logout(
    State(state): State<AppState>,
    Json(body): Json<LogoutReq>,
) -> ApiResult<StatusCode> {
    let hash = hash_refresh_token(&body.refresh_token);
    user_db::revoke_refresh_token(&state.db, &hash)
        .await.map_err(|e| ApiError(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

// -------- Google OAuth -----------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/auth/oauth/google",
    responses(
        (status = 307, description = "Redirect to Google"),
        (status = 400, description = "Google OAuth disabled"),
    ),
    tag = "auth",
)]
pub(crate) async fn google_start(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let cfg = generic_auth_auth::oauth::GoogleConfig {
        client_id: state.settings.auth.google.client_id.clone(),
        client_secret: state.settings.auth.google.client_secret.clone(),
        redirect_url: state.settings.auth.google.redirect_url.clone(),
    };
    if cfg.client_id.is_empty() {
        return Err(ApiError(AppError::BadRequest("google oauth disabled".into())));
    }
    let client = generic_auth_auth::oauth::google_client(&cfg).map_err(ApiError)?;
    let (url, _csrf) = generic_auth_auth::oauth::google_authorize_url(&client);
    // TODO: persist `_csrf` in a signed cookie and check on callback.
    Ok(Redirect::temporary(url.as_str()))
}

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct OAuthCallbackQuery {
    /// OAuth authorization code
    pub code: String,
    pub state: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/oauth/google/callback",
    params(OAuthCallbackQuery),
    responses((status = 200, body = TokenPair)),
    tag = "auth",
)]
pub(crate) async fn google_callback(
    State(state): State<AppState>,
    Query(q): Query<OAuthCallbackQuery>,
) -> ApiResult<Json<TokenPair>> {
    let cfg = generic_auth_auth::oauth::GoogleConfig {
        client_id: state.settings.auth.google.client_id.clone(),
        client_secret: state.settings.auth.google.client_secret.clone(),
        redirect_url: state.settings.auth.google.redirect_url.clone(),
    };
    let client = generic_auth_auth::oauth::google_client(&cfg).map_err(ApiError)?;
    let profile = generic_auth_auth::oauth::google_exchange_code(&client, q.code, &state.http)
        .await.map_err(ApiError)?;

    let user = upsert_oauth_user(&state, "google", &profile.sub,
        profile.email.as_deref(), profile.name.as_deref(),
        &serde_json::to_value(&profile).unwrap()).await?;

    Ok(Json(issue_tokens(&state, user.id, user.role, None).await?))
}

// -------- email verification -----------------------------------------------

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct VerifyEmailQuery {
    /// One-time verification token (URL-safe base64, 32 bytes)
    pub token: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/verify-email",
    params(VerifyEmailQuery),
    responses(
        (status = 200, description = "Email verified"),
        (status = 400, description = "Token invalid or expired"),
    ),
    tag = "auth",
)]
pub(crate) async fn verify_email(
    State(state): State<AppState>,
    Query(q): Query<VerifyEmailQuery>,
) -> ApiResult<StatusCode> {
    let hash = sha256_hex(&q.token);
    let user_id = user_db::consume_email_verification_token(&state.db, &hash)
        .await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::BadRequest("invalid or expired token".into())))?;

    user_db::mark_email_verified(&state.db, user_id)
        .await.map_err(|e| ApiError(e.into()))?;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/resend-verification",
    responses(
        (status = 204, description = "Verification email sent"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer_auth" = [])),
    tag = "auth",
)]
pub(crate) async fn resend_verification(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<StatusCode> {
    let user = user_db::find_by_id(&state.db, auth.user_id)
        .await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::Unauthorized))?;

    if user.email_verified {
        return Ok(StatusCode::NO_CONTENT);
    }

    if let Some(email) = user.email {
        let _ = spawn_verification_email(state, auth.user_id, email).await;
    }

    Ok(StatusCode::NO_CONTENT)
}

// -------- password recovery -----------------------------------------------

#[async_trait::async_trait]
pub trait PasswordResetPort: Send + Sync {
    /// Returns `(user_id, has_password_hash)`, or `None` if the email is not found.
    async fn find_by_email_for_reset(&self, email: &str) -> Result<Option<(Uuid, bool)>, AppError>;

    async fn store_reset_token(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: OffsetDateTime,
    ) -> Result<(), AppError>;

    async fn consume_reset_token(&self, token_hash: &str) -> Result<Option<Uuid>, AppError>;

    async fn update_password(&self, user_id: Uuid, phc: &str) -> Result<(), AppError>;

    async fn revoke_sessions(&self, user_id: Uuid) -> Result<(), AppError>;
}

#[async_trait::async_trait]
impl PasswordResetPort for AppState {
    async fn find_by_email_for_reset(&self, email: &str) -> Result<Option<(Uuid, bool)>, AppError> {
        Ok(user_db::find_by_email(&self.db, email)
            .await?
            .map(|row| (row.user.id, row.password_hash.is_some())))
    }

    async fn store_reset_token(
        &self, user_id: Uuid, token_hash: &str, expires_at: OffsetDateTime,
    ) -> Result<(), AppError> {
        user_db::store_password_reset_token(&self.db, user_id, token_hash, expires_at)
            .await.map_err(Into::into)
    }

    async fn consume_reset_token(&self, token_hash: &str) -> Result<Option<Uuid>, AppError> {
        user_db::consume_password_reset_token(&self.db, token_hash)
            .await.map_err(Into::into)
    }

    async fn update_password(&self, user_id: Uuid, phc: &str) -> Result<(), AppError> {
        user_db::update_password_hash(&self.db, user_id, phc)
            .await.map_err(Into::into)
    }

    async fn revoke_sessions(&self, user_id: Uuid) -> Result<(), AppError> {
        user_db::revoke_all_refresh_tokens(&self.db, user_id)
            .await.map_err(Into::into)
    }
}

/// Generates and stores a reset token. Returns the raw token so the caller can send the email.
/// Returns `None` when no action is needed (unknown email or OAuth-only account).
pub async fn request_password_reset(
    port: &dyn PasswordResetPort,
    email: &str,
    reset_ttl_hours: i64,
) -> Result<Option<String>, AppError> {
    let Some((user_id, has_password)) = port.find_by_email_for_reset(email).await? else {
        return Ok(None);
    };
    if !has_password {
        return Ok(None);
    }
    let token = generate_verification_token();
    let hash = sha256_hex(&token);
    let expires_at = OffsetDateTime::now_utc() + TDuration::hours(reset_ttl_hours);
    port.store_reset_token(user_id, &hash, expires_at).await?;
    Ok(Some(token))
}

/// Validates, consumes the token, updates the password hash, and revokes all sessions.
pub async fn perform_password_reset(
    port: &dyn PasswordResetPort,
    body: ResetPasswordReq,
) -> Result<(), AppError> {
    body.validate().map_err(AppError::from_validation_errors)?;
    let token_hash = sha256_hex(&body.token);
    let user_id = port.consume_reset_token(&token_hash).await?
        .ok_or_else(|| AppError::BadRequest("invalid or expired token".into()))?;
    let phc = hash_password(&body.new_password)?;
    port.update_password(user_id, &phc).await?;
    port.revoke_sessions(user_id).await?;
    Ok(())
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ForgotPasswordReq {
    pub email: String,
}

#[utoipa::path(
    post,
    operation_id = "forgot_password",
    path = "/api/v1/auth/forgot-password",
    request_body = ForgotPasswordReq,
    responses(
        (status = 204, description = "Reset email sent if account exists"),
    ),
    tag = "auth",
)]
pub(crate) async fn forgot_password(
    State(state): State<AppState>,
    Json(body): Json<ForgotPasswordReq>,
) -> ApiResult<StatusCode> {
    // Always 204 — don't reveal whether the email exists.
    if let Ok(Some(token)) = request_password_reset(&state, &body.email, 1).await {
        let url = format!(
            "{}/reset-password?token={}",
            state.settings.http.public_url, token
        );
        let email = body.email.clone();
        tokio::spawn(async move {
            if let Err(e) = send_password_reset_email(&state.settings.email, &email, &url).await {
                tracing::error!(error = %e, "failed to send password reset email");
            }
        });
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ResetPasswordReq {
    pub token: String,
    #[validate(length(min = 8))]
    pub new_password: String,
}

#[utoipa::path(
    post,
    operation_id = "reset_password",
    path = "/api/v1/auth/reset-password",
    request_body = ResetPasswordReq,
    responses(
        (status = 204, description = "Password updated"),
        (status = 400, description = "Token invalid or expired, or password too short"),
    ),
    tag = "auth",
)]
pub(crate) async fn reset_password(
    State(state): State<AppState>,
    Json(body): Json<ResetPasswordReq>,
) -> ApiResult<StatusCode> {
    perform_password_reset(&state, body).await.map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Generate a verification token, store its hash, and fire off email sending in background.
async fn spawn_verification_email(state: AppState, user_id: Uuid, email: String) -> Result<(), ()> {
    let token = generate_verification_token();
    let hash = sha256_hex(&token);
    let expires_at = OffsetDateTime::now_utc() + TDuration::hours(24);

    user_db::store_email_verification_token(&state.db, user_id, &hash, expires_at)
        .await.map_err(|_| ())?;

    let url = format!(
        "{}/verify-email?token={}",
        state.settings.http.public_url, token
    );

    tokio::spawn(async move {
        if let Err(e) = send_verification_email(
            &state.settings.email,
            &email,
            &url,
        ).await {
            tracing::error!(error = %e, "failed to send verification email");
        }
    });

    Ok(())
}

fn generate_verification_token() -> String {
    // Two v4 UUIDs concatenated → 256-bit URL-safe token.
    format!("{}{}", Uuid::new_v4().as_simple(), Uuid::new_v4().as_simple())
}

fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(input.as_bytes());
    hex::encode(hash)
}

// -------- helpers ----------------------------------------------------------

async fn upsert_oauth_user(
    state: &AppState,
    provider: &str,
    subject: &str,
    email: Option<&str>,
    display: Option<&str>,
    raw: &serde_json::Value,
) -> ApiResult<generic_auth_core::models::User> {
    if let Some(u) = user_db::find_user_by_oauth(&state.db, provider, subject)
        .await.map_err(|e| ApiError(e.into()))?
    {
        user_db::upsert_oauth_identity(&state.db, u.id, provider, subject, email, raw)
            .await.map_err(|e| ApiError(e.into()))?;
        return Ok(u);
    }
    let user = user_db::create_user(&state.db, user_db::NewUser {
        email,
        password_hash: None,
        display_name: display,
        role: Role::User,
        email_verified: email.is_some(),
    }).await.map_err(|e| ApiError(e.into()))?;
    user_db::upsert_oauth_identity(&state.db, user.id, provider, subject, email, raw)
        .await.map_err(|e| ApiError(e.into()))?;
    Ok(user)
}

async fn issue_tokens(
    state: &AppState,
    user_id: Uuid,
    role: Role,
    user_agent: Option<&str>,
) -> ApiResult<TokenPair> {
    let access = state.jwt.issue(user_id, role).map_err(ApiError)?;
    let refresh = generate_refresh_token();
    let hash = hash_refresh_token(&refresh);
    let exp = OffsetDateTime::now_utc()
        + TDuration::days(state.settings.auth.jwt_refresh_ttl_days);
    user_db::store_refresh_token(&state.db, user_id, &hash, exp, user_agent)
        .await.map_err(|e| ApiError(e.into()))?;
    Ok(TokenPair {
        access_token: access,
        refresh_token: refresh,
        access_expires_in: state.settings.auth.jwt_access_ttl_min * 60,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use time::OffsetDateTime;

    /// Mock implementation of `RegisterPort`. Records calls and returns
    /// canned responses. Errors aren't `Clone`, so each response slot is a
    /// `Mutex<Option<Result>>` that the trait method `take()`s.
    #[derive(Default)]
    struct MockPort {
        create_user_response: Mutex<Option<Result<User, AppError>>>,
        store_token_response: Mutex<Option<Result<(), AppError>>>,
        issue_token_response: Mutex<Option<Result<String, AppError>>>,

        create_user_calls: Mutex<Vec<(String, String, Option<String>)>>,
        store_token_calls: Mutex<Vec<(uuid::Uuid, String)>>,
    }

    impl MockPort {
        fn with_user(self, u: User) -> Self {
            *self.create_user_response.lock().unwrap() = Some(Ok(u));
            self
        }
        fn with_create_err(self, e: AppError) -> Self {
            *self.create_user_response.lock().unwrap() = Some(Err(e));
            self
        }
        fn with_access_token(self, tok: &str) -> Self {
            *self.issue_token_response.lock().unwrap() = Some(Ok(tok.into()));
            self
        }
        fn with_store_ok(self) -> Self {
            *self.store_token_response.lock().unwrap() = Some(Ok(()));
            self
        }
    }

    #[async_trait::async_trait]
    impl RegisterPort for MockPort {
        async fn create_user(
            &self, email: &str, phc: &str, display_name: Option<&str>,
        ) -> Result<User, AppError> {
            self.create_user_calls.lock().unwrap()
                .push((email.into(), phc.into(), display_name.map(|s| s.into())));
            self.create_user_response.lock().unwrap()
                .take()
                .expect("create_user_response not set")
        }

        async fn store_refresh_token(
            &self, user_id: uuid::Uuid, token_hash: &str, _expires_at: OffsetDateTime,
        ) -> Result<(), AppError> {
            self.store_token_calls.lock().unwrap().push((user_id, token_hash.into()));
            self.store_token_response.lock().unwrap()
                .take()
                .expect("store_token_response not set")
        }

        fn issue_access_token(&self, _u: uuid::Uuid, _r: Role) -> Result<String, AppError> {
            self.issue_token_response.lock().unwrap()
                .take()
                .expect("issue_token_response not set")
        }
    }

    fn sample_user(email: &str) -> User {
        let now = OffsetDateTime::now_utc();
        User {
            id: uuid::Uuid::new_v4(),
            email: Some(email.into()),
            display_name: None,
            role: Role::User,
            is_active: true,
            email_verified: false,
            created_at: now,
            updated_at: now,
        }
    }

    fn body(email: &str, password: &str) -> RegisterReq {
        RegisterReq { email: email.into(), password: password.into(), display_name: None }
    }

    // ---- happy path -------------------------------------------------------

    #[tokio::test]
    async fn happy_path_returns_token_pair_and_persists() {
        let user = sample_user("alice@example.com");
        let port = MockPort::default()
            .with_user(user.clone())
            .with_access_token("jwt.access.token")
            .with_store_ok();

        let pair = register_user(&port, body("alice@example.com", "longenoughpw"), 30, 14)
            .await.expect("register should succeed");

        assert_eq!(pair.access_token, "jwt.access.token");
        assert!(!pair.refresh_token.is_empty());
        assert_eq!(pair.access_expires_in, 30 * 60);

        // create_user called with the email and a non-empty Argon2 hash (not the raw password).
        let creates = port.create_user_calls.lock().unwrap();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].0, "alice@example.com");
        assert!(creates[0].1.starts_with("$argon2"));
        assert_ne!(creates[0].1, "longenoughpw");

        // refresh token stored as its SHA-256 hash, not the raw token.
        let stores = port.store_token_calls.lock().unwrap();
        assert_eq!(stores.len(), 1);
        assert_eq!(stores[0].0, user.id);
        assert_ne!(stores[0].1, pair.refresh_token);
        assert_eq!(stores[0].1, hash_refresh_token(&pair.refresh_token));
    }

    // ---- validation -------------------------------------------------------

    #[tokio::test]
    async fn rejects_invalid_email() {
        let port = MockPort::default(); // no DB calls expected
        let err = register_user(&port, body("not-an-email", "longenoughpw"), 30, 14)
            .await.unwrap_err();

        match err {
            AppError::ValidationFields { fields, .. } => {
                assert!(fields.contains_key("email"), "expected email field error, got {fields:?}");
            }
            other => panic!("expected ValidationFields, got {other:?}"),
        }
        assert!(port.create_user_calls.lock().unwrap().is_empty(),
            "create_user must not be called for invalid input");
    }

    #[tokio::test]
    async fn rejects_short_password() {
        let port = MockPort::default();
        let err = register_user(&port, body("alice@example.com", "short"), 30, 14)
            .await.unwrap_err();

        match err {
            AppError::ValidationFields { fields, .. } => {
                assert!(fields.contains_key("password"));
            }
            other => panic!("expected ValidationFields, got {other:?}"),
        }
    }

    // ---- DB error propagation --------------------------------------------

    #[tokio::test]
    async fn propagates_db_conflict_and_does_not_issue_tokens() {
        let port = MockPort::default()
            .with_create_err(AppError::Conflict("email taken".into()));

        let err = register_user(&port, body("alice@example.com", "longenoughpw"), 30, 14)
            .await.unwrap_err();

        assert!(matches!(err, AppError::Conflict(_)));
        // We never reach token storage. (issue_access_token would have panicked
        // since no canned response was set — that it didn't proves it wasn't called.)
        assert!(port.store_token_calls.lock().unwrap().is_empty());
    }
}

#[cfg(test)]
mod password_reset_tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockResetPort {
        find_response:    Mutex<Option<Result<Option<(Uuid, bool)>, AppError>>>,
        store_response:   Mutex<Option<Result<(), AppError>>>,
        consume_response: Mutex<Option<Result<Option<Uuid>, AppError>>>,
        update_response:  Mutex<Option<Result<(), AppError>>>,
        revoke_response:  Mutex<Option<Result<(), AppError>>>,

        store_calls:  Mutex<Vec<(Uuid, String)>>,
        update_calls: Mutex<Vec<(Uuid, String)>>,
        revoke_calls: Mutex<Vec<Uuid>>,
    }

    impl MockResetPort {
        fn with_user(self, user_id: Uuid) -> Self {
            *self.find_response.lock().unwrap() = Some(Ok(Some((user_id, true))));
            self
        }
        fn with_oauth_user(self, user_id: Uuid) -> Self {
            *self.find_response.lock().unwrap() = Some(Ok(Some((user_id, false))));
            self
        }
        fn with_no_user(self) -> Self {
            *self.find_response.lock().unwrap() = Some(Ok(None));
            self
        }
        fn with_store_ok(self) -> Self {
            *self.store_response.lock().unwrap() = Some(Ok(()));
            self
        }
        fn with_valid_token(self, user_id: Uuid) -> Self {
            *self.consume_response.lock().unwrap() = Some(Ok(Some(user_id)));
            self
        }
        fn with_invalid_token(self) -> Self {
            *self.consume_response.lock().unwrap() = Some(Ok(None));
            self
        }
        fn with_update_ok(self) -> Self {
            *self.update_response.lock().unwrap() = Some(Ok(()));
            self
        }
        fn with_revoke_ok(self) -> Self {
            *self.revoke_response.lock().unwrap() = Some(Ok(()));
            self
        }
    }

    #[async_trait::async_trait]
    impl PasswordResetPort for MockResetPort {
        async fn find_by_email_for_reset(&self, _email: &str) -> Result<Option<(Uuid, bool)>, AppError> {
            self.find_response.lock().unwrap().take().expect("find_response not set")
        }

        async fn store_reset_token(
            &self, user_id: Uuid, token_hash: &str, _expires_at: OffsetDateTime,
        ) -> Result<(), AppError> {
            self.store_calls.lock().unwrap().push((user_id, token_hash.into()));
            self.store_response.lock().unwrap().take().expect("store_response not set")
        }

        async fn consume_reset_token(&self, _token_hash: &str) -> Result<Option<Uuid>, AppError> {
            self.consume_response.lock().unwrap().take().expect("consume_response not set")
        }

        async fn update_password(&self, user_id: Uuid, phc: &str) -> Result<(), AppError> {
            self.update_calls.lock().unwrap().push((user_id, phc.into()));
            self.update_response.lock().unwrap().take().expect("update_response not set")
        }

        async fn revoke_sessions(&self, user_id: Uuid) -> Result<(), AppError> {
            self.revoke_calls.lock().unwrap().push(user_id);
            self.revoke_response.lock().unwrap().take().expect("revoke_response not set")
        }
    }

    fn reset_req(token: &str, password: &str) -> ResetPasswordReq {
        ResetPasswordReq { token: token.into(), new_password: password.into() }
    }

    // ---- request_password_reset -------------------------------------------

    #[tokio::test]
    async fn request_stores_token_for_password_user() {
        let user_id = Uuid::new_v4();
        let port = MockResetPort::default().with_user(user_id).with_store_ok();

        let token = request_password_reset(&port, "alice@example.com", 1)
            .await.expect("should succeed");

        let token = token.expect("should return token for password user");
        assert!(!token.is_empty());

        let stores = port.store_calls.lock().unwrap();
        assert_eq!(stores.len(), 1, "exactly one store call");
        assert_eq!(stores[0].0, user_id);
        // stored value must be the SHA-256 hash, not the raw token
        assert_ne!(stores[0].1, token, "must not store raw token");
        assert_eq!(stores[0].1, sha256_hex(&token), "must store SHA-256 of token");
    }

    #[tokio::test]
    async fn request_no_op_for_unknown_email() {
        let port = MockResetPort::default().with_no_user();

        let result = request_password_reset(&port, "nobody@example.com", 1)
            .await.expect("should not error");

        assert!(result.is_none());
        assert!(port.store_calls.lock().unwrap().is_empty(), "must not store token");
    }

    #[tokio::test]
    async fn request_no_op_for_oauth_only_account() {
        let user_id = Uuid::new_v4();
        let port = MockResetPort::default().with_oauth_user(user_id);

        let result = request_password_reset(&port, "oauth@example.com", 1)
            .await.expect("should not error");

        assert!(result.is_none());
        assert!(
            port.store_calls.lock().unwrap().is_empty(),
            "must not store token for OAuth-only account",
        );
    }

    // ---- perform_password_reset -------------------------------------------

    #[tokio::test]
    async fn reset_updates_password_and_revokes_sessions() {
        let user_id = Uuid::new_v4();
        let port = MockResetPort::default()
            .with_valid_token(user_id)
            .with_update_ok()
            .with_revoke_ok();

        perform_password_reset(&port, reset_req("some-token", "newpassword123"))
            .await.expect("should succeed");

        let updates = port.update_calls.lock().unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].0, user_id);
        assert!(updates[0].1.starts_with("$argon2"), "must store Argon2 hash");
        assert_ne!(updates[0].1, "newpassword123", "must not store raw password");

        let revokes = port.revoke_calls.lock().unwrap();
        assert_eq!(revokes.len(), 1, "must revoke sessions");
        assert_eq!(revokes[0], user_id);
    }

    #[tokio::test]
    async fn reset_rejects_short_password() {
        // No responses set — any DB call would panic, proving none are made.
        let port = MockResetPort::default();

        let err = perform_password_reset(&port, reset_req("some-token", "short"))
            .await.unwrap_err();

        match err {
            AppError::ValidationFields { fields, .. } => {
                assert!(fields.contains_key("new_password"), "expected new_password error, got {fields:?}");
            }
            other => panic!("expected ValidationFields, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reset_rejects_invalid_token() {
        // consume returns None; update/revoke responses intentionally not set.
        let port = MockResetPort::default().with_invalid_token();

        let err = perform_password_reset(&port, reset_req("bad-token", "validpassword"))
            .await.unwrap_err();

        assert!(matches!(err, AppError::BadRequest(_)));
        assert!(port.update_calls.lock().unwrap().is_empty(), "must not update password");
        assert!(port.revoke_calls.lock().unwrap().is_empty(), "must not revoke sessions");
    }
}