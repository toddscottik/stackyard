//! Shared models, persistence, and the engine client.
//!
//! Desktop and CLI both talk to one engine through [`client`]. Neither crate
//! starts runtimes or servers on its own.

mod apache;
mod archive;
mod catalog;
mod client;
pub mod configuration;
pub mod create;
mod database;
pub mod dependency_overview;
mod deps;
pub mod detect;
mod diagnostics;
mod error;
mod install;
mod logs;
mod manifest;
pub mod memcached;
mod paths;
pub mod php;
pub mod php_project;
pub mod preview;
mod project;
mod protocol;
pub mod recipes;
pub mod scopes;
mod store;
mod transport;
mod updates;

pub use apache::{
    accept_listen_port, create_ca, default_host, fingerprint, free_high_ports, issue_leaf,
    label_resolves_to_loopback, merge_hosts, module_directory, os_trust_command,
    parse_apache_version, port_is_free, quote, render_httpd, require_free, site_root, upstream_for,
    validate_host, HttpdSpec, ListenPorts, Material, Upstream, VirtualSite, DEFAULT_HTTPS_PORT,
    DEFAULT_HTTP_PORT, HOSTS_MARK, RESTART_IMPACT, TRUST_FILE,
};
pub use catalog::{catalog_for_host, current_target, find_package, CatalogPackage};
pub use client::{ensure_engine, roundtrip, roundtrip_within};
pub use database::{
    contained_sqlite, random_secret, read_secret, redact, require_empty_data_dir, require_ident,
    sqlite_backup, sqlite_init, sqlite_read, sqlite_write, write_secret, DEFAULT_MYSQL_PORT,
    DEFAULT_POSTGRES_PORT,
};
pub use deps::{
    manifest_fingerprint, plan_dependency, plan_dependency_with, resolve_manager, scope_dir,
    tool_version, DepAction, DepPlan, DepRequest, DependencyPreparation, PlannedCommand, Toolchain,
};
pub use diagnostics::{export_diagnostics, LIMITATIONS};
pub use error::{Error, Result};
pub use install::{
    adopt_existing, discover_candidates, download_and_install, ensure_default_node_manager,
    install_reader, probe_tool, record_existing_runtime, remove_package, select_for_project,
    Progress, DEFAULT_PROJECT,
};
pub use logs::{
    append_log, log_tail, read_log, read_log_follow, service_log_path, FollowLogChunk, LogChunk,
};
pub use manifest::{
    load_manifest, parse_manifest, start_order, EndpointProtocol, EndpointSpec,
    EnvironmentOwnership, ManagerKind, Manifest, ReadinessSpec, RestartPolicy, RuntimeRole,
    ScopeSpec, ServiceSpec, SiteKind, SiteSpec,
};
pub use paths::{
    default_data_dir, disposable_dir, endpoint_id, pipe_path, plain_path, prepare_data_dir,
    socket_path,
};
pub use project::{
    active_state, contained_dir, ensure_venv, environment_dir, import_project, list_project_views,
    load_project_manifest, prepend_path, project_view, require_startable, resolve_runtime,
    resolve_tool, service_launch, venv_python, EndpointView, Launch, ProjectView, ScopeView,
    ServiceView,
};
pub use protocol::{
    check_api_version, next_request_id, Command, DbView, DiscoveredRuntime, Outcome, Request,
    Response, SiteView, API_VERSION,
};
pub use store::{
    CacheInstanceRecord, DbBindingRecord, DbInstanceRecord, PackageRecord, ProcessRecord,
    ProjectRecord, PublicationRecord, ScopeSelectionRecord, ServiceObservationRecord, SqliteRecord,
    Store,
};
pub use transport::{recv, send};
pub use updates::{
    cleanup_unused, mark_install_finished, mark_install_running, reconcile_interrupted,
    record_external_version, refresh_updates, switch_project, UpdateNotice,
};
