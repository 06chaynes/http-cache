use http_cache::{CACacheManager, Cache, CacheManager, CacheMode};
use mockito::mock;
use surf::{http::Method, Client, Request, Url};

#[cfg(feature = "client-surf")]
#[async_std::test]
async fn default_mode() -> surf::Result<()> {
    let m = mock("GET", "/")
        .with_status(200)
        .with_header("cache-control", "max-age=86400, public")
        .with_body("test")
        .create();
    let url = format!("{}/", &mockito::server_url());
    let manager = CACacheManager::default();
    let path = manager.path.clone();
    let key = format!("GET:{}", &url);
    let req = Request::new(Method::Get, Url::parse(&url)?);

    // Make sure the record doesn't already exist
    manager.delete("GET", &Url::parse(&url)?).await?;

    // Construct Surf client with cache defaults
    let client = Client::new().with(Cache {
        mode: CacheMode::Default,
        cache_manager: CACacheManager::default(),
    });

    // Cold pass to load cache
    client.send(req.clone()).await?;
    m.assert();

    // Try to load cached object
    let data = cacache::read(&path, &key).await;
    assert!(data.is_ok());
    Ok(())
}
