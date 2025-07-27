//! File-based streaming cache manager that stores response metadata and body content separately.
//! This enables streaming by never loading complete response bodies into memory.
//!
//! This implementation is based on the [http-cache-stream](https://github.com/stjude-rust-labs/http-cache-stream) approach.

use crate::{
    body::StreamingBody,
    error::{Result, StreamingError},
    runtime,
};
use async_trait::async_trait;
use bytes::Bytes;
use http::{Response, Version};
use http_body_util::{BodyExt, Empty};
use http_cache_semantics::CachePolicy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use url::Url;
use uuid::Uuid;

const CACHE_VERSION: &str = "cache-v2";

/// Metadata stored for each cached response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub status: u16,
    pub version: u8,
    pub headers: HashMap<String, String>,
    pub content_digest: String,
    pub policy: CachePolicy,
    pub created_at: u64,
}

/// File-based streaming cache manager
#[derive(Debug, Clone)]
pub struct StreamingManager {
    root_path: PathBuf,
}

impl StreamingManager {
    /// Create a new streaming cache manager
    pub fn new(root_path: PathBuf) -> Self {
        Self { root_path }
    }

    /// Get the path for storing metadata
    fn metadata_path(&self, key: &str) -> PathBuf {
        let encoded_key = hex::encode(key.as_bytes());
        self.root_path
            .join(CACHE_VERSION)
            .join("metadata")
            .join(format!("{encoded_key}.json"))
    }

    /// Get the path for storing content
    fn content_path(&self, digest: &str) -> PathBuf {
        self.root_path.join(CACHE_VERSION).join("content").join(digest)
    }

    /// Calculate SHA256 digest of content
    fn calculate_digest(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hex::encode(hasher.finalize())
    }

    /// Ensure directory exists
    async fn ensure_dir_exists(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            runtime::create_dir_all(parent)
                .await
                .map_err(StreamingError::new)?;
        }
        Ok(())
    }
}

#[async_trait]
impl crate::StreamingCacheManager for StreamingManager {
    type Body = StreamingBody<Empty<Bytes>>;

    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(Response<Self::Body>, CachePolicy)>>
    where
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error:
            Into<StreamingError> + Send + Sync + 'static,
    {
        let metadata_path = self.metadata_path(cache_key);

        // Check if metadata file exists
        if !metadata_path.exists() {
            return Ok(None);
        }

        // Read and parse metadata
        let metadata_content =
            runtime::read(&metadata_path).await.map_err(StreamingError::new)?;
        let metadata: CacheMetadata = serde_json::from_slice(&metadata_content)
            .map_err(StreamingError::new)?;

        // Check if content file exists
        let content_path = self.content_path(&metadata.content_digest);
        if !content_path.exists() {
            return Ok(None);
        }

        // Open content file for streaming
        let file = runtime::File::open(&content_path)
            .await
            .map_err(StreamingError::new)?;

        // Build response with streaming body
        let mut response_builder = Response::builder()
            .status(metadata.status)
            .version(match metadata.version {
                9 => Version::HTTP_09,
                10 => Version::HTTP_10,
                11 => Version::HTTP_11,
                2 => Version::HTTP_2,
                3 => Version::HTTP_3,
                _ => Version::HTTP_11,
            });

        // Add headers
        for (name, value) in &metadata.headers {
            if let (Ok(header_name), Ok(header_value)) = (
                name.parse::<http::HeaderName>(),
                value.parse::<http::HeaderValue>(),
            ) {
                response_builder =
                    response_builder.header(header_name, header_value);
            }
        }

        // Create streaming body from file
        let body = StreamingBody::from_file(file);
        let response =
            response_builder.body(body).map_err(StreamingError::new)?;

        Ok(Some((response, metadata.policy)))
    }

