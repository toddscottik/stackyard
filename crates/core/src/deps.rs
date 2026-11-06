use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::error::{Error, Result};
use crate::manifest::{ManagerKind, Manifest, RuntimeRole};
use crate::project::{self, npm_launch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepAction {
    Install,
    Add,
    Remove,
    Update,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub env_remove: Vec<String>,
    pub working_directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepPlan {
    pub manager: ManagerKind,
    pub scope_dir: PathBuf,
    pub commands: Vec<PlannedCommand>,
    pub note: String,
}

#[derive(Debug, Clone, Copy)]
pub struct Toolchain<'a> {
    pub node: Option<&'a Path>,
    pub python: Option<&'a Path>,
    pub pnpm: Option<&'a Path>,
    pub uv: Option<&'a Path>,
    pub venv_python: Option<&'a Path>,
    pub php: Option<&'a Path>,
    pub composer: Option<&'a Path>,
}

pub fn resolve_manager(root: &Path, manifest: &Manifest) -> Result<ManagerKind> {
    let runtimes = manifest.runtimes();
    if runtimes.len() != 1 && manifest.manager.is_none() {
        return Err(Error::InvalidManifest {
            detail: "this project uses more than one runtime; set manager in stackyard.toml"
                .to_string(),
        });
    }
    if let Some(manager) = manifest.manager {
        if manager == ManagerKind::Composer && !runtimes.contains(&RuntimeRole::Php) {
            return Err(Error::InvalidManifest {
                detail: "Composer needs a PHP service".to_string(),
            });
        }
        if manager.is_node() && !runtimes.contains(&RuntimeRole::Node) {
            return Err(Error::InvalidManifest {
                detail: format!(
                    "manager {} does not match the project runtime",
                    manager.as_str()
                ),
            });
        }
        if manager.is_python() && !runtimes.contains(&RuntimeRole::Python) {
            return Err(Error::InvalidManifest {
                detail: format!(
                    "manager {} does not match the project runtime",
                    manager.as_str()
                ),
            });
        }
    }
    if runtimes.len() == 1 && runtimes.contains(&RuntimeRole::Php) {
        return Ok(ManagerKind::Composer);
    }
    if runtimes.contains(&RuntimeRole::Node)
        && (manifest.manager.is_none() || manifest.manager.is_some_and(ManagerKind::is_node))
        && !runtimes.contains(&RuntimeRole::Python)
    {
        return resolve_node_manager(root, manifest.manager);
    }
    if runtimes.contains(&RuntimeRole::Python) && !runtimes.contains(&RuntimeRole::Node) {
        return resolve_python_manager(root, manifest.manager);
    }
    manifest.manager.ok_or_else(|| Error::InvalidManifest {
        detail: "set manager in stackyard.toml".to_string(),
    })
}

pub struct DepRequest<'a> {
    pub action: DepAction,
    pub package: Option<&'a str>,
    pub scope: &'a str,
    pub scope_id: Option<&'a str>,
}

type VersionProbe<'a> = dyn Fn(&Path, &[String]) -> Result<String> + 'a;
type VenvProvision<'a> = dyn Fn(&Path, &Path) -> Result<()> + 'a;

pub struct DependencyPreparation<'a> {
    pub probe_version: &'a VersionProbe<'a>,
    pub create_venv: &'a VenvProvision<'a>,
}

pub fn plan_dependency(
    data_dir: &Path,
    project_id: &str,
    root: &Path,
    manifest: &Manifest,
    request: DepRequest<'_>,
    toolchain: Toolchain<'_>,
) -> Result<DepPlan> {
    plan_dependency_with(
        data_dir,
        project_id,
        root,
        manifest,
        request,
        toolchain,
        DependencyPreparation {
            probe_version: &tool_version,
            create_venv: &project::provision_venv,
        },
    )
}

