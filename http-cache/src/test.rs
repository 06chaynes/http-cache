use crate::{
    error, url_parse, CacheMode, HitOrMiss, HttpHeaders, HttpResponse,
    HttpVersion, Result, Url,
};
use http::{header::CACHE_CONTROL, StatusCode};

use std::str::FromStr;

const GET: &str = "GET";
const TEST_BODY: &[u8] = b"test";

#[test]
fn hit_miss() -> Result<()> {
    // Testing the Debug, Display, and Clone traits for the HitOrMiss ebnum
    let hit = HitOrMiss::HIT;
    assert_eq!(format!("{:?}", hit.clone()), "HIT");
    assert_eq!(hit.to_string(), "HIT".to_string(),);
    let miss = HitOrMiss::MISS;
    assert_eq!(format!("{:?}", miss.clone()), "MISS");
    assert_eq!(miss.to_string(), "MISS".to_string(),);
    Ok(())
}

#[test]
fn cache_mode() -> Result<()> {
    // Testing the Debug and Clone traits for the CacheMode enum
    let mode = CacheMode::Default;
    assert_eq!(mode.clone(), CacheMode::Default);
    assert_eq!(format!("{mode:?}"), "Default");
    Ok(())
}

#[test]
#[allow(clippy::default_constructed_unit_structs)]
fn test_errors() -> Result<()> {
    // Testing the Debug, Default, Display and Clone traits for the error types
    let bv = error::BadVersion::default();
    assert_eq!(format!("{:?}", bv.clone()), "BadVersion",);
    assert_eq!(bv.to_string(), "Unknown HTTP version".to_string(),);
    let bh = error::BadHeader::default();
    assert_eq!(format!("{:?}", bh.clone()), "BadHeader",);
    assert_eq!(bh.to_string(), "Error parsing header value".to_string(),);
    Ok(())
}

#[test]
fn response_methods_work() -> Result<()> {
    let url = Url::from_str("http://example.com")?;
    let mut res = HttpResponse {
        body: TEST_BODY.to_vec(),
        headers: HttpHeaders::new(),
        status: 200,
        url: url.clone(),
        version: HttpVersion::Http11,
        metadata: Some(b"Metadata".to_vec()),
    };
    // Verify debug output contains expected fields without checking exact format
    // (URL debug representation differs between url and ada-url crates)
    let debug = format!("{:?}", res.clone());
    assert!(debug.contains("HttpResponse"));
    assert!(debug.contains("body: [116, 101, 115, 116]"));
    assert!(debug.contains("status: 200"));
    assert!(debug.contains("example.com"));
    assert!(debug.contains("Http11"));
    res.add_warning(&url, 112, "Test Warning");
    let code = res.warning_code();
    assert!(code.is_some());
    assert_eq!(code.unwrap(), 112);
    res.remove_warning();
    let code = res.warning_code();
    assert!(code.is_none());
    let http_res = http::Response::builder()
        .header(CACHE_CONTROL.as_str(), "must-revalidate")
        .status(StatusCode::OK)
        .body(())?;
    let parts = http_res.into_parts().0;
    let cloned_headers = parts.headers.clone();
    res.update_headers(&parts)?;
    assert!(res.must_revalidate());
    assert_eq!(res.parts()?.headers, cloned_headers);
    res.headers.remove(CACHE_CONTROL.as_str());
    assert!(!res.must_revalidate());
    Ok(())
}

#[test]
fn version_http() -> Result<()> {
    assert_eq!(format!("{:?}", HttpVersion::Http09), "Http09");
    assert_eq!(format!("{}", HttpVersion::Http09), "HTTP/0.9");
    assert_eq!(format!("{:?}", HttpVersion::Http10), "Http10");
    assert_eq!(format!("{}", HttpVersion::Http10), "HTTP/1.0");
    assert_eq!(format!("{:?}", HttpVersion::Http11), "Http11");
    assert_eq!(format!("{}", HttpVersion::Http11), "HTTP/1.1");
    assert_eq!(format!("{:?}", HttpVersion::H2), "H2");
    assert_eq!(format!("{}", HttpVersion::H2), "HTTP/2.0");
    assert_eq!(format!("{:?}", HttpVersion::H3), "H3");
    assert_eq!(format!("{}", HttpVersion::H3), "HTTP/3.0");
    Ok(())
}

