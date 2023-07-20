# Supporting a Backend Cache Manager

This section is intended for those looking to implement a custom backend cache manager, or understand how the [`CacheManager`](https://docs.rs/http-cache/latest/http_cache/trait.CacheManager.html) trait works.

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

## How to implement a custom backend cache manager

This guide will use the [`cacache`](https://github.com/zkat/cacache-rs) backend cache manager as an example. The full source can be found [here](https://github.com/06chaynes/http-cache/blob/latest/http-cache/src/managers/cacache.rs). There are several ways to accomplish this, so feel free to experiment!

### Part One: The base structs

The first step is to create a struct that will hold the cache manager's configuration or potentially the cache itself. This struct will implement the `CacheManager` trait. In this case, we'll call it `CACacheManager` and it will have a field to store the path for the cache directory.

```rust
#[derive(Debug, Clone)]
pub struct CACacheManager {
    /// Directory where the cache will be stored.
    pub path: PathBuf,
}
```

Next we will create a struct to store the response and accompanying policy object. This struct will be used to store the response and policy object in the cache. We'll call it `Store`. This isn't strictly necessary, but I find this easier to work with.

```rust
#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse,
    policy: CachePolicy,
}
```

This struct will also derive [serde](https://github.com/serde-rs/serde) Deserialize and Serialize to ease the serialization and deserialization with [bincode](https://github.com/bincode-org/bincode).

### Part Two: Implementing the `CacheManager` trait

Now that we have our base structs, we can implement the `CacheManager` trait for our `CACacheManager` struct. We'll start with the `get` method, but first we must make sure we derive async_trait.

```rust
#[async_trait::async_trait]
impl CacheManager for CACacheManager {
    ...
```

The `get` method accepts a `&str` as the cache key and returns an `Result<Option<(HttpResponse, CachePolicy)>, BoxError>`. We will [`read`](https://docs.rs/cacache/latest/cacache/fn.read.html) function from `cacache` to lookup the cache key in the cache directory. If the cache key does not exist, we'll return `Ok(None)`. The object we will be serializing and deserializing is our `Store` struct.

```rust
...
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
...
```

Next we'll implement the `put` method. This method accepts a `String` as the cache key, a `HttpResponse` as the response, and a `CachePolicy` as the policy object. It returns an `Result<HttpResponse, BoxError>`. We will clone the response during our construction of the `Store` struct, then serialize the `Store` struct using [serialize](https://docs.rs/bincode/latest/bincode/fn.serialize.html) and write it to the cache directory using [`write`](https://docs.rs/cacache/latest/cacache/fn.write.html) from `cacache`.

```rust
...
async fn put(
    &self,
    cache_key: String,
    response: HttpResponse,
    policy: CachePolicy,
) -> Result<HttpResponse> {
    let data = Store { response: response.clone(), policy };
    let bytes = bincode::serialize(&data)?;
    cacache::write(&self.path, cache_key, bytes).await?;
    Ok(response)
}
...
```

Finally we'll implement the `delete` method. This method accepts a `&str` as the cache key and returns an `Result<(), BoxError>`. We will use [`remove`](https://docs.rs/cacache/latest/cacache/fn.remove.html) from `cacache` to remove the object from the cache directory.

```rust
...
async fn delete(&self, cache_key: &str) -> Result<()> {
    Ok(cacache::remove(&self.path, cache_key).await?)
}
...
```

Our `CACacheManager` struct now meets the requirements of the `CacheManager` trait and is ready for use!
