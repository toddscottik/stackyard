use super::{strings, Candidate, Draft, DraftEndpoint, DraftScope, ImportDetection, Inspection};
use crate::{ManagerKind, RuntimeRole};

#[derive(Clone, Copy)]
enum Framework {
    Flask,
    FastApi,
}

impl Framework {
    fn names(self) -> (&'static str, &'static str) {
        match self {
            Self::Flask => ("flask", "Flask"),
            Self::FastApi => ("fastapi", "FastAPI"),
        }
    }
}

pub(super) fn inspect(
    detection: &mut ImportDetection,
    draft: &mut Draft,
    candidate: &Candidate<'_>,
) -> Inspection<()> {
    let mut entries = Vec::new();
    let mut found_module = false;
    for module in ["app", "main", "wsgi", "asgi", "server"] {
        found_module |= inspect_module(
            detection,
            candidate,
            &format!("{module}.py"),
            module,
            &mut entries,
        )?;
    }
    if let Some(directory) = candidate.child(detection, "app")? {
        let path = candidate.join("app");
        let package = Candidate {
            directory: &directory,
            path: &path,
        };
        if package.exists(detection, "__init__.py")? {
            found_module |=
                inspect_module(detection, &package, "__init__.py", "app", &mut entries)?;
        }
        found_module |= inspect_module(detection, &package, "main.py", "app.main", &mut entries)?;
    }

    let requirements = candidate.marker(detection, "requirements.txt", "pip requirements file")?;
    let uv = candidate.marker(detection, "uv.lock", "uv lockfile")?;
    let mut unsupported = false;
    for lock in ["poetry.lock", "Pipfile", "Pipfile.lock", "pdm.lock"] {
        unsupported |= candidate.marker(
            detection,
            lock,
            "Metadata for an unsupported Python manager",
        )?;
    }
    let pyproject_path = candidate.join("pyproject.toml");
    let pyproject = candidate
        .text(detection, "pyproject.toml")?
        .map(|text| {
            toml::from_str::<toml::Table>(&text).map_err(|_| {
                detection.warn(
                    "malformed_metadata",
                    &pyproject_path,
                    "Expected valid TOML project metadata.",
                );
            })
        })
        .transpose()?;
    let tool = pyproject
        .as_ref()
        .and_then(|project| project.get("tool"))
        .and_then(toml::Value::as_table);
    if pyproject.is_some() {
        detection.record(&pyproject_path, "Python project metadata", "high");
    }
    unsupported |= tool.is_some_and(|tool| tool.contains_key("poetry") || tool.contains_key("pdm"));
    if unsupported {
        detection.warn(
            "unsupported_manager",
            candidate.path,
            "An unsupported Python manager is present. Review the dependency workflow manually.",
        );
        return Ok(());
    }
    let uv_tool = tool.is_some_and(|tool| tool.contains_key("uv"));
    let manager = if uv || uv_tool {
        if !pyproject.as_ref().is_some_and(|project| {
            project.get("project").is_some_and(toml::Value::is_table)
                || project
                    .get("dependency-groups")
                    .is_some_and(toml::Value::is_table)
        }) {
            detection.warn(
                "missing_metadata",
                &pyproject_path,
                "The uv workflow needs project or dependency-group declarations in pyproject.toml.",
            );
            return Ok(());
        }
        if uv_tool {
            detection.record(&pyproject_path, "uv project configuration", "high");
        }
        Some(ManagerKind::Uv)
    } else if requirements {
        Some(ManagerKind::Pip)
    } else {
        None
    };
    let [(framework, target)] = entries.as_slice() else {
        if found_module || requirements || uv || uv_tool {
            detection.warn("unknown_python_entrypoint", candidate.path, "Expected one conventional Flask or FastAPI application object. Set the module and launch command manually.");
        }
        return Ok(());
    };
    let Some(manager) = manager else {
        detection.warn("unknown_manager", candidate.path, "No requirements.txt or uv project metadata identified an install workflow. Choose how to install the Python dependencies.");
        return Ok(());
    };
    let mut command = match framework {
        Framework::Flask => strings(&["python", "-m", "flask", "--app", target, "run"]),
        Framework::FastApi => strings(&["python", "-m", "uvicorn", target]),
    };
    command.extend(strings(&["--host", "127.0.0.1", "--port", "{port}"]));
    let scope = DraftScope {
        id: "python".into(),
        directory: candidate.path.into(),
        runtime: RuntimeRole::Python,
        manager,
        lockfile_root: candidate.path.into(),
    };
    draft.add(detection, scope, command, DraftEndpoint::Http);
    Ok(())
}

fn inspect_module(
    detection: &mut ImportDetection,
    candidate: &Candidate<'_>,
    filename: &str,
    module: &str,
    entries: &mut Vec<(Framework, String)>,
) -> Inspection<bool> {
    let Some(text) = candidate.text(detection, filename)? else {
        return Ok(false);
    };
    for framework in [Framework::Flask, Framework::FastApi] {
        for symbol in application_objects(&text, framework) {
            let (_, class) = framework.names();
            detection.record(
                candidate.join(filename),
                format!("Conventional {class} application object {symbol}"),
                "medium",
            );
            entries.push((framework, format!("{module}:{symbol}")));
        }
    }
    Ok(true)
}

fn application_objects(text: &str, framework: Framework) -> Vec<&str> {
    let (package, class) = framework.names();
    let direct_import = format!("from {package} import {class}");
    let module_import = format!("import {package}");
    let qualified = format!("{package}.{class}");
    let lines: Vec<_> = top_level_lines(text).collect();
    let direct = lines.contains(&direct_import.as_str());
    let module = lines.contains(&module_import.as_str());
    lines
        .into_iter()
        .filter_map(|line| {
            let (left, right) = line.split_once('=')?;
            let symbol = left
                .split_once(':')
                .map_or(left, |(name, _)| name)
                .trim_end();
            if !python_ident(symbol) {
                return None;
            }
            let (callee, _) = right.trim_start().split_once('(')?;
            let callee = callee.trim_end();
            ((direct && callee == class) || (module && callee == qualified.as_str()))
                .then_some(symbol)
        })
        .collect()
}

fn python_ident(value: &str) -> bool {
    if value.len() > 64 {
        return false;
    }
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn top_level_lines(text: &str) -> impl Iterator<Item = &str> {
    let mut multiline = None;
    text.lines().filter_map(move |line| {
        let outside_string = multiline.is_none();
        let mut tail = line;
        // Ignore common multiline docstrings and string literals. Unusual
        // aliases, dynamic imports and application factories need manual setup.
        while !tail.is_empty() {
            if let Some(delimiter) = multiline {
                let Some((_, rest)) = tail.split_once(delimiter) else {
                    break;
                };
                multiline = None;
                tail = rest;
            } else {
                let code = tail.split('#').next().unwrap_or("");
                let Some((index, delimiter)) = ["\"\"\"", "'''"]
                    .into_iter()
                    .filter_map(|marker| code.find(marker).map(|index| (index, marker)))
                    .min_by_key(|(index, _)| *index)
                else {
                    break;
                };
                multiline = Some(delimiter);
                tail = &tail[index + delimiter.len()..];
            }
        }
        if outside_string && line.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            return line.split('#').next().map(str::trim_end);
        }
        None
    })
}
