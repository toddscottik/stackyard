use std::fs;
use std::path::PathBuf;

use super::*;
use crate::{parse_manifest, EndpointProtocol, SiteKind};

const NEXT: &str = r#"{"packageManager":"pnpm@12.16.1","scripts":{"dev":"next dev"},"dependencies":{"next":"16.4.3"}}"#;
const VITE: &str = r#"{"scripts":{"dev":"vite"},"devDependencies":{"vite":"8.1.0"}}"#;
const EXPRESS: &str = r#"{"packageManager":"npm@11.0.0","scripts":{"start":"node server.js"},"dependencies":{"express":"5.0.0"}}"#;
const FASTAPI: &str = "from fastapi import FastAPI\n\napplication = FastAPI()\n";
const FLASK: &str = "from flask import Flask\n\napp = Flask(__name__)\n";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = crate::disposable_dir("detect");
        fs::create_dir(&root).unwrap();
        Self(crate::plain_path(root.canonicalize().unwrap()))
    }

    fn put(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn detect(&self) -> ImportDetection {
        detect_project(&self.0).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn next_and_fastapi_in_conventional_directories_preserve_managers() {
    let root = Fixture::new();
    root.put("frontend/package.json", NEXT);
    root.put("frontend/pnpm-lock.yaml", "lockfileVersion: '9.0'\n");
    root.put("backend/main.py", FASTAPI);
    root.put("backend/uv.lock", "version = 1\n");
    root.put(
        "backend/pyproject.toml",
        "[project]\nname = 'test-api'\nversion = '0.1.0'\ndependencies = ['fastapi', 'uvicorn']\n",
    );
    let detection = root.detect();
    let draft = parse_manifest(&detection.manifest_text).unwrap();
    assert_eq!(draft.scopes.len(), 2);
    assert_eq!(draft.scopes[0].manager, Some(ManagerKind::Pnpm));
    assert_eq!(draft.scopes[1].manager, Some(ManagerKind::Uv));
    assert_eq!(
        draft.services[0].command,
        [
            "pnpm",
            "run",
            "dev",
            "--hostname",
            "127.0.0.1",
            "--port",
            "{port}"
        ]
    );
    assert_eq!(
        draft.services[1].command,
        [
            "python",
            "-m",
            "uvicorn",
            "main:application",
            "--host",
            "127.0.0.1",
            "--port",
            "{port}"
        ]
    );
    assert!(!root.0.join(MANIFEST).exists());
    assert!(!root.0.join("backend/.venv").exists());
}

#[test]
fn npm_vite_forwards_loopback_and_strict_port_with_separator() {
    let root = Fixture::new();
    root.put("package.json", VITE);
    root.put("package-lock.json", "{}");
    let draft = parse_manifest(&root.detect().manifest_text).unwrap();
    assert_eq!(
        draft.services[0].command,
        [
            "npm",
            "run",
            "dev",
            "--",
            "--host",
            "127.0.0.1",
            "--port",
            "{port}",
            "--strictPort"
        ]
    );
    assert_eq!(draft.services[0].endpoint.as_ref().unwrap().port, 0);
}

#[test]
fn next_workspace_members_inherit_the_verified_root_lockfile() {
    let root = Fixture::new();
    root.put("package.json", r#"{"packageManager":"pnpm@12.16.1"}"#);
    root.put("pnpm-workspace.yaml", "packages:\n  - 'apps/*'\n");
    root.put("pnpm-lock.yaml", "lockfileVersion: '9.0'\n");
    root.put("apps/frontend/package.json", NEXT);
    let draft = parse_manifest(&root.detect().manifest_text).unwrap();
    assert_eq!(draft.scopes[0].directory, Path::new("apps/frontend"));
    assert_eq!(draft.scopes[0].lockfile_root, Path::new("."));
}

#[test]
fn flask_uses_a_detected_application_and_requirements() {
    let root = Fixture::new();
    root.put("app.py", FLASK);
    root.put("requirements.txt", "flask==3.1.3\n");
    let draft = parse_manifest(&root.detect().manifest_text).unwrap();
    assert_eq!(draft.scopes[0].manager, Some(ManagerKind::Pip));
    assert_eq!(
        draft.services[0].command,
        [
            "python",
            "-m",
            "flask",
            "--app",
            "app:app",
            "run",
            "--host",
            "127.0.0.1",
            "--port",
            "{port}"
        ]
    );
}

#[test]
fn express_service_does_not_infer_port_or_inject_environment() {
    let root = Fixture::new();
    root.put("package.json", EXPRESS);
    root.put(
        "server.js",
        "require('express')().listen(4000, '127.0.0.1');",
    );
    let detection = root.detect();
    assert!(has_diagnostic(&detection, "review_fixed_port"));
    let draft = parse_manifest(&detection.manifest_text).unwrap();
    assert_eq!(draft.services[0].command, ["npm", "run", "start"]);
    assert!(draft.services[0].endpoint.is_none());
    assert!(draft.services[0].env.is_empty());
}

#[test]
fn php_prefers_public_front_controller_and_uses_managed_fastcgi() {
    let root = Fixture::new();
    root.put("composer.json", r#"{"name":"test/php-app"}"#);
    root.put("composer.lock", "{}");
    root.put("public/index.php", "<?php echo 'ok';");
    root.put("index.php", "<?php echo 'other';");
    let detection = root.detect();
    assert!(has_diagnostic(&detection, "review_php_root"));
    let draft = parse_manifest(&detection.manifest_text).unwrap();
    assert_eq!(draft.scopes[0].manager, Some(ManagerKind::Composer));
    assert_eq!(
        draft.services[0].command,
        ["php-cgi", "-b", "127.0.0.1:{port}"]
    );
    assert_eq!(
        draft.services[0].endpoint.as_ref().unwrap().protocol,
        EndpointProtocol::FastCgi
    );
    assert!(matches!(draft.site.unwrap().kind, SiteKind::Php { root, .. } if root == "public"));
}

#[test]
fn ambiguous_metadata_requires_manual_review() {
    for (json, files, expected) in [
        ("{", vec![], "malformed_metadata"),
        (
            NEXT,
            vec!["package-lock.json", "pnpm-lock.yaml"],
            "manager_conflict",
        ),
        (NEXT, vec!["yarn.lock"], "unsupported_manager"),
        (VITE, vec![], "unknown_manager"),
        (
            r#"{"packageManager":"npm@11.0.0","scripts":{"dev":"vite --port 7777"},"dependencies":{"vite":"8.1.0"}}"#,
            vec![],
            "unknown_node_entrypoint",
        ),
    ] {
        let root = Fixture::new();
        root.put("package.json", json);
        for file in files {
            root.put(file, "{}");
        }
        let detection = root.detect();
        assert!(has_diagnostic(&detection, expected));
        assert!(has_diagnostic(&detection, "manual_setup"));
        assert!(parse_manifest(&detection.manifest_text).is_err());
    }
}

#[test]
fn unknown_python_and_oversized_metadata_do_not_create_suggestions() {
    let root = Fixture::new();
    root.put("requirements.txt", "fastapi\n");
    root.put(
        "main.py",
        "\"\"\"\nfrom fastapi import FastAPI\napp = FastAPI()\n\"\"\"\n",
    );
    root.put("package.json", &" ".repeat(super::fs::MAX_TEXT_BYTES + 1));
    root.put("node_modules/backend/main.py", FASTAPI);
    let detection = root.detect();
    assert!(has_diagnostic(&detection, "unknown_python_entrypoint"));
    assert!(has_diagnostic(&detection, "inspection_skipped"));
    assert!(parse_manifest(&detection.manifest_text).is_err());
    assert!(detection
        .evidence
        .iter()
        .all(|item| !item.path.contains("node_modules")));
}

#[test]
fn existing_manifest_preempts_suggestions_even_if_legacy_or_invalid() {
    let root = Fixture::new();
    root.put(MANIFEST, "# Preserve exactly as found\nschema = 1");
    root.put("package.json", NEXT);
    let detection = root.detect();
    assert!(detection.has_manifest);
    assert!(detection.manifest_text.is_empty());
    assert_eq!(detection.evidence.len(), 1);
    assert_eq!(detection.evidence[0].path, MANIFEST);
}

#[test]
fn dense_fixture_leaves_room_in_the_ipc_frame() {
    let root = Fixture::new();
    let paths =
        APPLICATION_DIRS
            .iter()
            .map(|name| name.to_string())
            .chain(CONTAINER_DIRS.iter().flat_map(|container| {
                APPLICATION_DIRS
                    .iter()
                    .map(move |name| format!("{container}/{name}"))
            }));
    for path in paths {
        root.put(&format!("{path}/package.json"), VITE);
        root.put(&format!("{path}/package-lock.json"), "{}");
        root.put(&format!("{path}/app.py"), FLASK);
        root.put(&format!("{path}/requirements.txt"), "flask\n");
        root.put(&format!("{path}/composer.json"), "{}");
        root.put(&format!("{path}/public/index.php"), "<?php echo 'ok';");
    }
    let result = root.detect();
    assert_eq!(result.evidence.len(), MAX_EVIDENCE);
    assert_eq!(result.diagnostics.len(), MAX_DIAGNOSTICS);
    assert_eq!(
        parse_manifest(&result.manifest_text).unwrap().scopes.len(),
        MAX_SCOPES
    );
    assert!(serde_json::to_vec(&result).unwrap().len() < crate::protocol::MAX_MESSAGE_BYTES / 2);
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_supply_metadata_or_project_roots() {
    use std::os::unix::fs::symlink;

    let root = Fixture::new();
    let outside = Fixture::new();
    outside.put("package.json", NEXT);
    symlink(outside.0.join("package.json"), root.0.join("package.json")).unwrap();
    symlink(&outside.0, root.0.join("frontend")).unwrap();
    let detection = root.detect();
    assert_eq!(
        detection
            .diagnostics
            .iter()
            .filter(|item| item.code == "inspection_skipped")
            .count(),
        2
    );
    assert!(detection.evidence.is_empty());
    assert!(detect_project(&root.0.join("frontend")).is_err());
    symlink(outside.0.join("package.json"), root.0.join(MANIFEST)).unwrap();
    assert!(detect_project(&root.0).is_err());
}

#[cfg(windows)]
#[test]
fn junctions_cannot_supply_project_roots_or_candidate_metadata() {
    let root = Fixture::new();
    let outside = Fixture::new();
    outside.put("package.json", NEXT);
    let link = root.0.join("frontend");
    let status = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "New-Item -ItemType Junction -Path $env:STACKYARD_DETECT_JUNCTION -Target $env:STACKYARD_DETECT_TARGET -ErrorAction Stop | Out-Null",
        ])
        .env("STACKYARD_DETECT_JUNCTION", &link)
        .env("STACKYARD_DETECT_TARGET", &outside.0)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "create disposable junction: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert_eq!(fs::read_to_string(link.join("package.json")).unwrap(), NEXT);
    let result = root.detect();
    let explicit = detect_project(&link);
    fs::remove_dir(link).unwrap();
    assert!(has_diagnostic(&result, "inspection_skipped"));
    assert!(result.evidence.is_empty());
    assert!(explicit.is_err());
}

fn has_diagnostic(detection: &ImportDetection, expected: &str) -> bool {
    detection
        .diagnostics
        .iter()
        .any(|item| item.code == expected)
}
