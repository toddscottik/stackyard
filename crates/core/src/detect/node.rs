use serde_json::Value;

use super::{strings, Candidate, Draft, DraftEndpoint, DraftScope, ImportDetection, Inspection};
use crate::{ManagerKind, RuntimeRole};

pub(super) enum Workspace {
    None,
    Manual,
    Explicit {
        manager: ManagerKind,
        patterns: Vec<String>,
    },
}

pub(super) fn inspect_root(
    detection: &mut ImportDetection,
    draft: &mut Draft,
    root: &Candidate<'_>,
) -> Workspace {
    inspect(detection, draft, root, None).unwrap_or(Workspace::Manual)
}

pub(super) fn inspect_child(
    detection: &mut ImportDetection,
    draft: &mut Draft,
    candidate: &Candidate<'_>,
    workspace: &Workspace,
) {
    let _ = inspect(detection, draft, candidate, Some(workspace));
}

fn inspect(
    detection: &mut ImportDetection,
    draft: &mut Draft,
    candidate: &Candidate<'_>,
    parent: Option<&Workspace>,
) -> Inspection<Workspace> {
    let package = candidate.json(detection, "package.json")?;
    if package.is_some() {
        detection.record(
            candidate.join("package.json"),
            "Node package metadata",
            "high",
        );
    }
    let pnpm_workspace = candidate.text(detection, "pnpm-workspace.yaml")?;
    if pnpm_workspace.is_some() {
        detection.record(
            candidate.join("pnpm-workspace.yaml"),
            "pnpm workspace metadata",
            "high",
        );
    }
    let has_package = package.is_some();
    let package = package.unwrap_or(Value::Null);
    let local_manager = manager(detection, candidate, &package, pnpm_workspace.is_some())?;
    if !has_package && pnpm_workspace.is_none() {
        if local_manager.is_some() {
            detection.warn(
                "missing_metadata",
                &candidate.join("package.json"),
                "A Node lockfile was found without package.json. Review its workspace ownership.",
            );
            return Ok(Workspace::Manual);
        }
        return Ok(Workspace::None);
    }
    let has_workspace = package.get("workspaces").is_some() || pnpm_workspace.is_some();
    let (manager, owner) = match parent {
        Some(Workspace::Explicit { manager, patterns }) => {
            if !patterns
                .iter()
                .any(|pattern| matches_workspace(pattern, candidate.path))
            {
                detection.warn(
                    "workspace_membership",
                    candidate.path,
                    "This directory does not match a supported workspace member pattern. Review its package-manager ownership.",
                );
                return Ok(Workspace::None);
            }
            if local_manager.is_some_and(|local| local != *manager)
                || has_workspace
                || has_node_lock(candidate, detection)?
            {
                detection.warn("manager_conflict", candidate.path, "This workspace member declares a separate manager, workspace, or lockfile. Review the lockfile owner.");
                return Ok(Workspace::None);
            }
            detection.record(
                candidate.path,
                "Conventional package matches the root workspace",
                "medium",
            );
            (Some(*manager), ".")
        }
        Some(Workspace::Manual) => {
            detection.warn(
                "workspace_membership",
                candidate.path,
                "Root package metadata needs review before choosing a manager for this package.",
            );
            return Ok(Workspace::None);
        }
        _ => (local_manager, candidate.path),
    };

    let workspace = if has_workspace {
        match (
            manager,
            workspace_patterns(&package, pnpm_workspace.as_deref()),
        ) {
            (Some(manager), Some(patterns)) => Workspace::Explicit { manager, patterns },
            _ => {
                detection.warn("workspace_membership", candidate.path, "Workspace membership could not be established. Review the package-manager configuration manually.");
                Workspace::Manual
            }
        }
    } else {
        Workspace::None
    };
    let Some(manager) = manager else {
        detection.warn(
            "unknown_manager",
            &candidate.join("package.json"),
            "No npm or pnpm lockfile or packageManager declaration identified the manager. Choose it explicitly.",
        );
        return Ok(workspace);
    };
    if has_package {
        suggest(detection, draft, candidate, &package, manager, owner)?;
    }
    Ok(workspace)
}