#[test]
fn can_convert_versions_from_http() -> Result<()> {
    let v: HttpVersion = http::Version::HTTP_09.try_into()?;
    assert_eq!(v, HttpVersion::Http09);
    let v: http::Version = HttpVersion::Http09.into();
    assert_eq!(v, http::Version::HTTP_09);

    let v: HttpVersion = http::Version::HTTP_10.try_into()?;
    assert_eq!(v, HttpVersion::Http10);
    let v: http::Version = HttpVersion::Http10.into();
    assert_eq!(v, http::Version::HTTP_10);

    let v: HttpVersion = http::Version::HTTP_11.try_into()?;
    assert_eq!(v, HttpVersion::Http11);
    let v: http::Version = HttpVersion::Http11.into();
    assert_eq!(v, http::Version::HTTP_11);

    let v: HttpVersion = http::Version::HTTP_2.try_into()?;
    assert_eq!(v, HttpVersion::H2);
    let v: http::Version = HttpVersion::H2.into();
    assert_eq!(v, http::Version::HTTP_2);

    let v: HttpVersion = http::Version::HTTP_3.try_into()?;
    assert_eq!(v, HttpVersion::H3);
    let v: http::Version = HttpVersion::H3.into();
    assert_eq!(v, http::Version::HTTP_3);
    Ok(())
}

#[cfg(all(test, feature = "with-http-types"))]
mod with_http_types {
    use super::*;

    #[test]
    fn can_convert_versions_from_http_types() -> Result<()> {
        let v: HttpVersion = http_types::Version::Http0_9.try_into()?;
        assert_eq!(v, HttpVersion::Http09);
        let v: http_types::Version = HttpVersion::Http09.into();
        assert_eq!(v, http_types::Version::Http0_9);

        let v: HttpVersion = http_types::Version::Http1_0.try_into()?;
        assert_eq!(v, HttpVersion::Http10);
        let v: http_types::Version = HttpVersion::Http10.into();
        assert_eq!(v, http_types::Version::Http1_0);

        let v: HttpVersion = http_types::Version::Http1_1.try_into()?;
        assert_eq!(v, HttpVersion::Http11);
        let v: http_types::Version = HttpVersion::Http11.into();
        assert_eq!(v, http_types::Version::Http1_1);

        let v: HttpVersion = http_types::Version::Http2_0.try_into()?;
        assert_eq!(v, HttpVersion::H2);
        let v: http_types::Version = HttpVersion::H2.into();
        assert_eq!(v, http_types::Version::Http2_0);

        let v: HttpVersion = http_types::Version::Http3_0.try_into()?;
        assert_eq!(v, HttpVersion::H3);
        let v: http_types::Version = HttpVersion::H3.into();
        assert_eq!(v, http_types::Version::Http3_0);
        Ok(())
    }
}

#[cfg(feature = "manager-cacache")]
mod with_cacache {

    use super::*;
    use crate::{CACacheManager, CacheManager};

    use http_cache_semantics::CachePolicy;

    #[tokio::test]
    async fn cacache() -> Result<()> {
        let url = url_parse("http://example.com")?;
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);
        let http_res = HttpResponse {
            body: TEST_BODY.to_vec(),
            headers: Default::default(),
            status: 200,
            url: url.clone(),
            version: HttpVersion::Http11,
            metadata: Some(b"Metadata".to_vec()),
        };
        let req = http::Request::get("http://example.com").body(())?;
        let res =
            http::Response::builder().status(200).body(TEST_BODY.to_vec())?;
        let policy = CachePolicy::new(&req, &res);
        manager
            .put(format!("{}:{}", GET, &url), http_res.clone(), policy.clone())
            .await?;
        let data = manager.get(&format!("{}:{}", GET, &url)).await?;
        assert!(data.is_some());
        let test_data = data.unwrap();
        assert_eq!(test_data.0.body, TEST_BODY);
        assert_eq!(test_data.0.metadata, Some(b"Metadata".to_vec()));
        let clone = manager.clone();
        let clonedata = clone.get(&format!("{}:{}", GET, &url)).await?;
        assert!(clonedata.is_some());
        assert_eq!(clonedata.unwrap().0.body, TEST_BODY);
        manager.delete(&format!("{}:{}", GET, &url)).await?;
        let data = manager.get(&format!("{}:{}", GET, &url)).await?;
        assert!(data.is_none());

        manager.put(format!("{}:{}", GET, &url), http_res, policy).await?;
        manager.clear().await?;
        let data = manager.get(&format!("{}:{}", GET, &url)).await?;
        assert!(data.is_none());
        Ok(())
    }
}

#[cfg(feature = "manager-moka")]
mod with_moka {
    use super::*;
    use crate::{CacheManager, MokaManager};

    use http_cache_semantics::CachePolicy;
    use std::sync::Arc;

