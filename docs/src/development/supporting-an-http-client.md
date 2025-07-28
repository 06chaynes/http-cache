# Supporting an HTTP Client

This section is intended for those who wish to add support for a new HTTP client to `http-cache`, or understand how the [`Middleware`](https://docs.rs/http-cache/latest/http_cache/trait.Middleware.html) trait works. If you are looking to use `http-cache` with an HTTP client that is already supported, please see the [Client Implementations](../clients/clients.md) section.

The ecosystem supports both traditional caching (where entire response bodies are buffered) and streaming caching (for memory-efficient handling of large responses). The Tower implementation provides the most comprehensive streaming support.

## The `Middleware` trait

The [`Middleware`](https://docs.rs/http-cache/latest/http_cache/trait.Middleware.html) trait is the main trait that needs to be implemented to add support for a new HTTP client. It has nine methods that it requires:

- `is_method_get_head`: returns `true` if the method of the request is `GET` or `HEAD`, `false` otherwise
- `policy`: returns a [`CachePolicy`](https://docs.rs/http-cache-semantics/latest/http_cache_semantics/struct.CachePolicy.html) with default options for the given `HttpResponse`
- `policy_with_options`: returns a [`CachePolicy`](https://docs.rs/http-cache-semantics/latest/http_cache_semantics/struct.CachePolicy.html) with the provided [`CacheOptions`](https://docs.rs/http-cache-semantics/latest/http_cache_semantics/struct.CacheOptions.html) for the given `HttpResponse`
- `update_headers`: updates the request headers with the provided [`http::request::Parts`](https://docs.rs/http/latest/http/request/struct.Parts.html)
- `force_no_cache`: overrides the `Cache-Control` header to 'no-cache' derective
- `parts`: returns the [`http::request::Parts`](https://docs.rs/http/latest/http/request/struct.Parts.html) from the request
- `url`: returns the requested [`Url`](https://docs.rs/url/latest/url/struct.Url.html)
- `method`: returns the method of the request as a `String`
- `remote_fetch`: performs the request and returns the `HttpResponse`

Because the `remote_fetch` method is asynchronous, it currently requires [`async_trait`](https://github.com/dtolnay/async-trait) to be derived. This may change in the future.

### The `is_method_get_head` method

The `is_method_get_head` method is used to determine if the method of the request is `GET` or `HEAD`. It returns a `bool` where `true` indicates the method is `GET` or `HEAD`, and `false` if otherwise.

### The `policy` and `policy_with_options` methods

The `policy` method is used to generate the cache policy for the given `HttpResponse`. It returns a [`CachePolicy`](https://docs.rs/http-cache-semantics/latest/http_cache_semantics/struct.CachePolicy.html) with default options.

The `policy_with_options` method is used to generate the cache policy for the given `HttpResponse` with the provided [`CacheOptions`](https://docs.rs/http-cache-semantics/latest/http_cache_semantics/struct.CacheOptions.html). It returns a [`CachePolicy`](https://docs.rs/http-cache-semantics/latest/http_cache_semantics/struct.CachePolicy.html).

### The `update_headers` method

The `update_headers` method is used to update the request headers with the provided [`http::request::Parts`](https://docs.rs/http/latest/http/request/struct.Parts.html).

### The `force_no_cache` method

The `force_no_cache` method is used to override the `Cache-Control` header to 'no-cache' derective. This is used to allow caching but force revalidation before resuse.

### The `parts` method

The `parts` method is used to return the [`http::request::Parts`](https://docs.rs/http/latest/http/request/struct.Parts.html) from the request which eases working with the `http_cache_semantics` crate.

### The `url` method

The `url` method is used to return the requested [`Url`](https://docs.rs/url/latest/url/struct.Url.html) in a standard format.

### The `method` method

The `method` method is used to return the HTTP method of the request as a `String` to standardize the format.

### The `remote_fetch` method

The `remote_fetch` method is used to perform the request and return the `HttpResponse`. This goal here is to abstract away the HTTP client implementation and return a more generic response type.

## How to implement a custom HTTP client

This guide will use the [`surf`](https://github.com/http-rs/surf) HTTP client as an example. The full source can be found [here](https://github.com/06chaynes/http-cache/blob/main/http-cache-surf/src/lib.rs). There are several ways to accomplish this, so feel free to experiment!

### Part One: The base structs

First we will create a wrapper for the [`HttpCache`](https://docs.rs/http-cache/latest/http_cache/struct.HttpCache.html) struct. This is required because we cannot implement a trait for a type declared in another crate, see [docs](https://doc.rust-lang.org/error_codes/E0117.html) for more info. We will call it `Cache` in this case.

```rust
#[derive(Debug)]
pub struct Cache<T: CacheManager>(pub HttpCache<T>);
```

Next we will create a struct to store the request and anything else we will need for our `surf::Middleware` implementation (more on that later). This struct will also implement the http-cache [`Middleware`](https://docs.rs/http-cache/latest/http_cache/trait.Middleware.html) trait. We'll call it `SurfMiddleware` in this case.

```rust
pub(crate) struct SurfMiddleware<'a> {
    pub req: Request,
    pub client: Client,
    pub next: Next<'a>,
}
```

### Part Two: Implementing the `Middleware` trait

Now that we have our base structs, we can implement the `Middleware` trait for our `SurfMiddleware` struct. We'll start with the `is_method_get_head` method, but first we must make sure we derive async_trait.

```rust
#[async_trait::async_trait]
impl Middleware for SurfMiddleware<'_> {
    ...
```

The `is_method_get_head` will check the request stored in our `SurfMiddleware` struct and return `true` if the method is `GET` or `HEAD`, `false` otherwise.

```rust
fn is_method_get_head(&self) -> bool {
    self.req.method() == Method::Get || self.req.method() == Method::Head
}
```

Next we'll implement the `policy` method. This method accepts a reference to the `HttpResponse` and returns a [`CachePolicy`](https://docs.rs/http-cache-semantics/latest/http_cache_semantics/struct.CachePolicy.html) with default options. We'll use the [`http_cache_semantics::CachePolicy::new`](https://docs.rs/http-cache-semantics/latest/http_cache_semantics/struct.CachePolicy.html#method.new) method to generate the policy.

```rust
fn policy(&self, response: &HttpResponse) -> Result<CachePolicy> {
    Ok(CachePolicy::new(&self.parts()?, &response.parts()?))
}
```

The `policy_with_options` method is similar to the `policy` method, but accepts a [`CacheOptions`](https://docs.rs/http-cache-semantics/latest/http_cache_semantics/struct.CacheOptions.html) struct to override the default options. We'll use the [`http_cache_semantics::CachePolicy::new_options`](https://docs.rs/http-cache-semantics/latest/http_cache_semantics/struct.CachePolicy.html#method.new_options) method to generate the policy.

```rust
fn policy_with_options(
    &self,
    response: &HttpResponse,
    options: CacheOptions,
) -> Result<CachePolicy> {
    Ok(CachePolicy::new_options(
        &self.parts()?,
        &response.parts()?,
        SystemTime::now(),
        options,
    ))
}
```

Next we'll implement the `update_headers` method. This method accepts a reference to the [`http::request::Parts`](https://docs.rs/http/latest/http/request/struct.Parts.html) and updates the request headers. We will iterate over the part headers and attempt to convert the header value to a [`HeaderValue`](https://docs.rs/http/latest/http/header/struct.HeaderValue.html) and set the header on the request. If the conversion fails, we will return an error.

```rust
fn update_headers(&mut self, parts: &Parts) -> Result<()> {
    for header in parts.headers.iter() {
        let value = match HeaderValue::from_str(header.1.to_str()?) {
            Ok(v) => v,
            Err(_e) => return Err(Box::new(BadHeader)),
        };
        self.req.set_header(header.0.as_str(), value);
    }
    Ok(())
}
```

The `force_no_cache` method is used to override the `Cache-Control` header in the request to 'no-cache' derective. This is used to allow caching but force revalidation before resuse.

```rust
fn force_no_cache(&mut self) -> Result<()> {
    self.req.insert_header(CACHE_CONTROL.as_str(), "no-cache");
    Ok(())
}
```

The `parts` method is used to return the [`http::request::Parts`](https://docs.rs/http/latest/http/request/struct.Parts.html) from the request which eases working with the `http_cache_semantics` crate.

```rust
fn parts(&self) -> Result<Parts> {
    let mut converted = request::Builder::new()
        .method(self.req.method().as_ref())
        .uri(self.req.url().as_str())
        .body(())?;
    {
        let headers = converted.headers_mut();
        for header in self.req.iter() {
            headers.insert(
                http::header::HeaderName::from_str(header.0.as_str())?,
                http::HeaderValue::from_str(header.1.as_str())?,
            );
        }
    }
    Ok(converted.into_parts().0)
}
```

The `url` method is used to return the requested [`Url`](https://docs.rs/url/latest/url/struct.Url.html) in a standard format.

```rust
fn url(&self) -> Result<Url> {
    Ok(self.req.url().clone())
}
```

The `method` method is used to return the HTTP method of the request as a `String` to standardize the format.

```rust
fn method(&self) -> Result<String> {
    Ok(self.req.method().as_ref().to_string())
}
```

Finally, the `remote_fetch` method is used to perform the request and return the `HttpResponse`.

```rust
async fn remote_fetch(&mut self) -> Result<HttpResponse> {
    let url = self.req.url().clone();
    let mut res =
        self.next.run(self.req.clone(), self.client.clone()).await?;
    let mut headers = HashMap::new();
    for header in res.iter() {
        headers.insert(
            header.0.as_str().to_owned(),
            header.1.as_str().to_owned(),
        );
    }
    let status = res.status().into();
    let version = res.version().unwrap_or(Version::Http1_1);
    let body: Vec<u8> = res.body_bytes().await?;
    Ok(HttpResponse {
        body,
        headers,
        status,
        url,
        version: version.try_into()?,
    })
}
```

Our `SurfMiddleware` struct now meets the requirements of the `Middleware` trait. We can now implement the [`surf::middleware::Middleware`](https://docs.rs/surf/latest/surf/middleware/trait.Middleware.html) trait for our `Cache` struct.

### Part Three: Implementing the `surf::middleware::Middleware` trait

We have our `Cache` struct that wraps our `HttpCache` struct, but we need to implement the [`surf::middleware::Middleware`](https://docs.rs/surf/latest/surf/middleware/trait.Middleware.html) trait for it. This is required to use our `Cache` struct as a middleware with `surf`. This part may differ depending on the HTTP client you are supporting.

```rust
#[surf::utils::async_trait]
impl<T: CacheManager> surf::middleware::Middleware for Cache<T> {
    async fn handle(
        &self,
        req: Request,
        client: Client,
        next: Next<'_>,
    ) -> std::result::Result<surf::Response, http_types::Error> {
        let middleware = SurfMiddleware { req, client, next };
        let res = self.0.run(middleware).await.map_err(to_http_types_error)?;
        let mut converted = Response::new(StatusCode::Ok);
        for header in &res.headers {
            let val = HeaderValue::from_bytes(header.1.as_bytes().to_vec())?;
            converted.insert_header(header.0.as_str(), val);
        }
        converted.set_status(res.status.try_into()?);
        converted.set_version(Some(res.version.try_into()?));
        converted.set_body(res.body);
        Ok(surf::Response::from(converted))
    }
}
```

First we create a [`SurfMiddleware`](#part-two-implementing-the-middleware-trait) struct with the provided `req`, `client`, and `next` arguments. Then we call the `run` method on our `HttpCache` struct with our `SurfMiddleware` struct as the argument. This will perform the request and return the `HttpResponse`. We then convert the `HttpResponse` to a `surf::Response` and return it.
