use std::sync::Arc;

use price_merger_auth::JwtCodec;
use price_merger_core::{models::User, AppError};
use price_merger_db::{connect, run_migrations, users as user_db, DbConfig};
use price_merger_jobs::{storage::StorageConfig, ObjectStore};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Settings;
use crate::middleware::auth::{FindUserById, HasJwt};

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub db: PgPool,
    pub storage: Arc<ObjectStore>,
    pub jwt: Arc<JwtCodec>,
    pub http: reqwest::Client,
}

impl AppState {
    pub async fn build(settings: Arc<Settings>) -> anyhow::Result<Self> {
        let db = connect(&DbConfig {
            url: settings.db.url.clone(),
            max_connections: settings.db.max_connections,
            min_connections: settings.db.min_connections,
            acquire_timeout_secs: settings.db.acquire_timeout_secs,
        }).await?;

        if settings.db.run_migrations_on_start {
            run_migrations(&db).await?;
        }

        let storage = ObjectStore::new(StorageConfig {
            endpoint: settings.storage.endpoint.clone(),
            region: settings.storage.region.clone(),
            bucket: settings.storage.bucket.clone(),
            access_key: settings.storage.access_key.clone(),
            secret_key: settings.storage.secret_key.clone(),
            use_path_style: settings.storage.use_path_style,
        }).await?;

        let jwt = JwtCodec::new(price_merger_auth::JwtConfig {
            secret: settings.auth.jwt_secret.clone(),
            access_ttl_min: settings.auth.jwt_access_ttl_min,
            refresh_ttl_days: settings.auth.jwt_refresh_ttl_days,
            issuer: "price-merger".into(),
        });

        Ok(Self {
            settings,
            db,
            storage: Arc::new(storage),
            jwt: Arc::new(jwt),
            http: reqwest::Client::builder()
                .user_agent("price-merger/0.1")
                .build()?,
        })
    }
}

impl HasJwt for AppState {
    fn jwt_codec(&self) -> &JwtCodec {
        &self.jwt
    }
}

#[async_trait::async_trait]
impl FindUserById for AppState {
    async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>, AppError> {
        user_db::find_by_id(&self.db, id).await.map_err(Into::into)
    }
}
