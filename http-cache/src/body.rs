//! HTTP body types for streaming cache support.
//!
//! This module provides the [`StreamingBody`] type which allows HTTP cache middleware
//! to handle both cached (buffered) responses and streaming responses from upstream
//! servers without requiring full buffering of large responses.
//! This implementation provides efficient streaming capabilities for HTTP caching.

#![allow(missing_docs)]

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use http_body::{Body, Frame};
use pin_project_lite::pin_project;

use crate::error::StreamingError;

#[cfg(feature = "streaming")]
pin_project! {
    /// A body type that can represent either buffered data from cache, streaming body from upstream,
    /// or streaming from a file for file-based caching.
    ///
    /// This enum allows the HTTP cache middleware to efficiently handle:
    /// - Cached responses (buffered data)
    /// - Cache misses (streaming from upstream)
    /// - File-based cached responses (streaming from disk)
    ///
    /// # Variants
    ///
    /// - [`Buffered`](StreamingBody::Buffered): Contains cached response data that can be sent immediately
    /// - [`Streaming`](StreamingBody::Streaming): Wraps an upstream body for streaming responses
    /// - [`File`](StreamingBody::File): Streams directly from a file for zero-copy caching
    ///
    /// # Example
    ///
    /// ```rust
    /// use http_cache::StreamingBody;
    /// use bytes::Bytes;
    /// use http_body_util::Full;
    ///
    /// // Cached response - sent immediately from memory
    /// let cached: StreamingBody<Full<Bytes>> = StreamingBody::buffered(Bytes::from("Hello from cache!"));
    ///
    /// // Streaming response - passed through from upstream
    /// # struct MyBody;
    /// # impl http_body::Body for MyBody {
    /// #     type Data = bytes::Bytes;
    /// #     type Error = Box<dyn std::error::Error + Send + Sync>;
    /// #     fn poll_frame(
    /// #         self: std::pin::Pin<&mut Self>,
    /// #         _: &mut std::task::Context<'_>
    /// #     ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
    /// #         std::task::Poll::Ready(None)
    /// #     }
    /// # }
    /// let upstream_body = MyBody;
    /// let streaming = StreamingBody::streaming(upstream_body);
    /// ```
    #[project = StreamingBodyProj]
    pub enum StreamingBody<B> {
        Buffered {
            data: Option<Bytes>,
        },
        Streaming {
            #[pin]
            inner: B,
        },
        File {
            #[pin]
            file: crate::runtime::File,
            buf: Vec<u8>,
            finished: bool,
        },
    }
}

#[cfg(not(feature = "streaming"))]
pin_project! {
    /// A body type that can represent either buffered data from cache or streaming body from upstream.
    ///
    /// This enum allows the HTTP cache middleware to efficiently handle:
    /// - Cached responses (buffered data)
    /// - Cache misses (streaming from upstream)
    ///
    /// # Variants
    ///
    /// - [`Buffered`](StreamingBody::Buffered): Contains cached response data that can be sent immediately
    /// - [`Streaming`](StreamingBody::Streaming): Wraps an upstream body for streaming responses
    ///
    /// # Example
    ///
    /// ```rust
    /// use http_cache::StreamingBody;
    /// use bytes::Bytes;
    /// use http_body_util::Full;
    ///
    /// // Cached response - sent immediately from memory
    /// let cached: StreamingBody<Full<Bytes>> = StreamingBody::buffered(Bytes::from("Hello from cache!"));
    ///
    /// // Streaming response - passed through from upstream
    /// # struct MyBody;
    /// # impl http_body::Body for MyBody {
    /// #     type Data = bytes::Bytes;
    /// #     type Error = Box<dyn std::error::Error + Send + Sync>;
    /// #     fn poll_frame(
    /// #         self: std::pin::Pin<&mut Self>,
    /// #         _: &mut std::task::Context<'_>
    /// #     ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
    /// #         std::task::Poll::Ready(None)
    /// #     }
    /// # }
    /// let upstream_body = MyBody;
    /// let streaming = StreamingBody::streaming(upstream_body);
    /// ```
    #[project = StreamingBodyProj]
    pub enum StreamingBody<B> {
        Buffered {
            data: Option<Bytes>,
        },
        Streaming {
            #[pin]
            inner: B,
        },
    }
}

impl<B> StreamingBody<B> {
    /// Create a new buffered body from bytes
    pub fn buffered(data: Bytes) -> Self {
        Self::Buffered { data: Some(data) }
    }

    /// Create a new streaming body from an upstream body
    pub fn streaming(body: B) -> Self {
        Self::Streaming { inner: body }
    }

    /// Create a new file-based streaming body
    #[cfg(feature = "streaming")]
    pub fn from_file(file: crate::runtime::File) -> Self {
        Self::File { file, buf: Vec::new(), finished: false }
    }
}

