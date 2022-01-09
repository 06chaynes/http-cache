use crate::{CacheError, HttpResponse, Middleware, Result};

use std::convert::TryFrom;
use std::{collections::HashMap, convert::TryInto, time::SystemTime};

use http::{header::CACHE_CONTROL, request::Parts, HeaderValue};
use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use url::Url;

pub(crate) struct ReqwestMiddleware<'a> {
    pub req: reqwest::Request,
    pub next: reqwest_middleware::Next<'a>,
}

#[async_trait::async_trait]
impl Middleware for ReqwestMiddleware<'_> {
    fn is_method_get_head(&self) -> bool {
        self.req.method() == http::Method::GET || self.req.method() == http::Method::HEAD
    }
    fn new_policy(&self, response: &HttpResponse) -> Result<CachePolicy> {
        Ok(CachePolicy::new(
            &self.get_request_parts()?,
            &response.get_parts()?,
        ))
    }
    fn update_request_headers(&mut self, parts: Parts) -> Result<()> {
        let headers = parts.headers;
        for header in headers.iter() {
            self.req
                .headers_mut()
                .insert(header.0.clone(), header.1.clone());
        }
        Ok(())
    }
    fn set_no_cache(&mut self) -> Result<()> {
        self.req
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_str("no-cache")?);
        Ok(())
    }
    fn get_request_parts(&self) -> Result<Parts> {
        let copied_req = self.req.try_clone().ok_or(CacheError::BadRequest)?;
        Ok(http::Request::try_from(copied_req)?.into_parts().0)
    }
    fn before_request(&self, policy: &CachePolicy) -> Result<BeforeRequest> {
        Ok(policy.before_request(&self.get_request_parts()?, SystemTime::now()))
    }
    fn after_response(
        &self,
        policy: &CachePolicy,
        response: &HttpResponse,
    ) -> Result<AfterResponse> {
        Ok(policy.after_response(
            &self.get_request_parts()?,
            &response.get_parts()?,
            SystemTime::now(),
        ))
    }
    fn url(&self) -> Result<&Url> {
        Ok(self.req.url())
    }
    fn method(&self) -> Result<String> {
        Ok(self.req.method().as_ref().to_string())
    }
    async fn remote_fetch(&self) -> Result<HttpResponse> {
        let url = self.req.url().clone();
        let copied_req = self.req.try_clone().ok_or(CacheError::BadRequest)?;
        let res = self
            .next
            .clone()
            .run(
                copied_req,
                &mut task_local_extensions::Extensions::default(),
            )
            .await?;
        let mut headers = HashMap::new();
        for header in res.headers() {
            headers.insert(header.0.as_str().to_owned(), header.1.to_str()?.to_owned());
        }
        let status = res.status().into();
        let version = res.version();
        let body: Vec<u8> = res.text().await?.into_bytes();
        Ok(HttpResponse {
            body,
            headers,
            status,
            url,
            version: version.try_into()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{CACacheManager, Cache, CacheManager, CacheMode};
    use mockito::mock;
    use reqwest::{Client, Url};
    use reqwest_middleware::ClientBuilder;

    #[cfg(feature = "client-reqwest")]
    #[tokio::test]
    async fn default_mode() -> anyhow::Result<()> {
        let m = mock("GET", "/")
            .with_status(200)
            .with_header("cache-control", "max-age=86400, public")
            .with_body("test")
            .create();
        let url = format!("{}/", &mockito::server_url());
        let manager = CACacheManager::default();
        let path = manager.path.clone();
        let key = format!("GET:{}", &url);

        // Make sure the record doesn't already exist
        manager.delete("GET", &Url::parse(&url)?).await?;

        // Construct reqwest client with cache defaults
        let client = ClientBuilder::new(Client::new())
            .with(Cache {
                mode: CacheMode::Default,
                cache_manager: CACacheManager::default(),
            })
            .build();

        // Cold pass to load cache
        client.get(url).send().await?;
        m.assert();

        // Try to load cached object
        let data = cacache::read(&path, &key).await;
        assert!(data.is_ok());
        Ok(())
    }
}
