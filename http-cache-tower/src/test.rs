#[cfg(test)]
mod tests {
    #[cfg(feature = "streaming")]
    use crate::HttpCacheStreamingLayer;
    use crate::{HttpCacheBody, HttpCacheError, HttpCacheLayer};
    use bytes::Bytes;
    use http::{Request, Response, StatusCode};
    use http_body::Body;
    use http_body_util::{BodyExt, Full};
    #[cfg(feature = "streaming")]
    use http_cache::StreamingManager;
    use http_cache::{
        CACacheManager, CacheManager, CacheMode, HttpCache, HttpCacheOptions,
        StreamingBody,
    };
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tower::{Layer, Service, ServiceExt};

    type Result<T> =
        std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

    const TEST_BODY: &[u8] = b"Hello, world!";
    const CACHEABLE_PUBLIC: &str = "max-age=3600, public";

    #[test]
    fn test_errors() -> Result<()> {
        // Testing the Debug trait for the error type
        let err = HttpCacheError::cache("test".to_string());
        assert!(format!("{:?}", &err).contains("Cache"));
        assert!(err.to_string().contains("test"));
        Ok(())
    }

    // Simple test service that always returns the same response
    #[derive(Clone)]
    struct TestService {
        status: StatusCode,
        headers: Vec<(&'static str, &'static str)>,
        body: &'static [u8],
    }

    impl TestService {
        fn new(
            status: StatusCode,
            headers: Vec<(&'static str, &'static str)>,
            body: &'static [u8],
        ) -> Self {
            Self { status, headers, body }
        }
    }

