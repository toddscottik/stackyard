use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerKind {
    Npm,
    Pnpm,
    Pip,
    Uv,
}

impl ManagerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Pip => "pip",
            Self::Uv => "uv",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "npm" => Some(Self::Npm),
            "pnpm" => Some(Self::Pnpm),
            "pip" => Some(Self::Pip),
            "uv" => Some(Self::Uv),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeRole {
    Node,
    Python,
}

impl RuntimeRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Python => "python",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "node" => Some(Self::Node),
            "python" => Some(Self::Python),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartPolicy {
    Never,
    OnFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSpec {
    pub id: String,
    pub runtime: RuntimeRole,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SiteKind {
    Proxy { service_id: String },
    Static { root: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteSpec {
    pub host: String,
    pub kind: SiteKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub manager: Option<ManagerKind>,
    pub services: Vec<ServiceSpec>,
    pub site: Option<SiteSpec>,
}

impl Manifest {
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
    if raw.schema != 1 {
        return Err(Error::InvalidManifest {
            detail: format!(
                "schema {} is not supported; this build reads schema 1",
                raw.schema
            ),
        });
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
                detail: format!("manager {value:?} must be npm, pnpm, pip, or uv"),
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
    let manifest = Manifest {
        name: raw.name,
        manager,
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
    }
    Ok(manifest)
}

fn validate_site(name: &str, services: &[ServiceSpec], raw: RawSite) -> Result<SiteSpec> {
    let host = match raw.host {
        Some(host) => crate::apache::validate_host(&host, true)?,
        None => crate::apache::default_host(name)?,
    };
    let has_service = raw.service.is_some();
    let has_root = raw.root.is_some();
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
        detail: format!("service {} runtime must be node or python", raw.id),
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
    if path.is_absolute() {
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
    site: Option<RawSite>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSite {
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
}
