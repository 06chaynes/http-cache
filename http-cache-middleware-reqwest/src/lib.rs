//! The reqwest middleware implementation, requires the `client-reqwest` feature.
use http_cache_types::{CacheError, HttpResponse, Middleware, Result};

use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    str::FromStr,
    time::SystemTime,
};

use http::{
    header::{HeaderName, CACHE_CONTROL},
    request::Parts,
    HeaderValue, Method,
};
use http_cache_semantics::{CacheOptions, CachePolicy};
use reqwest::{Request, Response, ResponseBuilderExt};
use reqwest_middleware::Next;
use task_local_extensions::Extensions;
use url::Url;

/// Implements ['Middleware'] for reqwest
pub struct ReqwestMiddleware<'a> {
    pub req: Request,
    pub next: Next<'a>,
    pub extensions: &'a mut Extensions,
}

#[async_trait::async_trait]
impl Middleware for ReqwestMiddleware<'_> {
    fn is_method_get_head(&self) -> bool {
        self.req.method() == Method::GET || self.req.method() == Method::HEAD
    }
    fn policy(&self, response: &HttpResponse) -> Result<CachePolicy> {
        Ok(CachePolicy::new(&self.parts()?, &response.parts()?))
    }
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
    fn update_headers(&mut self, parts: Parts) -> Result<()> {
        let headers = parts.headers;
        for header in headers.iter() {
            self.req.headers_mut().insert(header.0.clone(), header.1.clone());
        }
        Ok(())
    }
    fn set_no_cache(&mut self) -> Result<()> {
        self.req
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_str("no-cache")?);
        Ok(())
    }
    fn parts(&self) -> Result<Parts> {
        let copied_req = self.req.try_clone().ok_or(CacheError::BadRequest)?;
        Ok(http::Request::try_from(copied_req)?.into_parts().0)
    }
    fn url(&self) -> Result<&Url> {
        Ok(self.req.url())
    }
    fn method(&self) -> Result<String> {
        Ok(self.req.method().as_ref().to_string())
    }
    async fn remote_fetch(&mut self) -> Result<HttpResponse> {
        let copied_req = self.req.try_clone().ok_or(CacheError::BadRequest)?;
        let res = self.next.clone().run(copied_req, self.extensions).await?;
        let mut headers = HashMap::new();
        for header in res.headers() {
            headers.insert(
                header.0.as_str().to_owned(),
                header.1.to_str()?.to_owned(),
            );
        }
        let url = res.url().clone();
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

/// Converts an [`HttpResponse`] to a reqwest [`Response`]
pub fn convert_response(response: HttpResponse) -> anyhow::Result<Response> {
    let mut ret_res = http::Response::builder()
        .status(response.status)
        .url(response.url)
        .version(response.version.try_into()?)
        .body(response.body)?;
    for header in response.headers {
        ret_res.headers_mut().insert(
            HeaderName::from_str(header.0.clone().as_str())?,
            HeaderValue::from_str(header.1.clone().as_str())?,
        );
    }
    Ok(Response::from(ret_res))
}