fn manager(
    detection: &mut ImportDetection,
    candidate: &Candidate<'_>,
    package: &Value,
    pnpm_workspace: bool,
) -> Inspection<Option<ManagerKind>> {
    let path = candidate.join("package.json");
    let mut managers = Vec::new();
    let mut unsupported = false;
    if let Some(value) = package.get("packageManager") {
        let Some((name, version)) = value.as_str().and_then(|value| value.split_once('@')) else {
            detection.warn(
                "malformed_metadata",
                &path,
                "packageManager must be a name@version string.",
            );
            return Err(());
        };
        if name.is_empty() || version.trim().is_empty() {
            detection.warn(
                "malformed_metadata",
                &path,
                "packageManager must be a name@version string.",
            );
            return Err(());
        }
        match ManagerKind::parse(name).filter(|manager| manager.is_node()) {
            Some(manager) => {
                detection.record(
                    &path,
                    format!("packageManager declares {}", manager.as_str()),
                    "high",
                );
                managers.push(manager);
            }
            None => {
                detection.record(
                    &path,
                    "packageManager declares an unsupported Node manager",
                    "high",
                );
                unsupported = true;
            }
        }
    }
    for (lock, manager) in [
        ("package-lock.json", ManagerKind::Npm),
        ("npm-shrinkwrap.json", ManagerKind::Npm),
        ("pnpm-lock.yaml", ManagerKind::Pnpm),
    ] {
        if candidate.marker(detection, lock, &format!("{} lockfile", manager.as_str()))? {
            managers.push(manager);
        }
    }
    for lock in ["yarn.lock", "bun.lock", "bun.lockb"] {
        unsupported |=
            candidate.marker(detection, lock, "Lockfile from an unsupported Node manager")?;
    }
    if pnpm_workspace {
        managers.push(ManagerKind::Pnpm);
    }
    if unsupported {
        detection.warn("unsupported_manager", &path, "Existing metadata refers to an unsupported manager. Review it and configure this application manually.");
        return Err(());
    }
    if managers
        .iter()
        .any(|manager| Some(manager) != managers.first())
    {
        detection.warn("manager_conflict", &path, "npm and pnpm signals conflict. Resolve the intended manager and lockfile ownership explicitly.");
        return Err(());
    }
    Ok(managers.first().copied())
}

fn has_node_lock(candidate: &Candidate<'_>, detection: &mut ImportDetection) -> Inspection<bool> {
    for lock in ["package-lock.json", "npm-shrinkwrap.json", "pnpm-lock.yaml"] {
        if candidate.exists(detection, lock)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn workspace_patterns(package: &Value, pnpm: Option<&str>) -> Option<Vec<String>> {
    if let Some(pnpm) = pnpm {
        // Handle the common block list without pretending to parse arbitrary YAML.
        let mut lines = pnpm
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'));
        if lines.next()?.trim_end() != "packages:" {
            return None;
        }
        return lines
            .map(|line| {
                let pattern = line.trim().strip_prefix("- ")?;
                let pattern = pattern.trim().trim_matches(['\'', '"']);
                supported_pattern(pattern).then(|| pattern.to_owned())
            })
            .collect();
    }
    let workspaces = package.get("workspaces")?;
    workspaces
        .as_array()?
        .iter()
        .map(|entry| {
            let pattern = entry.as_str()?;
            supported_pattern(pattern).then(|| pattern.to_owned())
        })
        .collect()
}

fn supported_pattern(pattern: &str) -> bool {
    let base = pattern.strip_suffix("/*").unwrap_or(pattern);
    !base.is_empty()
        && base.split('/').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        })
}

fn matches_workspace(pattern: &str, directory: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return directory
            .rsplit_once('/')
            .is_some_and(|(parent, _)| parent == prefix);
    }
    pattern == directory
}