pub fn plan_dependency_with(
    data_dir: &Path,
    project_id: &str,
    root: &Path,
    manifest: &Manifest,
    request: DepRequest<'_>,
    toolchain: Toolchain<'_>,
    preparation: DependencyPreparation<'_>,
) -> Result<DepPlan> {
    let DepRequest {
        action,
        package,
        scope,
        scope_id,
    } = request;
    let resolved = crate::scopes::resolve_dependency_scope(root, manifest, scope_id, scope)?;
    let manager = resolved.manager;
    let scope_dir = if manifest.schema == 1 {
        resolved.directory.clone()
    } else {
        resolved.lockfile_root.clone()
    };
    let env_dir =
        crate::scopes::resolved_environment(data_dir, project_id, root, manifest, &resolved.spec)?;
    if let Some(name) = package {
        validate_package_name(name)?;
    } else if matches!(
        action,
        DepAction::Add | DepAction::Remove | DepAction::Update
    ) {
        return Err(Error::InvalidMessage(
            "this dependency operation needs a package name".to_string(),
        ));
    }
    let mut note = String::new();
    let commands = match manager {
        ManagerKind::Npm => {
            let node = toolchain.node.ok_or_else(|| {
                Error::InvalidMessage("select a node runtime before running npm".to_string())
            })?;
            let (program, prefix) = npm_launch(node)?;
            node_commands(manager, program, prefix, action, package, &scope_dir)?
        }
        ManagerKind::Pnpm => {
            let pnpm = toolchain.pnpm.ok_or_else(|| {
                Error::InvalidMessage("select a pnpm tool before running pnpm".to_string())
            })?;
            check_package_manager_field(&scope_dir, "pnpm", pnpm, preparation.probe_version)?;
            node_commands(
                manager,
                pnpm.to_path_buf(),
                Vec::new(),
                action,
                package,
                &scope_dir,
            )?
        }
        ManagerKind::Pip => {
            if scope_dir.join("uv.lock").is_file()
                && resolved.spec.manager != Some(ManagerKind::Pip)
            {
                return Err(Error::InvalidManifest {
                    detail: "uv.lock is present; pip will not rewrite a uv project".to_string(),
                });
            }
            let base = toolchain.python.ok_or_else(|| {
                Error::InvalidMessage("select a python runtime before running pip".to_string())
            })?;
            let python = if let Some(venv) = toolchain.venv_python {
                venv.to_path_buf()
            } else {
                project::ensure_venv_with(base, &env_dir, preparation.create_venv)?
            };
            if matches!(
                action,
                DepAction::Add | DepAction::Remove | DepAction::Update
            ) {
                note = "pip changed the virtual environment; requirements files were not rewritten"
                    .to_string();
            }
            pip_commands(python, action, package, &scope_dir)?
        }
        ManagerKind::Uv => {
            let uv = toolchain.uv.ok_or_else(|| {
                Error::InvalidMessage("select a uv tool before running uv".to_string())
            })?;
            let python = toolchain.python.ok_or_else(|| {
                Error::InvalidMessage("select a python runtime before running uv".to_string())
            })?;
            uv_commands(uv, python, &env_dir, &scope_dir, action, package)?
        }
        ManagerKind::Composer => {
            let php = toolchain.php.ok_or_else(|| {
                Error::InvalidMessage("select a PHP runtime before running Composer".to_string())
            })?;
            let composer = toolchain.composer.ok_or_else(|| {
                Error::InvalidMessage("select a Composer phar before running Composer".to_string())
            })?;
            let profile_key =
                crate::scopes::scope_profile_key(project_id, &resolved.spec.id, manifest.schema)?;
            composer_commands(
                data_dir,
                &profile_key,
                php,
                composer,
                action,
                package,
                &scope_dir,
            )?
        }
    };
    if resolved.spec.runtime == RuntimeRole::Python {
        crate::Store::open(data_dir)?.bind_scope_environment(
            project_id,
            &resolved.spec.id,
            &env_dir.display().to_string(),
        )?;
    }
    Ok(DepPlan {
        manager,
        scope_dir,
        commands,
        note,
    })
}

