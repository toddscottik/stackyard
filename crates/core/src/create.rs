use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::catalog::{current_target, find_package, require_host, CatalogPackage};
use crate::error::{Error, Result};
use crate::paths::plain_path;
use crate::recipes::{recipes, render_recipe};
use crate::store::Store;

const RECEIPT: &str = ".stackyard-create";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentChoice {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRequest {
    pub parent: String,
    pub name: String,
    pub recipe: String,
    pub manager: String,
    pub runtime: ComponentChoice,
    pub tool: Option<ComponentChoice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePlan {
    pub request: CreateRequest,
    pub target: String,
    pub fingerprint: String,
    pub files: Vec<String>,
    pub downloads: Vec<CatalogPackage>,
    pub install_command: Vec<String>,
    pub launch_command: Vec<String>,
    pub notes: Vec<String>,
}

struct Snapshot {
    plan: CreatePlan,
    parent: PathBuf,
    anchor: File,
    files: Vec<(String, Vec<u8>)>,
    generated: bool,
}

struct StagedEntry {
    path: PathBuf,
    identity: String,
    directory: bool,
}

pub fn plan_create(data: &Path, request: &CreateRequest) -> Result<CreatePlan> {
    Ok(snapshot(data, request)?.plan)
}

pub fn verify_create_plan(data: &Path, plan: &CreatePlan) -> Result<CreatePlan> {
    Ok(verify_snapshot(data, plan)?.plan)
}

pub fn generate_create(data: &Path, plan: &CreatePlan, cancel: &AtomicBool) -> Result<PathBuf> {
    let reviewed = verify_snapshot(data, plan)?;
    let target = PathBuf::from(&plan.target);
    check_cancel(cancel, &target)?;
    if reviewed.generated {
        return Ok(target);
    }

    // A sibling directory puts both paths on the same filesystem for publication.
    let stage = make_stage(&reviewed.parent)?;
    let stage_anchor = open_directory(&stage)?;
    let stage_identity = file_identity(&stage_anchor)?;
    let mut staged = Vec::new();
    let outcome = (|| {
        let writing = write_source(&stage, &stage_identity, &reviewed, cancel, &mut staged);
        drop(stage_anchor);
        writing?;
        let current = verify_snapshot(data, plan)?;
        check_cancel(cancel, &target)?;
        if current.generated {
            return Ok(false);
        }
        if directory_identity(&stage)? != stage_identity
            || !matches_receipt(&stage, plan, &reviewed.files, false)?
        {
            return Err(failed(format!(
                "staging directory changed at {}",
                stage.display()
            )));
        }
        check_cancel(cancel, &target)?;
        let destination = reviewed.parent.join(&plan.request.name);
        match publish(&reviewed.anchor, &stage, &destination) {
            Ok(()) => Ok(true),
            Err(source) => {
                // A concurrent retry can win publication of the same reviewed files.
                if verify_snapshot(data, plan).is_ok_and(|state| state.generated) {
                    return Ok(false);
                }
                Err(path_error(
                    "publish project without replacing its destination",
                    &target,
                    source,
                ))
            }
        }
    })();

    match outcome {
        Ok(true) => Ok(target),
        other => {
            // Remove only the private directory we created, beneath the reviewed parent.
            let cleanup = cleanup_stage(&reviewed, &stage, &stage_identity, &staged);
            match (other, cleanup) {
                (Ok(_), Ok(())) => Ok(target),
                (Err(error), Ok(())) => Err(error),
                (result, Err(error)) => Err(failed(format!(
                    "{}; staging files may remain at {}: {error}",
                    result.err().map_or_else(
                        || format!("project generated at {}", target.display()),
                        |err| err.to_string(),
                    ),
                    plain_path(stage).display(),
                ))),
            }
        }
    }
}

fn verify_snapshot(data: &Path, reviewed: &CreatePlan) -> Result<Snapshot> {
    let current = snapshot(data, &reviewed.request)?;
    if &current.plan != reviewed {
        return Err(failed(format!(
            "creation plan for {} changed; review a new plan before applying",
            current.plan.target
        )));
    }
    Ok(current)
}

fn snapshot(data: &Path, request: &CreateRequest) -> Result<Snapshot> {
    validate_name(&request.name)?;
    let recipe = recipes()
        .into_iter()
        .find(|recipe| recipe.id == request.recipe && recipe.managers.contains(&request.manager))
        .ok_or_else(|| {
            failed(format!(
                "unknown project recipe/manager: {}/{}",
                request.recipe, request.manager
            ))
        })?;
    let rendered = render_recipe(&request.recipe, &request.manager, &request.name)?;

    let parent = canonical_parent(Path::new(&request.parent))?;
    let anchor = open_directory(&parent)?;
    let parent_identity = file_identity(&anchor)?;
    let target = parent.join(&request.name);
    let target_text = path_text(plain_path(target.clone()))?;
    let store = Store::open_read_only(data)?;
    if let Some(store) = &store {
        store.validate_project_root_id(&request.name, &target_text)?;
    }
    let registered = check_destination(data, store.as_ref(), &parent, &target, &request.name)?;

    let mut inputs = Sha256::new();
    digest_part(&mut inputs, b"stackyard-create-v1");
    digest_part(&mut inputs, parent_identity.as_bytes());
    digest_value(&mut inputs, &recipe)?;
    let mut downloads = Vec::new();
    let npm = request.manager == "npm";
    resolve_component(
        store.as_ref(),
        &request.runtime,
        &recipe.runtime,
        npm,
        &mut inputs,
        &mut downloads,
    )?;
    match (request.manager.as_str(), request.tool.as_ref()) {
        ("pnpm", Some(tool)) => {
            resolve_component(
                store.as_ref(),
                tool,
                "pnpm",
                false,
                &mut inputs,
                &mut downloads,
            )?;
        }
        ("pnpm", None) => return Err(failed("select an installed or catalog pnpm 12.16.1")),
        (_, Some(_)) => {
            return Err(failed(
                "this manager uses the selected runtime's built-in tools",
            ))
        }
        (_, None) => {}
    }

    let mut files = Vec::with_capacity(rendered.files.len() + 1);
    for (relative, bytes) in &rendered.files {
        validate_recipe_path(relative)?;
        if files.contains(relative) || relative == RECEIPT {
            return Err(failed(format!("duplicate recipe path {relative:?}")));
        }
        files.push(relative.clone());
        digest_part(&mut inputs, relative.as_bytes());
        digest_part(&mut inputs, bytes);
    }
    files.push(RECEIPT.to_string());
    let mut notes = vec![
        format!("{} recipe, version {}.", recipe.title, recipe.version),
        recipe.network.clone(),
        recipe.hot_reload.clone(),
    ];
    if !downloads.is_empty() {
        notes.push(
            "Stackyard reuses matching verified catalog components already installed.".into(),
        );
    }
    notes.push("Generate and register the source before installing project dependencies and starting the application.".into());

    let mut plan = CreatePlan {
        request: request.clone(),
        target: target_text,
        fingerprint: String::new(),
        files,
        downloads,
        install_command: rendered.install_command,
        launch_command: rendered.launch_command,
        notes,
    };
    digest_value(&mut inputs, &plan)?;
    plan.fingerprint = hex::encode(inputs.finalize());
    let generated = match optional_metadata(&target)? {
        Some(meta) if real_directory(&meta) => {
            if !matches_receipt(&target, &plan, &rendered.files, registered)? {
                return Err(occupied(&target));
            }
            true
        }
        Some(_) => return Err(occupied(&target)),
        None if registered => {
            return Err(failed(format!(
                "registered project directory is missing at {}",
                target.display()
            )));
        }
        None => false,
    };
    Ok(Snapshot {
        plan,
        parent,
        anchor,
        files: rendered.files,
        generated,
    })
}

fn validate_name(name: &str) -> Result<()> {
    let slug = (1..=48).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    let numbered_device = name.len() == 4
        && (name.starts_with("com") || name.starts_with("lpt"))
        && name.as_bytes()[3].is_ascii_digit();
    if !slug
        || matches!(name, "con" | "prn" | "aux" | "nul" | crate::DEFAULT_PROJECT)
        || numbered_device
    {
        return Err(failed(
            "Use 1–48 lowercase letters, digits or hyphens and start with a letter. Windows device names and default are reserved.",
        ));
    }
    Ok(())
}

fn validate_recipe_path(relative: &str) -> Result<()> {
    if relative
        .split('/')
        .any(|part| part.is_empty() || matches!(part, "." | "..") || part.contains(['\\', ':']))
    {
        return Err(failed(format!("invalid recipe path {relative:?}")));
    }
    Ok(())
}

pub(crate) fn canonical_parent(parent: &Path) -> Result<PathBuf> {
    if !parent.is_absolute() || parent.components().any(|c| c == Component::ParentDir) {
        return Err(failed(
            "select an existing absolute project parent directory",
        ));
    }
    if !real_directory(
        &fs::symlink_metadata(parent)
            .map_err(|source| path_error("read project parent", parent, source))?,
    ) {
        return Err(failed(format!(
            "project parent is not an ordinary directory: {}",
            parent.display()
        )));
    }
    parent
        .canonicalize()
        .map_err(|source| path_error("resolve project parent", parent, source))
}

fn check_destination(
    data: &Path,
    store: Option<&Store>,
    parent: &Path,
    target: &Path,
    name: &str,
) -> Result<bool> {
    let target = plain_path(target.to_path_buf());
    if data.exists() {
        let data = plain_path(
            data.canonicalize()
                .map_err(|source| path_error("resolve engine data directory", data, source))?,
        );
        if target.starts_with(&data) || data.starts_with(&target) {
            return Err(failed(format!(
                "project directory overlaps Stackyard data: {}",
                target.display()
            )));
        }
    }

    // Prevent a package manager from claiming this starter as part of an ancestor project.
    for dir in parent.ancestors().filter(|dir| dir.has_root()) {
        for marker in [
            "stackyard.toml",
            "package.json",
            "pnpm-workspace.yaml",
            "pnpm-lock.yaml",
            "package-lock.json",
            "npm-shrinkwrap.json",
            "yarn.lock",
            "pyproject.toml",
            "setup.cfg",
            "setup.py",
            "requirements.txt",
            "uv.lock",
            "composer.json",
        ] {
            let marker_path = dir.join(marker);
            if optional_metadata(&marker_path)?.is_some() {
                return Err(failed(format!(
                    "project directory is inside an existing workspace at {}",
                    plain_path(marker_path).display()
                )));
            }
        }
    }

    let mut registered = false;
    if let Some(store) = store {
        for project in store.list_projects()? {
            let stored = PathBuf::from(&project.root);
            let root = match stored.canonicalize() {
                Ok(canonical) => plain_path(canonical),
                Err(error) if error.kind() == io::ErrorKind::NotFound => stored,
                Err(error) => return Err(path_error("resolve imported project", &stored, error)),
            };
            if project.id == name {
                if root != target {
                    return Err(failed(format!(
                        "project id {name} is already imported from {}",
                        root.display()
                    )));
                }
                registered = true;
            } else if target.starts_with(&root) || root.starts_with(&target) {
                return Err(failed(format!(
                    "project directory overlaps imported project {} at {}",
                    project.id,
                    root.display()
                )));
            }
        }
    }
    Ok(registered)
}

fn resolve_component(
    store: Option<&Store>,
    choice: &ComponentChoice,
    role: &str,
    npm: bool,
    inputs: &mut Sha256,
    downloads: &mut Vec<CatalogPackage>,
) -> Result<()> {
    let kind = if role == "pnpm" { "tool" } else { "runtime" };
    match choice.kind.as_str() {
        "catalog" => {
            let package = find_package(&choice.id)?;
            require_host(&package)?;
            if package.kind != kind
                || package.name != role
                || !compatible_version(role, &package.version)
            {
                return Err(incompatible(role, &choice.id));
            }
            if package.sha256.len() != 64
                || !package.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                || !package.url.starts_with("https://")
                || !package.digest_source.starts_with("https://")
            {
                return Err(failed(format!(
                    "catalog component {} lacks a verified SHA-256 pin",
                    package.id
                )));
            }
            downloads.push(package);
        }
        "installed" => {
            let record = store
                .map(|store| store.get_package(&choice.id))
                .transpose()?
                .flatten()
                .ok_or_else(|| {
                    failed(format!("installed component {} was not found", choice.id))
                })?;
            if record.kind != kind
                || record.name != role
                || !compatible_version(role, &record.version)
            {
                return Err(incompatible(role, &choice.id));
            }
            if record.target != current_target() {
                return Err(Error::IncompatibleTarget {
                    id: record.id,
                    package: record.target,
                    host: current_target().to_string(),
                });
            }
            let launch = Path::new(&record.launch);
            digest_value(inputs, &record)?;
            digest_executable(inputs, launch)?;
            if npm {
                let (program, args) = crate::project::npm_launch(launch)?;
                if let Some(cli) = args.first() {
                    digest_executable(inputs, Path::new(cli))?;
                } else {
                    digest_executable(inputs, &program)?;
                }
            }
        }
        _ => {
            return Err(failed(format!(
                "unknown component choice kind {:?}",
                choice.kind
            )))
        }
    }
    Ok(())
}

fn compatible_version(role: &str, version: &str) -> bool {
    let version = version.strip_prefix('v').unwrap_or(version);
    if role == "pnpm" {
        return version == "12.16.1";
    }
    let release = version
        .split_once('+')
        .map_or(version, |(release, _)| release);
    let mut parts = release.split('.');
    let major = parts.next().and_then(|part| part.parse::<u32>().ok());
    let minor = parts.next().and_then(|part| part.parse::<u32>().ok());
    let patch = parts.next().and_then(|part| part.parse::<u32>().ok());
    patch.is_some()
        && parts.next().is_none()
        && match role {
            "node" => major == Some(24) && minor.is_some(),
            "python" => major == Some(3) && minor == Some(12),
            _ => false,
        }
}

fn incompatible(role: &str, id: &str) -> Error {
    let requirement = match role {
        "node" => "Node 24",
        "python" => "Python 3.12",
        "pnpm" => "pnpm 12.16.1",
        _ => role,
    };
    failed(format!(
        "component {id} is incompatible; this recipe requires {requirement}"
    ))
}

fn digest_executable(inputs: &mut Sha256, path: &Path) -> Result<()> {
    let resolved = path
        .canonicalize()
        .map_err(|source| path_error("resolve installed component", path, source))?;
    let metadata = fs::metadata(&resolved)
        .map_err(|source| path_error("read installed component", path, source))?;
    if !metadata.is_file() {
        return Err(failed(format!(
            "installed component is not a file: {}",
            path.display()
        )));
    }
    let file = File::open(&resolved)
        .map_err(|source| path_error("open installed component", path, source))?;
    digest_part(inputs, path_text(plain_path(resolved))?.as_bytes());
    digest_part(inputs, file_identity(&file)?.as_bytes());
    digest_part(
        inputs,
        format!("{:?} {}", metadata.modified(), metadata.len()).as_bytes(),
    );
    Ok(())
}

fn digest_value(inputs: &mut Sha256, value: &impl Serialize) -> Result<()> {
    let encoded = serde_json::to_vec(value)
        .map_err(|error| failed(format!("encode creation plan inputs: {error}")))?;
    digest_part(inputs, &encoded);
    Ok(())
}

fn digest_part(inputs: &mut Sha256, bytes: &[u8]) {
    inputs.update((bytes.len() as u64).to_le_bytes());
    inputs.update(bytes);
}

fn receipt(fingerprint: &str, identity: &str) -> Vec<u8> {
    format!("stackyard-create-v1\n{fingerprint}\n{identity}\n").into_bytes()
}

fn matches_receipt(
    target: &Path,
    plan: &CreatePlan,
    files: &[(String, Vec<u8>)],
    registered: bool,
) -> Result<bool> {
    let identity = directory_identity(target)?;
    if !matches_file(
        &target.join(RECEIPT),
        &receipt(&plan.fingerprint, &identity),
    )? {
        return Ok(false);
    }
    let mut expected_entries = BTreeSet::new();
    expected_entries.insert(PathBuf::from(RECEIPT));
    let mut directories = BTreeSet::new();
    directories.insert(PathBuf::new());
    for (relative, bytes) in files {
        let relative = Path::new(relative);
        expected_entries.insert(relative.to_path_buf());
        for dir in relative
            .ancestors()
            .skip(1)
            .filter(|dir| !dir.as_os_str().is_empty())
        {
            if !optional_metadata(&target.join(dir))?.is_some_and(|meta| real_directory(&meta)) {
                return Ok(false);
            }
            directories.insert(dir.to_path_buf());
            expected_entries.insert(dir.to_path_buf());
        }
        if !matches_file(&target.join(relative), bytes)? {
            return Ok(false);
        }
    }

    if !registered {
        for dir in directories {
            let absolute = target.join(&dir);
            for entry in fs::read_dir(&absolute)
                .map_err(|source| path_error("read generated directory", &absolute, source))?
            {
                let entry = entry
                    .map_err(|source| path_error("read generated entry", &absolute, source))?;
                if !expected_entries.contains(&dir.join(entry.file_name())) {
                    return Ok(false);
                }
            }
        }
    }
    Ok(directory_identity(target)? == identity)
}

fn matches_file(path: &Path, expected: &[u8]) -> Result<bool> {
    let Some(meta) = optional_metadata(path)? else {
        return Ok(false);
    };
    if !meta.is_file() || is_redirect(&meta) || meta.len() != expected.len() as u64 {
        return Ok(false);
    }
    let mut bytes = Vec::with_capacity(expected.len());
    File::open(path)
        .and_then(|file| file.take(expected.len() as u64 + 1).read_to_end(&mut bytes))
        .map_err(|source| path_error("read generated source", path, source))?;
    Ok(bytes == expected)
}

fn write_source(
    stage: &Path,
    identity: &str,
    reviewed: &Snapshot,
    cancel: &AtomicBool,
    staged: &mut Vec<StagedEntry>,
) -> Result<()> {
    let target = Path::new(&reviewed.plan.target);
    for (name, content) in &reviewed.files {
        check_cancel(cancel, target)?;
        let mut directory = stage.to_path_buf();
        let mut anchors = Vec::new();
        let parts: Vec<_> = name.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            directory.push(part);
            if let Some(entry) = staged.iter().find(|entry| entry.path == directory) {
                let anchor = open_directory(&directory)?;
                if file_identity(&anchor)? != entry.identity {
                    return Err(failed(format!(
                        "recipe directory changed at {}",
                        directory.display()
                    )));
                }
                anchors.push(anchor);
            } else {
                fs::create_dir(&directory)
                    .map_err(|source| path_error("create recipe directory", &directory, source))?;
                let anchor = open_directory(&directory)?;
                staged.push(StagedEntry {
                    path: directory.clone(),
                    identity: file_identity(&anchor)?,
                    directory: true,
                });
                anchors.push(anchor);
            }
        }
        write_new(&stage.join(name), content, staged)?;
    }
    check_cancel(cancel, target)?;
    write_new(
        &stage.join(RECEIPT),
        &receipt(&reviewed.plan.fingerprint, identity),
        staged,
    )
}

fn write_new(path: &Path, bytes: &[u8], staged: &mut Vec<StagedEntry>) -> Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| path_error("create recipe file", path, source))?;
    staged.push(StagedEntry {
        path: path.to_path_buf(),
        identity: file_identity(&file)?,
        directory: false,
    });
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| path_error("write recipe file", path, source))
}

