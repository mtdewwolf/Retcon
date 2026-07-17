//! OS integration: named pipes, credential vault, process management

pub mod error;
pub mod transport;

pub use error::TransportError;
pub use transport::{
    LocalEndpoint, TransportKind, TransportListener, TransportReadHalf, TransportStream,
    TransportWriteHalf, default_endpoint,
};
