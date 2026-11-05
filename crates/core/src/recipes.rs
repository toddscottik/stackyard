//! Versioned, source-based project starters. These fixtures are intentionally
//! embedded so creation never depends on a latest-version generator.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeInfo {
    pub id: String,
    pub version: u32,
    pub title: String,
    pub runtime: String,
    pub managers: Vec<String>,
    pub network: String,
    pub hot_reload: String,
    pub windows: String,
    pub macos: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedRecipe {
    pub files: Vec<(String, Vec<u8>)>,
    pub install_command: Vec<String>,
    pub launch_command: Vec<String>,
}

struct Definition {
    info: RecipeInfo,
    files: &'static [(&'static str, &'static [u8])],
}

const NODE_NPM: &[(&str, &[u8])] = &[
    (
        "package.json",
        include_bytes!("../../../fixtures/starters/node-http/npm/package.json"),
    ),
    (
        "package-lock.json",
        include_bytes!("../../../fixtures/starters/node-http/npm/package-lock.json"),
    ),
    (
        "server.js",
        include_bytes!("../../../fixtures/starters/node-http/server.js"),
    ),
    (
        "stackyard.toml",
        include_bytes!("../../../fixtures/starters/node-http/npm/stackyard.toml"),
    ),
];
const NODE_PNPM: &[(&str, &[u8])] = &[
    (
        "package.json",
        include_bytes!("../../../fixtures/starters/node-http/pnpm/package.json"),
    ),
    (
        "pnpm-lock.yaml",
        include_bytes!("../../../fixtures/starters/node-http/pnpm/pnpm-lock.yaml"),
    ),
    (
        "server.js",
        include_bytes!("../../../fixtures/starters/node-http/server.js"),
    ),
    (
        "stackyard.toml",
        include_bytes!("../../../fixtures/starters/node-http/pnpm/stackyard.toml"),
    ),
];
const NEXT: &[(&str, &[u8])] = &[
    (
        "package.json",
        include_bytes!("../../../fixtures/starters/next-app/package.json"),
    ),
    (
        "pnpm-lock.yaml",
        include_bytes!("../../../fixtures/starters/next-app/pnpm-lock.yaml"),
    ),
    (
        "app/layout.js",
        include_bytes!("../../../fixtures/starters/next-app/app/layout.js"),
    ),
    (
        "app/page.js",
        include_bytes!("../../../fixtures/starters/next-app/app/page.js"),
    ),
    (
        "stackyard.toml",
        include_bytes!("../../../fixtures/starters/next-app/stackyard.toml"),
    ),
];
const FLASK: &[(&str, &[u8])] = &[
    (
        "app.py",
        include_bytes!("../../../fixtures/starters/flask-app/app.py"),
    ),
    (
        "requirements.txt",
        include_bytes!("../../../fixtures/starters/flask-app/requirements.txt"),
    ),
    (
        "stackyard.toml",
        include_bytes!("../../../fixtures/starters/flask-app/stackyard.toml"),
    ),
];
const FASTAPI: &[(&str, &[u8])] = &[
    (
        "app.py",
        include_bytes!("../../../fixtures/starters/fastapi-app/app.py"),
    ),
    (
        "dev.py",
        include_bytes!("../../../fixtures/starters/fastapi-app/dev.py"),
    ),
    (
        "requirements.txt",
        include_bytes!("../../../fixtures/starters/fastapi-app/requirements.txt"),
    ),
    (
        "stackyard.toml",
        include_bytes!("../../../fixtures/starters/fastapi-app/stackyard.toml"),
    ),
];

fn definition(id: &str, manager: &str) -> Option<Definition> {
    let (runtime, managers, network, hot_reload, files) = match (id, manager) {
        ("node-http", "npm") => (
            "node",
            vec!["npm"],
            "This starter has no registry dependencies. A managed runtime may need a download.",
            "Node --watch restarts on source changes",
            NODE_NPM,
        ),
        ("node-http", "pnpm") => (
            "node",
            vec!["pnpm"],
            "This starter has no registry dependencies. Managed components may need downloads.",
            "Node --watch restarts on source changes",
            NODE_PNPM,
        ),
        ("next-app", "pnpm") => (
            "node",
            vec!["pnpm"],
            "Install pinned Next.js packages from the registry or a populated pnpm store.",
            "Next dev provides Fast Refresh",
            NEXT,
        ),
        ("flask-app", "pip") => (
            "python",
            vec!["pip"],
            "Install pinned direct requirements from the Python package index or a populated wheel cache.",
            "Flask debug reloader restarts on source changes",
            FLASK,
        ),
        ("fastapi-app", "pip") => (
            "python",
            vec!["pip"],
            "Install pinned direct requirements from the Python package index or a populated wheel cache.",
            "The starter restarts Uvicorn when app.py changes",
            FASTAPI,
        ),
        _ => return None,
    };
    Some(Definition {
        info: RecipeInfo {
            id: id.into(),
            version: 1,
            title: match id {
                "node-http" => "Node HTTP",
                "next-app" => "Next.js",
                "flask-app" => "Flask",
                _ => "FastAPI",
            }
            .into(),
            runtime: runtime.into(),
            managers: managers.into_iter().map(str::to_owned).collect(),
            network: network.into(),
            hot_reload: hot_reload.into(),
            windows:
                "Windows 10 x64 runtime pins verified. See Stage 11 acceptance for starter results."
                    .into(),
            macos: "Recipe source is portable; native macOS 14.5 arm64 acceptance pending".into(),
        },
        files,
    })
}

