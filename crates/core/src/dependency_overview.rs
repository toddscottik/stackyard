use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::error::{Error, Result};
use crate::manifest::ManagerKind;

const MAX_INPUT_BYTES: usize = 48 * 1024;
type InputFiles = Vec<(&'static str, PathBuf)>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyOverview {
    pub manager: String,
    pub directory: String,
    pub inputs: Vec<String>,
    pub packages: Vec<DeclaredDependency>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredDependency {
    pub name: String,
    pub requirement: String,
    pub group: String,
}

/// Read a bounded, redacted summary of direct declarations in the chosen dependency scope.
pub fn dependency_overview(
    data_dir: &Path,
    project_id: &str,
    scope: &str,
    scope_id: Option<&str>,
) -> Result<DependencyOverview> {
    let (project, manifest) = crate::load_project_manifest(data_dir, project_id)?;
    let root = Path::new(&project.root);
    let resolved = crate::scopes::resolve_dependency_scope(root, &manifest, scope_id, scope)?;
    let directory = if manifest.schema == 1 {
        resolved.directory
    } else {
        resolved.lockfile_root
    };
    let manager = resolved.manager;
    let (manifest_files, presence_files) = input_files(manager, &directory);
    let mut inputs = Vec::new();
    let mut packages = Vec::new();
    let mut exceeded_cap = false;
    for (name, path) in manifest_files {
        if let Some((contents, truncated)) = read_manifest(root, &path)? {
            inputs.push(name.to_string());
            exceeded_cap |= truncated;
            match manager {
                ManagerKind::Npm | ManagerKind::Pnpm if name == "package.json" => {
                    packages.extend(json_dependencies(
                        &contents,
                        &[
                            "dependencies",
                            "devDependencies",
                            "optionalDependencies",
                            "peerDependencies",
                        ],
                        false,
                    ));
                }
                ManagerKind::Composer if name == "composer.json" => {
                    packages.extend(json_dependencies(
                        &contents,
                        &["require", "require-dev"],
                        true,
                    ));
                }
                ManagerKind::Pip if name == "requirements.txt" => {
                    packages.extend(requirements(&contents))
                }
                ManagerKind::Uv if name == "pyproject.toml" => {
                    packages.extend(pyproject_dependencies(&contents))
                }
                _ => {}
            }
        }
    }
    let mut locks = Vec::new();
    for (name, path) in presence_files {
        if ordinary_file_present(root, &path)? {
            inputs.push(name.to_string());
            locks.push(name.to_string());
        }
    }
    packages.sort_by(|a, b| (&a.group, &a.name).cmp(&(&b.group, &b.name)));
    packages.dedup();
    let truncated = packages.len() > 128;
    packages.truncate(128);
    let has_pyproject = ordinary_file_present(root, &directory.join("pyproject.toml"))?;
    let lock_present = !locks.is_empty();
    let mut note = match manager {
        ManagerKind::Npm => if lock_present { "Direct package.json declarations are shown; the listed npm lockfile records the current resolution." } else { "Direct package.json declarations are shown; no npm lockfile was found, so installs resolve declarations." }.to_string(),
        ManagerKind::Pnpm => if lock_present { "Direct package.json declarations are shown; pnpm-lock.yaml records the current resolution." } else { "Direct package.json declarations are shown; no pnpm lockfile was found, so installs resolve declarations." }.to_string(),
        ManagerKind::Composer => if lock_present { "Direct composer.json declarations are shown; composer.lock records the current resolution." } else { "Direct composer.json declarations are shown; no composer.lock was found, so Composer resolves declarations." }.to_string(),
        ManagerKind::Pip => if has_pyproject && !inputs.iter().any(|i| i == "requirements.txt") { "pip installs requirements.txt only; pyproject.toml is present but is not used as a pip dependency input.".to_string() } else { "Direct requirements.txt declarations are shown; includes and transitive resolution are handled by pip.".to_string() },
        ManagerKind::Uv => if lock_present { "Direct project metadata is shown; uv.lock is present, and uv may refresh it during sync.".to_string() } else { "Direct project metadata is shown; no uv.lock was found, and uv resolves dependencies during sync.".to_string() },
    };
    if inputs.is_empty() {
        note.push_str(" No recognized dependency input or lock files were found.");
    }
    if packages.is_empty() {
        note.push_str(" No safe direct declarations were available to display.");
    }
    if exceeded_cap {
        note.push_str(
            " A manifest exceeded the 48 KiB read limit; displayed declarations may be incomplete.",
        );
    }
    if truncated {
        note.push_str(" The declaration list was capped at 128 entries.");
    }
    Ok(DependencyOverview {
        manager: manager.as_str().to_string(),
        directory: directory.display().to_string(),
        inputs,
        packages,
        note,
    })
}

fn input_files(manager: ManagerKind, dir: &Path) -> (InputFiles, InputFiles) {
    let (manifests, locks): (&[&str], &[&str]) = match manager {
        ManagerKind::Npm => (
            &["package.json"],
            &["package-lock.json", "npm-shrinkwrap.json"],
        ),
        ManagerKind::Pnpm => (&["package.json"], &["pnpm-lock.yaml"]),
        ManagerKind::Composer => (&["composer.json"], &["composer.lock"]),
        ManagerKind::Pip => (&["requirements.txt"], &[]),
        ManagerKind::Uv => (&["pyproject.toml"], &["uv.lock"]),
    };
    (
        manifests.iter().map(|n| (*n, dir.join(n))).collect(),
        locks.iter().map(|n| (*n, dir.join(n))).collect(),
    )
}

fn ordinary_file_present(root: &Path, path: &Path) -> Result<bool> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(Error::io("inspect dependency input", err)),
    };
    if !meta.is_file() || crate::create::is_redirect(&meta) {
        return Ok(false);
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| Error::io("resolve dependency input", e))?;
    Ok(canonical.starts_with(
        root.canonicalize()
            .map_err(|e| Error::io("resolve project root", e))?,
    ))
}

