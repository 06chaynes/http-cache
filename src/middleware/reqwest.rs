use crate::{CacheError, HttpResponse, Middleware, Result};

use std::{collections::HashMap, convert::TryInto, str::FromStr, time::SystemTime};

use http::{header::CACHE_CONTROL, request::Parts};
use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use url::Url;

pub(crate) struct ReqwestMiddleware<'a> {
    req: reqwest::Request,
    next: reqwest_middleware::Next<'a>,
    ext: task_local_extensions::Extensions,
}

#[async_trait::async_trait]
impl Middleware for ReqwestMiddleware<'_> {
    fn is_method_get_head(&self) -> bool {
        self.req.method() == http::Method::GET || self.req.method() == http::Method::HEAD
    }
    fn new_policy(&self, response: &HttpResponse) -> Result<CachePolicy> {
        todo!()
    }
    fn update_request_headers(&mut self, parts: Parts) -> Result<()> {
        todo!()
    }
    fn set_no_cache(&mut self) -> Result<()> {
        todo!()
    }
    fn get_request_parts(&self) -> Result<Parts> {
        todo!()
    }
    fn before_request(&self, policy: &CachePolicy) -> Result<BeforeRequest> {
        todo!()
    }
    fn after_response(
        &self,
        policy: &CachePolicy,
        response: &HttpResponse,
    ) -> Result<AfterResponse> {
        todo!()
    }
    fn url(&self) -> Result<&Url> {
        todo!()
    }
    fn method(&self) -> Result<String> {
        todo!()
    }
    async fn remote_fetch(&self) -> Result<HttpResponse> {
        todo!()
    }
}