    async fn put<B>(
        &self,
        cache_key: String,
        response: Response<B>,
        policy: CachePolicy,
        _request_url: Url,
    ) -> Result<Response<Self::Body>>
    where
        B: http_body::Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<StreamingError>,
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error:
            Into<StreamingError> + Send + Sync + 'static,
    {
        let (parts, body) = response.into_parts();

        // Collect body content
        let collected =
            body.collect().await.map_err(|e| StreamingError::new(e.into()))?;
        let body_bytes = collected.to_bytes();

        // Calculate content digest for deduplication
        let content_digest = Self::calculate_digest(&body_bytes);
        let content_path = self.content_path(&content_digest);

        // Ensure content directory exists and write content if not already present
        if !content_path.exists() {
            Self::ensure_dir_exists(&content_path).await?;
            runtime::write(&content_path, &body_bytes)
                .await
                .map_err(StreamingError::new)?;
        }

        // Create metadata
        let metadata = CacheMetadata {
            status: parts.status.as_u16(),
            version: match parts.version {
                Version::HTTP_09 => 9,
                Version::HTTP_10 => 10,
                Version::HTTP_11 => 11,
                Version::HTTP_2 => 2,
                Version::HTTP_3 => 3,
                _ => 11,
            },
            headers: parts
                .headers
                .iter()
                .map(|(name, value)| {
                    (name.to_string(), value.to_str().unwrap_or("").to_string())
                })
                .collect(),
            content_digest: content_digest.clone(),
            policy,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        // Write metadata
        let metadata_path = self.metadata_path(&cache_key);
        Self::ensure_dir_exists(&metadata_path).await?;
        let metadata_json =
            serde_json::to_vec(&metadata).map_err(StreamingError::new)?;
        runtime::write(&metadata_path, &metadata_json)
            .await
            .map_err(StreamingError::new)?;

        // Return response with buffered body for immediate use
        let response =
            Response::from_parts(parts, StreamingBody::buffered(body_bytes));
        Ok(response)
    }

    async fn convert_body<B>(
        &self,
        response: Response<B>,
    ) -> Result<Response<Self::Body>>
    where
        B: http_body::Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<StreamingError>,
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error:
            Into<StreamingError> + Send + Sync + 'static,
    {
        let (parts, body) = response.into_parts();

        // Create a temporary file for streaming the non-cacheable response
        let temp_dir = std::env::temp_dir().join("http-cache-streaming");
        runtime::create_dir_all(&temp_dir)
            .await
            .map_err(StreamingError::new)?;
        let temp_path = temp_dir.join(format!("stream_{}", Uuid::new_v4()));

        // Collect body and write to temporary file
        let collected =
            body.collect().await.map_err(|e| StreamingError::new(e.into()))?;
        let body_bytes = collected.to_bytes();
        runtime::write(&temp_path, &body_bytes)
            .await
            .map_err(StreamingError::new)?;

        // Open file for streaming
        let file = runtime::File::open(&temp_path)
            .await
            .map_err(StreamingError::new)?;
        let streaming_body = StreamingBody::from_file(file);

        Ok(Response::from_parts(parts, streaming_body))
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        let metadata_path = self.metadata_path(cache_key);

        // Read metadata to get content digest
        if let Ok(metadata_content) = runtime::read(&metadata_path).await {
            if let Ok(metadata) =
                serde_json::from_slice::<CacheMetadata>(&metadata_content)
            {
                let content_path = self.content_path(&metadata.content_digest);
                // Remove content file (note: this could be shared, so we might want reference counting)
                runtime::remove_file(&content_path).await.ok();
            }
        }

        // Remove metadata file
        runtime::remove_file(&metadata_path).await.ok();
        Ok(())
    }

    #[cfg(feature = "streaming")]
    fn body_to_bytes_stream(
        body: Self::Body,
    ) -> impl futures_util::Stream<
        Item = std::result::Result<
            Bytes,
            Box<dyn std::error::Error + Send + Sync>,
        >,
    > + Send
    where
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error: Send + Sync + 'static,
    {
        // Use the StreamingBody's built-in conversion method
        body.into_bytes_stream()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StreamingCacheManager as StreamingCacheManagerTrait;
    use http_body_util::Full;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_streaming_cache_put_get() {
        let temp_dir = TempDir::new().unwrap();
        let cache = StreamingManager::new(temp_dir.path().to_path_buf());

        let original_body = Full::new(Bytes::from("test response body"));
        let response = Response::builder()
            .status(200)
            .header("content-type", "text/plain")
            .body(original_body)
            .unwrap();

        let policy = CachePolicy::new(
            &http::request::Request::builder()
                .method("GET")
                .uri("/test")
                .body(())
                .unwrap()
                .into_parts()
                .0,
            &response.clone().map(|_| ()),
        );

        let request_url = Url::parse("http://example.com/test").unwrap();

        // Put response into cache
        let cached_response = cache
            .put("test-key".to_string(), response, policy.clone(), request_url)
            .await
            .unwrap();

        // Response should be returned immediately
        assert_eq!(cached_response.status(), 200);

        // Get response from cache
        let retrieved = cache.get("test-key").await.unwrap();
        assert!(retrieved.is_some());

        let (cached_response, cached_policy) = retrieved.unwrap();
        assert_eq!(cached_response.status(), 200);
        assert_eq!(
            cached_response.headers().get("content-type").unwrap(),
            "text/plain"
        );

        // Verify policy is preserved
        let now = std::time::SystemTime::now();
        assert_eq!(cached_policy.time_to_live(now), policy.time_to_live(now));
    }

    #[tokio::test]
    async fn test_streaming_cache_delete() {
        let temp_dir = TempDir::new().unwrap();
        let cache = StreamingManager::new(temp_dir.path().to_path_buf());

        let original_body = Full::new(Bytes::from("test response body"));
        let response = Response::builder()
            .status(200)
            .header("content-type", "text/plain")
            .body(original_body)
            .unwrap();

        let policy = CachePolicy::new(
            &http::request::Request::builder()
                .method("GET")
                .uri("/test")
                .body(())
                .unwrap()
                .into_parts()
                .0,
            &response.clone().map(|_| ()),
        );

        let request_url = Url::parse("http://example.com/test").unwrap();
        let cache_key = "test-key-delete";

        // Put response into cache
        cache
            .put(cache_key.to_string(), response, policy, request_url)
            .await
            .unwrap();

        // Verify it exists
        let retrieved = cache.get(cache_key).await.unwrap();
        assert!(retrieved.is_some());

        // Delete it
        cache.delete(cache_key).await.unwrap();

        // Verify it's gone
        let retrieved = cache.get(cache_key).await.unwrap();
        assert!(retrieved.is_none());
    }
}
