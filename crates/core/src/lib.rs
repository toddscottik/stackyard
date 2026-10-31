//! Shared models, persistence, and the engine client.
//!
//! Desktop and CLI both talk to one engine through [`client`]. Neither crate
//! starts runtimes or servers on its own.

mod client;
mod error;
mod paths;
mod protocol;
mod store;
mod transport;

pub use client::{ensure_engine, roundtrip};
pub use error::{Error, Result};
pub use paths::{default_data_dir, endpoint_id, pipe_path, prepare_data_dir, socket_path};
pub use protocol::{
    check_api_version, next_request_id, Command, Outcome, Request, Response, API_VERSION,
};
pub use store::Store;
pub use transport::{recv, send};
