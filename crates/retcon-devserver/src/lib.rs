//! Framework-aware development server detection and lifecycle management.

mod detect;
mod domain;
mod ports;
mod runtime;

pub use detect::detect_start_command;
pub use domain::{
    DevServerCommand, DevServerEvent, Framework, LogStream, ProjectCommand, ReadyInfo,
    StartOptions, StartResult,
};
pub use ports::{PortAllocator, PortReservation};
pub use runtime::{DevServer, DevServerError, EventStream};