fn read_manifest(root: &Path, path: &Path) -> Result<Option<(String, bool)>> {
    if !ordinary_file_present(root, path)? {
        return Ok(None);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|e| Error::io("open dependency manifest", e))?;
    let metadata = file
        .metadata()
        .map_err(|e| Error::io("inspect dependency manifest", e))?;
    if !metadata.is_file() || crate::create::is_redirect(&metadata) {
        return Ok(None);
    }
    read_capped(file).map(Some)
}

fn read_capped(file: File) -> Result<(String, bool)> {
    let mut bytes = Vec::new();
    file.take((MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::io("read dependency manifest", e))?;
    let truncated = bytes.len() > MAX_INPUT_BYTES;
    bytes.truncate(MAX_INPUT_BYTES);
    Ok((String::from_utf8(bytes).unwrap_or_default(), truncated))
}

fn json_dependencies(contents: &str, groups: &[&str], composer: bool) -> Vec<DeclaredDependency> {
    let Ok(value) = serde_json::from_str::<Json>(contents) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for group in groups {
        if let Some(entries) = value.get(*group).and_then(Json::as_object) {
            for (name, requirement) in entries {
                let req = requirement
                    .as_str()
                    .and_then(safe_requirement)
                    .unwrap_or_default();
                if if composer {
                    safe_composer_name(name)
                } else {
                    safe_npm_name(name)
                } {
                    out.push(DeclaredDependency {
                        name: name.clone(),
                        requirement: req,
                        group: (*group).into(),
                    });
                }
            }
        }
    }
    out
}

fn pyproject_dependencies(contents: &str) -> Vec<DeclaredDependency> {
    let Ok(value) = toml::from_str::<toml::Value>(contents) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(deps) = value
        .get("project")
        .and_then(|v| v.get("dependencies"))
        .and_then(toml::Value::as_array)
    {
        for dep in deps.iter().filter_map(toml::Value::as_str) {
            if let Some(d) = parse_requirement(dep, "dependencies") {
                out.push(d);
            }
        }
    }
    out
}

fn requirements(contents: &str) -> Vec<DeclaredDependency> {
    contents
        .lines()
        .filter_map(|line| parse_requirement(line.trim(), "requirements"))
        .collect()
}

fn parse_requirement(line: &str, group: &str) -> Option<DeclaredDependency> {
    if line.is_empty()
        || line.starts_with('#')
        || line.starts_with('-')
        || line.contains(";")
        || line.contains('@')
    {
        return None;
    }
    let end = line
        .find(['<', '>', '=', '!', '~', '['])
        .unwrap_or(line.len());
    let name = line[..end].trim();
    if !safe_name(name) {
        return None;
    }
    let requirement = safe_requirement(line[end..].trim()).unwrap_or_default();
    Some(DeclaredDependency {
        name: name.to_string(),
        requirement,
        group: group.into(),
    })
}

fn safe_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn safe_npm_name(value: &str) -> bool {
    if let Some(scoped) = value.strip_prefix('@') {
        let mut parts = scoped.split('/');
        return parts.next().is_some_and(valid_segment)
            && parts.next().is_some_and(valid_segment)
            && parts.next().is_none();
    }
    valid_segment(value)
}

fn safe_composer_name(value: &str) -> bool {
    if !value.contains('/') {
        return safe_name(value);
    }
    let mut parts = value.split('/');
    parts.next().is_some_and(valid_segment)
        && parts.next().is_some_and(valid_segment)
        && parts.next().is_none()
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn safe_requirement(value: &str) -> Option<String> {
    if value.len() > 160
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b" .*^~<>=!|,+-()".contains(&b))
    {
        return None;
    }
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_manifest_returns_direct_declarations_and_sanitizes_specs() {
        let found = json_dependencies(
            r#"{"dependencies":{"react":"^18.2.0","private-lib":"https://user:secret@example.test/repo"},"devDependencies":{"vite":"~5.0"}}"#,
            &["dependencies", "devDependencies"],
            false,
        );
        assert_eq!(
            found,
            vec![
                DeclaredDependency {
                    name: "private-lib".into(),
                    requirement: String::new(),
                    group: "dependencies".into()
                },
                DeclaredDependency {
                    name: "react".into(),
                    requirement: "^18.2.0".into(),
                    group: "dependencies".into()
                },
                DeclaredDependency {
                    name: "vite".into(),
                    requirement: "~5.0".into(),
                    group: "devDependencies".into()
                },
            ]
        );
        let serialized = serde_json::to_string(&found).unwrap();
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("https://"));
    }

    #[test]
    fn requirements_skip_directives_and_sensitive_urls() {
        let found = requirements("flask>=3.0\n-r private.txt\n-e git+https://user:pw@example/repo\nprivate @ https://host/path\n");
        assert_eq!(
            found,
            vec![DeclaredDependency {
                name: "flask".into(),
                requirement: ">=3.0".into(),
                group: "requirements".into()
            }]
        );
    }

    #[test]
    fn scoped_npm_and_composer_names_are_allowed_but_malformed_names_are_not() {
        let npm = json_dependencies(
            r#"{"dependencies":{"@scope/pkg":"^1.2","@bad/name/extra":"1","plain":"2"}}"#,
            &["dependencies"],
            false,
        );
        assert_eq!(
            npm.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            vec!["@scope/pkg", "plain"]
        );

        let composer = json_dependencies(
            r#"{"require":{"vendor/package":"^2.0","bad/vendor/name":"1","php":"^8.2"}}"#,
            &["require"],
            true,
        );
        assert_eq!(
            composer.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            vec!["php", "vendor/package"]
        );
    }

    #[test]
    fn lockfiles_are_presence_only_and_manifest_reads_are_capped() {
        let temp = std::env::temp_dir().join(format!("dependency-overview-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();
        let lock = temp.join("pnpm-lock.yaml");
        fs::write(&lock, vec![b'x'; 2 * 1024 * 1024]).unwrap();
        assert!(ordinary_file_present(&temp, &lock).unwrap());
        let manifest = temp.join("package.json");
        fs::write(&manifest, vec![b' '; MAX_INPUT_BYTES + 128]).unwrap();
        let (contents, capped) = read_manifest(&temp, &manifest).unwrap().unwrap();
        assert!(capped);
        assert_eq!(contents.len(), MAX_INPUT_BYTES);
        let _ = fs::remove_dir_all(temp);
    }
}
