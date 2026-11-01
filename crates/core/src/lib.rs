//! Shared models, persistence, and the engine client.
//!
//! Desktop and CLI both talk to one engine through [`client`]. Neither crate
//! starts runtimes or servers on its own.

mod apache;
mod archive;
mod catalog;
mod client;
mod deps;
mod error;
mod install;
mod logs;
mod manifest;
mod paths;
mod project;
mod protocol;
mod store;
mod transport;

pub use apache::{
    accept_listen_port, create_ca, default_host, fingerprint, free_high_ports, issue_leaf,
    label_resolves_to_loopback, merge_hosts, module_directory, os_trust_command,
    parse_apache_version, port_is_free, quote, render_httpd, require_free, site_root, upstream_for,
    validate_host, HttpdSpec, ListenPorts, Material, Upstream, VirtualSite, DEFAULT_HTTPS_PORT,
    DEFAULT_HTTP_PORT, HOSTS_MARK, RESTART_IMPACT, TRUST_FILE,
};
pub use catalog::{catalog_for_host, current_target, find_package, CatalogPackage};
pub use client::{ensure_engine, roundtrip};
pub use deps::{
    manifest_fingerprint, plan_dependency, resolve_manager, scope_dir, tool_version, DepAction,
    DepPlan, DepRequest, PlannedCommand, Toolchain,
};
pub use error::{Error, Result};
pub use install::{
    adopt_existing, discover_candidates, download_and_install, ensure_default_node_manager,
    install_reader, probe_tool, remove_package, select_for_project, Progress, DEFAULT_PROJECT,
};
pub use logs::{append_log, log_tail, read_log, service_log_path, LogChunk};
pub use manifest::{
    load_manifest, start_order, ManagerKind, Manifest, RestartPolicy, RuntimeRole, ServiceSpec,
    SiteKind, SiteSpec,
};
pub use paths::{default_data_dir, endpoint_id, pipe_path, prepare_data_dir, socket_path};
pub use project::{
    active_state, contained_dir, ensure_venv, environment_dir, import_project, list_project_views,
    load_project_manifest, prepend_path, project_view, require_startable, resolve_runtime,
    resolve_tool, service_launch, venv_python, Launch, ProjectView, ServiceView,
};
pub use protocol::{
    check_api_version, next_request_id, Command, DiscoveredRuntime, Outcome, Request, Response,
    SiteView, API_VERSION,
};
pub use store::{PackageRecord, ProcessRecord, ProjectRecord, PublicationRecord, Store};
pub use transport::{recv, send};