    #[tokio::test]
    async fn moka() -> Result<()> {
        // Added to test custom Debug impl
        let mm = MokaManager::default();
        assert_eq!(format!("{:?}", mm.clone()), "MokaManager { .. }",);
        let url = url_parse("http://example.com")?;
        let manager = Arc::new(mm);
        let http_res = HttpResponse {
            body: TEST_BODY.to_vec(),
            headers: Default::default(),
            status: 200,
            url: url.clone(),
            version: HttpVersion::Http11,
            metadata: Some(b"Metadata".to_vec()),
        };
        let req = http::Request::get("http://example.com").body(())?;
        let res =
            http::Response::builder().status(200).body(TEST_BODY.to_vec())?;
        let policy = CachePolicy::new(&req, &res);
        manager
            .put(format!("{}:{}", GET, &url), http_res.clone(), policy.clone())
            .await?;
        let data = manager.get(&format!("{}:{}", GET, &url)).await?;
        assert!(data.is_some());
        let response = data.unwrap();
        assert_eq!(response.0.body, TEST_BODY);
        assert_eq!(response.0.metadata, Some(b"Metadata".to_vec()));
        let clone = manager.clone();
        let clonedata = clone.get(&format!("{}:{}", GET, &url)).await?;
        assert!(clonedata.is_some());
        let response = clonedata.unwrap();
        assert_eq!(response.0.body, TEST_BODY);
        assert_eq!(response.0.metadata, Some(b"Metadata".to_vec()));
        manager.delete(&format!("{}:{}", GET, &url)).await?;
        let data = manager.get(&format!("{}:{}", GET, &url)).await?;
        assert!(data.is_none());

        manager.put(format!("{}:{}", GET, &url), http_res, policy).await?;
        manager.clear().await?;
        let data = manager.get(&format!("{}:{}", GET, &url)).await?;
        assert!(data.is_none());
        Ok(())
    }
}

#[cfg(feature = "manager-foyer")]
mod with_foyer {
    use super::*;
    use crate::{CacheManager, FoyerManager};

    use http_cache_semantics::CachePolicy;
    use std::sync::Arc;

    #[tokio::test]
    async fn foyer() -> Result<()> {
        // Added to test custom Debug impl
        let fm = FoyerManager::in_memory(100).await?;
        assert_eq!(format!("{:?}", fm.clone()), "FoyerManager { .. }",);
        let url = url_parse("http://example.com")?;
        let manager = Arc::new(fm);
        let http_res = HttpResponse {
            body: TEST_BODY.to_vec(),
            headers: Default::default(),
            status: 200,
            url: url.clone(),
            version: HttpVersion::Http11,
            metadata: Some(b"Metadata".to_vec()),
        };
        let req = http::Request::get("http://example.com").body(())?;
        let res =
            http::Response::builder().status(200).body(TEST_BODY.to_vec())?;
        let policy = CachePolicy::new(&req, &res);
        manager
            .put(format!("{}:{}", GET, &url), http_res.clone(), policy.clone())
            .await?;
        let data = manager.get(&format!("{}:{}", GET, &url)).await?;
        assert!(data.is_some());
        let response = data.unwrap();
        assert_eq!(response.0.body, TEST_BODY);
        assert_eq!(response.0.metadata, Some(b"Metadata".to_vec()));
        let clone = manager.clone();
        let clonedata = clone.get(&format!("{}:{}", GET, &url)).await?;
        assert!(clonedata.is_some());
        let response = clonedata.unwrap();
        assert_eq!(response.0.body, TEST_BODY);
        assert_eq!(response.0.metadata, Some(b"Metadata".to_vec()));
        manager.delete(&format!("{}:{}", GET, &url)).await?;
        let data = manager.get(&format!("{}:{}", GET, &url)).await?;
        assert!(data.is_none());

        // Note: FoyerManager doesn't have a clear() method like cacache/moka
        // since foyer handles eviction internally
        Ok(())
    }
}