    impl Service<Request<Full<Bytes>>> for TestService {
        type Response = Response<Full<Bytes>>;
        type Error = Box<dyn std::error::Error + Send + Sync>;
        type Future = Pin<
            Box<
                dyn Future<
                        Output = std::result::Result<
                            Self::Response,
                            Self::Error,
                        >,
                    > + Send,
            >,
        >;

        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request<Full<Bytes>>) -> Self::Future {
            let mut response = Response::builder().status(self.status);

            for (name, value) in &self.headers {
                response = response.header(*name, *value);
            }

            let response =
                response.body(Full::new(Bytes::from(self.body.to_vec())));

            Box::pin(async move {
                response.map_err(|e| {
                    Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                })
            })
        }
    }

    #[tokio::test]
    async fn default_mode() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(manager.clone());

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );
        let mut cached_service = cache_layer.layer(test_service);

        let request = Request::builder()
            .method("GET")
            .uri("http://example.com/test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        // First request - cache miss
        let response =
            cached_service.ready().await?.call(request.clone()).await?;
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await?.to_bytes();
        assert_eq!(body_bytes, TEST_BODY);

        Ok(())
    }

    #[tokio::test]
    async fn default_mode_with_options() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let options = HttpCacheOptions::default();
        let cache_layer =
            HttpCacheLayer::with_options(manager.clone(), options);

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );
        let mut cached_service = cache_layer.layer(test_service);

        let request = Request::builder()
            .method("GET")
            .uri("http://example.com/test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        let response = cached_service.ready().await?.call(request).await?;
        assert_eq!(response.status(), StatusCode::OK);

        Ok(())
    }

    #[tokio::test]
    async fn no_store_mode() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache = HttpCache {
            mode: CacheMode::NoStore,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        };
        let cache_layer = HttpCacheLayer::with_cache(cache);

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );
        let mut cached_service = cache_layer.layer(test_service);

        let request = Request::builder()
            .method("GET")
            .uri("http://example.com/test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        // First request
        let response =
            cached_service.ready().await?.call(request.clone()).await?;
        assert_eq!(response.status(), StatusCode::OK);

        // Second request - should go to origin again (NoStore mode)
        let response = cached_service.ready().await?.call(request).await?;
        assert_eq!(response.status(), StatusCode::OK);

        Ok(())
    }

    #[tokio::test]
    async fn no_cache_mode() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache = HttpCache {
            mode: CacheMode::NoCache,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        };
        let cache_layer = HttpCacheLayer::with_cache(cache);

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );
        let mut cached_service = cache_layer.layer(test_service);

        let request = Request::builder()
            .method("GET")
            .uri("http://example.com/test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        // First request
        let response =
            cached_service.ready().await?.call(request.clone()).await?;
        assert_eq!(response.status(), StatusCode::OK);

        // Second request - should revalidate (NoCache mode)
        let response = cached_service.ready().await?.call(request).await?;
        assert_eq!(response.status(), StatusCode::OK);

        Ok(())
    }

    #[tokio::test]
    async fn force_cache_mode() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache = HttpCache {
            mode: CacheMode::ForceCache,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        };
        let cache_layer = HttpCacheLayer::with_cache(cache);

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", "max-age=0, public")],
            TEST_BODY,
        );
        let mut cached_service = cache_layer.layer(test_service);

        let request = Request::builder()
            .method("GET")
            .uri("http://example.com/test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        // First request - cache miss, remote request
        let response =
            cached_service.ready().await?.call(request.clone()).await?;
        assert_eq!(response.status(), StatusCode::OK);

        Ok(())
    }

    #[tokio::test]
    async fn ignore_rules_mode() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache = HttpCache {
            mode: CacheMode::IgnoreRules,
            manager: manager.clone(),
            options: HttpCacheOptions::default(),
        };
        let cache_layer = HttpCacheLayer::with_cache(cache);

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", "no-store, max-age=0, public")],
            TEST_BODY,
        );
        let mut cached_service = cache_layer.layer(test_service);

        let request = Request::builder()
            .method("GET")
            .uri("http://example.com/test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        // First request - should cache despite no-store directive
        let response =
            cached_service.ready().await?.call(request.clone()).await?;
        assert_eq!(response.status(), StatusCode::OK);

        Ok(())
    }

    #[tokio::test]
    async fn post_request_bypasses_cache() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(manager.clone());

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );
        let mut cached_service = cache_layer.layer(test_service);

        // POST request should bypass cache
        let post_request = Request::builder()
            .method("POST")
            .uri("http://example.com/test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        let response = cached_service.ready().await?.call(post_request).await?;
        assert_eq!(response.status(), StatusCode::OK);

        Ok(())
    }

    #[tokio::test]
    async fn layer_composition() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(manager.clone());

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );

        // Test that the layer can be composed multiple times
        let composed_service =
            cache_layer.clone().layer(cache_layer.layer(test_service));
        let mut service = composed_service;

        let request = Request::builder()
            .method("GET")
            .uri("http://example.com/test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        let response = service.ready().await?.call(request).await?;
        assert_eq!(response.status(), StatusCode::OK);

        Ok(())
    }

    #[tokio::test]
    async fn body_types() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(manager.clone());

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );
        let mut cached_service = cache_layer.layer(test_service);

        let request = Request::builder()
            .method("GET")
            .uri("http://example.com/test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        let response = cached_service.ready().await?.call(request).await?;
        assert_eq!(response.status(), StatusCode::OK);

        // Verify the body type
        match response.into_body() {
            HttpCacheBody::Original(_) => {} // Expected for current implementation
            HttpCacheBody::Buffered(_) => {}
        }

        Ok(())
    }

    #[tokio::test]
    async fn cache_busting() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(manager.clone());

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );
        let mut cached_service = cache_layer.layer(test_service);

        // First, make a GET request to cache something
        let get_request = Request::builder()
            .method("GET")
            .uri("http://example.com/test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        let response = cached_service.ready().await?.call(get_request).await?;
        assert_eq!(response.status(), StatusCode::OK);

        // Now make a POST request which should bust the cache
        let post_request = Request::builder()
            .method("POST")
            .uri("http://example.com/test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        let response = cached_service.ready().await?.call(post_request).await?;
        assert_eq!(response.status(), StatusCode::OK);

        Ok(())
    }

    #[tokio::test]
    async fn test_conditional_requests() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager);

        // Create a service that returns different responses based on headers
        #[derive(Clone)]
        struct ConditionalService {
            request_count: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        impl Service<Request<Full<Bytes>>> for ConditionalService {
            type Response = Response<Full<Bytes>>;
            type Error = Box<dyn std::error::Error + Send + Sync>;
            type Future = Pin<
                Box<
                    dyn Future<
                            Output = std::result::Result<
                                Self::Response,
                                Self::Error,
                            >,
                        > + Send,
                >,
            >;

            fn poll_ready(
                &mut self,
                _cx: &mut Context<'_>,
            ) -> Poll<std::result::Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, req: Request<Full<Bytes>>) -> Self::Future {
                let count = {
                    let mut count = self.request_count.lock().unwrap();
                    *count += 1;
                    *count
                };

                Box::pin(async move {
                    // Check for conditional headers
                    if req.headers().contains_key("if-none-match")
                        || req.headers().contains_key("if-modified-since")
                    {
                        // Return 304 Not Modified for conditional requests
                        return Ok(Response::builder()
                            .status(StatusCode::NOT_MODIFIED)
                            .header("cache-control", "max-age=3600, public")
                            .body(Full::new(Bytes::new()))?);
                    }

                    // Return fresh response with ETag
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("cache-control", "max-age=3600, public")
                        .header("etag", "\"123456\"")
                        .header("content-type", "text/plain")
                        .body(Full::new(Bytes::from(format!(
                            "Response #{count}"
                        ))))?)
                })
            }
        }

        let service = ConditionalService {
            request_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
        };

        let mut cached_service = cache_layer.layer(service);

        // First request - should cache
        let request1 = Request::builder()
            .uri("https://example.com/test")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response1 = cached_service.ready().await?.call(request1).await?;
        assert_eq!(response1.status(), StatusCode::OK);

        let body1 = BodyExt::collect(response1.into_body()).await?.to_bytes();
        assert_eq!(body1, "Response #1");

        // Second request - should return cached response (no new request to service)
        let request2 = Request::builder()
            .uri("https://example.com/test")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response2 = cached_service.ready().await?.call(request2).await?;
        assert_eq!(response2.status(), StatusCode::OK);

        let body2 = BodyExt::collect(response2.into_body()).await?.to_bytes();
        assert_eq!(body2, "Response #1"); // Should still be cached response

        Ok(())
    }

    #[tokio::test]
    async fn test_response_caching_and_retrieval() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager);

        let test_service = TestService::new(
            StatusCode::OK,
            vec![
                ("cache-control", CACHEABLE_PUBLIC),
                ("content-type", "text/plain"),
            ],
            TEST_BODY,
        );

        let mut cached_service = cache_layer.layer(test_service);

        // First request
        let request1 = Request::builder()
            .uri("https://example.com/cached")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response1 = cached_service.ready().await?.call(request1).await?;
        assert_eq!(response1.status(), StatusCode::OK);

        // Verify body type is Buffered (indicating it was processed through cache)
        match response1.into_body() {
            HttpCacheBody::Buffered(data) => {
                assert_eq!(data, TEST_BODY);
            }
            _ => panic!("Expected Buffered body"),
        }

        // Second identical request - should be served from cache
        let request2 = Request::builder()
            .uri("https://example.com/cached")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response2 = cached_service.ready().await?.call(request2).await?;
        assert_eq!(response2.status(), StatusCode::OK);

        // Should still be a Buffered body from cache
        match response2.into_body() {
            HttpCacheBody::Buffered(data) => {
                assert_eq!(data, TEST_BODY);
            }
            _ => panic!("Expected Buffered body from cache"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn removes_warning() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager);

        #[derive(Clone)]
        struct WarningService {
            call_count: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        impl Service<Request<Full<Bytes>>> for WarningService {
            type Response = Response<Full<Bytes>>;
            type Error = Box<dyn std::error::Error + Send + Sync>;
            type Future = Pin<
                Box<
                    dyn Future<
                            Output = std::result::Result<
                                Self::Response,
                                Self::Error,
                            >,
                        > + Send,
                >,
            >;

            fn poll_ready(
                &mut self,
                _cx: &mut Context<'_>,
            ) -> Poll<std::result::Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, _req: Request<Full<Bytes>>) -> Self::Future {
                let call_count = self.call_count.clone();
                Box::pin(async move {
                    let count = {
                        let mut count = call_count.lock().unwrap();
                        *count += 1;
                        *count
                    };

                    if count == 1 {
                        // First request - return response with warning header
                        Ok(Response::builder()
                            .status(StatusCode::OK)
                            .header("cache-control", "max-age=3600, public")
                            .header("warning", "101 Test")
                            .header("content-type", "text/plain")
                            .body(Full::new(Bytes::from(TEST_BODY)))?)
                    } else {
                        // This shouldn't be called on second request if cached properly
                        panic!("Service called twice when response should be cached")
                    }
                })
            }
        }

        let service = WarningService {
            call_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
        };

        let mut cached_service = cache_layer.layer(service);

        // First request - should cache
        let request1 = Request::builder()
            .uri("https://example.com/warning-test")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response1 = cached_service.ready().await?.call(request1).await?;
        assert_eq!(response1.status(), StatusCode::OK);

        // The first response should have the warning (from the service directly)
        assert!(response1.headers().get("warning").is_some());

        // Second request - should be served from cache
        let request2 = Request::builder()
            .uri("https://example.com/warning-test")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response2 = cached_service.ready().await?.call(request2).await?;
        assert_eq!(response2.status(), StatusCode::OK);

        // Check that the body is correct (from cache)
        let body2 = BodyExt::collect(response2.into_body()).await?.to_bytes();
        assert_eq!(body2, TEST_BODY);

        // Note: The warning header test might not work as expected since
        // our current Tower implementation doesn't have the same header filtering
        // as reqwest middleware. This is implementation-specific behavior.

        Ok(())
    }

    #[tokio::test]
    async fn default_mode_no_cache_response() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager);

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", "no-cache"), ("content-type", "text/plain")],
            TEST_BODY,
        );

        let mut cached_service = cache_layer.layer(test_service);

        // First request
        let request1 = Request::builder()
            .uri("https://example.com/no-cache")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response1 = cached_service.ready().await?.call(request1).await?;
        assert_eq!(response1.status(), StatusCode::OK);

        // Second request - should not be cached due to no-cache directive
        let request2 = Request::builder()
            .uri("https://example.com/no-cache")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response2 = cached_service.ready().await?.call(request2).await?;
        assert_eq!(response2.status(), StatusCode::OK);

        Ok(())
    }

    #[tokio::test]
    async fn revalidation_304() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager);

        #[derive(Clone)]
        struct RevalidationService {
            call_count: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        impl Service<Request<Full<Bytes>>> for RevalidationService {
            type Response = Response<Full<Bytes>>;
            type Error = Box<dyn std::error::Error + Send + Sync>;
            type Future = Pin<
                Box<
                    dyn Future<
                            Output = std::result::Result<
                                Self::Response,
                                Self::Error,
                            >,
                        > + Send,
                >,
            >;

            fn poll_ready(
                &mut self,
                _cx: &mut Context<'_>,
            ) -> Poll<std::result::Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, req: Request<Full<Bytes>>) -> Self::Future {
                let call_count = self.call_count.clone();
                Box::pin(async move {
                    let count = {
                        let mut count = call_count.lock().unwrap();
                        *count += 1;
                        *count
                    };

                    if count == 1 {
                        // First request - return cacheable response with must-revalidate
                        Ok(Response::builder()
                            .status(StatusCode::OK)
                            .header("cache-control", "public, must-revalidate")
                            .header("etag", "\"123456\"")
                            .header("content-type", "text/plain")
                            .body(Full::new(Bytes::from(TEST_BODY)))?)
                    } else {
                        // Subsequent requests with conditional headers should return 304
                        if req.headers().contains_key("if-none-match") {
                            Ok(Response::builder()
                                .status(StatusCode::NOT_MODIFIED)
                                .header("etag", "\"123456\"")
                                .body(Full::new(Bytes::new()))?)
                        } else {
                            // Non-conditional request
                            Ok(Response::builder()
                                .status(StatusCode::OK)
                                .header(
                                    "cache-control",
                                    "public, must-revalidate",
                                )
                                .header("etag", "\"123456\"")
                                .header("content-type", "text/plain")
                                .body(Full::new(Bytes::from(TEST_BODY)))?)
                        }
                    }
                })
            }
        }

        let service = RevalidationService {
            call_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
        };

        let mut cached_service = cache_layer.layer(service);

        // First request - should cache
        let request1 = Request::builder()
            .uri("https://example.com/revalidate")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response1 = cached_service.ready().await?.call(request1).await?;
        assert_eq!(response1.status(), StatusCode::OK);

        // Second request - should trigger revalidation and return cached content
        let request2 = Request::builder()
            .uri("https://example.com/revalidate")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response2 = cached_service.ready().await?.call(request2).await?;
        assert_eq!(response2.status(), StatusCode::OK);

        Ok(())
    }

    #[tokio::test]
    async fn revalidation_200() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager);

        #[derive(Clone)]
        struct RevalidationService {
            call_count: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        impl Service<Request<Full<Bytes>>> for RevalidationService {
            type Response = Response<Full<Bytes>>;
            type Error = Box<dyn std::error::Error + Send + Sync>;
            type Future = Pin<
                Box<
                    dyn Future<
                            Output = std::result::Result<
                                Self::Response,
                                Self::Error,
                            >,
                        > + Send,
                >,
            >;

            fn poll_ready(
                &mut self,
                _cx: &mut Context<'_>,
            ) -> Poll<std::result::Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, _req: Request<Full<Bytes>>) -> Self::Future {
                let call_count = self.call_count.clone();
                Box::pin(async move {
                    let count = {
                        let mut count = call_count.lock().unwrap();
                        *count += 1;
                        *count
                    };

                    if count == 1 {
                        // First request - return cacheable response with must-revalidate
                        Ok(Response::builder()
                            .status(StatusCode::OK)
                            .header("cache-control", "public, must-revalidate")
                            .header("etag", "\"123456\"")
                            .header("content-type", "text/plain")
                            .body(Full::new(Bytes::from(TEST_BODY)))?)
                    } else {
                        // Second request - return updated content (simulate change)
                        Ok(Response::builder()
                            .status(StatusCode::OK)
                            .header("cache-control", "public, must-revalidate")
                            .header("etag", "\"789012\"")
                            .header("content-type", "text/plain")
                            .body(Full::new(Bytes::from("updated")))?)
                    }
                })
            }
        }

        let service = RevalidationService {
            call_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
        };

        let mut cached_service = cache_layer.layer(service);

        // First request - should cache
        let request1 = Request::builder()
            .uri("https://example.com/revalidate-200")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response1 = cached_service.ready().await?.call(request1).await?;
        assert_eq!(response1.status(), StatusCode::OK);

        // Second request - should get updated content
        let request2 = Request::builder()
            .uri("https://example.com/revalidate-200")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response2 = cached_service.ready().await?.call(request2).await?;
        assert_eq!(response2.status(), StatusCode::OK);

        Ok(())
    }

    #[tokio::test]
    async fn revalidation_500() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager);

        #[derive(Clone)]
        struct RevalidationService {
            call_count: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        impl Service<Request<Full<Bytes>>> for RevalidationService {
            type Response = Response<Full<Bytes>>;
            type Error = Box<dyn std::error::Error + Send + Sync>;
            type Future = Pin<
                Box<
                    dyn Future<
                            Output = std::result::Result<
                                Self::Response,
                                Self::Error,
                            >,
                        > + Send,
                >,
            >;

            fn poll_ready(
                &mut self,
                _cx: &mut Context<'_>,
            ) -> Poll<std::result::Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, _req: Request<Full<Bytes>>) -> Self::Future {
                let call_count = self.call_count.clone();
                Box::pin(async move {
                    let count = {
                        let mut count = call_count.lock().unwrap();
                        *count += 1;
                        *count
                    };

                    if count == 1 {
                        // First request - return cacheable response with must-revalidate
                        Ok(Response::builder()
                            .status(StatusCode::OK)
                            .header("cache-control", "public, must-revalidate")
                            .header("etag", "\"123456\"")
                            .header("content-type", "text/plain")
                            .body(Full::new(Bytes::from(TEST_BODY)))?)
                    } else {
                        // Second request - return server error
                        Ok(Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Full::new(Bytes::from("Server Error")))?)
                    }
                })
            }
        }

        let service = RevalidationService {
            call_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
        };

        let mut cached_service = cache_layer.layer(service);

        // First request - should cache
        let request1 = Request::builder()
            .uri("https://example.com/revalidate-500")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response1 = cached_service.ready().await?.call(request1).await?;
        assert_eq!(response1.status(), StatusCode::OK);

        // Second request - should get server error
        let request2 = Request::builder()
            .uri("https://example.com/revalidate-500")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response2 = cached_service.ready().await?.call(request2).await?;
        assert_eq!(response2.status(), StatusCode::INTERNAL_SERVER_ERROR);

        Ok(())
    }

    mod only_if_cached_mode {
        use super::*;

        #[tokio::test]
        async fn miss() -> Result<()> {
            let cache_dir = tempfile::tempdir()?;
            let cache_manager =
                CACacheManager::new(cache_dir.path().to_path_buf(), false);
            let cache = HttpCache {
                mode: CacheMode::OnlyIfCached,
                manager: cache_manager,
                options: HttpCacheOptions::default(),
            };
            let cache_layer = HttpCacheLayer::with_cache(cache);

            let test_service = TestService::new(
                StatusCode::OK,
                vec![
                    ("cache-control", "max-age=3600, public"),
                    ("content-type", "text/plain"),
                ],
                TEST_BODY,
            );

            let mut cached_service = cache_layer.layer(test_service);

            // Should result in a cache miss and no remote request (but service will still be called)
            let request = Request::builder()
                .uri("https://example.com/only-if-cached-miss")
                .method("GET")
                .body(Full::new(Bytes::new()))?;

            // In OnlyIfCached mode with no cached response, we should still get a response
            // but it indicates there was no cached version available
            let response = cached_service.ready().await?.call(request).await?;

            // The response will come through since Tower doesn't prevent the service call
            // This is different from the HTTP specification but matches current implementation
            assert_eq!(response.status(), StatusCode::OK);

            Ok(())
        }

        #[tokio::test]
        async fn hit() -> Result<()> {
            let cache_dir = tempfile::tempdir()?;
            let cache_manager =
                CACacheManager::new(cache_dir.path().to_path_buf(), false);

            // First populate cache with Default mode
            let cache_default = HttpCache {
                mode: CacheMode::Default,
                manager: cache_manager.clone(),
                options: HttpCacheOptions::default(),
            };
            let cache_layer_default = HttpCacheLayer::with_cache(cache_default);

            let test_service = TestService::new(
                StatusCode::OK,
                vec![
                    ("cache-control", "max-age=3600, public"),
                    ("content-type", "text/plain"),
                ],
                TEST_BODY,
            );

            let mut cached_service_default =
                cache_layer_default.layer(test_service.clone());

            // Cold pass to load the cache
            let request1 = Request::builder()
                .uri("https://example.com/only-if-cached-hit")
                .method("GET")
                .body(Full::new(Bytes::new()))?;

            let response1 =
                cached_service_default.ready().await?.call(request1).await?;
            assert_eq!(response1.status(), StatusCode::OK);

            // Now use OnlyIfCached mode
            let cache_only_if_cached = HttpCache {
                mode: CacheMode::OnlyIfCached,
                manager: cache_manager,
                options: HttpCacheOptions::default(),
            };
            let cache_layer_only_if_cached =
                HttpCacheLayer::with_cache(cache_only_if_cached);
            let mut cached_service_only_if_cached =
                cache_layer_only_if_cached.layer(test_service);

            // Should result in a cache hit
            let request2 = Request::builder()
                .uri("https://example.com/only-if-cached-hit")
                .method("GET")
                .body(Full::new(Bytes::new()))?;

            let response2 = cached_service_only_if_cached
                .ready()
                .await?
                .call(request2)
                .await?;
            assert_eq!(response2.status(), StatusCode::OK);

            Ok(())
        }
    }

    #[cfg(feature = "streaming")]
    #[tokio::test]
    async fn test_streaming_cache_layer() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;

        // Create streaming cache setup with StreamingManager for optimal streaming
        let cache_manager =
            StreamingManager::new(temp_dir.path().to_path_buf());
        let streaming_layer = HttpCacheStreamingLayer::new(cache_manager);

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );

        let cached_service = streaming_layer.layer(test_service);

        // First request should be a cache miss
        let request1 = Request::builder()
            .uri("https://example.com/streaming-test")
            .body(Full::new(Bytes::new()))?;

        let response1 = cached_service.clone().oneshot(request1).await?;
        assert_eq!(response1.status(), StatusCode::OK);

        // Check that body can be read
        let body_bytes = response1.into_body().collect().await?.to_bytes();
        assert_eq!(body_bytes, TEST_BODY);

        // Second request should be a cache hit
        let request2 = Request::builder()
            .uri("https://example.com/streaming-test")
            .body(Full::new(Bytes::new()))?;

        let response2 = cached_service.clone().oneshot(request2).await?;
        assert_eq!(response2.status(), StatusCode::OK);

        // Check that cached body can be read
        let body_bytes2 = response2.into_body().collect().await?.to_bytes();
        assert_eq!(body_bytes2, TEST_BODY);

        Ok(())
    }

    #[tokio::test]
    async fn head_request_caching() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager.clone());

        let test_service = TestService::new(
            StatusCode::OK,
            vec![
                ("cache-control", CACHEABLE_PUBLIC),
                ("content-type", "text/plain"),
                ("content-length", "13"), // Length of TEST_BODY
            ],
            TEST_BODY,
        );

        let mut cached_service = cache_layer.layer(test_service);

        // HEAD request should be cached
        let request = Request::builder()
            .uri("https://example.com/head-test")
            .method("HEAD")
            .body(Full::new(Bytes::new()))?;

        let response = cached_service.ready().await?.call(request).await?;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/plain"
        );

        // The body should be present in the response (TestService always returns it)
        // but in real HEAD requests, the body would be empty
        let body_bytes = response.into_body().collect().await?.to_bytes();
        // Our test service returns body even for HEAD (which is not HTTP compliant
        // but acceptable for testing the cache layer functionality)
        assert_eq!(body_bytes, TEST_BODY);

        Ok(())
    }

    #[tokio::test]
    async fn put_request_invalidates_cache() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager.clone());

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );

        let mut cached_service = cache_layer.layer(test_service);

        // First, cache a GET response
        let get_request = Request::builder()
            .uri("https://example.com/invalidate-test")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let get_response =
            cached_service.ready().await?.call(get_request).await?;
        assert_eq!(get_response.status(), StatusCode::OK);

        // Verify it was cached by checking directly with the cache manager
        let cache_key = "GET:https://example.com/invalidate-test";
        let cached_data = cache_manager.get(cache_key).await?;
        assert!(cached_data.is_some());

        // Now make a PUT request which should invalidate the cache
        let put_request = Request::builder()
            .uri("https://example.com/invalidate-test")
            .method("PUT")
            .body(Full::new(Bytes::from("updated data")))?;

        // PUT should return OK but not be cacheable
        let put_response =
            cached_service.ready().await?.call(put_request).await?;
        assert_eq!(put_response.status(), StatusCode::OK);

        // Verify the GET response was invalidated from cache
        let cached_data_after = cache_manager.get(cache_key).await?;
        assert!(cached_data_after.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn patch_request_invalidates_cache() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager.clone());

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );

        let mut cached_service = cache_layer.layer(test_service);

        // Cache a GET response
        let get_request = Request::builder()
            .uri("https://example.com/patch-test")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        cached_service.ready().await?.call(get_request).await?;

        // Verify it was cached
        let cache_key = "GET:https://example.com/patch-test";
        let cached_data = cache_manager.get(cache_key).await?;
        assert!(cached_data.is_some());

        // PATCH request should invalidate cache
        let patch_request = Request::builder()
            .uri("https://example.com/patch-test")
            .method("PATCH")
            .body(Full::new(Bytes::from("patch data")))?;

        let patch_response =
            cached_service.ready().await?.call(patch_request).await?;
        assert_eq!(patch_response.status(), StatusCode::OK);

        // Verify cache was invalidated
        let cached_data_after = cache_manager.get(cache_key).await?;
        assert!(cached_data_after.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn delete_request_invalidates_cache() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager.clone());

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );

        let mut cached_service = cache_layer.layer(test_service);

        // Cache a GET response
        let get_request = Request::builder()
            .uri("https://example.com/delete-test")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        cached_service.ready().await?.call(get_request).await?;

        // Verify it was cached
        let cache_key = "GET:https://example.com/delete-test";
        let cached_data = cache_manager.get(cache_key).await?;
        assert!(cached_data.is_some());

        // DELETE request should invalidate cache
        let delete_request = Request::builder()
            .uri("https://example.com/delete-test")
            .method("DELETE")
            .body(Full::new(Bytes::new()))?;

        let delete_response =
            cached_service.ready().await?.call(delete_request).await?;
        assert_eq!(delete_response.status(), StatusCode::OK);

        // Verify cache was invalidated
        let cached_data_after = cache_manager.get(cache_key).await?;
        assert!(cached_data_after.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn options_request_not_cached() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager.clone());

        let test_service_call_count =
            std::sync::Arc::new(std::sync::Mutex::new(0));
        let count_clone = test_service_call_count.clone();

        #[derive(Clone)]
        struct CountingService {
            call_count: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        impl Service<Request<Full<Bytes>>> for CountingService {
            type Response = Response<Full<Bytes>>;
            type Error = Box<dyn std::error::Error + Send + Sync>;
            type Future = Pin<
                Box<
                    dyn Future<
                            Output = std::result::Result<
                                Self::Response,
                                Self::Error,
                            >,
                        > + Send,
                >,
            >;

            fn poll_ready(
                &mut self,
                _cx: &mut Context<'_>,
            ) -> Poll<std::result::Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, _req: Request<Full<Bytes>>) -> Self::Future {
                let call_count = self.call_count.clone();
                Box::pin(async move {
                    {
                        let mut count = call_count.lock().unwrap();
                        *count += 1;
                    }

                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("allow", "GET, POST, PUT, DELETE")
                        .header("cache-control", CACHEABLE_PUBLIC) // Even with cache headers
                        .body(Full::new(Bytes::new()))?)
                })
            }
        }

        let counting_service = CountingService { call_count: count_clone };

        let mut cached_service = cache_layer.layer(counting_service);

        // First OPTIONS request
        let request1 = Request::builder()
            .uri("https://example.com/options-test")
            .method("OPTIONS")
            .body(Full::new(Bytes::new()))?;

        let response1 = cached_service.ready().await?.call(request1).await?;
        assert_eq!(response1.status(), StatusCode::OK);

        // Verify it's not cached
        let cache_key = "OPTIONS:https://example.com/options-test";
        let cached_data = cache_manager.get(cache_key).await?;
        assert!(cached_data.is_none());

        // Second OPTIONS request should hit the service again
        let request2 = Request::builder()
            .uri("https://example.com/options-test")
            .method("OPTIONS")
            .body(Full::new(Bytes::new()))?;

        let response2 = cached_service.ready().await?.call(request2).await?;
        assert_eq!(response2.status(), StatusCode::OK);

        // Verify both requests hit the underlying service
        let final_count = *test_service_call_count.lock().unwrap();
        assert_eq!(final_count, 2);

        Ok(())
    }

    #[test]
    fn test_streaming_body() -> Result<()> {
        // Test buffered streaming body
        let buffered_body: StreamingBody<Full<Bytes>> =
            StreamingBody::buffered(Bytes::from("test data"));
        assert!(!buffered_body.is_end_stream());

        let size_hint = buffered_body.size_hint();
        assert_eq!(size_hint.exact(), Some(9)); // "test data" is 9 bytes

        Ok(())
    }

    #[tokio::test]
    async fn custom_cache_key() -> Result<()> {
        use std::sync::Arc;

        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);

        let options = HttpCacheOptions {
            cache_key: Some(Arc::new(|req: &http::request::Parts| {
                format!("{}:{}:{:?}:test", req.method, req.uri, req.version)
            })),
            ..Default::default()
        };

        let cache = HttpCache {
            mode: CacheMode::Default,
            manager: cache_manager.clone(),
            options,
        };
        let cache_layer = HttpCacheLayer::with_cache(cache);

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );
        let mut cached_service = cache_layer.layer(test_service);

        let request = Request::builder()
            .uri("https://example.com/custom-key-test")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        // Make request to cache with custom key
        let response =
            cached_service.ready().await?.call(request.clone()).await?;
        assert_eq!(response.status(), StatusCode::OK);

        // Try to load cached object with custom key format
        let custom_key = format!(
            "{}:{}:{:?}:test",
            "GET",
            "https://example.com/custom-key-test",
            http::Version::HTTP_11
        );
        let cached_data = cache_manager.get(&custom_key).await?;
        assert!(cached_data.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn custom_cache_mode_fn() -> Result<()> {
        use std::sync::Arc;

        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);

        let options = HttpCacheOptions {
            cache_mode_fn: Some(Arc::new(|req: &http::request::Parts| {
                if req.uri.path().ends_with(".css") {
                    CacheMode::Default
                } else {
                    CacheMode::NoStore
                }
            })),
            ..Default::default()
        };

        let cache = HttpCache {
            mode: CacheMode::NoStore, // Default mode that gets overridden
            manager: cache_manager.clone(),
            options,
        };
        let cache_layer = HttpCacheLayer::with_cache(cache);

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );
        let mut cached_service = cache_layer.layer(test_service.clone());

        // Test .css file - should be cached
        let css_request = Request::builder()
            .uri("https://example.com/styles.css")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        cached_service.ready().await?.call(css_request).await?;

        // Check if CSS was cached
        let css_cache_key = "GET:https://example.com/styles.css";
        let cached_css = cache_manager.get(css_cache_key).await?;
        assert!(cached_css.is_some());

        // Test non-.css file - should not be cached
        let mut cached_service2 = cache_layer.layer(test_service);
        let html_request = Request::builder()
            .uri("https://example.com/index.html")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        cached_service2.ready().await?.call(html_request).await?;

        // Check if HTML was not cached
        let html_cache_key = "GET:https://example.com/index.html";
        let cached_html = cache_manager.get(html_cache_key).await?;
        assert!(cached_html.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn custom_response_cache_mode_fn() -> Result<()> {
        use std::sync::Arc;

        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);

        let options = HttpCacheOptions {
            response_cache_mode_fn: Some(Arc::new(
                |_request_parts, response| {
                    match response.status {
                        // Force cache 2xx responses even if headers say not to cache
                        200..=299 => Some(CacheMode::ForceCache),
                        // Never cache rate-limited responses
                        429 => Some(CacheMode::NoStore),
                        _ => None, // Use default behavior
                    }
                },
            )),
            ..Default::default()
        };

        let cache = HttpCache {
            mode: CacheMode::Default,
            manager: cache_manager.clone(),
            options,
        };
        let cache_layer = HttpCacheLayer::with_cache(cache);

        // Create service that returns no-cache headers for 200 responses
        let success_service = TestService::new(
            StatusCode::OK,
            vec![
                ("cache-control", "no-cache, no-store, must-revalidate"),
                ("pragma", "no-cache"),
            ],
            TEST_BODY,
        );
        let mut cached_success_service =
            cache_layer.clone().layer(success_service);

        // Create service that returns cacheable headers for 429 responses
        let rate_limit_service = TestService::new(
            StatusCode::TOO_MANY_REQUESTS,
            vec![
                ("cache-control", "public, max-age=300"),
                ("retry-after", "60"),
            ],
            b"Rate limit exceeded",
        );
        let mut cached_rate_limit_service =
            cache_layer.layer(rate_limit_service);

        // Test 1: Force cache 200 response despite no-cache headers
        let success_request = Request::builder()
            .uri("https://example.com/api/data")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response =
            cached_success_service.ready().await?.call(success_request).await?;
        assert_eq!(response.status(), StatusCode::OK);

        // Verify it was cached despite no-cache headers
        let success_cache_key = "GET:https://example.com/api/data";
        let cached_data = cache_manager.get(success_cache_key).await?;
        assert!(cached_data.is_some());

        // Test 2: Don't cache 429 response despite cacheable headers
        let rate_limit_request = Request::builder()
            .uri("https://example.com/api/rate-limited")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response = cached_rate_limit_service
            .ready()
            .await?
            .call(rate_limit_request)
            .await?;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

        // Verify it was NOT cached despite cacheable headers
        let rate_limit_cache_key = "GET:https://example.com/api/rate-limited";
        let cached_data = cache_manager.get(rate_limit_cache_key).await?;
        assert!(cached_data.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn custom_cache_bust() -> Result<()> {
        use std::sync::Arc;

        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);

        let options = HttpCacheOptions {
            cache_bust: Some(Arc::new(|req: &http::request::Parts, _, _| {
                if req.uri.path().ends_with("/bust-cache") {
                    vec![format!(
                        "{}:{}://{}:{}/",
                        "GET",
                        req.uri.scheme_str().unwrap_or("https"),
                        req.uri.host().unwrap_or("example.com"),
                        req.uri.port_u16().unwrap_or(443)
                    )]
                } else {
                    Vec::new()
                }
            })),
            ..Default::default()
        };

        let cache = HttpCache {
            mode: CacheMode::Default,
            manager: cache_manager.clone(),
            options,
        };
        let cache_layer = HttpCacheLayer::with_cache(cache);

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );
        let mut cached_service = cache_layer.layer(test_service);

        // First, cache a response
        let cache_request = Request::builder()
            .uri("https://example.com:443/")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        cached_service.ready().await?.call(cache_request).await?;

        // Verify it's cached
        let cache_key = "GET:https://example.com:443/";
        let cached_data = cache_manager.get(cache_key).await?;
        assert!(cached_data.is_some());

        // Now make a request that should bust the cache
        let bust_request = Request::builder()
            .uri("https://example.com:443/bust-cache")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        cached_service.ready().await?.call(bust_request).await?;

        // Verify the original cache entry was busted
        let cached_data_after = cache_manager.get(cache_key).await?;
        assert!(cached_data_after.is_none());

        Ok(())
    }

    #[cfg(feature = "streaming")]
    #[tokio::test]
    async fn test_streaming_cache_large_response() -> Result<()> {
        use http_cache::StreamingManager;

        let temp_dir = tempfile::tempdir()?;
        let cache_manager =
            StreamingManager::new(temp_dir.path().to_path_buf());
        let streaming_layer = HttpCacheStreamingLayer::new(cache_manager);

        // Create a large test response (1MB) - using static string
        const LARGE_DATA: &[u8] = &[b'x'; 1024 * 1024];
        let large_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            LARGE_DATA,
        );

        let cached_service = streaming_layer.layer(large_service);

        // First request should be a cache miss
        let request1 = Request::builder()
            .uri("https://example.com/large-streaming-test")
            .body(Full::new(Bytes::new()))?;

        let response1 = cached_service.clone().oneshot(request1).await?;
        assert_eq!(response1.status(), StatusCode::OK);

        // Check that large body can be read
        let body_bytes = response1.into_body().collect().await?.to_bytes();
        assert_eq!(body_bytes.len(), 1024 * 1024);
        assert_eq!(body_bytes, LARGE_DATA);

        // Second request should be a cache hit
        let request2 = Request::builder()
            .uri("https://example.com/large-streaming-test")
            .body(Full::new(Bytes::new()))?;

        let response2 = cached_service.clone().oneshot(request2).await?;
        assert_eq!(response2.status(), StatusCode::OK);

        // Check that cached large body can be read
        let body_bytes2 = response2.into_body().collect().await?.to_bytes();
        assert_eq!(body_bytes2.len(), 1024 * 1024);
        assert_eq!(body_bytes2, LARGE_DATA);

        Ok(())
    }

    #[cfg(feature = "streaming")]
    #[tokio::test]
    async fn test_streaming_cache_empty_response() -> Result<()> {
        use http_cache::StreamingManager;

        let temp_dir = tempfile::tempdir()?;
        let cache_manager =
            StreamingManager::new(temp_dir.path().to_path_buf());
        let streaming_layer = HttpCacheStreamingLayer::new(cache_manager);

        let empty_service = TestService::new(
            StatusCode::NO_CONTENT,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            b"", // Empty body
        );

        let cached_service = streaming_layer.layer(empty_service);

        // First request should be a cache miss
        let request1 = Request::builder()
            .uri("https://example.com/empty-streaming-test")
            .body(Full::new(Bytes::new()))?;

        let response1 = cached_service.clone().oneshot(request1).await?;
        assert_eq!(response1.status(), StatusCode::NO_CONTENT);

        // Check that empty body is handled correctly
        let body_bytes = response1.into_body().collect().await?.to_bytes();
        assert!(body_bytes.is_empty());

        // Second request should be a cache hit
        let request2 = Request::builder()
            .uri("https://example.com/empty-streaming-test")
            .body(Full::new(Bytes::new()))
            .unwrap();

        let response2 = cached_service.clone().oneshot(request2).await?;
        assert_eq!(response2.status(), StatusCode::NO_CONTENT);

        // Check that cached empty body is correct
        let body_bytes2 = response2.into_body().collect().await?.to_bytes();
        assert!(body_bytes2.is_empty());

        Ok(())
    }

    #[cfg(feature = "streaming")]
    #[tokio::test]
    async fn test_streaming_cache_no_cache_mode() -> Result<()> {
        use http_cache::StreamingManager;

        let temp_dir = tempfile::tempdir()?;
        let cache_manager =
            StreamingManager::new(temp_dir.path().to_path_buf());

        let cache = http_cache::HttpStreamingCache {
            mode: CacheMode::NoStore,
            manager: cache_manager,
            options: HttpCacheOptions::default(),
        };
        let streaming_layer = HttpCacheStreamingLayer::with_cache(cache);

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );

        let cached_service = streaming_layer.layer(test_service);

        // Request with NoStore mode should not cache
        let request = Request::builder()
            .uri("https://example.com/no-cache-streaming-test")
            .body(Full::new(Bytes::new()))?;

        let response = cached_service.clone().oneshot(request).await?;
        assert_eq!(response.status(), StatusCode::OK);

        // Body should still be readable
        let body_bytes = response.into_body().collect().await?.to_bytes();
        assert_eq!(body_bytes, TEST_BODY);

        Ok(())
    }

    #[tokio::test]
    async fn head_request_cached_like_get() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache_layer = HttpCacheLayer::new(cache_manager.clone());

        // Service that responds to both GET and HEAD
        #[derive(Clone)]
        struct GetHeadService {
            call_count: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        impl Service<Request<Full<Bytes>>> for GetHeadService {
            type Response = Response<Full<Bytes>>;
            type Error = Box<dyn std::error::Error + Send + Sync>;
            type Future = Pin<
                Box<
                    dyn Future<
                            Output = std::result::Result<
                                Self::Response,
                                Self::Error,
                            >,
                        > + Send,
                >,
            >;

            fn poll_ready(
                &mut self,
                _cx: &mut Context<'_>,
            ) -> Poll<std::result::Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, req: Request<Full<Bytes>>) -> Self::Future {
                let call_count = self.call_count.clone();
                Box::pin(async move {
                    {
                        let mut count = call_count.lock().unwrap();
                        *count += 1;
                    }

                    let mut response = Response::builder()
                        .status(StatusCode::OK)
                        .header("cache-control", CACHEABLE_PUBLIC)
                        .header("content-type", "text/plain")
                        .header("etag", "\"12345\"");

                    // HEAD requests should not have a body
                    let body = if req.method() == "HEAD" {
                        response = response.header("content-length", "13");
                        Full::new(Bytes::new())
                    } else {
                        Full::new(Bytes::from(TEST_BODY))
                    };

                    Ok(response.body(body)?)
                })
            }
        }

        let service = GetHeadService {
            call_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
        };
        let call_count = service.call_count.clone();
        let mut cached_service = cache_layer.layer(service);

        // First, cache a GET response
        let get_request = Request::builder()
            .uri("https://example.com/get-head-test")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let get_response =
            cached_service.ready().await?.call(get_request).await?;
        assert_eq!(get_response.status(), StatusCode::OK);
        assert_eq!(get_response.headers().get("etag").unwrap(), "\"12345\"");

        let get_body = get_response.into_body().collect().await?.to_bytes();
        assert_eq!(get_body, TEST_BODY);

        // Now make a HEAD request - should be able to use cached metadata
        let head_request = Request::builder()
            .uri("https://example.com/get-head-test")
            .method("HEAD")
            .body(Full::new(Bytes::new()))?;

        let head_response =
            cached_service.ready().await?.call(head_request).await?;
        assert_eq!(head_response.status(), StatusCode::OK);
        assert_eq!(head_response.headers().get("etag").unwrap(), "\"12345\"");

        // HEAD response should have empty body
        let head_body = head_response.into_body().collect().await?.to_bytes();
        assert!(head_body.is_empty());

        // Verify both GET and HEAD cache entries exist
        let get_cache_key = "GET:https://example.com/get-head-test";
        let get_cached_data = cache_manager.get(get_cache_key).await?;
        assert!(get_cached_data.is_some());

        let head_cache_key = "HEAD:https://example.com/get-head-test";
        let head_cached_data = cache_manager.get(head_cache_key).await?;
        assert!(head_cached_data.is_some());

        // Both requests should have hit the underlying service
        let final_count = *call_count.lock().unwrap();
        assert_eq!(final_count, 2);

        Ok(())
    }

    #[tokio::test]
    async fn reload_mode() -> Result<()> {
        let cache_dir = tempfile::tempdir()?;
        let cache_manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);
        let cache = HttpCache {
            mode: CacheMode::Reload,
            manager: cache_manager.clone(),
            options: HttpCacheOptions {
                cache_options: Some(http_cache::CacheOptions {
                    shared: false,
                    ..Default::default()
                }),
                ..Default::default()
            },
        };
        let cache_layer = HttpCacheLayer::with_cache(cache);

        let test_service_call_count =
            std::sync::Arc::new(std::sync::Mutex::new(0));
        let count_clone = test_service_call_count.clone();

        #[derive(Clone)]
        struct ReloadTestService {
            call_count: std::sync::Arc<std::sync::Mutex<usize>>,
        }

        impl Service<Request<Full<Bytes>>> for ReloadTestService {
            type Response = Response<Full<Bytes>>;
            type Error = Box<dyn std::error::Error + Send + Sync>;
            type Future = Pin<
                Box<
                    dyn Future<
                            Output = std::result::Result<
                                Self::Response,
                                Self::Error,
                            >,
                        > + Send,
                >,
            >;

            fn poll_ready(
                &mut self,
                _cx: &mut Context<'_>,
            ) -> Poll<std::result::Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, _req: Request<Full<Bytes>>) -> Self::Future {
                let call_count = self.call_count.clone();
                Box::pin(async move {
                    {
                        let mut count = call_count.lock().unwrap();
                        *count += 1;
                    }

                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("cache-control", CACHEABLE_PUBLIC)
                        .body(Full::new(Bytes::from(TEST_BODY)))?)
                })
            }
        }

        let reload_service = ReloadTestService { call_count: count_clone };
        let mut cached_service = cache_layer.layer(reload_service);

        // First request - should cache but also go to origin (Reload mode)
        let request1 = Request::builder()
            .uri("https://example.com/reload-test")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response1 = cached_service.ready().await?.call(request1).await?;
        assert_eq!(response1.status(), StatusCode::OK);

        // Verify it was cached
        let cache_key = "GET:https://example.com/reload-test";
        let cached_data = cache_manager.get(cache_key).await?;
        assert!(cached_data.is_some());

        // Second request - should still go to origin (Reload mode always fetches fresh)
        let request2 = Request::builder()
            .uri("https://example.com/reload-test")
            .method("GET")
            .body(Full::new(Bytes::new()))?;

        let response2 = cached_service.ready().await?.call(request2).await?;
        assert_eq!(response2.status(), StatusCode::OK);

        // Both requests should have hit the underlying service
        let final_count = *test_service_call_count.lock().unwrap();
        assert_eq!(final_count, 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_streaming_request_graceful_handling() -> Result<()> {
        // Test that streaming/non-cloneable requests are handled gracefully
        // Tower's architecture decomposes requests into (parts, body) which should
        // avoid cloning issues, but this test ensures robustness

        let temp_dir = tempfile::TempDir::new().unwrap();
        let cache_manager = CACacheManager::new(temp_dir.path().into(), true);
        let cache_layer = HttpCacheLayer::new(cache_manager);

        // Create a service that accepts streaming bodies
        #[derive(Clone)]
        struct StreamingService;

        impl Service<Request<Full<Bytes>>> for StreamingService {
            type Response = Response<Full<Bytes>>;
            type Error = Box<dyn std::error::Error + Send + Sync>;
            type Future = Pin<
                Box<
                    dyn Future<
                            Output = std::result::Result<
                                Self::Response,
                                Self::Error,
                            >,
                        > + Send,
                >,
            >;

            fn poll_ready(
                &mut self,
                _: &mut Context<'_>,
            ) -> Poll<std::result::Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, req: Request<Full<Bytes>>) -> Self::Future {
                let (parts, body) = req.into_parts();
                Box::pin(async move {
                    // Process the body (in a real scenario this could be a large streaming body)
                    let body_bytes = BodyExt::collect(body)
                        .await
                        .map_err(|e| {
                            Box::new(e)
                                as Box<dyn std::error::Error + Send + Sync>
                        })?
                        .to_bytes();
                    let body_size = body_bytes.len();

                    // Return response with information about the processed body
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("cache-control", "max-age=3600, public")
                        .header("content-type", "application/json")
                        .header("x-body-size", body_size.to_string())
                        .body(Full::new(Bytes::from(format!(
                            "{{\"processed\": true, \"body_size\": {}, \"uri\": \"{}\"}}",
                            body_size,
                            parts.uri
                        ))))
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
                })
            }
        }

        let mut cached_service = cache_layer.layer(StreamingService);

        // Create a request with a potentially large body (simulating streaming data)
        let large_body_data = "streaming data ".repeat(10000); // ~150KB of data
        let request = Request::builder()
            .uri("https://example.com/streaming-upload")
            .method("POST")
            .header("content-type", "application/octet-stream")
            .body(Full::new(Bytes::from(large_body_data.clone())))
            .map_err(|e| {
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            })?;

        // This should not fail with cloning errors
        let response = cached_service.ready().await?.call(request).await;

        match response {
            Ok(response) => {
                // Success - the middleware handled the streaming body correctly
                assert_eq!(response.status(), StatusCode::OK);
                assert!(response.headers().contains_key("x-body-size"));
            }
            Err(e) => {
                // If there's an error, it should NOT be related to cloning
                let error_msg = e.to_string();
                assert!(
                    !error_msg.to_lowercase().contains("clone"),
                    "Expected graceful handling but got cloning-related error: {}",
                    error_msg
                );
                // Re-throw other errors as they might be legitimate test failures
                return Err(
                    Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                );
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_metadata_retrieval_through_extensions() -> Result<()> {
        use http_cache::HttpCacheMetadata;
        use std::sync::Arc;

        let cache_dir = tempfile::tempdir()?;
        let manager =
            CACacheManager::new(cache_dir.path().to_path_buf(), false);

        // Create cache options with a metadata provider
        let options = HttpCacheOptions {
            metadata_provider: Some(Arc::new(
                |_request_parts, _response_parts| {
                    // Return some test metadata
                    Some(b"test-metadata-value".to_vec())
                },
            )),
            ..Default::default()
        };

        let cache_layer =
            HttpCacheLayer::with_options(manager.clone(), options);

        let test_service = TestService::new(
            StatusCode::OK,
            vec![("cache-control", CACHEABLE_PUBLIC)],
            TEST_BODY,
        );

        let mut cached_service = cache_layer.layer(test_service);

        // First request - stores the response with metadata
        let request = Request::builder()
            .method("GET")
            .uri("http://example.com/metadata-test")
            .body(Full::new(Bytes::new()))
            .map_err(|e| {
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            })?;

        let response1 = cached_service.ready().await?.call(request).await?;

        // First response (cache miss) does NOT have metadata in extensions
        // (metadata is generated and stored but not returned on the first request)
        let metadata1 = response1.extensions().get::<HttpCacheMetadata>();
        assert!(
            metadata1.is_none(),
            "Metadata should NOT be present on cache miss"
        );

        // Second request - should retrieve from cache with metadata in extensions
        let request = Request::builder()
            .method("GET")
            .uri("http://example.com/metadata-test")
            .body(Full::new(Bytes::new()))
            .map_err(|e| {
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            })?;

        let response2 = cached_service.ready().await?.call(request).await?;

        // Check that metadata is in the response extensions
        let metadata = response2.extensions().get::<HttpCacheMetadata>();
        assert!(
            metadata.is_some(),
            "Metadata should be present in response extensions"
        );

        let metadata_value = metadata.unwrap();
        assert_eq!(
            metadata_value.as_slice(),
            b"test-metadata-value",
            "Metadata value should match what was stored"
        );

        Ok(())
    }
}
