//! Retcon's long-running local core service.

pub mod error;
pub mod lifecycle;
pub mod rpc;
pub mod server;
pub mod state;

pub use error::{CoreError, ErrorCode, ErrorSource};
pub use lifecycle::{CoreConfig, CoreRuntime};
