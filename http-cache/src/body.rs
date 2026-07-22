//! HTTP body types for streaming cache support.
//!
//! This module provides the [`StreamingBody`] type which allows HTTP cache middleware
//! to handle both cached (buffered) responses and streaming responses from upstream
//! servers without requiring full buffering of large responses.
//!
//! # Variants
//!
//! - **Buffered**: Contains cached response data that can be sent immediately
//! - **Streaming**: Wraps an upstream body for streaming responses
//! - **File**: Streams from a [`tokio::fs::File`] in 64KB chunks (only with `streaming` feature)
//!
//! # Example
//!
//! ```rust
//! use http_cache::StreamingBody;
//! use bytes::Bytes;
//! use http_body_util::Full;
//!
//! // Cached response - sent immediately from memory
//! let cached: StreamingBody<Full<Bytes>> = StreamingBody::buffered(Bytes::from("Hello!"));
//!
//! // Streaming response - passed through from upstream
//! let upstream = Full::new(Bytes::from("From upstream"));
//! let streaming: StreamingBody<Full<Bytes>> = StreamingBody::streaming(upstream);
//! ```

// Note: pin_project_lite does not support doc comments on enum variant fields,
// so we allow missing_docs for the generated enum variants and fields.
// The module-level and enum-level documentation provides full coverage.
#![allow(missing_docs)]

use std::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
#[cfg(feature = "streaming")]
use bytes::BytesMut;
use http_body::{Body, Frame};
use pin_project_lite::pin_project;

use crate::error::StreamingError;

/// Default buffer size for streaming from disk (64KB).
///
/// This size is optimized for modern SSDs and NVMe drives, reducing syscall
/// overhead while maintaining reasonable memory usage.
#[cfg(feature = "streaming")]
const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// End-of-stream integrity check for the `File` variant.
#[cfg(feature = "streaming")]
pub struct FileCheck {
    hasher: blake3::Hasher,
    expected: [u8; 32],
    on_corrupt: Box<dyn FnOnce() + Send>,
}

#[cfg(feature = "streaming")]
impl FileCheck {
    /// Compares the streamed hash against the expected one; fires
    /// `on_corrupt` and returns the error on mismatch.
    fn finish(self) -> Result<(), StreamingError> {
        if self.hasher.finalize().as_bytes() == &self.expected {
            return Ok(());
        }
        (self.on_corrupt)();
        Err(StreamingError::new(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "cached body checksum mismatch",
        ))))
    }

    /// Fires `on_corrupt` without a hash compare (truncated stream).
    fn corrupt(self) {
        (self.on_corrupt)();
    }
}

#[cfg(feature = "streaming")]
impl fmt::Debug for FileCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileCheck")
            .field("expected", &self.expected)
            .finish_non_exhaustive()
    }
}

// When streaming feature is enabled, include the File variant
#[cfg(feature = "streaming")]
pin_project! {
    /// A body type that can represent either buffered data from cache or streaming body from upstream.
    ///
    /// This enum allows the HTTP cache middleware to efficiently handle:
    /// - Cached responses (buffered data)
    /// - Cache misses (streaming from upstream)
    /// - Disk-cached responses (streaming from file)
    ///
    /// # Variants
    ///
    /// - **Buffered**: Contains cached response data that can be sent immediately
    /// - **Streaming**: Wraps an upstream body for streaming responses
    /// - **File**: Streams from a [`tokio::fs::File`] in 64KB chunks (only with `streaming` feature)
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
            reader: tokio::fs::File,
            buffer: BytesMut,
            done: bool,
            size: u64,
            check: Option<Box<FileCheck>>,
        },
    }
}