impl<B> Body for StreamingBody<B>
where
    B: Body + Unpin,
    B::Error: Into<StreamingError>,
    B::Data: Into<Bytes>,
{
    type Data = Bytes;
    type Error = StreamingError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.as_mut().project() {
            StreamingBodyProj::Buffered { data } => {
                if let Some(bytes) = data.take() {
                    if bytes.is_empty() {
                        Poll::Ready(None)
                    } else {
                        Poll::Ready(Some(Ok(Frame::data(bytes))))
                    }
                } else {
                    Poll::Ready(None)
                }
            }
            StreamingBodyProj::Streaming { inner } => {
                inner.poll_frame(cx).map(|opt| {
                    opt.map(|res| {
                        res.map(|frame| frame.map_data(Into::into))
                            .map_err(Into::into)
                    })
                })
            }
            #[cfg(feature = "streaming")]
            StreamingBodyProj::File { file, buf, finished } => {
                if *finished {
                    return Poll::Ready(None);
                }

                // Prepare buffer
                buf.resize(8192, 0);

                cfg_if::cfg_if! {
                    if #[cfg(feature = "streaming-tokio")] {
                        use tokio::io::AsyncRead;
                        use crate::runtime::ReadBuf;

                        let mut read_buf = ReadBuf::new(buf);
                        match file.poll_read(cx, &mut read_buf) {
                            Poll::Pending => Poll::Pending,
                            Poll::Ready(Err(e)) => {
                                *finished = true;
                                Poll::Ready(Some(Err(StreamingError::new(e))))
                            }
                            Poll::Ready(Ok(())) => {
                                let n = read_buf.filled().len();
                                if n == 0 {
                                    // EOF
                                    *finished = true;
                                    Poll::Ready(None)
                                } else {
                                    let chunk = Bytes::copy_from_slice(&buf[..n]);
                                    buf.clear();
                                    Poll::Ready(Some(Ok(Frame::data(chunk))))
                                }
                            }
                        }
                    } else if #[cfg(feature = "streaming-smol")] {
                        use futures::io::AsyncRead;

                        match file.poll_read(cx, buf) {
                            Poll::Pending => Poll::Pending,
                            Poll::Ready(Err(e)) => {
                                *finished = true;
                                Poll::Ready(Some(Err(StreamingError::new(e))))
                            }
                            Poll::Ready(Ok(0)) => {
                                // EOF
                                *finished = true;
                                Poll::Ready(None)
                            }
                            Poll::Ready(Ok(n)) => {
                                let chunk = Bytes::copy_from_slice(&buf[..n]);
                                buf.clear();
                                Poll::Ready(Some(Ok(Frame::data(chunk))))
                            }
                        }
                    }
                }
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            StreamingBody::Buffered { data } => data.is_none(),
            StreamingBody::Streaming { inner } => inner.is_end_stream(),
            #[cfg(feature = "streaming")]
            StreamingBody::File { finished, .. } => *finished,
        }
    }

    fn size_hint(&self) -> http_body::SizeHint {
        match self {
            StreamingBody::Buffered { data } => {
                if let Some(bytes) = data {
                    let len = bytes.len() as u64;
                    http_body::SizeHint::with_exact(len)
                } else {
                    http_body::SizeHint::with_exact(0)
                }
            }
            StreamingBody::Streaming { inner } => inner.size_hint(),
            #[cfg(feature = "streaming")]
            StreamingBody::File { .. } => {
                // We don't know the file size in advance without an additional stat call
                http_body::SizeHint::default()
            }
        }
    }
}

impl<B> From<Bytes> for StreamingBody<B> {
    fn from(bytes: Bytes) -> Self {
        Self::buffered(bytes)
    }
}

#[cfg(feature = "streaming")]
impl<B> StreamingBody<B>
where
    B: Body + Unpin + Send,
    B::Error: Into<StreamingError>,
    B::Data: Into<Bytes>,
{
    /// Convert this streaming body into a stream of Bytes for use with reqwest.
    ///
    /// This method creates a stream that's compatible with `reqwest::Body::wrap_stream()`,
    /// allowing for streaming without collecting the entire body into memory first.
    /// This is particularly useful for file-based cached responses which can stream
    /// directly from disk.
    pub fn into_bytes_stream(
        self,
    ) -> impl futures_util::Stream<
        Item = Result<Bytes, Box<dyn std::error::Error + Send + Sync>>,
    > + Send {
        use futures_util::TryStreamExt;

        http_body_util::BodyStream::new(self)
            .map_ok(|frame| {
                // Extract data from frame, StreamingBody always produces Bytes
                frame.into_data().unwrap_or_else(|_| Bytes::new())
            })
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                Box::new(std::io::Error::other(format!("Stream error: {e}")))
            })
    }
}
