//! OS integration: named pipes, credential vault, process management

pub mod error;
pub mod ide;
pub mod transport;

pub use error::TransportError;
pub use ide::{
    IdeAction, IdeDetection, IdeError, IdeIntegration, IdeLaunchReceipt, IdeLaunchRequest,
    LaunchSpec, SUPPORTED_IDE_IDS, SystemIdeIntegration, build_launch_spec,
};
pub use transport::{
    LocalEndpoint, TransportKind, TransportListener, TransportReadHalf, TransportStream,
    TransportWriteHalf, default_endpoint,
};