pub fn recipes() -> Vec<RecipeInfo> {
    let mut items = [
        ("node-http", "npm"),
        ("next-app", "pnpm"),
        ("flask-app", "pip"),
        ("fastapi-app", "pip"),
    ]
    .into_iter()
    .filter_map(|(id, manager)| definition(id, manager).map(|entry| entry.info))
    .collect::<Vec<_>>();
    items[0].managers = vec!["npm".into(), "pnpm".into()];
    items
}

/// Render a starter using a lowercase slug for its project name.
pub fn render_recipe(id: &str, manager: &str, name: &str) -> Result<RenderedRecipe> {
    if name.is_empty()
        || name.len() > 48
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || !name.as_bytes()[0].is_ascii_lowercase()
    {
        return Err(Error::InvalidMessage(
            "project name must be a lowercase slug beginning with a letter".into(),
        ));
    }
    let entry = definition(id, manager).ok_or_else(|| {
        Error::InvalidMessage(format!("unsupported recipe/manager: {id}/{manager}"))
    })?;
    let files: Vec<_> = entry
        .files
        .iter()
        .map(|(path, bytes)| {
            let content = String::from_utf8_lossy(bytes).replace("__STACKYARD_NAME__", name);
            ((*path).to_string(), content.into_bytes())
        })
        .collect();
    let install_command = match manager {
        "npm" => vec!["npm".into(), "ci".into()],
        "pnpm" => vec!["pnpm".into(), "install".into(), "--frozen-lockfile".into()],
        "pip" => vec![
            "python".into(),
            "-m".into(),
            "pip".into(),
            "install".into(),
            "-r".into(),
            "requirements.txt".into(),
        ],
        _ => unreachable!("definition validates manager"),
    };
    let manifest_bytes = &files
        .iter()
        .find(|(path, _)| path == "stackyard.toml")
        .ok_or_else(|| Error::InvalidMessage("Recipe has no launch manifest".into()))?
        .1;
    let manifest = std::str::from_utf8(manifest_bytes)
        .map_err(|error| Error::InvalidMessage(format!("Recipe manifest is not UTF-8: {error}")))?;
    let launch_command = crate::parse_manifest(manifest)?
        .service("web")
        .ok_or_else(|| Error::InvalidMessage("Recipe has no web service".into()))?
        .command
        .clone();
    Ok(RenderedRecipe {
        files,
        install_command,
        launch_command,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_generated_manifest_parses_and_names_are_safe() {
        assert_eq!(recipes().len(), 4);
        assert_eq!(recipes()[0].managers, ["npm", "pnpm"]);
        for (id, manager) in [
            ("node-http", "npm"),
            ("node-http", "pnpm"),
            ("next-app", "pnpm"),
            ("flask-app", "pip"),
            ("fastapi-app", "pip"),
        ] {
            let rendered = render_recipe(id, manager, "hello-app").unwrap();
            let manifest = rendered
                .files
                .iter()
                .find(|(path, _)| path == "stackyard.toml")
                .unwrap();
            let text = std::str::from_utf8(&manifest.1).unwrap();
            let parsed = crate::manifest::parse_manifest(text).unwrap();
            assert_eq!(parsed.schema, 2);
            assert_eq!(parsed.scopes[0].id, "app");
            assert_eq!(parsed.services[0].id, "web");
            assert!(parsed.services[0].is_http_endpoint());
            assert!(text.contains("port = \"auto\""));
            assert!(!text.contains("__STACKYARD_NAME__"));
        }
        assert!(render_recipe("node-http", "npm", "Bad_Name").is_err());
        assert!(render_recipe("node-http", "npm", "../escape").is_err());
    }
}
