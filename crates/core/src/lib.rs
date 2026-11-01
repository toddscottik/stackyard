//! Shared models, persistence, and the engine client.
//!
//! Desktop and CLI both talk to one engine through [`client`]. Neither crate
//! starts runtimes or servers on its own.

mod archive;
mod catalog;
mod client;
mod error;
mod install;
mod paths;
mod protocol;
mod store;
mod transport;

pub use catalog::{catalog_for_host, current_target, find_package, CatalogPackage};
pub use client::{ensure_engine, roundtrip};
pub use error::{Error, Result};
pub use install::{
    adopt_existing, discover_candidates, download_and_install, install_reader, probe_tool,
    remove_package, select_for_project, Progress, DEFAULT_PROJECT,
};
pub use paths::{default_data_dir, endpoint_id, pipe_path, prepare_data_dir, socket_path};
pub use protocol::{
    check_api_version, next_request_id, Command, DiscoveredRuntime, Outcome, Request, Response,
    API_VERSION,
};
pub use store::PackageRecord;
pub use store::Store;
pub use transport::{recv, send};
