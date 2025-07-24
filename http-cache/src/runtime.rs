//! Runtime abstraction for async I/O operations.

#[cfg(not(any(feature = "streaming-tokio", feature = "streaming-smol")))]
compile_error!("when using the streaming feature, either feature `streaming-tokio` or `streaming-smol` must be enabled");

#[cfg(all(feature = "streaming-tokio", feature = "streaming-smol"))]
compile_error!(
    "features `streaming-tokio` and `streaming-smol` are mutually exclusive"
);

#[cfg(feature = "streaming")]
cfg_if::cfg_if! {
    if #[cfg(all(feature = "streaming-tokio", not(feature = "streaming-smol")))] {
        pub use tokio::fs::File;
        pub use tokio::io::ReadBuf;

        use std::io;
        use std::path::Path;

        pub async fn read<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
            tokio::fs::read(path).await
        }

        pub async fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()> {
            tokio::fs::write(path, contents).await
        }

        pub async fn create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
            tokio::fs::create_dir_all(path).await
        }

        pub async fn remove_file<P: AsRef<Path>>(path: P) -> io::Result<()> {
            tokio::fs::remove_file(path).await
        }
    } else if #[cfg(all(feature = "streaming-smol", not(feature = "streaming-tokio")))] {
        pub use smol::fs::File;

        use std::io;
        use std::path::Path;

        pub async fn read<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
            smol::fs::read(path).await
        }

        pub async fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()> {
            smol::fs::write(path, contents).await
        }

        pub async fn create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
            smol::fs::create_dir_all(path).await
        }

        pub async fn remove_file<P: AsRef<Path>>(path: P) -> io::Result<()> {
            smol::fs::remove_file(path).await
        }

        // For smol, we need to create a ReadBuf-like abstraction
        #[allow(dead_code)]
        pub struct ReadBuf<'a> {
            buf: &'a mut [u8],
            filled: usize,
        }

        #[allow(dead_code)]
        impl<'a> ReadBuf<'a> {
            pub fn new(buf: &'a mut [u8]) -> Self {
                Self { buf, filled: 0 }
            }

            pub fn filled(&self) -> &[u8] {
                &self.buf[..self.filled]
            }

            pub fn initialize_unfilled(&mut self) -> &mut [u8] {
                &mut self.buf[self.filled..]
            }

            pub fn advance(&mut self, n: usize) {
                self.filled = (self.filled + n).min(self.buf.len());
            }
        }
    }
}
