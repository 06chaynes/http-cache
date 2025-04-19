use std::path::PathBuf;
use std::result::Result as StdResult;

use crate::{Body, CacheManager, HttpResponse, Parts, Result};

use bytes::Bytes;
use cacache::{Reader, Writer};
use futures::{Stream, StreamExt};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, StreamBody};
use http_cache_semantics::CachePolicy;
use serde::{Deserialize, Serialize};

#[cfg(feature = "cacache-async-std")]
use futures::{AsyncReadExt, AsyncWriteExt};
#[cfg(feature = "cacache-tokio")]
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Implements [`CacheManager`] with [`cacache`](https://github.com/zkat/cacache-rs) as the backend.
#[cfg_attr(docsrs, doc(cfg(feature = "manager-cacache")))]
#[derive(Debug, Clone)]
pub struct CACacheManager {
    /// Directory where the cache will be stored.
    pub path: PathBuf,
}

impl Default for CACacheManager {
    fn default() -> Self {
        Self { path: "./http-cacache".into() }
    }
}

// Cache binary value layout:
// [u32 - size of the NoBodyStore][Store][response body bytes]
// bincode works with pre-defined slice of bytes, so we need this u32 in front.

#[derive(Debug, Deserialize, Serialize)]
enum BodyKind {
    Full,
    Streaming,
}

#[derive(Debug, Deserialize, Serialize)]
struct Store {
    parts: Parts,
    policy: CachePolicy,
    body_kind: BodyKind,
}

#[allow(dead_code)]
impl CACacheManager {
    /// Clears out the entire cache.
    pub async fn clear(&self) -> Result<()> {
        cacache::clear(&self.path).await?;
        Ok(())
    }
}

#[cfg(feature = "cacache-async-std")]
mod cacache_stream {
    use super::*;
    use futures::AsyncRead;
    use std::pin::Pin;
    use std::task::Context;
    use std::task::Poll;

    // Some custom dummy implementation of Stream for cacache Reader.

    const BUFSIZE: usize = 4096;

    pub struct CACacheReaderStream {
        reader: Reader,
        buf: [u8; BUFSIZE],
    }

    impl CACacheReaderStream {
        pub fn new(reader: Reader) -> Self {
            Self { reader, buf: [0; BUFSIZE] }
        }
    }

    impl Stream for CACacheReaderStream {
        type Item = StdResult<http_body::Frame<Bytes>, std::io::Error>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            let Self { reader, buf } = &mut *self;
            let reader = Pin::new(reader);
            match reader.poll_read(cx, buf) {
                Poll::Ready(Ok(0)) => Poll::Ready(None),
                Poll::Ready(Ok(n)) => {
                    let bytes = Bytes::from(buf[..n].to_vec());
                    let frame = http_body::Frame::data(bytes);
                    Poll::Ready(Some(Ok(frame)))
                }
                Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
                Poll::Pending => Poll::Pending,
            }
        }
    }
}

#[cfg(feature = "cacache-tokio")]
mod cacache_stream {
    use super::*;
    use tokio_util::io::ReaderStream;

    pub struct CACacheReaderStream {
        inner: ReaderStream<Reader>,
    }

    impl CACacheReaderStream {
        pub fn new(reader: Reader) -> Self {
            Self { inner: ReaderStream::new(reader) }
        }
    }

    impl Stream for CACacheReaderStream {
        type Item = StdResult<http_body::Frame<Bytes>, std::io::Error>;

        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            <ReaderStream<Reader> as Stream>::poll_next(
                std::pin::Pin::new(&mut self.inner),
                cx,
            )
            .map(|opt| opt.map(|res| res.map(http_body::Frame::data)))
        }
    }
}

use cacache_stream::CACacheReaderStream;

#[async_trait::async_trait]
impl CacheManager for CACacheManager {
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        let mut reader = match Reader::open(&self.path, cache_key).await {
            Ok(reader) => reader,
            Err(err) => match err {
                cacache::Error::EntryNotFound(..) => return Ok(None),
                _ => return Err(err.into()),
            },
        };

        // Reading "head" part length
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf).await?;
        let store_len = u32::from_le_bytes(buf);

        // Reading "head" part
        let mut buf = vec![0; store_len as usize];
        reader.read_exact(buf.as_mut_slice()).await?;
        let store: Store = bincode::deserialize(&buf)?;

        let body = match store.body_kind {
            BodyKind::Full => {
                let mut body = Vec::new();
                reader.read_to_end(&mut body).await?;
                Body { inner: crate::BodyInner::Full(body.into()) }
            }
            BodyKind::Streaming => Body {
                inner: crate::BodyInner::Streaming(BoxBody::new(
                    StreamBody::new(CACacheReaderStream::new(reader))
                        .map_err(Into::into),
                )),
            },
        };
        Ok(Some((HttpResponse::from_parts(store.parts, body), store.policy)))
    }

    async fn put(
        &self,
        cache_key: String,
        response: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse> {
        let mut writer = Writer::create(&self.path, &cache_key).await?;
        let (parts, body) = response.into_parts();
        let body_kind = match &body.inner {
            crate::BodyInner::Full(_) => BodyKind::Full,
            crate::BodyInner::Streaming(_) => BodyKind::Streaming,
        };
        let data = Store { parts, policy, body_kind };
        let bytes = bincode::serialize(&data)?;
        let store_len = (bytes.len() as u32).to_le_bytes();

        // Writing "head" part length
        writer.write_all(&store_len).await?;

        // Writing "head" part
        writer.write_all(&bytes).await?;

        // Writing body itself
        match body.inner {
            crate::BodyInner::Full(data) => {
                writer.write_all(&data).await?;
            }
            crate::BodyInner::Streaming(box_body) => {
                let mut stream = box_body.into_data_stream();
                while let Some(chunk_result) = stream.next().await {
                    let chunk = chunk_result?;
                    writer.write_all(&chunk).await?;
                }
            }
        }
        writer.commit().await?;

        // Safety: at this point we successfully created this cache entry,
        // so it is safe to unwrap here (cacache::Error::EntryNotFound should be impossible).
        // FIXME: does it make sense to return error here instead of unwrapping? If yes, then which error?
        let (response, _) = self.get(&cache_key).await?.unwrap();

        Ok(response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        Ok(cacache::remove(&self.path, cache_key).await?)
    }
}
