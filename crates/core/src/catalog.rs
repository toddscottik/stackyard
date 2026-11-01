use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const CATALOG_JSON: &str = include_str!("../../../catalog/packages.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CatalogFile {
    pub revision: u32,
    pub packages: Vec<CatalogPackage>,
    #[serde(default)]
    pub news: Vec<NewsItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct NewsItem {
    pub id: String,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CatalogPackage {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub version: String,
    pub target: String,
    pub format: String,
    pub url: String,
    pub sha256: String,
    pub digest_source: String,
    pub launch: String,
    pub version_args: Vec<String>,
}

pub fn current_target() -> &'static str {
    if cfg!(all(windows, target_arch = "x86_64")) {
        "windows-x64"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-arm64"
    } else {
        "unsupported"
    }
}

pub fn load_catalog() -> Result<CatalogFile> {
    serde_json::from_str(CATALOG_JSON)
        .map_err(|err| Error::InvalidMessage(format!("bundled catalog is invalid: {err}")))
}

pub fn find_package(id: &str) -> Result<CatalogPackage> {
    load_catalog()?
        .packages
        .into_iter()
        .find(|package| package.id == id)
        .ok_or_else(|| Error::PackageNotFound { id: id.to_string() })
}

pub fn catalog_for_host() -> Result<Vec<CatalogPackage>> {
    let host = current_target();
    Ok(load_catalog()?
        .packages
        .into_iter()
        .filter(|package| package.target == host)
        .collect())
}

pub fn require_host(package: &CatalogPackage) -> Result<()> {
    let host = current_target();
    if package.target == host {
        Ok(())
    } else {
        Err(Error::IncompatibleTarget {
            id: package.id.clone(),
            package: package.target.clone(),
            host: host.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_has_digests_and_urls() {
        let catalog = load_catalog().expect("catalog");
        assert!(catalog.revision >= 1);
        assert!(!catalog.packages.is_empty());
        for package in &catalog.packages {
            assert_eq!(package.sha256.len(), 64);
            assert!(package.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(package.url.starts_with("https://"));
            assert!(!package.launch.is_empty());
            assert!(!package.id.contains('/') && !package.id.contains('\\'));
        }
    }

    #[test]
    fn host_catalog_matches_this_machine() {
        let packages = catalog_for_host().expect("host catalog");
        let host = current_target();
        assert!(packages.iter().all(|package| package.target == host));
        if host == "windows-x64" {
            assert!(packages.iter().any(|package| package.name == "node"));
            assert!(packages.iter().any(|package| package.name == "python"));
        }
    }
}
