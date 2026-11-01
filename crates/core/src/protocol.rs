use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const API_VERSION: u32 = 1;
pub const MAX_SETTING_BYTES: usize = 4096;
pub const MAX_MESSAGE_BYTES: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub v: u32,
    pub id: String,
    pub cmd: Command,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Command {
    Status,
    GetSetting {
        key: String,
    },
    PutSetting {
        key: String,
        value: String,
    },
    Stop,
    ListCatalog,
    ListPackages,
    Discover,
    StartInstall {
        catalog_id: String,
    },
    InstallStatus {
        operation_id: String,
    },
    CancelInstall {
        operation_id: String,
    },
    UseExisting {
        role: String,
        path: String,
    },
    SelectPackage {
        project_id: String,
        package_id: String,
    },
    RemovePackage {
        package_id: String,
    },
    ImportProject {
        path: String,
    },
    ListProjects,
    ProjectStatus {
        project_id: String,
    },
    StartProject {
        project_id: String,
    },
    StopProject {
        project_id: String,
    },
    RestartProject {
        project_id: String,
    },
    CancelStart {
        project_id: String,
    },
    ReadLogs {
        project_id: String,
        service_id: String,
        offset: u64,
    },
    Exec {
        project_id: String,
        service_id: String,
        args: Vec<String>,
    },
    DepInstall {
        project_id: String,
        scope: String,
    },
    DepAdd {
        project_id: String,
        package: String,
        scope: String,
    },
    DepRemove {
        project_id: String,
        package: String,
        scope: String,
    },
    DepUpdate {
        project_id: String,
        package: String,
        scope: String,
    },
    DepList {
        project_id: String,
        scope: String,
    },
    DepStatus {
        operation_id: String,
    },
    DepCancel {
        operation_id: String,
    },
    FrontStatus,
    PublishProject {
        project_id: String,
    },
    UnpublishProject {
        project_id: String,
    },
    RestartFront,
    SetFrontPorts {
        http_port: u16,
        https_port: u16,
    },
    TrustInstall {
        directory: String,
    },
    TrustRemove {
        directory: String,
    },
    WriteHosts {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub v: u32,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Outcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Status {
        pid: u32,
        data_dir: String,
        api_version: u32,
        schema_version: u32,
        last_event: Option<String>,
    },
    Setting {
        key: String,
        value: Option<String>,
    },
    Stopped,
    Catalog {
        packages: Vec<crate::catalog::CatalogPackage>,
    },
    Packages {
        packages: Vec<crate::store::PackageRecord>,
    },
    Discovered {
        items: Vec<DiscoveredRuntime>,
    },
    Operation {
        id: String,
        catalog_id: String,
        state: String,
        bytes: u64,
        message: String,
    },
    Project {
        project: crate::project::ProjectView,
    },
    Projects {
        projects: Vec<crate::project::ProjectView>,
    },
    Logs {
        project_id: String,
        service_id: String,
        offset: u64,
        next_offset: u64,
        text: String,
    },
    Task {
        id: String,
        label: String,
        state: String,
        message: String,
    },
    Front {
        state: String,
        pid: Option<u32>,
        http_port: u16,
        https_port: u16,
        restart_impact: String,
        ca_path: String,
        resolves_localhost: bool,
        sites: Vec<SiteView>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteView {
    pub project_id: String,
    pub host: String,
    pub mode: String,
    pub upstream: String,
    pub http_url: String,
    pub https_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredRuntime {
    pub role: String,
    pub path: String,
}

pub fn next_request_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

pub fn validate_setting_key(key: &str) -> Result<()> {
    let chars_ok = !key.is_empty()
        && key.len() <= 64
        && key.starts_with(|c: char| c.is_ascii_lowercase())
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if chars_ok {
        Ok(())
    } else {
        Err(Error::InvalidSettingKey {
            key: key.to_string(),
        })
    }
}

pub fn validate_setting_value(value: &str) -> Result<()> {
    let len = value.len();
    if len > MAX_SETTING_BYTES {
        Err(Error::SettingValueTooLarge {
            len,
            limit: MAX_SETTING_BYTES,
        })
    } else {
        Ok(())
    }
}

pub fn check_api_version(found: u32) -> Result<()> {
    if found == API_VERSION {
        Ok(())
    } else {
        Err(Error::UnsupportedApi {
            found,
            supported: API_VERSION,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_normal_key_and_rejects_path_like_keys() {
        assert!(validate_setting_key("display_name").is_ok());
        assert!(validate_setting_key("").is_err());
        assert!(validate_setting_key("1name").is_err());
        assert!(validate_setting_key("../secret").is_err());
        assert!(validate_setting_key("HasCaps").is_err());
    }

    #[test]
    fn request_round_trips_through_json() {
        let request = Request {
            v: API_VERSION,
            id: "t".to_string(),
            cmd: Command::PutSetting {
                key: "display_name".to_string(),
                value: "Ada".to_string(),
            },
        };
        let bytes = serde_json::to_vec(&request).expect("encode");
        let decoded: Request = serde_json::from_slice(&bytes).expect("decode");
        assert_eq!(decoded, request);
    }
}
