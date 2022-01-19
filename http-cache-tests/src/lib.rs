#![allow(unused_imports, dead_code)]

#[cfg(test)]
mod client_surf;

#[cfg(test)]
mod client_reqwest;

use http::{header::CACHE_CONTROL, StatusCode};
use http_cache::*;
use http_types::{headers::HeaderValue, Method, Version};
use std::{collections::HashMap, convert::TryInto, str::FromStr};
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

const CACHEABLE_PRIVATE: &str = "max-age=86400, private";

#[cfg(test)]
mod http_cache_tests {
    use crate::*;

    #[test]
    fn response_methods_work() -> anyhow::Result<()> {
        let url = Url::from_str("http://example.com")?;
        let mut res = HttpResponse {
            body: TEST_BODY.to_vec(),
            headers: HashMap::default(),
            status: 200,
            url: url.clone(),
            version: HttpVersion::Http11,
        };
        res.add_warning(url, 112, "Test Warning");
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
        res.update_headers(parts)?;
        assert!(res.must_revalidate());
        assert_eq!(res.parts()?.headers, cloned_headers);
        res.headers.remove(CACHE_CONTROL.as_str());
        assert!(!res.must_revalidate());
        Ok(())
    }

    #[test]
    fn can_convert_versions_from_http() -> anyhow::Result<()> {
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

    #[test]
    fn can_convert_versions_from_http_types() -> anyhow::Result<()> {
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
