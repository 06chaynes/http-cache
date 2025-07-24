use crate::{
    error, CacheMode, HitOrMiss, HttpCacheOptions, HttpResponse, HttpVersion,
    Result,
};
use http::{header::CACHE_CONTROL, StatusCode};
use http_cache_semantics::CacheOptions;
use url::Url;

use std::{collections::HashMap, str::FromStr};

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
fn cache_options() -> Result<()> {
    // Testing the Debug, Default and Clone traits for the HttpCacheOptions struct
    let mut opts = HttpCacheOptions::default();
    assert_eq!(format!("{:?}", opts.clone()), "HttpCacheOptions { cache_options: None, cache_key: \"Fn(&request::Parts) -> String\", cache_mode_fn: \"Fn(&request::Parts) -> CacheMode\", cache_bust: \"Fn(&request::Parts) -> Vec<String>\", cache_status_headers: true }");
    opts.cache_options = Some(CacheOptions::default());
    assert_eq!(format!("{:?}", opts.clone()), "HttpCacheOptions { cache_options: Some(CacheOptions { shared: true, cache_heuristic: 0.1, immutable_min_time_to_live: 86400s, ignore_cargo_cult: false }), cache_key: \"Fn(&request::Parts) -> String\", cache_mode_fn: \"Fn(&request::Parts) -> CacheMode\", cache_bust: \"Fn(&request::Parts) -> Vec<String>\", cache_status_headers: true }");
    opts.cache_options = None;
    opts.cache_key = Some(std::sync::Arc::new(|req: &http::request::Parts| {
        format!("{}:{}:{:?}:test", req.method, req.uri, req.version)
    }));
    assert_eq!(format!("{opts:?}"), "HttpCacheOptions { cache_options: None, cache_key: \"Fn(&request::Parts) -> String\", cache_mode_fn: \"Fn(&request::Parts) -> CacheMode\", cache_bust: \"Fn(&request::Parts) -> Vec<String>\", cache_status_headers: true }");
    opts.cache_status_headers = false;
    assert_eq!(format!("{opts:?}"), "HttpCacheOptions { cache_options: None, cache_key: \"Fn(&request::Parts) -> String\", cache_mode_fn: \"Fn(&request::Parts) -> CacheMode\", cache_bust: \"Fn(&request::Parts) -> Vec<String>\", cache_status_headers: false }");
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
        headers: HashMap::default(),
        status: 200,
        url: url.clone(),
        version: HttpVersion::Http11,
    };
    assert_eq!(format!("{:?}", res.clone()), "HttpResponse { body: [116, 101, 115, 116], headers: {}, status: 200, url: Url { scheme: \"http\", cannot_be_a_base: false, username: \"\", password: None, host: Some(Domain(\"example.com\")), port: None, path: \"/\", query: None, fragment: None }, version: Http11 }");
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

    #[cfg(feature = "cacache-tokio")]
    use tokio::test as async_test;

    #[cfg(feature = "cacache-smol")]
    #[test]
    fn cacache() -> Result<()> {
        smol::block_on(async {
            let url = Url::parse("http://example.com")?;
            let cache_dir = tempfile::tempdir().unwrap();
            let manager =
                CACacheManager::new(cache_dir.path().to_path_buf(), true);
            let http_res = HttpResponse {
                body: TEST_BODY.to_vec(),
                headers: Default::default(),
                status: 200,
                url: url.clone(),
                version: HttpVersion::Http11,
            };
            let req = http::Request::get("http://example.com").body(())?;
            let res = http::Response::builder()
                .status(200)
                .body(TEST_BODY.to_vec())?;
            let policy = CachePolicy::new(&req, &res);
            manager.put("test".to_string(), http_res, policy).await?;
            let (cached_res, _policy) =
                manager.get("test").await?.ok_or("Missing cache record")?;
            assert_eq!(cached_res.body, TEST_BODY);
            Ok(())
        })
    }

    #[cfg(feature = "cacache-tokio")]
    #[async_test]
    async fn cacache() -> Result<()> {
        let url = Url::parse("http://example.com")?;
        let cache_dir = tempfile::tempdir().unwrap();
        let manager = CACacheManager::new(cache_dir.path().to_path_buf(), true);
        let http_res = HttpResponse {
            body: TEST_BODY.to_vec(),
            headers: Default::default(),
            status: 200,
            url: url.clone(),
            version: HttpVersion::Http11,
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
        assert_eq!(data.unwrap().0.body, TEST_BODY);
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

    #[test]
    fn moka() -> Result<()> {
        smol::block_on(async {
            // Added to test custom Debug impl
            let mm = MokaManager::default();
            assert_eq!(format!("{:?}", mm.clone()), "MokaManager { .. }",);
            let url = Url::parse("http://example.com")?;
            let manager = Arc::new(mm);
            let http_res = HttpResponse {
                body: TEST_BODY.to_vec(),
                headers: Default::default(),
                status: 200,
                url: url.clone(),
                version: HttpVersion::Http11,
            };
            let req = http::Request::get("http://example.com").body(())?;
            let res = http::Response::builder()
                .status(200)
                .body(TEST_BODY.to_vec())?;
            let policy = CachePolicy::new(&req, &res);
            manager
                .put(
                    format!("{}:{}", GET, &url),
                    http_res.clone(),
                    policy.clone(),
                )
                .await?;
            let data = manager.get(&format!("{}:{}", GET, &url)).await?;
            assert!(data.is_some());
            assert_eq!(data.unwrap().0.body, TEST_BODY);
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
        })
    }
}

#[cfg(feature = "manager-cacache")]
mod interface_tests {
    use crate::{
        CACacheManager, CacheMode, HttpCache, HttpCacheInterface,
        HttpCacheOptions,
    };
    use http::{Request, Response, StatusCode};
    use std::sync::Arc;
    use url::Url;

    #[cfg(feature = "cacache-tokio")]
    use tokio::test as async_test;

    #[cfg(feature = "cacache-smol")]
    #[test]
    fn test_http_cache_interface_analyze_request() {
        smol::block_on(async {
            let cache_dir = tempfile::tempdir().unwrap();
            let manager =
                CACacheManager::new(cache_dir.path().to_path_buf(), true);
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
        })
    }

    #[cfg(feature = "cacache-tokio")]
    #[async_test]
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

    #[cfg(feature = "cacache-smol")]
    #[test]
    fn test_http_cache_interface_lookup_and_process() {
        smol::block_on(async {
            let cache_dir = tempfile::tempdir().unwrap();
            let manager =
                CACacheManager::new(cache_dir.path().to_path_buf(), true);
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
                .process_response(analysis.clone(), response)
                .await
                .unwrap();
            assert_eq!(processed.status(), StatusCode::OK);

            // Try to look up the cached response
            let cached = cache
                .lookup_cached_response(&analysis.cache_key)
                .await
                .unwrap();
            assert!(cached.is_some());

            let (cached_response, _policy) = cached.unwrap();
            assert_eq!(cached_response.status, StatusCode::OK);
            assert_eq!(cached_response.body, b"Hello, world!");

            // Temporary directory will be automatically cleaned up when dropped
        })
    }

    #[cfg(feature = "cacache-tokio")]
    #[async_test]
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
        let processed =
            cache.process_response(analysis.clone(), response).await.unwrap();
        assert_eq!(processed.status(), StatusCode::OK);

        // Try to look up the cached response
        let cached =
            cache.lookup_cached_response(&analysis.cache_key).await.unwrap();
        assert!(cached.is_some());

        let (cached_response, _policy) = cached.unwrap();
        assert_eq!(cached_response.status, StatusCode::OK);
        assert_eq!(cached_response.body, b"Hello, world!");

        // Temporary directory will be automatically cleaned up when dropped
    }

    #[cfg(feature = "cacache-smol")]
    #[test]
    fn test_http_cache_interface_conditional_requests() {
        smol::block_on(async {
            let cache_dir = tempfile::tempdir().unwrap();
            let manager =
                CACacheManager::new(cache_dir.path().to_path_buf(), true);
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
                .process_response(analysis.clone(), response)
                .await
                .unwrap();

            // Look up the cached response
            let cached = cache
                .lookup_cached_response(&analysis.cache_key)
                .await
                .unwrap();
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
        })
    }

    #[cfg(feature = "cacache-tokio")]
    #[async_test]
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
        let _processed =
            cache.process_response(analysis.clone(), response).await.unwrap();

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

    #[cfg(feature = "cacache-smol")]
    #[test]
    fn test_http_cache_interface_not_modified() {
        smol::block_on(async {
            let cache_dir = tempfile::tempdir().unwrap();
            let manager =
                CACacheManager::new(cache_dir.path().to_path_buf(), true);
            let cache = HttpCache {
                mode: CacheMode::Default,
                manager,
                options: HttpCacheOptions::default(),
            };

            // Create a cached response
            let cached_response = crate::HttpResponse {
                body: b"Cached content".to_vec(),
                headers: std::collections::HashMap::new(),
                status: 200,
                url: Url::parse("https://example.com/test").unwrap(),
                version: crate::HttpVersion::Http11,
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

            // Temporary directory will be automatically cleaned up when dropped
        })
    }

    #[cfg(feature = "cacache-tokio")]
    #[async_test]
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
            headers: std::collections::HashMap::new(),
            status: 200,
            url: Url::parse("https://example.com/test").unwrap(),
            version: crate::HttpVersion::Http11,
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

        // Temporary directory will be automatically cleaned up when dropped
    }

    #[cfg(feature = "cacache-smol")]
    #[test]
    fn test_http_cache_interface_cache_bust() {
        smol::block_on(async {
            let cache_dir = tempfile::tempdir().unwrap();
            let manager =
                CACacheManager::new(cache_dir.path().to_path_buf(), true);
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

            let cache =
                HttpCache { mode: CacheMode::Default, manager, options };

            // Test that cache bust keys are included in analysis
            let req = Request::builder()
                .method("DELETE")
                .uri("https://example.com/test")
                .body(())
                .unwrap();
            let (parts, _) = req.into_parts();

            let analysis = cache.analyze_request(&parts, None).unwrap();
            assert!(!analysis.cache_bust_keys.is_empty());
            assert_eq!(
                analysis.cache_bust_keys[0],
                "GET:https://example.com/test"
            );

            // Temporary directory will be automatically cleaned up when dropped
        })
    }

    #[cfg(feature = "cacache-tokio")]
    #[async_test]
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

    #[cfg(feature = "cacache-smol")]
    #[test]
    fn test_cache_mode_override_precedence() {
        smol::block_on(async {
            let cache_dir = tempfile::tempdir().unwrap();
            let manager =
                CACacheManager::new(cache_dir.path().to_path_buf(), true);

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
            let analysis = cache
                .analyze_request(&parts, Some(CacheMode::ForceCache))
                .unwrap();
            assert_eq!(analysis.cache_mode, CacheMode::ForceCache);
            assert!(analysis.should_cache); // ForceCache overrides NoStore
        })
    }

    #[cfg(feature = "cacache-tokio")]
    #[async_test]
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

    #[cfg(feature = "cacache-smol")]
    #[test]
    fn test_custom_cache_key_generation() {
        smol::block_on(async {
            let cache_dir = tempfile::tempdir().unwrap();
            let manager =
                CACacheManager::new(cache_dir.path().to_path_buf(), true);

            // Test with custom cache key generator
            let options = HttpCacheOptions {
                cache_key: Some(Arc::new(|req: &http::request::Parts| {
                    format!("custom:{}:{}", req.method, req.uri)
                })),
                ..HttpCacheOptions::default()
            };
            let cache =
                HttpCache { mode: CacheMode::Default, manager, options };

            let req = Request::builder()
                .method("GET")
                .uri("https://example.com/test")
                .body(())
                .unwrap();
            let (parts, _) = req.into_parts();

            let analysis = cache.analyze_request(&parts, None).unwrap();
            assert_eq!(
                analysis.cache_key,
                "custom:GET:https://example.com/test"
            );
        })
    }

    #[cfg(feature = "cacache-tokio")]
    #[async_test]
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

    #[cfg(feature = "cacache-smol")]
    #[test]
    fn test_cache_status_headers_disabled() {
        smol::block_on(async {
            let cache_dir = tempfile::tempdir().unwrap();
            let manager =
                CACacheManager::new(cache_dir.path().to_path_buf(), true);

            // Test with cache status headers disabled
            let options = HttpCacheOptions {
                cache_status_headers: false,
                ..HttpCacheOptions::default()
            };
            let cache =
                HttpCache { mode: CacheMode::Default, manager, options };

            // Create a cached response
            let cached_response = crate::HttpResponse {
                body: b"Cached content".to_vec(),
                headers: std::collections::HashMap::new(),
                status: 200,
                url: Url::parse("https://example.com/test").unwrap(),
                version: crate::HttpVersion::Http11,
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
        })
    }

    #[cfg(feature = "cacache-tokio")]
    #[async_test]
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
            headers: std::collections::HashMap::new(),
            status: 200,
            url: Url::parse("https://example.com/test").unwrap(),
            version: crate::HttpVersion::Http11,
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

    #[cfg(feature = "cacache-smol")]
    #[test]
    fn test_process_response_non_cacheable() {
        smol::block_on(async {
            let cache_dir = tempfile::tempdir().unwrap();
            let manager =
                CACacheManager::new(cache_dir.path().to_path_buf(), true);
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
                .process_response(analysis.clone(), response)
                .await
                .unwrap();
            assert_eq!(processed.status(), StatusCode::OK);
            assert_eq!(processed.body(), b"Hello, world!");

            // Verify it wasn't cached
            let cached = cache
                .lookup_cached_response(&analysis.cache_key)
                .await
                .unwrap();
            assert!(cached.is_none());
        })
    }

    #[cfg(feature = "cacache-tokio")]
    #[async_test]
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
        let processed =
            cache.process_response(analysis.clone(), response).await.unwrap();
        assert_eq!(processed.status(), StatusCode::OK);
        assert_eq!(processed.body(), b"Hello, world!");

        // Verify it wasn't cached
        let cached =
            cache.lookup_cached_response(&analysis.cache_key).await.unwrap();
        assert!(cached.is_none());
    }

    #[cfg(feature = "cacache-smol")]
    #[test]
    fn test_cache_analysis_fields() {
        smol::block_on(async {
            let cache_dir = tempfile::tempdir().unwrap();
            let manager =
                CACacheManager::new(cache_dir.path().to_path_buf(), true);
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

            let analysis = cache
                .analyze_request(&parts, Some(CacheMode::NoCache))
                .unwrap();

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
        })
    }

    #[cfg(feature = "cacache-tokio")]
    #[async_test]
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