#[cfg(feature = "manager-cacache")]
mod interface_tests {
    use crate::{
        url_parse, CACacheManager, CacheMode, HttpCache, HttpCacheInterface,
        HttpCacheOptions,
    };
    use http::{Request, Response, StatusCode};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_http_cache_interface_analyze_request() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);
        let cache = HttpCache {
            mode: CacheMode::Default,
            manager,
            options: HttpCacheOptions::default(),
        };

        // Test GET request (should be cacheable)
        let req = Request::builder()
            .method("GET")
            .uri("https://example.com/test")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let analysis = cache.analyze_request(&parts, None).unwrap();
        assert!(analysis.should_cache);
        assert!(!analysis.cache_key.is_empty());
        assert_eq!(analysis.cache_mode, CacheMode::Default);

        // Test POST request (should not be cacheable by default)
        let req = Request::builder()
            .method("POST")
            .uri("https://example.com/test")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let analysis = cache.analyze_request(&parts, None).unwrap();
        assert!(!analysis.should_cache);

        // Temporary directory will be automatically cleaned up when dropped
    }

    #[tokio::test]
    async fn test_http_cache_interface_lookup_and_process() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);
        let cache = HttpCache {
            mode: CacheMode::Default,
            manager,
            options: HttpCacheOptions::default(),
        };

        // Test cache miss
        let result =
            cache.lookup_cached_response("nonexistent_key").await.unwrap();
        assert!(result.is_none());

        // Create a response to cache
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("cache-control", "max-age=3600")
            .header("content-type", "text/plain")
            .body(b"Hello, world!".to_vec())
            .unwrap();

        // Analyze a request for this response
        let req = Request::builder()
            .method("GET")
            .uri("https://example.com/test")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();
        let analysis = cache.analyze_request(&parts, None).unwrap();

        // Process the response (should cache it)
        let processed = cache
            .process_response(
                analysis.clone(),
                response,
                Some(b"Metadata".to_vec()),
            )
            .await
            .unwrap();
        assert_eq!(processed.status(), StatusCode::OK);

        // Try to look up the cached response
        let cached =
            cache.lookup_cached_response(&analysis.cache_key).await.unwrap();
        assert!(cached.is_some());

        let (cached_response, _policy) = cached.unwrap();
        assert_eq!(cached_response.status, StatusCode::OK);
        assert_eq!(cached_response.body, b"Hello, world!");
        assert_eq!(cached_response.metadata, Some(b"Metadata".to_vec()));

        // Temporary directory will be automatically cleaned up when dropped
    }

    #[tokio::test]
    async fn test_http_cache_interface_conditional_requests() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);
        let cache = HttpCache {
            mode: CacheMode::Default,
            manager,
            options: HttpCacheOptions::default(),
        };

        // Create and cache a response with an ETag
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("cache-control", "max-age=3600")
            .header("etag", "\"123456\"")
            .body(b"Hello, world!".to_vec())
            .unwrap();

        let req = Request::builder()
            .method("GET")
            .uri("https://example.com/test")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();
        let analysis = cache.analyze_request(&parts, None).unwrap();

        // Cache the response
        let _processed = cache
            .process_response(
                analysis.clone(),
                response,
                Some(b"Metadata".to_vec()),
            )
            .await
            .unwrap();

        // Look up the cached response
        let cached =
            cache.lookup_cached_response(&analysis.cache_key).await.unwrap();
        assert!(cached.is_some());

        let (cached_response, policy) = cached.unwrap();

        // Test preparing conditional request
        let mut request_parts = parts.clone();
        let result = cache.prepare_conditional_request(
            &mut request_parts,
            &cached_response,
            &policy,
        );
        assert!(result.is_ok());

        // Check if conditional headers were added (implementation dependent)
        // This tests that the method doesn't panic and returns Ok

        // Temporary directory will be automatically cleaned up when dropped
    }

    #[tokio::test]
    async fn test_http_cache_interface_not_modified() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);
        let cache = HttpCache {
            mode: CacheMode::Default,
            manager,
            options: HttpCacheOptions::default(),
        };

        // Create a cached response
        let cached_response = crate::HttpResponse {
            body: b"Cached content".to_vec(),
            headers: crate::HttpHeaders::new(),
            status: 200,
            url: url_parse("https://example.com/test").unwrap(),
            version: crate::HttpVersion::Http11,
            metadata: Some(b"Metadata".to_vec()),
        };

        // Create fresh response parts (simulating 304 Not Modified)
        let fresh_response = Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header("last-modified", "Wed, 21 Oct 2015 07:28:00 GMT")
            .body(())
            .unwrap();
        let (fresh_parts, _) = fresh_response.into_parts();

        // Test handling not modified
        let result = cache
            .handle_not_modified(cached_response.clone(), &fresh_parts)
            .await;
        assert!(result.is_ok());

        let updated_response = result.unwrap();
        assert_eq!(updated_response.body, b"Cached content");
        assert_eq!(updated_response.status, 200);
        assert_eq!(updated_response.metadata, Some(b"Metadata".to_vec()));

        // Temporary directory will be automatically cleaned up when dropped
    }

    #[tokio::test]
    async fn test_http_cache_interface_cache_bust() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);
        let options = HttpCacheOptions {
            cache_bust: Some(Arc::new(
                |req: &http::request::Parts,
                 _key: &Option<crate::CacheKey>,
                 _url: &str| {
                    // Bust cache for DELETE requests
                    if req.method == http::Method::DELETE {
                        vec!["GET:https://example.com/test".to_string()]
                    } else {
                        vec![]
                    }
                },
            )),
            ..HttpCacheOptions::default()
        };

        let cache = HttpCache { mode: CacheMode::Default, manager, options };

        // Test that cache bust keys are included in analysis
        let req = Request::builder()
            .method("DELETE")
            .uri("https://example.com/test")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let analysis = cache.analyze_request(&parts, None).unwrap();
        assert!(!analysis.cache_bust_keys.is_empty());
        assert_eq!(analysis.cache_bust_keys[0], "GET:https://example.com/test");

        // Temporary directory will be automatically cleaned up when dropped
    }

    #[tokio::test]
    async fn test_cache_mode_override_precedence() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);

        // Test cache_mode_fn is used when no override
        let options = HttpCacheOptions {
            cache_mode_fn: Some(Arc::new(|_| CacheMode::NoStore)),
            ..HttpCacheOptions::default()
        };
        let cache = HttpCache {
            mode: CacheMode::Default,
            manager: manager.clone(),
            options,
        };

        let req = Request::builder()
            .method("GET")
            .uri("https://example.com/test")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        // Without override, should use cache_mode_fn (NoStore)
        let analysis = cache.analyze_request(&parts, None).unwrap();
        assert_eq!(analysis.cache_mode, CacheMode::NoStore);
        assert!(!analysis.should_cache); // NoStore means not cacheable

        // With override, should use override instead of cache_mode_fn
        let analysis =
            cache.analyze_request(&parts, Some(CacheMode::ForceCache)).unwrap();
        assert_eq!(analysis.cache_mode, CacheMode::ForceCache);
        assert!(analysis.should_cache); // ForceCache overrides NoStore
    }

    #[tokio::test]
    async fn test_custom_cache_key_generation() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);

        // Test with custom cache key generator
        let options = HttpCacheOptions {
            cache_key: Some(Arc::new(|req: &http::request::Parts| {
                format!("custom:{}:{}", req.method, req.uri)
            })),
            ..HttpCacheOptions::default()
        };
        let cache = HttpCache { mode: CacheMode::Default, manager, options };

        let req = Request::builder()
            .method("GET")
            .uri("https://example.com/test")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let analysis = cache.analyze_request(&parts, None).unwrap();
        assert_eq!(analysis.cache_key, "custom:GET:https://example.com/test");
    }

    #[tokio::test]
    async fn test_cache_status_headers_disabled() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);

        // Test with cache status headers disabled
        let options = HttpCacheOptions {
            cache_status_headers: false,
            ..HttpCacheOptions::default()
        };
        let cache = HttpCache { mode: CacheMode::Default, manager, options };

        // Create a cached response
        let cached_response = crate::HttpResponse {
            body: b"Cached content".to_vec(),
            headers: crate::HttpHeaders::new(),
            status: 200,
            url: url_parse("https://example.com/test").unwrap(),
            version: crate::HttpVersion::Http11,
            metadata: Some(b"Metadata".to_vec()),
        };

        // Create fresh response parts
        let fresh_response = Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header("last-modified", "Wed, 21 Oct 2015 07:28:00 GMT")
            .body(())
            .unwrap();
        let (fresh_parts, _) = fresh_response.into_parts();

        // Test handling not modified with headers disabled
        let result = cache
            .handle_not_modified(cached_response.clone(), &fresh_parts)
            .await
            .unwrap();

        // Should not have cache status headers
        assert!(!result.headers.contains_key(crate::XCACHE));
        assert!(!result.headers.contains_key(crate::XCACHELOOKUP));
    }

    #[tokio::test]
    async fn test_process_response_non_cacheable() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);
        let cache = HttpCache {
            mode: CacheMode::Default,
            manager,
            options: HttpCacheOptions::default(),
        };

        // Create a POST request (not cacheable)
        let req = Request::builder()
            .method("POST")
            .uri("https://example.com/test")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();
        let analysis = cache.analyze_request(&parts, None).unwrap();
        assert!(!analysis.should_cache);

        // Create a response
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/plain")
            .body(b"Hello, world!".to_vec())
            .unwrap();

        // Process the response (should NOT cache it)
        let processed = cache
            .process_response(analysis.clone(), response, None)
            .await
            .unwrap();
        assert_eq!(processed.status(), StatusCode::OK);
        assert_eq!(processed.body(), b"Hello, world!");

        // Verify it wasn't cached
        let cached =
            cache.lookup_cached_response(&analysis.cache_key).await.unwrap();
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_cache_analysis_fields() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);
        let cache = HttpCache {
            mode: CacheMode::ForceCache,
            manager,
            options: HttpCacheOptions::default(),
        };

        let req = Request::builder()
            .method("GET")
            .uri("https://example.com/test?param=value")
            .header("user-agent", "test-agent")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let analysis =
            cache.analyze_request(&parts, Some(CacheMode::NoCache)).unwrap();

        // Test all fields are properly populated
        assert!(!analysis.cache_key.is_empty());
        assert!(analysis.cache_key.contains("GET"));
        assert!(analysis.cache_key.contains("https://example.com/test"));
        assert!(analysis.should_cache); // GET with NoCache mode should be cacheable
        assert_eq!(analysis.cache_mode, CacheMode::NoCache); // Override should take precedence
        assert!(analysis.cache_bust_keys.is_empty()); // No cache bust configured

        // Test request_parts are properly cloned
        assert_eq!(analysis.request_parts.method, "GET");
        assert_eq!(
            analysis.request_parts.uri.to_string(),
            "https://example.com/test?param=value"
        );
        assert!(analysis.request_parts.headers.contains_key("user-agent"));
    }
}

