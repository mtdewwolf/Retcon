//! Cross-platform local transport: Windows named pipes and Unix domain sockets.

#![allow(missing_docs)]

use std::path::{Path, PathBuf};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::error::TransportError;

/// How a local endpoint is reached on the host OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    /// Windows named pipe (`\\.\pipe\...`).
    NamedPipe,
    /// Unix domain socket file.
    UnixSocket,
}

/// A bindable local endpoint advertised through discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEndpoint {
    pub kind: TransportKind,
    pub path: PathBuf,
}

impl LocalEndpoint {
    /// Stable string form used in discovery files and logs.
    #[must_use]
    pub fn path_string(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

/// Resolve the default transport endpoint for a Retcon data directory.
#[must_use]
pub fn default_endpoint(data_dir: &Path) -> LocalEndpoint {
    #[cfg(windows)]
    {
        let _ = data_dir;
        LocalEndpoint {
            kind: TransportKind::NamedPipe,
            path: PathBuf::from(format!(
                r"\\.\pipe\retcon-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            )),
        }
    }
    #[cfg(unix)]
    {
        LocalEndpoint {
            kind: TransportKind::UnixSocket,
            path: data_dir.join("core.sock"),
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = data_dir;
        LocalEndpoint {
            kind: TransportKind::UnixSocket,
            path: PathBuf::from("/tmp/retcon.sock"),
        }
    }
}

enum ListenerInner {
    #[cfg(windows)]
    NamedPipe(windows_pipe::WindowsPipeListener),
    #[cfg(unix)]
    Unix(tokio::net::UnixListener),
}

/// Accepts inbound local transport connections.
pub struct TransportListener {
    endpoint: LocalEndpoint,
    inner: ListenerInner,
}

impl TransportListener {
    /// Bind the platform-default transport for `data_dir`.
    pub async fn bind(data_dir: &Path) -> Result<Self, TransportError> {
        let endpoint = default_endpoint(data_dir);
        Self::bind_endpoint(endpoint).await
    }

    /// Bind a specific endpoint, removing stale Unix socket files when needed.
    pub async fn bind_endpoint(endpoint: LocalEndpoint) -> Result<Self, TransportError> {
        match endpoint.kind {
            #[cfg(windows)]
            TransportKind::NamedPipe => Ok(Self {
                endpoint: endpoint.clone(),
                inner: ListenerInner::NamedPipe(windows_pipe::WindowsPipeListener::new(
                    endpoint.path_string(),
                )?),
            }),
            #[cfg(unix)]
            TransportKind::UnixSocket => {
                if endpoint.path.exists() {
                    std::fs::remove_file(&endpoint.path)
                        .map_err(|source| TransportError::io("remove stale unix socket", source))?;
                }
                if let Some(parent) = endpoint.path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|source| TransportError::io("create socket directory", source))?;
                }
                let listener = tokio::net::UnixListener::bind(&endpoint.path)
                    .map_err(|source| TransportError::io("bind unix socket", source))?;
                Ok(Self {
                    endpoint,
                    inner: ListenerInner::Unix(listener),
                })
            }
            #[cfg(windows)]
            TransportKind::UnixSocket => Err(TransportError::InvalidPath(
                "unix sockets are not supported on Windows".into(),
            )),
            #[cfg(unix)]
            TransportKind::NamedPipe => Err(TransportError::InvalidPath(
                "named pipes are not supported on Unix".into(),
            )),
            #[cfg(not(any(windows, unix)))]
            _ => Err(TransportError::InvalidPath(
                "unsupported platform for local transport".into(),
            )),
        }
    }

    /// The endpoint clients should connect to.
    #[must_use]
    pub fn endpoint(&self) -> &LocalEndpoint {
        &self.endpoint
    }

    /// Accept the next inbound connection.
    pub async fn accept(&mut self) -> Result<TransportStream, TransportError> {
        match &mut self.inner {
            #[cfg(windows)]
            ListenerInner::NamedPipe(listener) => listener.accept().await,
            #[cfg(unix)]
            ListenerInner::Unix(listener) => {
                let (stream, _) = listener
                    .accept()
                    .await
                    .map_err(|source| TransportError::io("accept unix connection", source))?;
                Ok(TransportStream::unix(stream))
            }
        }
    }
}

enum TransportStreamInner {
    #[cfg(windows)]
    PipeServer(tokio::net::windows::named_pipe::NamedPipeServer),
    #[cfg(windows)]
    PipeClient(tokio::net::windows::named_pipe::NamedPipeClient),
    #[cfg(unix)]
    Unix(tokio::net::UnixStream),
}

/// A connected local transport stream.
pub struct TransportStream {
    inner: TransportStreamInner,
}

impl TransportStream {
    #[cfg(unix)]
    fn unix(stream: tokio::net::UnixStream) -> Self {
        Self {
            inner: TransportStreamInner::Unix(stream),
        }
    }

    /// Open a client connection to `endpoint`.
    pub async fn connect(endpoint: &LocalEndpoint) -> Result<Self, TransportError> {
        match endpoint.kind {
            #[cfg(windows)]
            TransportKind::NamedPipe => {
                use tokio::net::windows::named_pipe::ClientOptions;
                let client = ClientOptions::new()
                    .open(endpoint.path_string())
                    .map_err(|source| TransportError::io("open named pipe client", source))?;
                Ok(Self {
                    inner: TransportStreamInner::PipeClient(client),
                })
            }
            #[cfg(unix)]
            TransportKind::UnixSocket => {
                let stream = tokio::net::UnixStream::connect(&endpoint.path)
                    .await
                    .map_err(|source| TransportError::io("connect unix socket", source))?;
                Ok(Self::unix(stream))
            }
            #[cfg(windows)]
            TransportKind::UnixSocket => Err(TransportError::InvalidPath(
                "unix sockets are not supported on Windows".into(),
            )),
            #[cfg(unix)]
            TransportKind::NamedPipe => Err(TransportError::InvalidPath(
                "named pipes are not supported on Unix".into(),
            )),
            #[cfg(not(any(windows, unix)))]
            _ => Err(TransportError::InvalidPath(
                "unsupported platform for local transport".into(),
            )),
        }
    }

    /// Split into independent read and write halves.
    pub fn into_split(self) -> (TransportReadHalf, TransportWriteHalf) {
        match self.inner {
            #[cfg(windows)]
            TransportStreamInner::PipeServer(stream) => {
                let (read, write) = tokio::io::split(stream);
                (
                    TransportReadHalf {
                        inner: ReadHalfInner::PipeServer(read),
                    },
                    TransportWriteHalf {
                        inner: WriteHalfInner::PipeServer(write),
                    },
                )
            }
            #[cfg(windows)]
            TransportStreamInner::PipeClient(stream) => {
                let (read, write) = tokio::io::split(stream);
                (
                    TransportReadHalf {
                        inner: ReadHalfInner::PipeClient(read),
                    },
                    TransportWriteHalf {
                        inner: WriteHalfInner::PipeClient(write),
                    },
                )
            }
            #[cfg(unix)]
            TransportStreamInner::Unix(stream) => {
                let (read, write) = stream.into_split();
                (
                    TransportReadHalf {
                        inner: ReadHalfInner::Unix(read),
                    },
                    TransportWriteHalf {
                        inner: WriteHalfInner::Unix(write),
                    },
                )
            }
        }
    }
}

enum ReadHalfInner {
    #[cfg(windows)]
    PipeServer(tokio::io::ReadHalf<tokio::net::windows::named_pipe::NamedPipeServer>),
    #[cfg(windows)]
    PipeClient(tokio::io::ReadHalf<tokio::net::windows::named_pipe::NamedPipeClient>),
    #[cfg(unix)]
    Unix(tokio::net::unix::OwnedReadHalf),
}

/// Read half of a transport stream.
pub struct TransportReadHalf {
    inner: ReadHalfInner,
}

impl AsyncRead for TransportReadHalf {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut self.inner {
            #[cfg(windows)]
            ReadHalfInner::PipeServer(read) => std::pin::Pin::new(read).poll_read(cx, buf),
            #[cfg(windows)]
            ReadHalfInner::PipeClient(read) => std::pin::Pin::new(read).poll_read(cx, buf),
            #[cfg(unix)]
            ReadHalfInner::Unix(read) => std::pin::Pin::new(read).poll_read(cx, buf),
        }
    }
}

enum WriteHalfInner {
    #[cfg(windows)]
    PipeServer(tokio::io::WriteHalf<tokio::net::windows::named_pipe::NamedPipeServer>),
    #[cfg(windows)]
    PipeClient(tokio::io::WriteHalf<tokio::net::windows::named_pipe::NamedPipeClient>),
    #[cfg(unix)]
    Unix(tokio::net::unix::OwnedWriteHalf),
}

/// Write half of a transport stream.
pub struct TransportWriteHalf {
    inner: WriteHalfInner,
}

impl AsyncWrite for TransportWriteHalf {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match &mut self.inner {
            #[cfg(windows)]
            WriteHalfInner::PipeServer(write) => std::pin::Pin::new(write).poll_write(cx, buf),
            #[cfg(windows)]
            WriteHalfInner::PipeClient(write) => std::pin::Pin::new(write).poll_write(cx, buf),
            #[cfg(unix)]
            WriteHalfInner::Unix(write) => std::pin::Pin::new(write).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match &mut self.inner {
            #[cfg(windows)]
            WriteHalfInner::PipeServer(write) => std::pin::Pin::new(write).poll_flush(cx),
            #[cfg(windows)]
            WriteHalfInner::PipeClient(write) => std::pin::Pin::new(write).poll_flush(cx),
            #[cfg(unix)]
            WriteHalfInner::Unix(write) => std::pin::Pin::new(write).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match &mut self.inner {
            #[cfg(windows)]
            WriteHalfInner::PipeServer(write) => std::pin::Pin::new(write).poll_shutdown(cx),
            #[cfg(windows)]
            WriteHalfInner::PipeClient(write) => std::pin::Pin::new(write).poll_shutdown(cx),
            #[cfg(unix)]
            WriteHalfInner::Unix(write) => std::pin::Pin::new(write).poll_shutdown(cx),
        }
    }
}

#[cfg(windows)]
mod windows_pipe {
    use tokio::net::windows::named_pipe::ServerOptions;

    use super::{TransportError, TransportStream, TransportStreamInner};

    pub struct WindowsPipeListener {
        path: String,
        first_instance: bool,
        pending: Option<tokio::net::windows::named_pipe::NamedPipeServer>,
    }

    impl WindowsPipeListener {
        pub fn new(path: String) -> Result<Self, TransportError> {
            let pending = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&path)
                .map_err(|source| TransportError::io("create named pipe server", source))?;
            Ok(Self {
                path,
                first_instance: false,
                pending: Some(pending),
            })
        }

        pub async fn accept(&mut self) -> Result<TransportStream, TransportError> {
            let server = if let Some(server) = self.pending.take() {
                server
            } else {
                ServerOptions::new()
                    .first_pipe_instance(self.first_instance)
                    .create(&self.path)
                    .map_err(|source| TransportError::io("create named pipe server", source))?
            };
            self.first_instance = false;
            server
                .connect()
                .await
                .map_err(|source| TransportError::io("accept named pipe connection", source))?;
            self.pending = Some(
                ServerOptions::new()
                    .first_pipe_instance(false)
                    .create(&self.path)
                    .map_err(|source| TransportError::io("create named pipe server", source))?,
            );
            Ok(TransportStream {
                inner: TransportStreamInner::PipeServer(server),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_endpoint_matches_platform() {
        let endpoint = default_endpoint(Path::new("/tmp/retcon"));
        #[cfg(windows)]
        {
            assert_eq!(endpoint.kind, TransportKind::NamedPipe);
            assert!(endpoint.path_string().starts_with(r"\\.\pipe\retcon-"));
        }
        #[cfg(unix)]
        {
            assert_eq!(endpoint.kind, TransportKind::UnixSocket);
            assert_eq!(endpoint.path, PathBuf::from("/tmp/retcon/core.sock"));
        }
    }
}
