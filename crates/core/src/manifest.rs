use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagerKind {
    Npm,
    Pnpm,
    Pip,
    Uv,
    Composer,
}

impl ManagerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Pip => "pip",
            Self::Uv => "uv",
            Self::Composer => "composer",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "npm" => Some(Self::Npm),
            "pnpm" => Some(Self::Pnpm),
            "pip" => Some(Self::Pip),
            "uv" => Some(Self::Uv),
            "composer" => Some(Self::Composer),
            _ => None,
        }
    }

    pub fn is_node(self) -> bool {
        matches!(self, Self::Npm | Self::Pnpm)
    }

    pub fn is_python(self) -> bool {
        matches!(self, Self::Pip | Self::Uv)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRole {
    Node,
    Python,
    Php,
}

impl RuntimeRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Python => "python",
            Self::Php => "php",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "node" => Some(Self::Node),
            "python" => Some(Self::Python),
            "php" => Some(Self::Php),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartPolicy {
    Never,
    OnFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentOwnership {
    Project,
    Managed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeSpec {
    pub id: String,
    pub directory: PathBuf,
    pub runtime: RuntimeRole,
    pub manager: Option<ManagerKind>,
    pub lockfile_root: PathBuf,
    pub environment: EnvironmentOwnership,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointProtocol {
    Http,
    Tcp,
    #[serde(rename = "fastcgi")]
    FastCgi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointSpec {
    pub protocol: EndpointProtocol,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReadinessSpec {
    Http {
        path: String,
        statuses: Vec<u16>,
        timeout_secs: u64,
    },
    Tcp {
        timeout_secs: u64,
    },
    Process {
        grace_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceSpec {
    pub id: String,
    pub runtime: RuntimeRole,
    pub scope_id: String,
    pub endpoint: Option<EndpointSpec>,
    pub readiness: ReadinessSpec,
    pub command: Vec<String>,
    pub working_directory: PathBuf,
    pub depends_on: Vec<String>,
    pub ready_path: String,
    pub ready_timeout_secs: u64,
    pub port: u16,
    pub restart: RestartPolicy,
    pub restart_max: u32,
    pub env: Vec<(String, String)>,
}

impl ServiceSpec {
    pub fn is_http_endpoint(&self) -> bool {
        self.endpoint
            .as_ref()
            .is_some_and(|endpoint| endpoint.protocol == EndpointProtocol::Http)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SiteKind {
    Proxy { service_id: String },
    Static { root: String },
    Php { service_id: String, root: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteSpec {
    pub host: String,
    pub kind: SiteKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub name: String,
    pub manager: Option<ManagerKind>,
    pub scopes: Vec<ScopeSpec>,
    pub services: Vec<ServiceSpec>,
    pub site: Option<SiteSpec>,
}

impl Manifest {
    pub fn scope(&self, id: &str) -> Option<&ScopeSpec> {
        self.scopes.iter().find(|scope| scope.id == id)
    }
    pub fn service(&self, id: &str) -> Option<&ServiceSpec> {
        self.services.iter().find(|service| service.id == id)
    }

    pub fn runtimes(&self) -> BTreeSet<RuntimeRole> {
        self.services
            .iter()
            .map(|service| service.runtime)
            .collect()
    }
}

pub fn load_manifest(root: &Path) -> Result<Manifest> {
    let path = root.join("stackyard.toml");
    let text = fs::read_to_string(&path).map_err(|source| Error::InvalidManifest {
        detail: format!("could not read {}: {source}", path.display()),
    })?;
    parse_manifest(&text)
}

pub fn parse_manifest(text: &str) -> Result<Manifest> {
    let raw: RawManifest = toml::from_str(text).map_err(|err| Error::InvalidManifest {
        detail: err.to_string(),
    })?;
    validate(raw)
}

pub fn start_order(services: &[ServiceSpec]) -> Result<Vec<String>> {
    let ids: BTreeSet<&str> = services.iter().map(|service| service.id.as_str()).collect();
    let mut pending: Vec<&ServiceSpec> = services.iter().collect();
    let mut order = Vec::new();
    while !pending.is_empty() {
        let next = pending.iter().position(|service| {
            service
                .depends_on
                .iter()
                .all(|dependency| order.iter().any(|done: &String| done == dependency))
        });
        let Some(index) = next else {
            return Err(Error::InvalidManifest {
                detail: format!(
                    "service dependency cycle among {}",
                    pending
                        .iter()
                        .map(|service| service.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        };
        let service = pending.remove(index);
        for dependency in &service.depends_on {
            if !ids.contains(dependency.as_str()) {
                return Err(Error::InvalidManifest {
                    detail: format!(
                        "service {} depends on unknown service {dependency}",
                        service.id
                    ),
                });
            }
        }
        order.push(service.id.clone());
    }
    Ok(order)
}

fn validate(raw: RawManifest) -> Result<Manifest> {
    if raw.schema == 2 {
        return validate_v2(raw);
    }
    if raw.schema != 1 {
        return Err(Error::InvalidManifest {
            detail: format!(
                "schema {} is not supported; this build reads schema 1",
                raw.schema
            ),
        });
    }
    if !raw.scope.is_empty()
        || raw
            .service
            .iter()
            .any(|s| !s.scope.is_empty() || s.endpoint.is_some() || s.readiness.is_some())
    {
        return invalid("schema 1 cannot contain scope, endpoint, or readiness declarations");
    }
    validate_name(&raw.name)?;
    if raw.service.is_empty()
        && raw
            .site
            .as_ref()
            .and_then(|site| site.root.as_ref())
            .is_none()
    {
        return Err(Error::InvalidManifest {
            detail: "at least one [[service]] is required, or a [site] with root for static files"
                .to_string(),
        });
    }
    let manager = match raw.manager {
        Some(value) => Some(
            ManagerKind::parse(&value).ok_or_else(|| Error::InvalidManifest {
                detail: format!("manager {value:?} must be npm, pnpm, pip, uv, or composer"),
            })?,
        ),
        None => None,
    };
    let mut services = Vec::with_capacity(raw.service.len());
    let mut seen = BTreeSet::new();
    for service in raw.service {
        if !seen.insert(service.id.clone()) {
            return Err(Error::InvalidManifest {
                detail: format!("duplicate service id {}", service.id),
            });
        }
        services.push(validate_service(service)?);
    }
    let site = match raw.site {
        Some(site) => Some(validate_site(&raw.name, &services, site)?),
        None => None,
    };
    let roles: BTreeSet<_> = services.iter().map(|service| service.runtime).collect();
    let scopes: Vec<_> = roles
        .iter()
        .map(|runtime| ScopeSpec {
            id: if roles.len() == 1 {
                "default".into()
            } else {
                format!("legacy-{}", runtime.as_str())
            },
            directory: ".".into(),
            runtime: *runtime,
            manager: manager.filter(|manager| manager_matches(*manager, *runtime)),
            lockfile_root: ".".into(),
            environment: if *runtime == RuntimeRole::Python {
                EnvironmentOwnership::Managed
            } else {
                EnvironmentOwnership::Project
            },
        })
        .collect();
    for service in &mut services {
        service.scope_id = scopes
            .iter()
            .find(|scope| scope.runtime == service.runtime)
            .expect("normalized scope")
            .id
            .clone();
    }
    let manifest = Manifest {
        schema: raw.schema,
        name: raw.name,
        manager,
        scopes,
        services,
        site,
    };
    let _ = start_order(&manifest.services)?;
    if let Some(manager) = manifest.manager {
        let runtimes = manifest.runtimes();
        if manager.is_node() && !runtimes.contains(&RuntimeRole::Node) {
            return Err(Error::InvalidManifest {
                detail: format!("manager {} needs a node service", manager.as_str()),
            });
        }
        if manager.is_python() && !runtimes.contains(&RuntimeRole::Python) {
            return Err(Error::InvalidManifest {
                detail: format!("manager {} needs a python service", manager.as_str()),
            });
        }
        if manager == ManagerKind::Composer && !runtimes.contains(&RuntimeRole::Php) {
            return Err(Error::InvalidManifest {
                detail: "manager composer needs a php service".to_string(),
            });
        }
    }
    Ok(manifest)
}

fn validate_v2(raw: RawManifest) -> Result<Manifest> {
    if raw.manager.is_some() {
        return invalid("schema 2 uses managers on [[scope]], not at the top level");
    }
    validate_name(&raw.name)?;
    if raw.scope.is_empty() {
        return invalid("schema 2 requires at least one [[scope]]");
    }
    let mut scopes = Vec::with_capacity(raw.scope.len());
    let mut ids = BTreeSet::new();
    for scope in raw.scope {
        validate_ident(&scope.id, "scope id")?;
        if !ids.insert(scope.id.clone()) {
            return invalid(&format!("duplicate scope id {}", scope.id));
        }
        let runtime = RuntimeRole::parse(&scope.runtime).ok_or_else(|| Error::InvalidManifest {
            detail: format!("scope {} runtime must be node, python, or php", scope.id),
        })?;
        let directory = relative_directory(&scope.directory)?;
        let lockfile_root = relative_directory(&scope.lockfile_root)?;
        if !path_contains(&lockfile_root, &directory) {
            return invalid(&format!(
                "scope {} lockfile_root must contain its directory",
                scope.id
            ));
        }
        let manager = Some(parse_manager(scope.manager.as_deref().ok_or_else(
            || Error::InvalidManifest {
                detail: format!("scope {} requires an explicit manager", scope.id),
            },
        )?)?);
        if manager.is_some_and(|m| !manager_matches(m, runtime)) {
            return invalid(&format!(
                "scope {} manager does not match runtime {}",
                scope.id,
                runtime.as_str()
            ));
        }
        let environment = match (scope.environment.as_deref(), runtime) {
            (None, RuntimeRole::Python) => EnvironmentOwnership::Managed,
            (None, _) => EnvironmentOwnership::Project,
            (Some("managed"), RuntimeRole::Python) => EnvironmentOwnership::Managed,
            (Some("project"), RuntimeRole::Python) => {
                return invalid("Python scopes require managed environments")
            }
            (Some("project"), _) => EnvironmentOwnership::Project,
            (Some("managed"), _) => {
                return invalid("managed environments are supported only for Python scopes")
            }
            (Some(value), _) => {
                return invalid(&format!(
                    "scope environment {value:?} must be project or managed"
                ))
            }
        };
        scopes.push(ScopeSpec {
            id: scope.id,
            directory,
            runtime,
            manager,
            lockfile_root,
            environment,
        });
    }
    for (i, a) in scopes.iter().enumerate() {
        for b in scopes.iter().skip(i + 1) {
            if a.lockfile_root == b.lockfile_root && a.manager != b.manager {
                return invalid(&format!(
                    "scopes {} and {} assign conflicting managers to the same lockfile root",
                    a.id, b.id
                ));
            }
        }
    }
    let mut services = Vec::with_capacity(raw.service.len());
    let mut service_ids = BTreeSet::new();
    for service in raw.service {
        if !service_ids.insert(service.id.clone()) {
            return invalid(&format!("duplicate service id {}", service.id));
        }
        let scope = scopes
            .iter()
            .find(|s| s.id == service.scope)
            .ok_or_else(|| Error::InvalidManifest {
                detail: format!(
                    "service {} references unknown scope {}",
                    service.id, service.scope
                ),
            })?;
        let mut normalized = validate_service(RawService {
            id: service.id,
            runtime: scope.runtime.as_str().into(),
            command: service.command,
            working_directory: service.working_directory,
            depends_on: service.depends_on,
            ready_path: "/".into(),
            ready_timeout_secs: 15,
            port: 0,
            restart: service.restart,
            restart_max: service.restart_max,
            env: service.env,
            scope: service.scope.clone(),
            endpoint: None,
            readiness: None,
        })?;
        if !path_contains(&scope.directory, &normalized.working_directory) {
            return invalid(&format!(
                "service {} working_directory must stay within scope {}",
                normalized.id, scope.id
            ));
        }
        normalized.scope_id = scope.id.clone();
        normalized.endpoint = service.endpoint.map(validate_endpoint).transpose()?;
        let readiness = service.readiness.ok_or_else(|| Error::InvalidManifest {
            detail: format!(
                "service {} requires an explicit readiness table",
                normalized.id
            ),
        })?;
        normalized.readiness =
            validate_readiness(readiness, normalized.endpoint.as_ref(), &normalized.id)?;
        if normalized.endpoint.is_none() && normalized.command.iter().any(|a| a.contains("{port}"))
        {
            return invalid(&format!(
                "service {} uses {{port}} without an endpoint",
                normalized.id
            ));
        }
        if normalized.endpoint.is_some()
            && normalized.command.iter().any(|a| a.contains("{port}"))
            && normalized.endpoint.as_ref().unwrap().port != 0
        {
            return invalid(&format!(
                "service {} uses {{port}} with a fixed endpoint port",
                normalized.id
            ));
        }
        normalized.ready_path = match &normalized.readiness {
            ReadinessSpec::Http { path, .. } => path.clone(),
            _ => "/".into(),
        };
        normalized.ready_timeout_secs = match &normalized.readiness {
            ReadinessSpec::Http { timeout_secs, .. } | ReadinessSpec::Tcp { timeout_secs } => {
                *timeout_secs
            }
            ReadinessSpec::Process { .. } => 0,
        };
        normalized.port = normalized.endpoint.as_ref().map_or(0, |e| e.port);
        services.push(normalized);
    }
    if services.is_empty() && raw.site.as_ref().and_then(|s| s.root.as_ref()).is_none() {
        return invalid(
            "at least one [[service]] is required, or a [site] with root for static files",
        );
    }
    let site = raw
        .site
        .map(|site| validate_site(&raw.name, &services, site))
        .transpose()?;
    if let Some(SiteSpec {
        kind: SiteKind::Proxy { service_id },
        ..
    }) = &site
    {
        let service = services
            .iter()
            .find(|s| s.id == *service_id)
            .expect("site validation checks reference");
        if !service.is_http_endpoint() {
            return invalid(&format!(
                "site proxy service {service_id} must declare an HTTP endpoint"
            ));
        }
    }
    let manifest = Manifest {
        schema: 2,
        name: raw.name,
        manager: None,
        scopes,
        services,
        site,
    };
    let _ = start_order(&manifest.services)?;
    Ok(manifest)
}

fn invalid<T>(detail: &str) -> Result<T> {
    Err(Error::InvalidManifest {
        detail: detail.to_string(),
    })
}
fn parse_manager(value: &str) -> Result<ManagerKind> {
    ManagerKind::parse(value).ok_or_else(|| Error::InvalidManifest {
        detail: format!("manager {value:?} must be npm, pnpm, pip, uv, or composer"),
    })
}
fn path_contains(parent: &Path, child: &Path) -> bool {
    parent == Path::new(".") || child == parent || child.starts_with(parent)
}
fn validate_endpoint(endpoint: RawEndpoint) -> Result<EndpointSpec> {
    let protocol = match endpoint.protocol.as_str() {
        "http" => EndpointProtocol::Http,
        "tcp" => EndpointProtocol::Tcp,
        "fastcgi" => EndpointProtocol::FastCgi,
        x => {
            return invalid(&format!(
                "endpoint protocol {x:?} must be http, tcp, or fastcgi"
            ))
        }
    };
    let port = match endpoint.port {
        RawPort::Auto(s) if s == "auto" => 0,
        RawPort::Auto(s) => {
            return invalid(&format!("endpoint port {s:?} must be auto or a number"))
        }
        RawPort::Number(p) if p >= 1024 => p,
        RawPort::Number(p) => {
            return invalid(&format!("endpoint port {p} must be 1024..=65535 or auto"))
        }
    };
    Ok(EndpointSpec { protocol, port })
}
fn validate_readiness(
    r: RawReadiness,
    endpoint: Option<&EndpointSpec>,
    id: &str,
) -> Result<ReadinessSpec> {
    let result = match r.kind.as_str() {
        "http" => {
            let (Some(path), Some(statuses)) = (r.path, r.statuses) else {
                return invalid("HTTP readiness requires path and statuses");
            };
            if r.grace_ms != 0 {
                return invalid("HTTP readiness cannot set grace_ms");
            }
            validate_ready_path(&path)?;
            if !(1..=120).contains(&r.timeout_secs)
                || statuses.is_empty()
                || statuses.iter().any(|s| !(100..=599).contains(s))
            {
                return invalid(
                    "HTTP readiness needs statuses in 100..=599 and timeout_secs in 1..=120",
                );
            }
            if endpoint.is_none_or(|e| e.protocol != EndpointProtocol::Http) {
                return invalid(&format!(
                    "service {id} HTTP readiness requires an HTTP endpoint"
                ));
            }
            ReadinessSpec::Http {
                path,
                statuses,
                timeout_secs: r.timeout_secs,
            }
        }
        "tcp" => {
            if r.path.is_some() || r.statuses.is_some() || r.grace_ms != 0 {
                return invalid("TCP readiness cannot set path, statuses, or grace_ms");
            }
            if endpoint.is_none() {
                return invalid(&format!("service {id} TCP readiness requires an endpoint"));
            }
            if !(1..=120).contains(&r.timeout_secs) {
                return invalid("TCP readiness timeout_secs must be 1..=120");
            }
            ReadinessSpec::Tcp {
                timeout_secs: r.timeout_secs,
            }
        }
        "process" => {
            if endpoint.is_some() {
                return invalid(&format!(
                    "service {id} process readiness cannot declare an endpoint"
                ));
            }
            if r.path.is_some() || r.statuses.is_some() || !(1..=30000).contains(&r.grace_ms) {
                return invalid(
                    "process readiness requires grace_ms in 1..=30000 and no HTTP fields",
                );
            }
            ReadinessSpec::Process {
                grace_ms: r.grace_ms,
            }
        }
        k => {
            return invalid(&format!(
                "readiness kind {k:?} must be http, tcp, or process"
            ))
        }
    };
    Ok(result)
}

fn validate_site(name: &str, services: &[ServiceSpec], raw: RawSite) -> Result<SiteSpec> {
    let host = match raw.host {
        Some(host) => crate::apache::validate_host(&host, true)?,
        None => crate::apache::default_host(name)?,
    };
    let has_service = raw.service.is_some();
    let has_root = raw.root.is_some();
    if let Some(handler) = raw.handler.as_deref() {
        if handler != "php" {
            return Err(Error::InvalidManifest {
                detail: format!("site handler {handler:?} is not supported"),
            });
        }
        let (Some(service_id), Some(root)) = (raw.service, raw.root) else {
            return Err(Error::InvalidManifest {
                detail: "PHP site needs handler = php, service and document root".into(),
            });
        };
        let service = services
            .iter()
            .find(|service| service.id == service_id)
            .ok_or_else(|| Error::InvalidManifest {
                detail: format!("PHP site references unknown service {service_id}"),
            })?;
        if service.runtime != RuntimeRole::Php
            || service.command.first().map(String::as_str) != Some("php-cgi")
        {
            return Err(Error::InvalidManifest {
                detail: "PHP document roots need a selected php-cgi service".into(),
            });
        }
        let _ = relative_directory(&root)?;
        return Ok(SiteSpec {
            host,
            kind: SiteKind::Php { service_id, root },
        });
    }
    let kind = match (raw.service, raw.root) {
        (Some(service_id), None) => {
            if !services.iter().any(|service| service.id == service_id) {
                return Err(Error::InvalidManifest {
                    detail: format!("site proxies unknown service {service_id}"),
                });
            }
            SiteKind::Proxy { service_id }
        }
        (None, Some(root)) => {
            let _ = relative_directory(&root)?;
            SiteKind::Static { root }
        }
        _ => {
            return Err(Error::InvalidManifest {
                detail: if has_service && has_root {
                    "site sets both service and root; choose a proxy or a static directory"
                        .to_string()
                } else {
                    "site needs service or root".to_string()
                },
            });
        }
    };
    Ok(SiteSpec { host, kind })
}

fn validate_service(raw: RawService) -> Result<ServiceSpec> {
    validate_ident(&raw.id, "service id")?;
    let runtime = RuntimeRole::parse(&raw.runtime).ok_or_else(|| Error::InvalidManifest {
        detail: format!("service {} runtime must be node, python, or php", raw.id),
    })?;
    if raw.command.is_empty() || raw.command.iter().any(|arg| arg.is_empty()) {
        return Err(Error::InvalidManifest {
            detail: format!(
                "service {} command must be a non-empty argument list",
                raw.id
            ),
        });
    }
    for arg in &raw.command {
        if arg.contains('\0') || arg.contains('\n') || arg.len() > 4096 {
            return Err(Error::InvalidManifest {
                detail: format!(
                    "service {} has an argument that cannot be passed safely",
                    raw.id
                ),
            });
        }
    }
    let working_directory = relative_directory(&raw.working_directory)?;
    if !(1..=120).contains(&raw.ready_timeout_secs) {
        return Err(Error::InvalidManifest {
            detail: format!("service {} ready_timeout_secs must be 1..=120", raw.id),
        });
    }
    if raw.port != 0 && !(1024..=65535).contains(&raw.port) {
        return Err(Error::InvalidManifest {
            detail: format!(
                "service {} port must be 0 for an assigned high port, or 1024..=65535",
                raw.id
            ),
        });
    }
    validate_ready_path(&raw.ready_path)?;
    let restart = match raw.restart.as_str() {
        "never" => RestartPolicy::Never,
        "on-failure" => RestartPolicy::OnFailure,
        other => {
            return Err(Error::InvalidManifest {
                detail: format!(
                    "service {} restart {other:?} must be never or on-failure",
                    raw.id
                ),
            });
        }
    };
    if restart == RestartPolicy::Never && raw.restart_max != 0 {
        return Err(Error::InvalidManifest {
            detail: format!(
                "service {} sets restart_max without restart = \"on-failure\"",
                raw.id
            ),
        });
    }
    if raw.restart_max > 5 {
        return Err(Error::InvalidManifest {
            detail: format!("service {} restart_max must be 0..=5", raw.id),
        });
    }
    let mut env = Vec::new();
    for (key, value) in raw.env {
        if !is_env_key(&key) || value.contains('\0') {
            return Err(Error::InvalidManifest {
                detail: format!(
                    "service {} has an environment entry that cannot be passed",
                    raw.id
                ),
            });
        }
        env.push((key, value));
    }
    for dependency in &raw.depends_on {
        validate_ident(dependency, "depends_on")?;
        if dependency == &raw.id {
            return Err(Error::InvalidManifest {
                detail: format!("service {} cannot depend on itself", raw.id),
            });
        }
    }
    Ok(ServiceSpec {
        id: raw.id,
        runtime,
        scope_id: String::new(),
        endpoint: Some(EndpointSpec {
            protocol: if raw.command.first().map(String::as_str) == Some("php-cgi")
                && runtime == RuntimeRole::Php
            {
                EndpointProtocol::FastCgi
            } else {
                EndpointProtocol::Http
            },
            port: raw.port,
        }),
        readiness: if raw.command.first().map(String::as_str) == Some("php-cgi")
            && runtime == RuntimeRole::Php
        {
            ReadinessSpec::Tcp {
                timeout_secs: raw.ready_timeout_secs,
            }
        } else {
            ReadinessSpec::Http {
                path: raw.ready_path.clone(),
                statuses: (200..=299).collect(),
                timeout_secs: raw.ready_timeout_secs,
            }
        },
        command: raw.command,
        working_directory,
        depends_on: raw.depends_on,
        ready_path: raw.ready_path,
        ready_timeout_secs: raw.ready_timeout_secs,
        port: raw.port,
        restart,
        restart_max: raw.restart_max,
        env,
    })
}

fn manager_matches(manager: ManagerKind, runtime: RuntimeRole) -> bool {
    matches!(
        (manager, runtime),
        (ManagerKind::Npm | ManagerKind::Pnpm, RuntimeRole::Node)
            | (ManagerKind::Pip | ManagerKind::Uv, RuntimeRole::Python)
            | (ManagerKind::Composer, RuntimeRole::Php)
    )
}

fn validate_name(name: &str) -> Result<()> {
    let ok = (1..=63).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidManifest {
            detail: format!(
                "name {name:?} must be 1-63 characters of lowercase ascii, digits, and hyphens"
            ),
        })
    }
}

fn validate_ident(value: &str, label: &str) -> Result<()> {
    let ok = (1..=32).contains(&value.len())
        && value.starts_with(|c: char| c.is_ascii_lowercase())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidManifest {
            detail: format!("{label} {value:?} is not a lowercase identifier"),
        })
    }
}

fn validate_ready_path(path: &str) -> Result<()> {
    let ok = path.starts_with('/')
        && path.len() <= 200
        && !path.contains("..")
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '.' | '-'));
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidManifest {
            detail: format!("ready_path {path:?} must be a relative HTTP path"),
        })
    }
}

fn relative_directory(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
    {
        return Err(Error::InvalidManifest {
            detail: format!("working_directory {value:?} must stay inside the project"),
        });
    }
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(Error::InvalidManifest {
            detail: format!("working_directory {value:?} cannot contain .."),
        });
    }
    Ok(path.to_path_buf())
}

fn is_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_') && key.len() <= 64
        }
        _ => false,
    }
}

fn default_dot() -> String {
    ".".to_string()
}

fn default_slash() -> String {
    "/".to_string()
}

fn default_timeout() -> u64 {
    15
}

fn default_restart() -> String {
    "never".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    schema: u32,
    name: String,
    #[serde(default)]
    manager: Option<String>,
    #[serde(default)]
    service: Vec<RawService>,
    #[serde(default)]
    scope: Vec<RawScope>,
    #[serde(default)]
    site: Option<RawSite>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSite {
    #[serde(default)]
    handler: Option<String>,
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    service: Option<String>,
    #[serde(default)]
    root: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawService {
    id: String,
    #[serde(default)]
    runtime: String,
    command: Vec<String>,
    #[serde(default = "default_dot")]
    working_directory: String,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default = "default_slash")]
    ready_path: String,
    #[serde(default = "default_timeout")]
    ready_timeout_secs: u64,
    #[serde(default)]
    port: u16,
    #[serde(default = "default_restart")]
    restart: String,
    #[serde(default)]
    restart_max: u32,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    endpoint: Option<RawEndpoint>,
    #[serde(default)]
    readiness: Option<RawReadiness>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScope {
    id: String,
    directory: String,
    runtime: String,
    #[serde(default)]
    manager: Option<String>,
    #[serde(default = "default_dot")]
    lockfile_root: String,
    #[serde(default)]
    environment: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEndpoint {
    protocol: String,
    port: RawPort,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawPort {
    Auto(String),
    Number(u16),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReadiness {
    kind: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    statuses: Option<Vec<u16>>,
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
    #[serde(default)]
    grace_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEB: &str = r#"
schema = 1
name = "node-http"

[[service]]
id = "web"
runtime = "node"
command = ["node", "server.js"]
"#;

    #[test]
    fn parses_a_service_and_orders_dependencies() {
        let text = r#"
schema = 1
name = "demo"

[[service]]
id = "api"
runtime = "python"
command = ["python", "serve"]
depends_on = ["setup"]

[[service]]
id = "setup"
runtime = "python"
command = ["python", "serve"]
"#;
        let manifest = parse_manifest(text).expect("manifest");
        assert_eq!(
            start_order(&manifest.services).expect("order"),
            ["setup", "api"]
        );
    }

    #[test]
    fn rejects_cycles_escapes_and_conflicting_restart() {
        assert!(parse_manifest(WEB).is_ok());
        let cycle = r#"
schema = 1
name = "demo"
[[service]]
id = "a"
runtime = "node"
command = ["node", "a.js"]
depends_on = ["b"]
[[service]]
id = "b"
runtime = "node"
command = ["node", "b.js"]
depends_on = ["a"]
"#;
        assert!(parse_manifest(cycle).is_err());
        let escape = WEB.replace(
            "command = [\"node\", \"server.js\"]",
            "working_directory = \"../secret\"\ncommand = [\"node\", \"server.js\"]",
        );
        assert!(parse_manifest(&escape).is_err());
        let restart = format!("{WEB}\nrestart_max = 2\n");
        assert!(parse_manifest(&restart).is_err());
    }

    const MIXED_V2: &str = r#"
schema = 2
name = "mixed-app"
[[scope]]
id = "frontend"
directory = "web"
runtime = "node"
manager = "pnpm"
lockfile_root = "web"
[[scope]]
id = "backend"
directory = "api"
runtime = "python"
manager = "uv"
lockfile_root = "api"
environment = "managed"
[[service]]
id = "web"
scope = "frontend"
command = ["pnpm", "dev", "--port", "{port}"]
working_directory = "web"
endpoint = { protocol = "http", port = "auto" }
readiness = { kind = "http", path = "/", statuses = [200, 302], timeout_secs = 12 }
[[service]]
id = "worker"
scope = "backend"
command = ["python", "worker.py"]
working_directory = "api"
readiness = { kind = "process", grace_ms = 500 }
"#;

    #[test]
    fn parses_schema_two_scopes_endpoints_and_process_worker() {
        let manifest = parse_manifest(MIXED_V2).expect("schema 2 manifest");
        assert_eq!(manifest.schema, 2);
        assert_eq!(
            manifest.scope("backend").unwrap().manager,
            Some(ManagerKind::Uv)
        );
        assert_eq!(manifest.service("worker").unwrap().scope_id, "backend");
        assert_eq!(manifest.service("worker").unwrap().endpoint, None);
        assert_eq!(
            manifest
                .service("web")
                .unwrap()
                .endpoint
                .as_ref()
                .unwrap()
                .port,
            0
        );
    }

    #[test]
    fn rejects_schema_two_invalid_ownership_references_and_readiness() {
        assert!(
            parse_manifest(&MIXED_V2.replace("scope = \"frontend\"", "scope = \"missing\""))
                .is_err()
        );
        assert!(
            parse_manifest(&MIXED_V2.replace("manager = \"pnpm\"", "manager = \"pip\"")).is_err()
        );
        assert!(
            parse_manifest(&MIXED_V2.replace("directory = \"api\"", "directory = \"../api\""))
                .is_err()
        );
        assert!(parse_manifest(
            &MIXED_V2
                .replace("port = \"auto\"", "port = 3000")
                .replace("protocol = \"http\"", "protocol = \"tcp\"")
        )
        .is_err());
        assert!(parse_manifest(&MIXED_V2.replace(
            "readiness = { kind = \"process\", grace_ms = 500 }",
            "readiness = { kind = \"http\", path = \"/\", statuses = [200] }"
        ))
        .is_err());
        assert!(
            parse_manifest(&MIXED_V2.replace("schema = 2", "schema = 2\nmanager = \"pnpm\""))
                .is_err()
        );
    }

    #[test]
    fn schema_two_requires_manager_and_lockfile_root_to_contain_scope() {
        let no_manager = MIXED_V2
            .replace("manager = \"pnpm\"\n", "")
            .replace("manager = \"uv\"\n", "");
        assert!(parse_manifest(&no_manager).is_err());

        let nested_scope = MIXED_V2
            .replace("directory = \"web\"", "directory = \"web/client\"")
            .replace(
                "working_directory = \"web\"",
                "working_directory = \"web/client\"",
            );
        assert!(parse_manifest(&nested_scope).is_ok());

        let nested_lockfile_root = nested_scope.replace(
            "lockfile_root = \"web\"",
            "lockfile_root = \"web/client/packages\"",
        );
        assert!(parse_manifest(&nested_lockfile_root).is_err());
    }

    #[test]
    fn schema_two_rejects_project_owned_python_environment() {
        let project_python =
            MIXED_V2.replace("environment = \"managed\"", "environment = \"project\"");
        assert!(parse_manifest(&project_python).is_err());
    }

    #[test]
    fn schema_one_php_cgi_keeps_fastcgi_tcp_normalization() {
        let text = r#"schema = 1
name = "php-app"
[[service]]
id = "php"
runtime = "php"
command = ["php-cgi", "-b", "127.0.0.1:9000"]
port = 9000
"#;
        let service = parse_manifest(text).unwrap().services.remove(0);
        assert_eq!(
            service.endpoint.as_ref().unwrap().protocol,
            EndpointProtocol::FastCgi
        );
        assert_eq!(service.readiness, ReadinessSpec::Tcp { timeout_secs: 15 });
    }
}
