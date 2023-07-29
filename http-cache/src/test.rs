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
    assert_eq!(format!("{:?}", mode), "Default");
    Ok(())
}

#[test]
fn cache_options() -> Result<()> {
    // Testing the Debug, Default and Clone traits for the HttpCacheOptions struct
    let mut opts = HttpCacheOptions::default();
    assert_eq!(format!("{:?}", opts.clone()), "HttpCacheOptions { cache_options: None, cache_key: \"Fn(&request::Parts) -> String\" }");
    opts.cache_options = Some(CacheOptions::default());
    assert_eq!(format!("{:?}", opts.clone()), "HttpCacheOptions { cache_options: Some(CacheOptions { shared: true, cache_heuristic: 0.1, immutable_min_time_to_live: 86400s, ignore_cargo_cult: false }), cache_key: \"Fn(&request::Parts) -> String\" }");
    opts.cache_options = None;
    opts.cache_key = Some(std::sync::Arc::new(|req: &http::request::Parts| {
        format!("{}:{}:{:?}:test", req.method, req.uri, req.version)
    }));
    assert_eq!(format!("{:?}", opts), "HttpCacheOptions { cache_options: None, cache_key: \"Fn(&request::Parts) -> String\" }");
    Ok(())
}

#[test]
fn test_errors() -> Result<()> {
    // Testing the Debug, Default, Display and Clone traits for the error types
    let bv = error::BadVersion;
    assert_eq!(format!("{:?}", bv.clone()), "BadVersion",);
    assert_eq!(bv.to_string(), "Unknown HTTP version".to_string(),);
    let bh = error::BadHeader;
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
    assert_eq!(format!("{:?}", HttpVersion::H2), "H2");
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

    #[cfg(feature = "cacache-async-std")]
    use async_attributes::test as async_test;
    #[cfg(feature = "cacache-tokio")]
    use tokio::test as async_test;

    #[async_test]
    async fn cacache() -> Result<()> {
        let url = Url::parse("http://example.com")?;
        let manager = CACacheManager { path: "./http-cacache-test".into() };
        assert_eq!(
            &format!("{:?}", manager),
            "CACacheManager { path: \"./http-cacache-test\" }"
        );
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
        std::fs::remove_dir_all("./http-cacache-test")?;
        Ok(())
    }
}

#[cfg(feature = "manager-moka")]
mod with_moka {
    use super::*;
    use crate::{CacheManager, MokaManager};

    use http_cache_semantics::CachePolicy;
    use std::sync::Arc;

    #[async_attributes::test]
    async fn moka() -> Result<()> {
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