#[cfg(feature = "manager-cacache")]
mod metadata_provider_tests {
    use crate::{
        CACacheManager, CacheMode, HttpCache, HttpCacheInterface,
        HttpCacheOptions,
    };
    use http::{Request, Response, StatusCode};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_metadata_provider_generates_metadata() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);

        // Configure cache with a metadata provider
        let options = HttpCacheOptions {
            metadata_provider: Some(Arc::new(
                |request_parts, response_parts| {
                    // Generate metadata based on request path and response status
                    let metadata = format!(
                        "path={};status={}",
                        request_parts.uri.path(),
                        response_parts.status.as_u16()
                    );
                    Some(metadata.into_bytes())
                },
            )),
            ..Default::default()
        };

        let cache = HttpCache { mode: CacheMode::Default, manager, options };

        // Create a GET request
        let request = Request::builder()
            .method("GET")
            .uri("https://example.com/api/data")
            .body(())
            .unwrap();
        let (request_parts, _) = request.into_parts();

        let analysis = cache.analyze_request(&request_parts, None).unwrap();

        // Create a cacheable response
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("cache-control", "max-age=3600")
            .body(b"response body".to_vec())
            .unwrap();

        // Process the response (should generate and store metadata)
        let _ = cache
            .process_response(analysis.clone(), response, None)
            .await
            .unwrap();

        // Look up the cached response
        let cached =
            cache.lookup_cached_response(&analysis.cache_key).await.unwrap();
        assert!(cached.is_some());

        let (cached_response, _policy) = cached.unwrap();

        // Verify metadata was generated and stored
        assert!(cached_response.metadata.is_some());
        let metadata = cached_response.metadata.unwrap();
        let metadata_str = String::from_utf8(metadata).unwrap();
        assert_eq!(metadata_str, "path=/api/data;status=200");
    }

    #[tokio::test]
    async fn test_metadata_provider_returns_none() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);

        // Configure cache with a metadata provider that returns None
        let options = HttpCacheOptions {
            metadata_provider: Some(Arc::new(
                |_request_parts, _response_parts| {
                    None // Don't generate metadata for this response
                },
            )),
            ..Default::default()
        };

        let cache = HttpCache { mode: CacheMode::Default, manager, options };

        // Create a GET request
        let request = Request::builder()
            .method("GET")
            .uri("https://example.com/test")
            .body(())
            .unwrap();
        let (request_parts, _) = request.into_parts();

        let analysis = cache.analyze_request(&request_parts, None).unwrap();

        // Create a cacheable response
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("cache-control", "max-age=3600")
            .body(b"response body".to_vec())
            .unwrap();

        // Process the response
        let _ = cache
            .process_response(analysis.clone(), response, None)
            .await
            .unwrap();

        // Look up the cached response
        let cached =
            cache.lookup_cached_response(&analysis.cache_key).await.unwrap();
        assert!(cached.is_some());

        let (cached_response, _policy) = cached.unwrap();

        // Verify metadata is None
        assert!(cached_response.metadata.is_none());
    }

    #[tokio::test]
    async fn test_explicit_metadata_overrides_provider() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);

        // Configure cache with a metadata provider
        let options = HttpCacheOptions {
            metadata_provider: Some(Arc::new(
                |_request_parts, _response_parts| {
                    Some(b"from-provider".to_vec())
                },
            )),
            ..Default::default()
        };

        let cache = HttpCache { mode: CacheMode::Default, manager, options };

        // Create a GET request
        let request = Request::builder()
            .method("GET")
            .uri("https://example.com/test")
            .body(())
            .unwrap();
        let (request_parts, _) = request.into_parts();

        let analysis = cache.analyze_request(&request_parts, None).unwrap();

        // Create a cacheable response
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("cache-control", "max-age=3600")
            .body(b"response body".to_vec())
            .unwrap();

        // Process the response with explicit metadata (should override provider)
        let _ = cache
            .process_response(
                analysis.clone(),
                response,
                Some(b"explicit-metadata".to_vec()),
            )
            .await
            .unwrap();

        // Look up the cached response
        let cached =
            cache.lookup_cached_response(&analysis.cache_key).await.unwrap();
        assert!(cached.is_some());

        let (cached_response, _policy) = cached.unwrap();

        // Verify explicit metadata takes precedence over provider
        assert!(cached_response.metadata.is_some());
        let metadata = cached_response.metadata.unwrap();
        assert_eq!(metadata, b"explicit-metadata");
    }

    #[tokio::test]
    async fn test_metadata_provider_with_conditional_logic() {
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);

        // Configure cache with a metadata provider that only generates metadata for certain paths
        let options = HttpCacheOptions {
            metadata_provider: Some(Arc::new(
                |request_parts, response_parts| {
                    // Only generate metadata for API paths
                    if request_parts.uri.path().starts_with("/api/") {
                        let content_type = response_parts
                            .headers
                            .get("content-type")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("unknown");
                        Some(
                            format!("content-type={}", content_type)
                                .into_bytes(),
                        )
                    } else {
                        None
                    }
                },
            )),
            ..Default::default()
        };

        let cache = HttpCache { mode: CacheMode::Default, manager, options };

        // Test 1: API path should generate metadata
        let api_request = Request::builder()
            .method("GET")
            .uri("https://example.com/api/users")
            .body(())
            .unwrap();
        let (api_parts, _) = api_request.into_parts();

        let api_analysis = cache.analyze_request(&api_parts, None).unwrap();

        let api_response = Response::builder()
            .status(StatusCode::OK)
            .header("cache-control", "max-age=3600")
            .header("content-type", "application/json")
            .body(b"[]".to_vec())
            .unwrap();

        let _ = cache
            .process_response(api_analysis.clone(), api_response, None)
            .await
            .unwrap();

        let cached = cache
            .lookup_cached_response(&api_analysis.cache_key)
            .await
            .unwrap();
        let (cached_response, _) = cached.unwrap();
        assert!(cached_response.metadata.is_some());
        let metadata_str =
            String::from_utf8(cached_response.metadata.unwrap()).unwrap();
        assert_eq!(metadata_str, "content-type=application/json");

        // Test 2: Non-API path should not generate metadata
        let static_request = Request::builder()
            .method("GET")
            .uri("https://example.com/static/style.css")
            .body(())
            .unwrap();
        let (static_parts, _) = static_request.into_parts();

        let static_analysis =
            cache.analyze_request(&static_parts, None).unwrap();

        let static_response = Response::builder()
            .status(StatusCode::OK)
            .header("cache-control", "max-age=3600")
            .header("content-type", "text/css")
            .body(b"body {}".to_vec())
            .unwrap();

        let _ = cache
            .process_response(static_analysis.clone(), static_response, None)
            .await
            .unwrap();

        let cached = cache
            .lookup_cached_response(&static_analysis.cache_key)
            .await
            .unwrap();
        let (cached_response, _) = cached.unwrap();
        assert!(cached_response.metadata.is_none());
    }
}