// When streaming feature is disabled, no File variant
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
    /// - **Buffered**: Contains cached response data that can be sent immediately
    /// - **Streaming**: Wraps an upstream body for streaming responses
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
    /// Create a new buffered body from bytes.
    ///
    /// The bytes are consumed on the first poll and sent as a single frame.
    #[must_use]
    pub fn buffered(data: Bytes) -> Self {
        Self::Buffered { data: Some(data) }
    }

    /// Create a new streaming body from an upstream body.
    ///
    /// The upstream body is passed through without additional buffering.
    #[must_use]
    pub fn streaming(body: B) -> Self {
        Self::Streaming { inner: body }
    }

    /// Create a new file-streaming body from a [`tokio::fs::File`] with known size.
    ///
    /// This allows streaming large cached responses from disk without
    /// loading the entire body into memory. Data is read in 64KB chunks.
    ///
    /// # Cursor contract
    ///
    /// The caller must position `file` at the start of the body bytes before
    /// calling this function. Exactly `size` bytes are streamed from the
    /// current cursor position; trailing file content is ignored and a file
    /// shorter than `size` yields a stream error.
    ///
    /// `size` is used to provide accurate size hints to downstream consumers.
    #[cfg(feature = "streaming")]
    #[must_use]
    pub fn from_file_with_size(file: tokio::fs::File, size: u64) -> Self {
        Self::File {
            reader: file,
            buffer: BytesMut::with_capacity(STREAM_BUFFER_SIZE),
            done: false,
            size,
            check: None,
        }
    }

    /// Like [`from_file_with_size`](Self::from_file_with_size), but hashes
    /// the streamed bytes and compares against `checksum` when the stream
    /// completes. On mismatch the final frame is replaced with an error and
    /// `on_corrupt` is invoked once.
    #[cfg(feature = "streaming")]
    #[must_use]
    pub fn from_file_verified(
        file: tokio::fs::File,
        size: u64,
        checksum: [u8; 32],
        on_corrupt: impl FnOnce() + Send + 'static,
    ) -> Self {
        Self::File {
            reader: file,
            buffer: BytesMut::with_capacity(STREAM_BUFFER_SIZE),
            done: false,
            size,
            check: Some(Box::new(FileCheck {
                hasher: blake3::Hasher::new(),
                expected: checksum,
                on_corrupt: Box::new(on_corrupt),
            })),
        }
    }
}

#[cfg(feature = "streaming")]
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
            StreamingBodyProj::File { reader, buffer, done, size, check } => {
                if *done {
                    return Poll::Ready(None);
                }
                if *size == 0 {
                    *done = true;
                    if let Some(c) = check.take() {
                        if let Err(e) = c.finish() {
                            return Poll::Ready(Some(Err(e)));
                        }
                    }
                    return Poll::Ready(None);
                }

                use tokio::io::AsyncRead;

                // Resize buffer to full capacity for reading (this is safe - fills with zeros)
                buffer.resize(STREAM_BUFFER_SIZE, 0);

                let mut read_buf = tokio::io::ReadBuf::new(buffer.as_mut());

                match reader.poll_read(cx, &mut read_buf) {
                    Poll::Ready(Ok(())) => {
                        let filled_len = read_buf.filled().len();
                        if filled_len == 0 {
                            *done = true;
                            buffer.clear();
                            if let Some(c) = check.take() {
                                c.corrupt();
                            }
                            Poll::Ready(Some(Err(StreamingError::new(
                                Box::new(std::io::Error::new(
                                    std::io::ErrorKind::UnexpectedEof,
                                    "cached body file shorter than expected",
                                )),
                            ))))
                        } else {
                            // Never yield more than the remaining body bytes.
                            let take = (*size).min(filled_len as u64) as usize;
                            buffer.truncate(take);
                            *size -= take as u64;
                            if let Some(c) = check.as_deref_mut() {
                                c.hasher.update(&buffer[..take]);
                            }
                            if *size == 0 {
                                *done = true;
                                if let Some(c) = check.take() {
                                    if let Err(e) = c.finish() {
                                        buffer.clear();
                                        return Poll::Ready(Some(Err(e)));
                                    }
                                }
                            }
                            let bytes = buffer.split().freeze();
                            Poll::Ready(Some(Ok(Frame::data(bytes))))
                        }
                    }
                    Poll::Ready(Err(e)) => {
                        *done = true;
                        buffer.clear();
                        Poll::Ready(Some(Err(StreamingError::new(Box::new(e)))))
                    }
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            StreamingBody::Buffered { data } => data.is_none(),
            StreamingBody::Streaming { inner } => inner.is_end_stream(),
            StreamingBody::File { done, .. } => *done,
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
            StreamingBody::File { size, .. } => {
                http_body::SizeHint::with_exact(*size)
            }
        }
    }
}