fn make_stage(parent: &Path) -> Result<PathBuf> {
    let mut random = [0; 16];
    getrandom::getrandom(&mut random)
        .map_err(|error| failed(format!("generate unique staging path: {error}")))?;
    let stage = parent.join(format!(".stackyard-create-{}", hex::encode(random)));
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    #[cfg(not(unix))]
    let _ = &mut builder;
    builder
        .create(&stage)
        .map_err(|source| path_error("create private project staging directory", &stage, source))?;
    Ok(stage)
}

fn cleanup_stage(
    reviewed: &Snapshot,
    stage: &Path,
    identity: &str,
    staged: &[StagedEntry],
) -> Result<()> {
    if stage.parent() != Some(reviewed.parent.as_path())
        || directory_identity(&reviewed.parent)? != file_identity(&reviewed.anchor)?
        || directory_identity(stage)? != identity
    {
        return Err(failed("the staging directory or its parent changed"));
    }
    for entry in staged.iter().filter(|entry| entry.directory) {
        if directory_identity(&entry.path)? != entry.identity {
            return Err(failed(format!(
                "recipe directory changed at {}",
                entry.path.display()
            )));
        }
    }
    for entry in staged.iter().rev() {
        let result = if entry.directory {
            fs::remove_dir(&entry.path)
        } else {
            let Some(meta) = optional_metadata(&entry.path)? else {
                continue;
            };
            if !meta.is_file() || is_redirect(&meta) {
                return Err(failed(format!(
                    "staged file changed at {}",
                    entry.path.display()
                )));
            }
            let file = File::open(&entry.path)
                .map_err(|source| path_error("open staged file", &entry.path, source))?;
            if file_identity(&file)? != entry.identity {
                return Err(failed(format!(
                    "staged file changed at {}",
                    entry.path.display()
                )));
            }
            fs::remove_file(&entry.path)
        };
        result.map_err(|source| path_error("remove staged entry", &entry.path, source))?;
    }
    fs::remove_dir(stage)
        .map_err(|source| path_error("remove private project staging directory", stage, source))
}