pub fn scope_dir(root: &Path, scope: &str) -> Result<PathBuf> {
    if scope.is_empty() || scope == "." {
        return root
            .canonicalize()
            .map(crate::paths::plain_path)
            .map_err(|source| Error::io("open project directory", source));
    }
    let relative = Path::new(scope);
    if relative.is_absolute()
        || scope.contains('\\')
        || relative
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(Error::InvalidMessage(format!(
            "scope {scope:?} must be a directory inside the project"
        )));
    }
    project::contained_dir(root, relative)
}

pub fn manifest_fingerprint(dir: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for name in [
        "package.json",
        "package-lock.json",
        "pnpm-lock.yaml",
        "npm-shrinkwrap.json",
        "requirements.txt",
        "pyproject.toml",
        "uv.lock",
        "composer.json",
        "composer.lock",
    ] {
        hasher.update(name.as_bytes());
        match fs::read(dir.join(name)) {
            Ok(bytes) => {
                hasher.update(b"1");
                hasher.update(bytes);
            }
            Err(_) => hasher.update(b"0"),
        }
    }
    hex::encode(hasher.finalize())
}

fn composer_commands(
    data_dir: &Path,
    project_id: &str,
    php: &Path,
    composer: &Path,
    action: DepAction,
    package: Option<&str>,
    dir: &Path,
) -> Result<Vec<PlannedCommand>> {
    if !dir.join("composer.json").is_file() {
        return Err(Error::InvalidManifest {
            detail: "Composer scope needs composer.json".to_string(),
        });
    }
    if composer.extension().and_then(|ext| ext.to_str()) != Some("phar") {
        return Err(Error::InvalidMessage(
            "select composer.phar; Stackyard runs it with the selected PHP".to_string(),
        ));
    }
    let (prefix, mut env) = crate::php_project::php_launch_config(data_dir, project_id, php)?;
    env.extend([
        (
            "COMPOSER".into(),
            dir.join("composer.json").display().to_string(),
        ),
        (
            "COMPOSER_HOME".into(),
            data_dir
                .join("php")
                .join("composer")
                .join(project_id)
                .join("home")
                .display()
                .to_string(),
        ),
        (
            "COMPOSER_CACHE_DIR".into(),
            data_dir
                .join("php")
                .join("composer")
                .join(project_id)
                .join("cache")
                .display()
                .to_string(),
        ),
    ]);
    let mut args = prefix;
    args.push(composer.display().to_string());
    args.extend(["--no-interaction".to_string(), "--no-ansi".to_string()]);
    let base = args.clone();
    match action {
        DepAction::Install => args.push("install".to_string()),
        DepAction::Add => args.extend(["require".to_string(), package.unwrap().to_string()]),
        DepAction::Remove => args.extend(["remove".to_string(), package.unwrap().to_string()]),
        DepAction::Update => args.extend(["update".to_string(), package.unwrap().to_string()]),
        DepAction::List => args.extend(["show".to_string(), "--format=json".to_string()]),
    }
    let mut commands = vec![PlannedCommand {
        program: php.to_path_buf(),
        args,
        env: env.clone(),
        env_remove: vec!["COMPOSER_VENDOR_DIR".into(), "COMPOSER_BIN_DIR".into()],
        working_directory: dir.to_path_buf(),
    }];
    if action != DepAction::List {
        let mut args = base;
        args.push("check-platform-reqs".to_string());
        commands.push(PlannedCommand {
            program: php.to_path_buf(),
            args,
            env,
            env_remove: vec!["COMPOSER_VENDOR_DIR".into(), "COMPOSER_BIN_DIR".into()],
            working_directory: dir.to_path_buf(),
        });
    }
    Ok(commands)
}

