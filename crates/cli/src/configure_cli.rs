use std::fs::File;
use std::io::Read;
use std::path::Path;

use stackyard_core::{ensure_engine, roundtrip, Command, Error, Result};

pub async fn run(data: &Path, args: &[String]) -> Result<()> {
    let command = match args {
        [action, path] if action == "inspect" => Command::InspectConfig { path: path.clone() },
        [action, file] if action == "preview" => Command::PreviewConfig { draft: read_draft(file)? },
        [action, file, fingerprint] if action == "apply" => Command::ApplyConfig {
            draft: read_draft(file)?,
            fingerprint: fingerprint.clone(),
        },
        [action, project_id] if action == "reload" => Command::ReloadProject { project_id: project_id.clone() },
        [action, project_id] if action == "forget" => Command::ForgetProject { project_id: project_id.clone() },
        _ => return Err(Error::InvalidMessage(
            "Use configure inspect PATH, configure preview DRAFT.json, configure apply DRAFT.json FINGERPRINT, configure reload PROJECT, or configure forget PROJECT".into(),
        )),
    };
    let dir = ensure_engine(data).await?;
    super::print_outcome(&roundtrip(&dir, command).await?);
    Ok(())
}

fn read_draft(path: &str) -> Result<Box<stackyard_core::configuration::ConfigDraft>> {
    const LIMIT: u64 = 48 * 1024;
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|source| Error::io("open configuration draft", source))?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| Error::io("read configuration draft", source))?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::InvalidMessage(
            "Configuration draft exceeds 48 KiB".into(),
        ));
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| Error::InvalidMessage(format!("Invalid configuration draft: {error}")))
}