fn suggest(
    detection: &mut ImportDetection,
    draft: &mut Draft,
    candidate: &Candidate<'_>,
    package: &Value,
    manager: ManagerKind,
    owner: &str,
) -> Inspection<()> {
    let path = candidate.join("package.json");
    let scripts = object_field(package, "scripts", detection, &path)?;
    let dependencies = object_field(package, "dependencies", detection, &path)?;
    let dev_dependencies = object_field(package, "devDependencies", detection, &path)?;
    let script = |name| {
        scripts
            .and_then(|scripts| scripts.get(name))
            .and_then(Value::as_str)
    };
    let depends = |name| {
        [dependencies, dev_dependencies]
            .iter()
            .flatten()
            .any(|deps| deps.contains_key(name))
    };
    let launch = if depends("next")
        && matches!(
            script("dev").map(str::trim),
            Some("next dev" | "next dev --turbo" | "next dev --turbopack" | "next dev --webpack")
        ) {
        Some(("dev", "Next.js dev script", Some("--hostname")))
    } else if depends("vite")
        && matches!(
            script("dev").map(str::trim),
            Some("vite" | "vite dev" | "vite serve")
        )
    {
        Some(("dev", "Vite dev script", Some("--host")))
    } else if depends("express") {
        ["dev", "start"]
            .into_iter()
            .find(|name| script(*name).is_some_and(simple_node_script))
            .map(|name| (name, "Express application script", None))
    } else {
        None
    };
    let Some((name, detail, host_flag)) = launch else {
        // A workspace root with dependencies or scripts may only coordinate
        // child packages. Keep real unknown application scripts visible.
        if script("dev").is_some()
            || script("start").is_some()
            || depends("next")
            || depends("vite")
            || depends("express")
        {
            detection.warn("unknown_node_entrypoint", &path, "No supported development script was identified. Review the existing scripts and launch arguments manually.");
        }
        return Ok(());
    };
    detection.record(&path, detail, "high");
    let mut command = strings(&[manager.as_str(), "run", name]);
    let endpoint = if let Some(host_flag) = host_flag {
        if manager == ManagerKind::Npm {
            command.push("--".into());
        }
        command.extend(strings(&[host_flag, "127.0.0.1", "--port", "{port}"]));
        if depends("vite") && host_flag == "--host" {
            command.push("--strictPort".into());
        }
        DraftEndpoint::Http
    } else {
        detection.warn(
            "review_fixed_port",
            &path,
            "Express controls its own host and port. Confirm loopback binding and configure a fixed endpoint if needed. This draft only checks that the process stays running.",
        );
        DraftEndpoint::Manual
    };
    let scope = DraftScope {
        id: "node".into(),
        directory: candidate.path.into(),
        runtime: RuntimeRole::Node,
        manager,
        lockfile_root: owner.into(),
    };
    draft.add(detection, scope, command, endpoint);
    Ok(())
}

fn object_field<'a>(
    package: &'a Value,
    field: &str,
    detection: &mut ImportDetection,
    path: &str,
) -> Inspection<Option<&'a serde_json::Map<String, Value>>> {
    package
        .get(field)
        .map(|value| {
            value
                .as_object()
                .filter(|entries| entries.values().all(Value::is_string))
                .ok_or_else(|| {
                    detection.warn(
                        "malformed_metadata",
                        path,
                        format!("{field} must map names to strings."),
                    );
                })
        })
        .transpose()
}

fn simple_node_script(script: &str) -> bool {
    let mut words = script.split_ascii_whitespace();
    if !matches!(words.next(), Some("node" | "nodemon" | "tsx")) {
        return false;
    }
    let Some(path) = words.next() else {
        return false;
    };
    let path = path.strip_prefix("./").unwrap_or(path);
    words.next().is_none()
        && [".js", ".mjs", ".cjs", ".ts"]
            .iter()
            .any(|suffix| path.ends_with(suffix))
        && path.split('/').all(|segment| {
            !segment.is_empty()
                && !segment.starts_with(['-', '.'])
                && segment
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_'))
        })
}
