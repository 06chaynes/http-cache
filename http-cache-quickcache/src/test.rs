use crate::QuickManager;
use std::sync::Arc;

use http_cache::*;
use http_cache_reqwest::Cache;
use http_cache_semantics::CachePolicy;
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use url::Url;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

pub(crate) fn build_mock(
    cache_control_val: &str,
    body: &[u8],
    status: u16,
    expect: u64,
) -> Mock {
    Mock::given(method(GET))
        .respond_with(
            ResponseTemplate::new(status)
                .insert_header("cache-control", cache_control_val)
                .set_body_bytes(body),
        )
        .expect(expect)
}

const GET: &str = "GET";

const TEST_BODY: &[u8] = b"test";

const CACHEABLE_PUBLIC: &str = "max-age=86400, public";

#[tokio::test]
async fn quickcache() -> Result<()> {
    // Added to test custom Debug impl
    assert_eq!(
        format!("{:?}", QuickManager::default()),
        "QuickManager { cache: \"Cache<String, Arc<Vec<u8>>>\", .. }",
    );
    let url = Url::parse("http://example.com")?;
    let manager = Arc::new(QuickManager::default());
    let http_res = HttpResponse {
        body: TEST_BODY.to_vec(),
        headers: Default::default(),
        status: 200,
        url: url.clone(),
        version: HttpVersion::Http11,
    };
    let req = http::Request::get("http://example.com").body(())?;
    let res = http::Response::builder().status(200).body(TEST_BODY.to_vec())?;
    let policy = CachePolicy::new(&req, &res);
    CacheManager::put(
        &*manager,
        format!("{}:{}", GET, &url),
        http_res.clone(),
        policy.clone(),
    )
    .await?;
    let data =
        CacheManager::get(&*manager, &format!("{}:{}", GET, &url)).await?;
    assert!(data.is_some());
    assert_eq!(data.unwrap().0.body, TEST_BODY);
    CacheManager::delete(&*manager, &format!("{}:{}", GET, &url)).await?;
    let data =
        CacheManager::get(&*manager, &format!("{}:{}", GET, &url)).await?;
    assert!(data.is_none());
    Ok(())
}

#[tokio::test]
async fn default_mode() -> Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = QuickManager::default();

    // Construct reqwest client with cache defaults
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        }))
        .build();

    // Cold pass to load cache
    client.get(url.clone()).send().await?;

    // Try to load cached object
    let data =
        CacheManager::get(&manager, &format!("{}:{}", GET, &Url::parse(&url)?))
            .await?;
    assert!(data.is_some());

    // Hot pass to make sure the expect response was returned
    let res = client.get(url).send().await?;
    assert_eq!(res.bytes().await?, TEST_BODY);
    Ok(())
}

#[tokio::test]
async fn default_mode_with_options() -> Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = QuickManager::default();

    // Construct reqwest client with cache options override
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions {
                cache_key: None,
                cache_options: Some(CacheOptions {
                    shared: false,
                    ..Default::default()
                }),
                cache_mode_fn: None,
                cache_bust: None,
                cache_status_headers: true,
            },
        }))
        .build();

    // Cold pass to load cache
    client.get(url.clone()).send().await?;

    // Try to load cached object
    let data = http_cache::CacheManager::get(
        &manager,
        &format!("{}:{}", GET, &Url::parse(&url)?),
    )
    .await?;
    assert!(data.is_some());
    Ok(())
}

#[tokio::test]
async fn no_cache_mode() -> Result<()> {
    let mock_server = MockServer::start().await;
    let m = build_mock(CACHEABLE_PUBLIC, TEST_BODY, 200, 2);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = QuickManager::default();

    // Construct reqwest client with cache defaults
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::NoCache,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        }))
        .build();

    // Remote request and should cache
    client.get(url.clone()).send().await?;

    // Try to load cached object
    let data = http_cache::CacheManager::get(
        &manager,
        &format!("{}:{}", GET, &Url::parse(&url)?),
    )
    .await?;
    assert!(data.is_some());

    // To verify our endpoint receives the request rather than a cache hit
    client.get(url).send().await?;
    Ok(())
}

#[tokio::test]
async fn head_request_caching() -> Result<()> {
    let mock_server = MockServer::start().await;
    let m = Mock::given(method("HEAD"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", CACHEABLE_PUBLIC)
                .insert_header("content-type", "text/plain")
                .insert_header("content-length", "4"), // HEAD responses should not have a body
        )
        .expect(1);
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = QuickManager::default();

    // Construct reqwest client with cache defaults
    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        }))
        .build();

    // HEAD request should be cached
    let res = client.head(url.clone()).send().await?;
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers().get("content-type").unwrap(), "text/plain");

    // Try to load cached object - should use HEAD method in cache key
    let data = http_cache::CacheManager::get(
        &manager,
        &format!("HEAD:{}", &Url::parse(&url)?),
    )
    .await?;
    assert!(data.is_some());

    let cached_response = data.unwrap().0;
    assert_eq!(cached_response.status, 200);

    Ok(())
}

