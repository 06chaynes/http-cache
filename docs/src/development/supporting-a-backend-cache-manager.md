# Supporting a Backend Cache Manager

This section is intended for those looking to implement a custom backend cache manager, or understand how the [`CacheManager`](https://docs.rs/http-cache/latest/http_cache/trait.CacheManager.html) and [`StreamingCacheManager`](https://docs.rs/http-cache/latest/http_cache/trait.StreamingCacheManager.html) traits work.

## The `CacheManager` trait

The [`CacheManager`](https://docs.rs/http-cache/latest/http_cache/trait.CacheManager.html) trait is the main trait that needs to be implemented to support a new backend cache manager. It has three methods that it requires:

- `get`: retrieve a cached response given the provided cache key
- `put`: store a response and related policy object in the cache associated with the provided cache key
- `delete`: remove a cached response from the cache associated with the provided cache key

Because the methods are asynchronous, they currently require [`async_trait`](https://github.com/dtolnay/async-trait) to be derived. This may change in the future.

### The `get` method

The `get` method is used to retrieve a cached response given the provided cache key. It returns an `Result<Option<(HttpResponse, CachePolicy)>, BoxError>` where `HttpResponse` is the cached response and [`CachePolicy`](https://docs.rs/http-cache-semantics/latest/http_cache_semantics/struct.CachePolicy.html) is the associated cache policy object that provides us helpful metadata. If the cache key does not exist in the cache, `Ok(None)` is returned.

### The `put` method

The `put` method is used to store a response and related policy object in the cache associated with the provided cache key. It returns an `Result<HttpResponse, BoxError>` where `HttpResponse` is the passed response.

### The `delete` method

The `delete` method is used to remove a cached response from the cache associated with the provided cache key. It returns an `Result<(), BoxError>`.

## The `StreamingCacheManager` trait

The [`StreamingCacheManager`](https://docs.rs/http-cache/latest/http_cache/trait.StreamingCacheManager.html) trait extends the traditional `CacheManager` to support streaming operations for memory-efficient handling of large responses. It includes all the methods from `CacheManager` plus additional streaming-specific methods:

- `get_stream`: retrieve a cached response as a stream given the provided cache key
- `put_stream`: store a streaming response in the cache associated with the provided cache key
- `stream_response`: create a streaming response body from cached data

The streaming approach is particularly useful for large responses where you don't want to buffer the entire response body in memory.

## How to implement a custom backend cache manager

This guide shows examples of implementing both traditional and streaming cache managers. We'll use the [`CACacheManager`](https://github.com/06chaynes/http-cache/blob/main/http-cache/src/managers/cacache.rs) as an example of implementing the `CacheManager` trait for traditional disk-based caching, and the [`StreamingManager`](https://github.com/06chaynes/http-cache/blob/main/http-cache/src/managers/streaming_cache.rs) as an example of implementing the `StreamingManager` trait for streaming support that stores response metadata and body content separately to enable memory-efficient handling of large responses. There are several ways to accomplish this, so feel free to experiment!

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
/// File-based streaming cache manager
#[derive(Debug, Clone)]
pub struct StreamingManager {
    root_path: PathBuf,
    ref_counter: ContentRefCounter,
    config: StreamingCacheConfig,
}
```

The `StreamingManager` follows a **"simple and reliable"** design philosophy:

- **Focused functionality**: Core streaming operations without unnecessary complexity
- **Simple configuration**: Minimal options with sensible defaults
- **Predictable behavior**: Straightforward LRU eviction and error handling
- **Easy maintenance**: Clean code paths for debugging and troubleshooting

This approach prioritizes maintainability and reliability over feature completeness, making it easier to understand, debug, and extend.

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
    pub policy: CachePolicy,
    pub created_at: u64,
}
```

This struct derives [serde](https://github.com/serde-rs/serde) Deserialize and Serialize to ease the serialization and deserialization with JSON for the streaming metadata, and [bincode](https://github.com/bincode-org/bincode) for the traditional Store struct.

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

#[async_trait::async_trait]
impl CacheManager for CACacheManager {
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        let store: Store = match cacache::read(&self.path, cache_key).await {
            Ok(d) => bincode::deserialize(&d)?,
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
        let bytes = bincode::serialize(&data)?;
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

For streaming caching that handles large responses without buffering them entirely in memory, you implement the `StreamingCacheManager` trait. The `StreamingCacheManager` trait extends `CacheManager` with streaming-specific methods. We'll start with the implementation signature, but first we must make sure we derive async_trait.

```rust
#[async_trait::async_trait]
impl StreamingCacheManager for StreamingManager {
    type Body = StreamingBody<Empty<Bytes>>;
    ...
```

#### Helper methods

First, let's implement some helper methods that our cache will need:

```rust
impl StreamingManager {
    /// Create a new streaming cache manager with default configuration
    pub fn new(root_path: PathBuf) -> Self {
        Self::new_with_config(root_path, StreamingCacheConfig::default())
    }

    /// Create a new streaming cache manager with custom configuration
    pub fn new_with_config(
        root_path: PathBuf,
        config: StreamingCacheConfig,
    ) -> Self {
        Self { 
            root_path, 
            ref_counter: ContentRefCounter::new(), 
            config 
        }
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

    // Create streaming body from file
    let body = StreamingBody::from_file(file);
    let response = response_builder.body(body)?;

    Ok(Some((response, metadata.policy)))
}
```

#### The streaming `put` method

The `put` method accepts a `String` as the cache key, a streaming `Response<B>`, a `CachePolicy`, and a request URL. It stores the response body content in a file and the metadata separately, enabling efficient retrieval without loading the entire response into memory.

```rust
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

Our `StreamingManager` struct now meets the requirements of both the `CacheManager` and `StreamingCacheManager` traits and provides streaming support without buffering large response bodies in memory!