#[cfg(not(feature = "streaming"))]
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
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            StreamingBody::Buffered { data } => data.is_none(),
            StreamingBody::Streaming { inner } => inner.is_end_stream(),
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
        }
    }
}

impl<B> From<Bytes> for StreamingBody<B> {
    fn from(bytes: Bytes) -> Self {
        Self::buffered(bytes)
    }
}

#[cfg(feature = "streaming")]
impl<B: fmt::Debug> fmt::Debug for StreamingBody<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Buffered { data } => f
                .debug_struct("StreamingBody::Buffered")
                .field("has_data", &data.is_some())
                .field("len", &data.as_ref().map(|b| b.len()))
                .finish(),
            Self::Streaming { inner } => f
                .debug_struct("StreamingBody::Streaming")
                .field("inner", inner)
                .finish(),
            Self::File { done, size, .. } => f
                .debug_struct("StreamingBody::File")
                .field("done", done)
                .field("size", &size)
                .finish_non_exhaustive(),
        }
    }
}

#[cfg(not(feature = "streaming"))]
impl<B: fmt::Debug> fmt::Debug for StreamingBody<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Buffered { data } => f
                .debug_struct("StreamingBody::Buffered")
                .field("has_data", &data.is_some())
                .field("len", &data.as_ref().map(|b| b.len()))
                .finish(),
            Self::Streaming { inner } => f
                .debug_struct("StreamingBody::Streaming")
                .field("inner", inner)
                .finish(),
        }
    }
}

#[cfg(feature = "streaming")]
impl<B> StreamingBody<B>
where
    B: Body + Unpin + Send,
    B::Error: Into<StreamingError>,
    B::Data: Into<Bytes>,
{
    /// Convert this streaming body into a stream of Bytes.
    ///
    /// This method allows for streaming without collecting the entire body into memory first.
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

#[cfg(all(test, feature = "streaming"))]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use tokio::io::AsyncWriteExt;

    async fn file_with(content: &[u8]) -> (tokio::fs::File, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("body.bin");
        let mut f = tokio::fs::File::create(&path).await.unwrap();
        f.write_all(content).await.unwrap();
        f.sync_all().await.unwrap();
        drop(f);
        (tokio::fs::File::open(&path).await.unwrap(), dir)
    }

    #[tokio::test]
    async fn file_body_stops_at_size() {
        let (f, _dir) = file_with(b"0123456789trailing-garbage").await;
        let body: StreamingBody<http_body_util::Empty<Bytes>> =
            StreamingBody::from_file_with_size(f, 10);
        let collected = body.collect().await.unwrap().to_bytes();
        assert_eq!(collected.as_ref(), b"0123456789");
    }

    #[tokio::test]
    async fn file_body_errors_on_truncated_file() {
        let (f, _dir) = file_with(b"short").await;
        let body: StreamingBody<http_body_util::Empty<Bytes>> =
            StreamingBody::from_file_with_size(f, 100);
        assert!(body.collect().await.is_err());
    }

    #[tokio::test]
    async fn file_body_size_hint_reports_remaining() {
        use http_body::Body;
        let payload = vec![7u8; STREAM_BUFFER_SIZE + 100];
        let (f, _dir) = file_with(&payload).await;
        let mut body: StreamingBody<http_body_util::Empty<Bytes>> =
            StreamingBody::from_file_with_size(f, payload.len() as u64);
        assert_eq!(body.size_hint().exact(), Some(payload.len() as u64));
        let frame =
            std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                .await
                .unwrap()
                .unwrap();
        let n = frame.into_data().unwrap().len() as u64;
        assert_eq!(body.size_hint().exact(), Some(payload.len() as u64 - n));
    }
}