#[tokio::test]
async fn put_request_invalidates_cache() -> Result<()> {
    let mock_server = MockServer::start().await;

    // Mock GET request for caching
    let m_get = Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", CACHEABLE_PUBLIC)
                .set_body_bytes(TEST_BODY),
        )
        .expect(1);

    // Mock PUT request that should invalidate cache
    let m_put = Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1);

    let mock_guard_get = mock_server.register_as_scoped(m_get).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = QuickManager::default();

    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        }))
        .build();

    // First, cache a GET response
    client.get(url.clone()).send().await?;

    // Verify it's cached
    let data = http_cache::CacheManager::get(
        &manager,
        &format!("GET:{}", &Url::parse(&url)?),
    )
    .await?;
    assert!(data.is_some());

    drop(mock_guard_get);
    let _mock_guard_put = mock_server.register_as_scoped(m_put).await;

    // PUT request should invalidate the cached GET response
    let put_res = client.put(url.clone()).send().await?;
    assert_eq!(put_res.status(), 204);

    // Verify cache was invalidated
    let data = http_cache::CacheManager::get(
        &manager,
        &format!("GET:{}", &Url::parse(&url)?),
    )
    .await?;
    assert!(data.is_none());

    Ok(())
}

#[tokio::test]
async fn patch_request_invalidates_cache() -> Result<()> {
    let mock_server = MockServer::start().await;

    let m_get = Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", CACHEABLE_PUBLIC)
                .set_body_bytes(TEST_BODY),
        )
        .expect(1);

    let m_patch = Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1);

    let mock_guard_get = mock_server.register_as_scoped(m_get).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = QuickManager::default();

    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        }))
        .build();

    // Cache a GET response
    client.get(url.clone()).send().await?;

    // Verify it's cached
    let data = http_cache::CacheManager::get(
        &manager,
        &format!("GET:{}", &Url::parse(&url)?),
    )
    .await?;
    assert!(data.is_some());

    drop(mock_guard_get);
    let _mock_guard_patch = mock_server.register_as_scoped(m_patch).await;

    // PATCH request should invalidate cache
    let patch_res = client.patch(url.clone()).send().await?;
    assert_eq!(patch_res.status(), 200);

    // Verify cache was invalidated
    let data = http_cache::CacheManager::get(
        &manager,
        &format!("GET:{}", &Url::parse(&url)?),
    )
    .await?;
    assert!(data.is_none());

    Ok(())
}

#[tokio::test]
async fn delete_request_invalidates_cache() -> Result<()> {
    let mock_server = MockServer::start().await;

    let m_get = Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", CACHEABLE_PUBLIC)
                .set_body_bytes(TEST_BODY),
        )
        .expect(1);

    let m_delete = Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1);

    let mock_guard_get = mock_server.register_as_scoped(m_get).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = QuickManager::default();

    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        }))
        .build();

    // Cache a GET response
    client.get(url.clone()).send().await?;

    // Verify it's cached
    let data = http_cache::CacheManager::get(
        &manager,
        &format!("GET:{}", &Url::parse(&url)?),
    )
    .await?;
    assert!(data.is_some());

    drop(mock_guard_get);
    let _mock_guard_delete = mock_server.register_as_scoped(m_delete).await;

    // DELETE request should invalidate cache
    let delete_res = client.delete(url.clone()).send().await?;
    assert_eq!(delete_res.status(), 204);

    // Verify cache was invalidated
    let data = http_cache::CacheManager::get(
        &manager,
        &format!("GET:{}", &Url::parse(&url)?),
    )
    .await?;
    assert!(data.is_none());

    Ok(())
}

#[tokio::test]
async fn options_request_not_cached() -> Result<()> {
    let mock_server = MockServer::start().await;
    let m = Mock::given(method("OPTIONS"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("allow", "GET, POST, PUT, DELETE")
                .insert_header("cache-control", CACHEABLE_PUBLIC), // Even with cache headers
        )
        .expect(2); // Should be called twice since not cached
    let _mock_guard = mock_server.register_as_scoped(m).await;
    let url = format!("{}/", &mock_server.uri());
    let manager = QuickManager::default();

    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        }))
        .build();

    // First OPTIONS request
    let res1 =
        client.request(reqwest::Method::OPTIONS, url.clone()).send().await?;
    assert_eq!(res1.status(), 200);

    // Verify it's not cached
    let data = http_cache::CacheManager::get(
        &manager,
        &format!("OPTIONS:{}", &Url::parse(&url)?),
    )
    .await?;
    assert!(data.is_none());

    // Second OPTIONS request should hit the server again
    let res2 =
        client.request(reqwest::Method::OPTIONS, url.clone()).send().await?;
    assert_eq!(res2.status(), 200);

    Ok(())
}
