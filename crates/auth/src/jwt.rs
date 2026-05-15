use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use generic_auth_core::{models::Role, AppError};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub secret: String,
    pub access_ttl_min: i64,
    pub refresh_ttl_days: i64,
    pub issuer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,        // user id
    pub role: String,     // role name
    pub iss: String,
    pub iat: i64,         // unix seconds
    pub exp: i64,         // unix seconds
    pub jti: Uuid,        // unique token id
}

pub struct JwtCodec {
    enc: EncodingKey,
    dec: DecodingKey,
    cfg: JwtConfig,
}

impl JwtCodec {
    pub fn new(cfg: JwtConfig) -> Self {
        let enc = EncodingKey::from_secret(cfg.secret.as_bytes());
        let dec = DecodingKey::from_secret(cfg.secret.as_bytes());
        Self { enc, dec, cfg }
    }

    pub fn issue(&self, user_id: Uuid, role: Role) -> Result<String, AppError> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let exp = now + self.cfg.access_ttl_min * 60;
        let claims = Claims {
            sub: user_id,
            role: role.as_str().to_string(),
            iss: self.cfg.issuer.clone(),
            iat: now,
            exp,
            jti: Uuid::new_v4(),
        };
        encode(&Header::default(), &claims, &self.enc)
            .map_err(|e| AppError::Auth(format!("encode jwt: {e}")))
    }

    pub fn verify(&self, token: &str) -> Result<Claims, AppError> {
        let mut v = Validation::default();
        v.set_issuer(&[&self.cfg.issuer]);
        decode::<Claims>(token, &self.dec, &v)
            .map(|d| d.claims)
            .map_err(|e| AppError::Unauthorized.wrap(e))
    }
}

trait WrapErr {
    fn wrap<E: std::fmt::Display>(self, _: E) -> Self;
}
impl WrapErr for AppError {
    fn wrap<E: std::fmt::Display>(self, _e: E) -> Self {
        // We deliberately don't surface JWT internals to callers.
        self
    }
}