#[cfg(all(test, feature = "rate-limiting"))]
mod rate_limiting_tests {
    use super::*;
    use crate::rate_limiting::{
        CacheAwareRateLimiter, DomainRateLimiter, Quota,
    };
    use crate::url_hostname;
    use crate::HttpCacheOptions;
    use std::num::NonZero;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    // Mock rate limiter that tracks calls for testing
    #[derive(Debug)]
    struct MockRateLimiter {
        calls: Arc<Mutex<Vec<String>>>,
        delay: Duration,
    }

    impl MockRateLimiter {
        fn new(delay: Duration) -> Self {
            Self { calls: Arc::new(Mutex::new(Vec::new())), delay }
        }

        fn get_calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    #[async_trait::async_trait]
    impl CacheAwareRateLimiter for MockRateLimiter {
        async fn until_key_ready(&self, key: &str) {
            self.calls.lock().unwrap().push(key.to_string());
            if !self.delay.is_zero() {
                // Use std::thread::sleep for simplicity in tests
                std::thread::sleep(self.delay);
            }
        }

        fn check_key(&self, _key: &str) -> bool {
            true
        }
    }

    #[test]
    fn test_domain_rate_limiter_creation() {
        let quota = Quota::per_second(NonZero::new(1).unwrap());
        let limiter = DomainRateLimiter::new(quota);

        // Test that we can check keys without panicking
        assert!(limiter.check_key("example.com"));
        assert!(limiter.check_key("another.com"));
    }