fn check_cancel(cancel: &AtomicBool, target: &Path) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled {
            id: target.display().to_string(),
        });
    }
    Ok(())
}

fn occupied(target: &Path) -> Error {
    failed(format!(
        "project destination already exists at {}; select an unused project name or parent",
        plain_path(target.to_path_buf()).display()
    ))
}

fn path_error(action: &str, path: &Path, error: io::Error) -> Error {
    failed(format!("{action} at {}: {error}", path.display()))
}

fn failed(message: impl Into<String>) -> Error {
    Error::OperationFailed(message.into())
}

fn path_text(path: PathBuf) -> Result<String> {
    path.to_str().map(str::to_string).ok_or_else(|| {
        failed(format!(
            "project path cannot be represented as Unicode: {}",
            path.display()
        ))
    })
}

fn optional_metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(path_error("inspect project path", path, error)),
    }
}

fn real_directory(meta: &fs::Metadata) -> bool {
    meta.is_dir() && !is_redirect(meta)
}

pub(crate) fn is_redirect(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

pub(crate) fn open_directory(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ_WRITE: u32 = 0x3;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x00200000;
        // Hold the reviewed parent open without FILE_SHARE_DELETE during publication.
        options
            .share_mode(FILE_SHARE_READ_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY);
    }
    let file = options
        .open(path)
        .map_err(|source| path_error("open project directory", path, source))?;
    let metadata = file
        .metadata()
        .map_err(|source| path_error("read project directory", path, source))?;
    if !real_directory(&metadata) {
        return Err(failed(format!(
            "project directory is not an ordinary directory: {}",
            path.display()
        )));
    }
    Ok(file)
}

