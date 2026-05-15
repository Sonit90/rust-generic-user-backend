//! S3-compatible object storage wrapper (RustFS / MinIO / AWS S3).
//!
//! Used by both the `api` (for upload/download) and `jobs` (for purging).

use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_s3::{config::Builder as S3ConfigBuilder, primitives::ByteStream, Client};
use bytes::Bytes;
use price_merger_core::AppError;

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub use_path_style: bool,
}

#[derive(Clone)]
pub struct ObjectStore {
    client: Client,
    bucket: String,
}

impl ObjectStore {
    pub async fn new(cfg: StorageConfig) -> Result<Self, AppError> {
        let creds = Credentials::new(
            cfg.access_key, cfg.secret_key, None, None, "static",
        );
        let shared = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(cfg.region))
            .credentials_provider(creds)
            .endpoint_url(cfg.endpoint)
            .load()
            .await;

        let s3 = S3ConfigBuilder::from(&shared)
            .force_path_style(cfg.use_path_style)
            .build();

        Ok(Self {
            client: Client::from_conf(s3),
            bucket: cfg.bucket,
        })
    }

    pub async fn put(&self, key: &str, body: Bytes, content_type: Option<&str>) -> Result<(), AppError> {
        let mut req = self.client.put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(body));
        if let Some(ct) = content_type { req = req.content_type(ct); }
        req.send().await
            .map_err(|e| AppError::Storage(format!("put {key}: {e}")))?;
        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<Bytes, AppError> {
        let resp = self.client.get_object()
            .bucket(&self.bucket).key(key).send().await
            .map_err(|e| AppError::Storage(format!("get {key}: {e}")))?;
        let agg = resp.body.collect().await
            .map_err(|e| AppError::Storage(format!("read {key}: {e}")))?;
        Ok(agg.into_bytes())
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        self.client.delete_object().bucket(&self.bucket).key(key).send().await
            .map_err(|e| AppError::Storage(format!("delete {key}: {e}")))?;
        Ok(())
    }
}
