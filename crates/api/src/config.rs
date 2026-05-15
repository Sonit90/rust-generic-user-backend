use config::{Config, Environment, File};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    #[serde(skip)]
    pub app_env: String,
    pub http: Http,
    pub db: Db,
    pub auth: Auth,
    pub cors: Cors,
    pub email: Email,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Http {
    pub bind: String,
    pub public_url: String,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Db {
    pub url: String,
    pub max_connections: u32,
    #[serde(default = "default_min")] pub min_connections: u32,
    #[serde(default = "default_acquire")] pub acquire_timeout_secs: u64,
    #[serde(default)] pub run_migrations_on_start: bool,
}
fn default_min() -> u32 { 2 }
fn default_acquire() -> u64 { 10 }

#[derive(Debug, Clone, Deserialize)]
pub struct Auth {
    pub jwt_secret: String,
    pub jwt_access_ttl_min: i64,
    pub jwt_refresh_ttl_days: i64,
    pub password_min_length: usize,
    pub google: GoogleAuth,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoogleAuth {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Cors {
    pub allowed_origins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Email {
    /// SMTP hostname (e.g. smtp.mailjet.com, smtp.sendgrid.net).
    /// Leave blank to disable sending and log the verification link instead.
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub from_email: String,
    pub from_name: String,
}

impl Settings {
    /// Load `config/default.toml`, then `config/<APP_ENV>.toml`, then env vars
    /// of the form `APP__SECTION__KEY`. `DATABASE_URL` overrides `db.url`.
    pub fn load() -> anyhow::Result<Self> {
        let app_env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".into());

        // Resolve the config dir relative to the binary's CWD.
        let mut builder = Config::builder()
            .add_source(File::with_name("config/default").required(false))
            .add_source(File::with_name(&format!("config/{app_env}")).required(false))
            .add_source(
                Environment::with_prefix("APP")
                    .separator("__")
                    .try_parsing(true)
                    .list_separator(",")
                    .with_list_parse_key("cors.allowed_origins"),
            );

        if let Ok(url) = std::env::var("DATABASE_URL") {
            builder = builder.set_override("db.url", url)?;
        }

        let mut s: Settings = builder.build()?.try_deserialize()?;
        s.app_env = app_env;
        Ok(s)
    }
}