    #[test]
    fn test_direct_rate_limiter_creation() {
        let quota = Quota::per_second(NonZero::new(10).unwrap()); // Higher quota for testing
        let limiter = DomainRateLimiter::new(quota);

        // Test that we can check keys (key is ignored for direct limiter)
        // Use the same key since it's a direct limiter
        assert!(limiter.check_key("any-key"));
    }

    #[tokio::test]
    async fn test_rate_limiting_options_integration() {
        // Test that HttpCacheOptions properly stores and uses rate limiter
        let rate_limiter = MockRateLimiter::new(Duration::from_millis(1));
        let call_counter = rate_limiter.calls.clone();

        let options = HttpCacheOptions {
            rate_limiter: Some(Arc::new(rate_limiter)),
            ..Default::default()
        };

        // Verify rate limiter is stored
        assert!(options.rate_limiter.is_some());

        // Simulate rate limiting call
        if let Some(limiter) = &options.rate_limiter {
            limiter.until_key_ready("test-domain").await;
        }

        // Verify the call was recorded
        assert_eq!(call_counter.lock().unwrap().len(), 1);
        assert_eq!(call_counter.lock().unwrap()[0], "test-domain");
    }

    #[tokio::test]
    async fn test_rate_limiting_with_actual_governor() {
        // Test with actual governor rate limiter
        let quota = Quota::per_second(NonZero::new(2).unwrap()); // 2 requests per second
        let limiter = DomainRateLimiter::new(quota);

        let start = Instant::now();

        // First request should be immediate
        limiter.until_key_ready("example.com").await;
        let first_duration = start.elapsed();

        // Second request should also be immediate (within burst)
        limiter.until_key_ready("example.com").await;
        let second_duration = start.elapsed();

        // Both should be very fast
        assert!(first_duration < Duration::from_millis(10));
        assert!(second_duration < Duration::from_millis(10));

        // Test with different domain (should be separate rate limit)
        limiter.until_key_ready("other.com").await;
        let third_duration = start.elapsed();
        assert!(third_duration < Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_direct_rate_limiter_behavior() {
        // Test direct rate limiter that applies globally
        let quota = Quota::per_second(NonZero::new(10).unwrap()); // Higher quota for testing
        let limiter = DomainRateLimiter::new(quota);

        let start = Instant::now();

        // First request should be immediate
        limiter.until_key_ready("any-domain").await;
        let first_duration = start.elapsed();
        assert!(first_duration < Duration::from_millis(100));

        // Test that check_key works (should still have quota)
        assert!(limiter.check_key("any-domain"));
    }

    #[test]
    fn test_http_cache_options_debug_with_rate_limiting() {
        let quota = Quota::per_second(NonZero::new(1).unwrap());
        let rate_limiter = DomainRateLimiter::new(quota);

        let options = HttpCacheOptions {
            rate_limiter: Some(Arc::new(rate_limiter)),
            ..Default::default()
        };

        let debug_string = format!("{:?}", options);

        // Verify debug output includes rate limiter field
        assert!(debug_string.contains("rate_limiter"));
        assert!(debug_string.contains("Option<CacheAwareRateLimiter>"));
    }

    #[test]
    fn test_http_cache_options_default_no_rate_limiting() {
        let options = HttpCacheOptions::default();

        // Verify default has no rate limiter
        assert!(options.rate_limiter.is_none());
    }

    // Integration test that would require more complex setup
    // This tests the flow conceptually but would need a full middleware setup
    #[tokio::test]
    async fn test_rate_limiter_key_extraction() {
        let url = url_parse("https://api.example.com/users").unwrap();
        let host = url_hostname(&url).unwrap_or("unknown");

        assert_eq!(host, "api.example.com");

        // Test with different URLs
        let url2 = url_parse("https://other-api.example.com/posts").unwrap();
        let host2 = url_hostname(&url2).unwrap_or("unknown");

        assert_eq!(host2, "other-api.example.com");

        // Test with localhost
        let url3 = url_parse("http://localhost:8080/test").unwrap();
        let host3 = url_hostname(&url3).unwrap_or("unknown");

        assert_eq!(host3, "localhost");
    }
}