pub fn tool_version(program: &Path, args: &[String]) -> Result<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|source| Error::io("read tool version", source))?;
    if !output.status.success() {
        return Err(Error::ProbeFailed {
            path: program.to_path_buf(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().next().unwrap_or("").trim().to_string())
}

pub(crate) fn resolve_node_manager(
    root: &Path,
    explicit: Option<ManagerKind>,
) -> Result<ManagerKind> {
    if explicit.is_some_and(|manager| !manager.is_node()) {
        return Err(Error::InvalidMessage(
            "manager_conflict: Node scopes require npm or pnpm".into(),
        ));
    }
    let package_manager = package_manager_name(root)?;
    let npm_lock =
        root.join("package-lock.json").is_file() || root.join("npm-shrinkwrap.json").is_file();
    let pnpm_lock = root.join("pnpm-lock.yaml").is_file();
    if npm_lock && pnpm_lock && explicit.is_none() {
        return Err(Error::InvalidManifest {
            detail: "package-lock.json and pnpm-lock.yaml both exist; set manager in stackyard.toml to npm or pnpm"
                .to_string(),
        });
    }
    if let Some(manager) = explicit {
        if manager == ManagerKind::Npm && pnpm_lock && !npm_lock {
            return Err(Error::InvalidManifest {
                detail: "pnpm-lock.yaml is present; choosing npm would create a second lockfile"
                    .to_string(),
            });
        }
        if manager == ManagerKind::Pnpm && npm_lock && !pnpm_lock {
            return Err(Error::InvalidManifest {
                detail:
                    "package-lock.json is present; choosing pnpm would create a second lockfile"
                        .to_string(),
            });
        }
        return Ok(manager);
    }
    if let Some(manager) = package_manager {
        if manager == ManagerKind::Npm && pnpm_lock && !npm_lock {
            return Err(Error::InvalidManifest {
                detail: "packageManager says npm but pnpm-lock.yaml is the only lockfile"
                    .to_string(),
            });
        }
        if manager == ManagerKind::Pnpm && npm_lock && !pnpm_lock {
            return Err(Error::InvalidManifest {
                detail: "packageManager says pnpm but package-lock.json is the only lockfile"
                    .to_string(),
            });
        }
        return Ok(manager);
    }
    if pnpm_lock {
        return Ok(ManagerKind::Pnpm);
    }
    if npm_lock {
        return Ok(ManagerKind::Npm);
    }
    Ok(ManagerKind::Pnpm)
}

pub(crate) fn resolve_python_manager(
    root: &Path,
    explicit: Option<ManagerKind>,
) -> Result<ManagerKind> {
    let uv_lock = root.join("uv.lock").is_file();
    match explicit {
        Some(ManagerKind::Pip) if uv_lock => Err(Error::InvalidManifest {
            detail:
                "uv.lock is present; set manager to uv or remove that lockfile before using pip"
                    .to_string(),
        }),
        Some(manager) => Ok(manager),
        None if uv_lock => Ok(ManagerKind::Uv),
        None => Ok(ManagerKind::Pip),
    }
}

pub(crate) fn package_manager_name(root: &Path) -> Result<Option<ManagerKind>> {
    let path = root.join("package.json");
    if !path.is_file() {
        return Ok(None);
    }
    let text =
        fs::read_to_string(&path).map_err(|source| Error::io("read package.json", source))?;
    let value: Value = serde_json::from_str(&text).map_err(|err| Error::InvalidManifest {
        detail: format!("package.json is invalid: {err}"),
    })?;
    let Some(field) = value.get("packageManager").and_then(Value::as_str) else {
        return Ok(None);
    };
    let name = field.split('@').next().unwrap_or(field);
    match name {
        "npm" => Ok(Some(ManagerKind::Npm)),
        "pnpm" => Ok(Some(ManagerKind::Pnpm)),
        "yarn" => Err(Error::InvalidManifest {
            detail: "packageManager yarn is not supported; choose npm or pnpm".to_string(),
        }),
        other => Err(Error::InvalidManifest {
            detail: format!("packageManager {other:?} is not supported"),
        }),
    }
}

fn check_package_manager_field(
    root: &Path,
    expected: &str,
    program: &Path,
    probe: &VersionProbe<'_>,
) -> Result<()> {
    let path = root.join("package.json");
    if !path.is_file() {
        return Ok(());
    }
    let text =
        fs::read_to_string(&path).map_err(|source| Error::io("read package.json", source))?;
    let value: Value = serde_json::from_str(&text).map_err(|err| Error::InvalidManifest {
        detail: format!("package.json is invalid: {err}"),
    })?;
    let Some(field) = value.get("packageManager").and_then(Value::as_str) else {
        return Ok(());
    };
    let Some((name, version)) = field.split_once('@') else {
        return Ok(());
    };
    if name != expected || version.is_empty() {
        return Ok(());
    }
    let actual = probe(program, &["--version".to_string()]).or_else(|error| match error {
        Error::Cancelled { .. } => Err(error),
        _ => probe(program, &["-v".to_string()]),
    })?;
    let actual_version = actual.split_whitespace().last().unwrap_or(actual.as_str());
    if actual_version != version {
        return Err(Error::InvalidMessage(format!(
            "package.json asks for {name}@{version}; the selected tool is {actual_version}"
        )));
    }
    Ok(())
}

fn node_commands(
    manager: ManagerKind,
    program: PathBuf,
    prefix: Vec<String>,
    action: DepAction,
    package: Option<&str>,
    scope_dir: &Path,
) -> Result<Vec<PlannedCommand>> {
    let mut tail = match action {
        DepAction::Install if manager == ManagerKind::Npm && has_npm_lock(scope_dir) => {
            vec!["ci".to_string()]
        }
        DepAction::Install
            if manager == ManagerKind::Pnpm && scope_dir.join("pnpm-lock.yaml").is_file() =>
        {
            vec!["install".to_string(), "--frozen-lockfile".to_string()]
        }
        DepAction::Install => vec!["install".to_string()],
        DepAction::Add if manager == ManagerKind::Pnpm => {
            vec!["add".to_string(), package.unwrap().to_string()]
        }
        DepAction::Add => vec!["install".to_string(), package.unwrap().to_string()],
        DepAction::Remove if manager == ManagerKind::Pnpm => {
            vec!["remove".to_string(), package.unwrap().to_string()]
        }
        DepAction::Remove => vec!["uninstall".to_string(), package.unwrap().to_string()],
        DepAction::Update => vec!["update".to_string(), package.unwrap().to_string()],
        DepAction::List => vec!["ls".to_string(), "--depth".to_string(), "0".to_string()],
    };
    let mut args = prefix;
    args.append(&mut tail);
    Ok(vec![planned(program, args, scope_dir, &[])])
}

fn has_npm_lock(dir: &Path) -> bool {
    dir.join("package-lock.json").is_file() || dir.join("npm-shrinkwrap.json").is_file()
}

fn pip_commands(
    python: PathBuf,
    action: DepAction,
    package: Option<&str>,
    scope_dir: &Path,
) -> Result<Vec<PlannedCommand>> {
    let mut args = vec!["-m".to_string(), "pip".to_string()];
    match action {
        DepAction::Install => {
            let requirements = scope_dir.join("requirements.txt");
            if requirements.is_file() {
                args.extend([
                    "install".to_string(),
                    "-r".to_string(),
                    "requirements.txt".to_string(),
                ]);
            } else if scope_dir.join("pyproject.toml").is_file() {
                return Err(Error::InvalidMessage(
                    "pyproject.toml has no pip workflow here; add requirements.txt or select uv"
                        .to_string(),
                ));
            } else {
                return Err(Error::InvalidMessage(
                    "no requirements.txt to install".to_string(),
                ));
            }
        }
        DepAction::Add | DepAction::Update => {
            args.push("install".to_string());
            if action == DepAction::Update {
                args.push("--upgrade".to_string());
            }
            args.push(package.unwrap().to_string());
        }
        DepAction::Remove => args.extend([
            "uninstall".to_string(),
            "-y".to_string(),
            package.unwrap().to_string(),
        ]),
        DepAction::List => args.push("list".to_string()),
    }
    let env = vec![
        ("PIP_DISABLE_PIP_VERSION_CHECK".to_string(), "1".to_string()),
        ("PIP_NO_INPUT".to_string(), "1".to_string()),
    ];
    Ok(vec![planned(python, args, scope_dir, &env)])
}

fn uv_commands(
    uv: &Path,
    python: &Path,
    env_dir: &Path,
    root: &Path,
    action: DepAction,
    package: Option<&str>,
) -> Result<Vec<PlannedCommand>> {
    let env = vec![
        ("UV_PYTHON_DOWNLOADS".to_string(), "never".to_string()),
        ("UV_PYTHON".to_string(), python.display().to_string()),
        (
            "UV_PROJECT_ENVIRONMENT".to_string(),
            env_dir.display().to_string(),
        ),
    ];
    let program = uv.to_path_buf();
    let commands = match action {
        DepAction::Install => vec![vec!["sync".to_string()]],
        DepAction::Add => vec![vec!["add".to_string(), package.unwrap().to_string()]],
        DepAction::Remove => vec![vec!["remove".to_string(), package.unwrap().to_string()]],
        DepAction::Update => vec![
            vec![
                "lock".to_string(),
                "--upgrade-package".to_string(),
                package.unwrap().to_string(),
            ],
            vec!["sync".to_string()],
        ],
        DepAction::List => vec![vec![
            "pip".to_string(),
            "list".to_string(),
            "--python".to_string(),
            project::venv_python(env_dir).display().to_string(),
        ]],
    };
    Ok(commands
        .into_iter()
        .map(|args| planned(program.clone(), args, root, &env))
        .collect())
}

fn planned(
    program: PathBuf,
    args: Vec<String>,
    cwd: &Path,
    env: &[(String, String)],
) -> PlannedCommand {
    PlannedCommand {
        env_remove: Vec::new(),
        program,
        args,
        env: env.to_vec(),
        working_directory: cwd.to_path_buf(),
    }
}

pub fn validate_package_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('-')
        && !name.contains([' ', '\0', '\n', '\r', '"', '\''])
        && name.chars().all(|c| c.is_ascii_graphic());
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidMessage(format!(
            "package name {name:?} is not accepted"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::parse_manifest;
    use std::fs;

    fn write_project(body: &str, extra: &[(&str, &str)]) -> PathBuf {
        let root = crate::paths::disposable_dir("deps");
        fs::create_dir_all(root.join("packages/web")).expect("dirs");
        fs::write(root.join("stackyard.toml"), body).expect("toml");
        for (name, text) in extra {
            let path = root.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("parent");
            }
            fs::write(path, text).expect("file");
        }
        root
    }

    fn node_manifest(manager: &str) -> String {
        format!(
            r#"
schema = 1
name = "demo"
manager = "{manager}"
[[service]]
id = "web"
runtime = "node"
command = ["node", "server.js"]
"#
        )
    }

    #[test]
    fn lockfiles_choose_the_manager_and_conflicts_ask() {
        let npm = write_project(
            r#"
schema = 1
name = "demo"
[[service]]
id = "web"
runtime = "node"
command = ["node", "server.js"]
"#,
            &[("package-lock.json", "{}\n")],
        );
        let manifest =
            parse_manifest(&fs::read_to_string(npm.join("stackyard.toml")).unwrap()).unwrap();
        assert_eq!(resolve_manager(&npm, &manifest).unwrap(), ManagerKind::Npm);

        let pnpm = write_project(
            r#"
schema = 1
name = "demo"
[[service]]
id = "web"
runtime = "node"
command = ["node", "server.js"]
"#,
            &[
                ("pnpm-lock.yaml", "lockfileVersion: '9.0'\n"),
                ("package.json", "{\"packageManager\":\"pnpm@12.16.1\"}\n"),
            ],
        );
        let manifest =
            parse_manifest(&fs::read_to_string(pnpm.join("stackyard.toml")).unwrap()).unwrap();
        assert_eq!(
            resolve_manager(&pnpm, &manifest).unwrap(),
            ManagerKind::Pnpm
        );

        let both = write_project(
            &node_manifest("npm").replace("manager = \"npm\"\n", ""),
            &[
                ("package-lock.json", "{}\n"),
                ("pnpm-lock.yaml", "lockfileVersion: '9.0'\n"),
            ],
        );
        let manifest =
            parse_manifest(&fs::read_to_string(both.join("stackyard.toml")).unwrap()).unwrap();
        assert!(resolve_manager(&both, &manifest).is_err());

        let uv = write_project(
            r#"
schema = 1
name = "demo"
[[service]]
id = "web"
runtime = "python"
command = ["python", "serve"]
"#,
            &[("uv.lock", "version = 1\n")],
        );
        let manifest =
            parse_manifest(&fs::read_to_string(uv.join("stackyard.toml")).unwrap()).unwrap();
        assert_eq!(resolve_manager(&uv, &manifest).unwrap(), ManagerKind::Uv);
        let _ = fs::remove_dir_all(npm);
        let _ = fs::remove_dir_all(pnpm);
        let _ = fs::remove_dir_all(both);
        let _ = fs::remove_dir_all(uv);
    }

    #[test]
    fn scope_and_package_names_stay_inside_the_project() {
        let root = write_project(&node_manifest("npm"), &[]);
        let web = root.join("packages/web");
        fs::create_dir_all(&web).unwrap();
        let resolved = scope_dir(&root, "packages/web").unwrap();
        assert_eq!(resolved, crate::plain_path(web.canonicalize().unwrap()));
        assert!(scope_dir(&root, "../outside").is_err());
        assert!(validate_package_name("-rf").is_err());
        assert!(validate_package_name("@scope/name").is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn shipped_fixtures_keep_their_package_managers() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
        let expect = [
            ("node-http", ManagerKind::Npm),
            ("next-app", ManagerKind::Pnpm),
            ("flask-app", ManagerKind::Pip),
            ("fastapi-app", ManagerKind::Pip),
        ];
        for (name, manager) in expect {
            let root = fixtures.join(name);
            let manifest =
                parse_manifest(&fs::read_to_string(root.join("stackyard.toml")).unwrap())
                    .unwrap_or_else(|err| panic!("{name}: {err}"));
            assert_eq!(
                resolve_manager(&root, &manifest).unwrap_or_else(|err| panic!("{name}: {err}")),
                manager,
                "{name}"
            );
            assert_eq!(manifest.services.len(), 1, "{name}");
        }
    }

    #[test]
    fn npm_plan_uses_the_lockfile_command() {
        let root = write_project(&node_manifest("npm"), &[("package-lock.json", "{}\n")]);
        let node = root
            .join("bin")
            .join(if cfg!(windows) { "node.exe" } else { "node" });
        fs::create_dir_all(node.parent().unwrap()).unwrap();
        fs::write(&node, b"node").unwrap();
        let npm_name = if cfg!(windows) { "npm.cmd" } else { "npm" };
        fs::write(node.parent().unwrap().join(npm_name), b"npm").unwrap();
        let manifest =
            parse_manifest(&fs::read_to_string(root.join("stackyard.toml")).unwrap()).unwrap();
        let plan = plan_dependency(
            &root,
            "demo",
            &root,
            &manifest,
            DepRequest {
                action: DepAction::Install,
                package: None,
                scope: ".",
                scope_id: None,
            },
            Toolchain {
                node: Some(&node),
                python: None,
                pnpm: None,
                uv: None,
                venv_python: None,
                php: None,
                composer: None,
            },
        )
        .unwrap();
        assert_eq!(plan.manager, ManagerKind::Npm);
        assert_eq!(plan.commands[0].args.last().unwrap(), "ci");
        let _ = fs::remove_dir_all(root);
    }
}
