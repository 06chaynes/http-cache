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


        pub async fn read_dir<P: AsRef<Path>>(path: P) -> io::Result<tokio::fs::ReadDir> {
            tokio::fs::read_dir(path).await
        }

        pub async fn metadata<P: AsRef<Path>>(path: P) -> io::Result<std::fs::Metadata> {
            tokio::fs::metadata(path).await
        }

        pub async fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
            tokio::fs::rename(from, to).await
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


        pub async fn read_dir<P: AsRef<Path>>(path: P) -> io::Result<smol::fs::ReadDir> {
            smol::fs::read_dir(path).await
        }

        pub async fn metadata<P: AsRef<Path>>(path: P) -> io::Result<std::fs::Metadata> {
            smol::fs::metadata(path).await
        }

        pub async fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
            smol::fs::rename(from, to).await
        }

    }
}
