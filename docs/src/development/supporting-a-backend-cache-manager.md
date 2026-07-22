# Supporting a Backend Cache Manager

This section is intended for those looking to implement a custom backend cache manager, or understand how the [`CacheManager`](https://docs.rs/http-cache/latest/http_cache/trait.CacheManager.html) and [`StreamingCacheManager`](https://docs.rs/http-cache/latest/http_cache/trait.StreamingCacheManager.html) traits work.

## The `CacheManager` trait

The [`CacheManager`](https://docs.rs/http-cache/latest/http_cache/trait.CacheManager.html) trait is the main trait that needs to be implemented to support a new backend cache manager. It has three methods that it requires:

- `get`: retrieve a cached response given the provided cache key
- `put`: store a response and related policy object in the cache associated with the provided cache key
- `delete`: remove a cached response from the cache associated with the provided cache key

The methods are asynchronous and use native Rust async functions in traits (AFIT), stabilized in Rust 1.75.

### The `get` method

The `get` method is used to retrieve a cached response given the provided cache key. It returns an `Result<Option<(HttpResponse, CachePolicy)>, BoxError>` where `HttpResponse` is the cached response and [`CachePolicy`](https://docs.rs/http-cache-semantics/latest/http_cache_semantics/struct.CachePolicy.html) is the associated cache policy object that provides us helpful metadata. If the cache key does not exist in the cache, `Ok(None)` is returned.

### The `put` method

The `put` method is used to store a response and related policy object in the cache associated with the provided cache key. It returns an `Result<HttpResponse, BoxError>` where `HttpResponse` is the passed response.

### The `delete` method

The `delete` method is used to remove a cached response from the cache associated with the provided cache key. It returns an `Result<(), BoxError>`.

## The `StreamingCacheManager` trait

The [`StreamingCacheManager`](https://docs.rs/http-cache/latest/http_cache/trait.StreamingCacheManager.html) trait supports memory-efficient handling of large responses. Rather than collecting bodies into memory, `get` returns a `Response<Self::Body>` whose body streams from the underlying storage.

Required items:

- `type Body` — the body type returned by `get`; must implement `http_body::Body`
- `get` — retrieve a cached response, body-as-stream
- `put` — store a response, consuming its body
- `update_metadata` — update the stored headers, cache policy, and user metadata for an
  existing entry **without** touching the body file; used by 304 revalidation, where the
  body is known-unchanged and re-reading/re-writing it would be wasted work
- `convert_body` — produce a `Self::Body` from a generic upstream body for responses that are not being cached
- `delete` — remove a cached entry
- `empty_body` — produce an empty `Self::Body` (used for 504 responses on `OnlyIfCached` misses)
- `body_to_bytes_stream` (behind the `streaming` feature) — adapt `Self::Body` into a `futures_util::Stream` for clients that prefer a bytes stream

The streaming approach is particularly useful for large responses where you do not want to buffer the entire body in memory on a cache hit.

## How to implement a custom backend cache manager

This guide shows examples of implementing both traditional and streaming cache managers. The traditional example is based on [`CACacheManager`](https://github.com/06chaynes/http-cache/blob/main/http-cache/src/managers/cacache.rs). The streaming example below is a **simplified illustrative implementation** to demonstrate the shape of the trait — the real [`StreamingManager`](https://github.com/06chaynes/http-cache/blob/main/http-cache/src/managers/streaming_cache.rs) uses a different design (redb for metadata, raw files for bodies with a nonce header for crash-detection, moka as an in-memory hot cache). Read that source for a production reference; there are several ways to satisfy the trait.

### Part One: The base structs

We'll show the base structs for both traditional and streaming cache managers.

For traditional caching, we'll use a simple struct that stores the cache directory path:

```rust
/// Traditional cache manager using cacache for disk-based storage
#[derive(Debug, Clone)]
pub struct CACacheManager {
    /// Directory where the cache will be stored.
    pub path: PathBuf,
    /// Options for removing cache entries.
    pub remove_opts: cacache::RemoveOpts,
}
```

For streaming caching, we'll use a struct that stores the root path for the cache directory and organizes content separately:

```rust
/// File-based streaming cache manager (illustrative)
#[derive(Debug, Clone)]
pub struct StreamingManager {
    root_path: PathBuf,
}
```

This illustrative implementation favors simplicity: metadata stored as JSON, content hashed and stored in a separate directory, no eviction logic. A production implementation — like the real `StreamingManager` — may add concerns such as crash-safety (atomic rename + fsync), an in-memory hot cache, and bounded memory use on reads. Start simple; layer concerns in as you need them.

For traditional caching, we use a simple `Store` struct that contains both the response and policy together:

```rust
/// Store struct for traditional caching
#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse,
    policy: CachePolicy,
}
```

For streaming caching, we create a metadata struct that stores response information separately from the content:

```rust
/// Metadata stored for each cached response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub status: u16,
    pub version: u8,
    pub headers: HashMap<String, String>,
    pub content_digest: String,
    pub body_size: u64,
    pub policy: CachePolicy,
    pub created_at: u64,
}
```

This struct derives [serde](https://github.com/serde-rs/serde) Deserialize and Serialize to ease the serialization and deserialization with JSON for the streaming metadata, and [postcard](https://github.com/jamesmunns/postcard) for the traditional Store struct.

**Important:** The `bincode` serialization format has been deprecated due to RUSTSEC-2025-0141 (bincode is unmaintained). New implementations should use `postcard` instead. The library still supports bincode through legacy feature flags (`manager-cacache-bincode`, `manager-moka-bincode`) for backward compatibility, but these will be removed in the next major version.

### Part Two: Implementing the traditional `CacheManager` trait

For traditional caching that stores entire response bodies, you implement just the `CacheManager` trait. Here's the `CACacheManager` implementation using the `cacache` library:

```rust
impl CACacheManager {
    /// Creates a new CACacheManager with the given path.
    pub fn new(path: PathBuf, remove_fully: bool) -> Self {
        Self {
            path,
            remove_opts: cacache::RemoveOpts::new().remove_fully(remove_fully),
        }
    }
}

impl CacheManager for CACacheManager {
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        let store: Store = match cacache::read(&self.path, cache_key).await {
            Ok(d) => postcard::from_bytes(&d)?,
            Err(_e) => {
                return Ok(None);
            }
        };
        Ok(Some((store.response, store.policy)))
    }

    async fn put(
        &self,
        cache_key: String,
        response: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse> {
        let data = Store { response, policy };
        let bytes = postcard::to_allocvec(&data)?;
        cacache::write(&self.path, cache_key, bytes).await?;
        Ok(data.response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        self.remove_opts.clone().remove(&self.path, cache_key).await?;
        Ok(())
    }
}
```

### Part Three: Implementing the `StreamingCacheManager` trait

For streaming caching that handles large responses without buffering them entirely in memory, you implement the `StreamingCacheManager` trait. It is a **separate** trait from `CacheManager` (not a supertrait extension) — a type typically implements either one or the other, not both. We'll start with the implementation signature:

```rust
impl StreamingCacheManager for StreamingManager {
    type Body = StreamingBody<Empty<Bytes>>;
    ...
```

#### Helper methods

First, let's implement some helper methods that our cache will need:

```rust
impl StreamingManager {
    /// Create a new streaming cache manager.
    pub fn new(root_path: PathBuf) -> Self {
        Self { root_path }
    }

    /// Get the path for storing metadata
    fn metadata_path(&self, key: &str) -> PathBuf {
        let encoded_key = hex::encode(key.as_bytes());
        self.root_path
            .join("cache-v2")
            .join("metadata")
            .join(format!("{encoded_key}.json"))
    }

    /// Get the path for storing content
    fn content_path(&self, digest: &str) -> PathBuf {
        self.root_path.join("cache-v2").join("content").join(digest)
    }

    /// Calculate SHA256 digest of content
    fn calculate_digest(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hex::encode(hasher.finalize())
    }
}
```

#### The streaming `get` method

The `get` method accepts a `&str` as the cache key and returns a `Result<Option<(Response<Self::Body>, CachePolicy)>>`. This method reads the metadata file to get response information, then creates a streaming body that reads directly from the cached content file without loading it into memory.

```rust
async fn get(
    &self,
    cache_key: &str,
) -> Result<Option<(Response<Self::Body>, CachePolicy)>> {
    let metadata_path = self.metadata_path(cache_key);

    // Check if metadata file exists
    if !metadata_path.exists() {
        return Ok(None);
    }

    // Read and parse metadata
    let metadata_content = tokio::fs::read(&metadata_path).await?;
    let metadata: CacheMetadata = serde_json::from_slice(&metadata_content)?;

    // Check if content file exists
    let content_path = self.content_path(&metadata.content_digest);
    if !content_path.exists() {
        return Ok(None);
    }

    // Open content file for streaming
    let file = tokio::fs::File::open(&content_path).await?;

    // Build response with streaming body
    let mut response_builder = Response::builder()
        .status(metadata.status)
        .version(/* convert from metadata.version */);

    // Add headers
    for (name, value) in &metadata.headers {
        if let (Ok(header_name), Ok(header_value)) = (
            name.parse::<http::HeaderName>(),
            value.parse::<http::HeaderValue>(),
        ) {
            response_builder = response_builder.header(header_name, header_value);
        }
    }

    // Create streaming body from file. The `from_file_with_size` constructor
    // requires the caller to have already positioned the file cursor at the
    // start of the body bytes and to supply the exact body length.
    let body_size = metadata.body_size;
    let body = StreamingBody::from_file_with_size(file, body_size);
    let response = response_builder.body(body)?;

    Ok(Some((response, metadata.policy)))
}
```

#### The streaming `put` method

The `put` method accepts a `String` as the cache key, a streaming `Response<B>`, a `CachePolicy`, and a request URL. It stores the response body content in a file and the metadata separately, enabling efficient retrieval without loading the entire response into memory.

For simplicity, this illustrative version collects the body up front (it needs the whole
byte slice to compute a content digest for dedup). **This differs from the production
`StreamingManager`**, which spools the body to its temp file frame-by-frame — at most one
frame is ever held in memory regardless of response size — and enforces `max_body_size` as
a *decline, not an error*: an oversize response simply is not cached, and the caller still
receives the full body streamed through. A production implementation following that design
would write each `Frame` to the content file as it arrives rather than calling
`body.collect()`.

```rust
async fn put<B>(
    &self,
    cache_key: String,
    response: Response<B>,
    policy: CachePolicy,
    _request_url: Url,
    _metadata: Option<Vec<u8>>,
) -> Result<Response<Self::Body>>
where
    B: http_body::Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<StreamingError>,
{
    let (parts, body) = response.into_parts();

    // Collect body content
    let collected = body.collect().await?;
    let body_bytes = collected.to_bytes();

    // Calculate content digest for deduplication
    let content_digest = Self::calculate_digest(&body_bytes);
    let content_path = self.content_path(&content_digest);

    // Ensure content directory exists and write content if not already present
    if !content_path.exists() {
        if let Some(parent) = content_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&content_path, &body_bytes).await?;
    }

    // Create metadata
    let metadata = CacheMetadata {
        status: parts.status.as_u16(),
        version: match parts.version {
            Version::HTTP_11 => 11,
            Version::HTTP_2 => 2,
            // ... other versions
            _ => 11,
        },
        headers: parts.headers.iter()
            .map(|(name, value)| {
                (name.to_string(), value.to_str().unwrap_or("").to_string())
            })
            .collect(),
        content_digest: content_digest.clone(),
        body_size: body_bytes.len() as u64,
        policy,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    // Write metadata
    let metadata_path = self.metadata_path(&cache_key);
    if let Some(parent) = metadata_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let metadata_json = serde_json::to_vec(&metadata)?;
    tokio::fs::write(&metadata_path, &metadata_json).await?;

    // Return response with buffered body for immediate use
    let response = Response::from_parts(parts, StreamingBody::buffered(body_bytes));
    Ok(response)
}
```

#### The streaming `delete` method

The `delete` method accepts a `&str` as the cache key. It removes both the metadata file and the associated content file from the cache directory.

```rust
async fn delete(&self, cache_key: &str) -> Result<()> {
    let metadata_path = self.metadata_path(cache_key);

    // Read metadata to get content digest
    if let Ok(metadata_content) = tokio::fs::read(&metadata_path).await {
        if let Ok(metadata) = serde_json::from_slice::<CacheMetadata>(&metadata_content) {
            let content_path = self.content_path(&metadata.content_digest);
            // Remove content file
            tokio::fs::remove_file(&content_path).await.ok();
        }
    }

    // Remove metadata file
    tokio::fs::remove_file(&metadata_path).await.ok();
    Ok(())
}
```

Our `StreamingManager` struct now meets the requirements of the `StreamingCacheManager` trait and provides streaming support without buffering large response bodies in memory on read.