fn directory_identity(path: &Path) -> Result<String> {
    file_identity(&open_directory(path)?)
}

#[cfg(unix)]
pub(crate) fn file_identity(file: &File) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    let meta = file
        .metadata()
        .map_err(|source| Error::io("read file identity", source))?;
    Ok(format!(
        "{}:{}:{:?}",
        meta.dev(),
        meta.ino(),
        meta.created().ok()
    ))
}

#[cfg(windows)]
pub(crate) fn file_identity(file: &File) -> Result<String> {
    use std::mem::MaybeUninit;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct FileInformation {
        attributes: u32,
        created: [u32; 2],
        accessed: [u32; 2],
        modified: [u32; 2],
        volume_serial: u32,
        file_size: [u32; 2],
        links: u32,
        index_high: u32,
        index_low: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            handle: std::os::windows::io::RawHandle,
            info: *mut FileInformation,
        ) -> i32;
    }

    let mut info = MaybeUninit::uninit();
    // The OS initializes the output on success. The handle remains open for the call.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
        return Err(Error::io("read file identity", io::Error::last_os_error()));
    }
    let info = unsafe { info.assume_init() };
    Ok(format!(
        "{}:{}:{}",
        info.volume_serial, info.index_high, info.index_low
    ))
}

#[cfg(windows)]
pub(crate) fn publish(_parent: &File, stage: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, destination: *const u16, flags: u32) -> i32;
    }

    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let stage = wide(stage);
    let target = wide(target);
    // Zero flags omit both MOVEFILE_REPLACE_EXISTING and MOVEFILE_COPY_ALLOWED.
    if unsafe { MoveFileExW(stage.as_ptr(), target.as_ptr(), 0) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn publish(parent: &File, stage: &Path, target: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;

    let name = |path: &Path| -> io::Result<CString> {
        let file = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
        CString::new(file.as_bytes()).map_err(io::Error::other)
    };
    let stage = name(stage)?;
    let target = name(target)?;
    let parent = parent.as_raw_fd();
    // Both basenames resolve within the opened parent, and the OS refuses an existing destination.
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            parent,
            stage.as_ptr(),
            parent,
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            parent,
            stage.as_ptr(),
            parent,
            target.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub(crate) fn publish(_parent: &File, _stage: &Path, _target: &Path) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{PackageRecord, ProjectRecord};

    struct Fixture {
        root: PathBuf,
        data: PathBuf,
        parent: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = crate::paths::disposable_dir("create");
            let data = root.join("data");
            let parent = root.join("Проекты with spaces");
            fs::create_dir_all(&parent).expect("create test parent");
            Self { root, data, parent }
        }

        fn request(&self) -> CreateRequest {
            // A recorded file fixture exercises pure preview without invoking an executable.
            fs::create_dir_all(&self.data).expect("create test data");
            let executable = self.root.join("fixture-python.exe");
            fs::write(&executable, b"fixture").expect("write fixture executable");
            let store = Store::open(&self.data).expect("open test store");
            let package = PackageRecord {
                id: "existing-python".into(),
                kind: "runtime".into(),
                name: "python".into(),
                version: "3.12.3".into(),
                target: current_target().into(),
                source: "existing".into(),
                path: executable.display().to_string(),
                launch: executable.display().to_string(),
                digest: None,
                removable: false,
            };
            store
                .insert_package(&package)
                .expect("record fixture executable");
            CreateRequest {
                parent: self.parent.display().to_string(),
                name: "starter".into(),
                recipe: "flask-app".into(),
                manager: "pip".into(),
                runtime: ComponentChoice {
                    kind: "installed".into(),
                    id: package.id,
                },
                tool: None,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).expect("remove disposable creation fixture");
        }
    }

    #[test]
    fn preview_is_pure_and_exact_generation_can_retry() {
        let fixture = Fixture::new();
        let request = fixture.request();
        let plan = plan_create(&fixture.data, &request).expect("preview");
        assert!(!Path::new(&plan.target).exists());
        assert_eq!(fs::read_dir(&fixture.parent).unwrap().count(), 0);
        assert!(plan.files.iter().any(|file| file == "app.py"));
        assert!(plan.downloads.is_empty());

        let cancel = AtomicBool::new(true);
        assert!(matches!(
            generate_create(&fixture.data, &plan, &cancel),
            Err(Error::Cancelled { .. })
        ));
        assert_eq!(fs::read_dir(&fixture.parent).unwrap().count(), 0);
        cancel.store(false, Ordering::Relaxed);
        let root = generate_create(&fixture.data, &plan, &cancel).expect("publish");
        assert_eq!(root, PathBuf::from(&plan.target));
        assert!(root.join("stackyard.toml").is_file());
        assert_eq!(
            generate_create(&fixture.data, &plan, &cancel).unwrap(),
            root
        );

        fs::write(root.join("install-output"), b"kept").unwrap();
        assert!(generate_create(&fixture.data, &plan, &cancel).is_err());
        Store::open(&fixture.data)
            .unwrap()
            .insert_project(&ProjectRecord {
                id: request.name.clone(),
                name: request.name,
                root: plan.target.clone(),
            })
            .unwrap();
        assert_eq!(
            generate_create(&fixture.data, &plan, &cancel).unwrap(),
            root
        );
        fs::write(root.join("app.py"), b"edited").unwrap();
        assert!(generate_create(&fixture.data, &plan, &cancel).is_err());
        assert_eq!(fs::read(root.join("app.py")).unwrap(), b"edited");
        assert_eq!(fs::read(root.join("install-output")).unwrap(), b"kept");
    }

    #[test]
    fn rejects_invalid_names_and_changed_parent_workspace_or_plan() {
        let fixture = Fixture::new();
        let mut request = fixture.request();
        for invalid in [
            "../escape",
            "con",
            "com1",
            "lpt9",
            "Upper",
            "has space",
            "with.dot",
        ] {
            request.name = invalid.into();
            assert!(plan_create(&fixture.data, &request).is_err(), "{invalid}");
        }
        request.name = "starter".into();
        let plan = plan_create(&fixture.data, &request).expect("preview");
        let mut tampered = plan.clone();
        tampered.install_command.push("different".into());
        assert!(verify_create_plan(&fixture.data, &tampered).is_err());

        let original = fixture.root.join("original-parent");
        fs::rename(&fixture.parent, &original).unwrap();
        fs::create_dir(&fixture.parent).unwrap();
        assert!(verify_create_plan(&fixture.data, &plan).is_err());
        let new_plan = plan_create(&fixture.data, &request).expect("new parent preview");
        fs::write(fixture.parent.join("pnpm-workspace.yaml"), b"packages: []").unwrap();
        assert!(verify_create_plan(&fixture.data, &new_plan).is_err());
    }

    #[test]
    fn destination_is_exclusive_even_when_empty_and_imported_ids_conflict() {
        let fixture = Fixture::new();
        let request = fixture.request();
        let plan = plan_create(&fixture.data, &request).expect("preview");
        let reviewed = verify_snapshot(&fixture.data, &plan).unwrap();
        let stage = make_stage(&reviewed.parent).unwrap();
        let target = reviewed.parent.join(&request.name);
        fs::create_dir(&target).unwrap();
        // Check the publication primitive directly to cover a race after final validation.
        assert!(publish(&reviewed.anchor, &stage, &target).is_err());
        assert!(stage.is_dir());
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
        assert!(verify_create_plan(&fixture.data, &plan).is_err());
        fs::remove_dir(target).unwrap();
        fs::remove_dir(stage).unwrap();

        Store::open(&fixture.data)
            .unwrap()
            .insert_project(&ProjectRecord {
                id: request.name.clone(),
                name: request.name,
                root: fixture.root.join("elsewhere").display().to_string(),
            })
            .unwrap();
        assert!(verify_create_plan(&fixture.data, &plan).is_err());
    }

    #[test]
    fn simultaneous_retries_publish_one_source_tree() {
        let fixture = Fixture::new();
        let request = fixture.request();
        let plan = plan_create(&fixture.data, &request).expect("preview");
        let barrier = std::sync::Barrier::new(2);
        let apply = || {
            barrier.wait();
            generate_create(&fixture.data, &plan, &AtomicBool::new(false)).unwrap()
        };
        let (first, second) = std::thread::scope(|scope| {
            let first = scope.spawn(apply);
            let second = scope.spawn(apply);
            (first.join().unwrap(), second.join().unwrap())
        });
        assert_eq!(first, second);
        assert_eq!(fs::read_dir(&fixture.parent).unwrap().count(), 1);
    }

    #[test]
    fn cleanup_preserves_an_untracked_staging_file() {
        let fixture = Fixture::new();
        let request = fixture.request();
        let reviewed = snapshot(&fixture.data, &request).unwrap();
        let stage = make_stage(&reviewed.parent).unwrap();
        let identity = directory_identity(&stage).unwrap();
        let mut staged = Vec::new();
        write_new(&stage.join("owned"), b"owned", &mut staged).unwrap();
        fs::write(stage.join("unrelated"), b"keep").unwrap();
        assert!(cleanup_stage(&reviewed, &stage, &identity, &staged).is_err());
        assert!(!stage.join("owned").exists());
        assert_eq!(fs::read(stage.join("unrelated")).unwrap(), b"keep");
    }

    #[test]
    fn verified_catalog_requirement_does_not_change_after_a_download() {
        let Some(package) = crate::catalog::catalog_for_host()
            .unwrap()
            .into_iter()
            .find(|p| p.name == "python")
        else {
            return;
        };
        let fixture = Fixture::new();
        let mut request = fixture.request();
        request.runtime = ComponentChoice {
            kind: "catalog".into(),
            id: package.id.clone(),
        };
        let plan = plan_create(&fixture.data, &request).unwrap();
        assert_eq!(plan.downloads, std::slice::from_ref(&package));
        Store::open(&fixture.data)
            .unwrap()
            .insert_package(&PackageRecord {
                id: package.id,
                kind: package.kind,
                name: package.name,
                version: package.version,
                target: package.target,
                source: "managed".into(),
                path: "installed".into(),
                launch: "installed".into(),
                digest: Some(package.sha256),
                removable: true,
            })
            .unwrap();
        assert_eq!(verify_create_plan(&fixture.data, &plan).unwrap(), plan);
    }
}
