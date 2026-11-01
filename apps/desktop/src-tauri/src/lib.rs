use serde::Serialize;
use stackyard_core::{default_data_dir, ensure_engine, roundtrip};
use stackyard_core::{Command, Outcome};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusView {
    pid: u32,
    data_dir: String,
    api_version: u32,
    schema_version: u32,
    last_event: Option<String>,
}

#[tauri::command]
async fn engine_status() -> Result<StatusView, String> {
    let dir = ensure_engine(&default_data_dir())
        .await
        .map_err(|err| err.to_string())?;
    match roundtrip(&dir, Command::Status)
        .await
        .map_err(|err| err.to_string())?
    {
        Outcome::Status {
            pid,
            data_dir,
            api_version,
            schema_version,
            last_event,
        } => Ok(StatusView {
            pid,
            data_dir,
            api_version,
            schema_version,
            last_event,
        }),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
async fn engine_setting(key: String) -> Result<Option<String>, String> {
    let dir = ensure_engine(&default_data_dir())
        .await
        .map_err(|err| err.to_string())?;
    match roundtrip(&dir, Command::GetSetting { key })
        .await
        .map_err(|err| err.to_string())?
    {
        Outcome::Setting { value, .. } => Ok(value),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
async fn put_setting(key: String, value: String) -> Result<(), String> {
    let dir = ensure_engine(&default_data_dir())
        .await
        .map_err(|err| err.to_string())?;
    roundtrip(&dir, Command::PutSetting { key, value })
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageView {
    id: String,
    source: String,
    version: String,
    path: String,
    launch: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogView {
    id: String,
    name: String,
    version: String,
    target: String,
}

#[tauri::command]
async fn list_packages() -> Result<Vec<PackageView>, String> {
    let dir = ensure_engine(&default_data_dir())
        .await
        .map_err(|err| err.to_string())?;
    match roundtrip(&dir, Command::ListPackages)
        .await
        .map_err(|err| err.to_string())?
    {
        Outcome::Packages { packages } => Ok(packages
            .into_iter()
            .map(|package| PackageView {
                id: package.id,
                source: package.source,
                version: package.version,
                path: package.path,
                launch: package.launch,
            })
            .collect()),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
async fn list_catalog() -> Result<Vec<CatalogView>, String> {
    let dir = ensure_engine(&default_data_dir())
        .await
        .map_err(|err| err.to_string())?;
    match roundtrip(&dir, Command::ListCatalog)
        .await
        .map_err(|err| err.to_string())?
    {
        Outcome::Catalog { packages } => Ok(packages
            .into_iter()
            .map(|package| CatalogView {
                id: package.id,
                name: package.name,
                version: package.version,
                target: package.target,
            })
            .collect()),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[tauri::command]
async fn install_package(catalog_id: String) -> Result<String, String> {
    let dir = ensure_engine(&default_data_dir())
        .await
        .map_err(|err| err.to_string())?;
    let started = roundtrip(
        &dir,
        Command::StartInstall {
            catalog_id: catalog_id.clone(),
        },
    )
    .await
    .map_err(|err| err.to_string())?;
    let Outcome::Operation { id, .. } = started else {
        return Err(format!("unexpected engine reply: {started:?}"));
    };
    for _ in 0..600 {
        let status = roundtrip(
            &dir,
            Command::InstallStatus {
                operation_id: id.clone(),
            },
        )
        .await
        .map_err(|err| err.to_string())?;
        if let Outcome::Operation { state, message, .. } = status {
            if state == "succeeded" {
                return Ok(message);
            }
            if state != "running" {
                return Err(message);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err(format!("install of {catalog_id} timed out"))
}

#[tauri::command]
async fn use_existing(role: String, path: String) -> Result<String, String> {
    let dir = ensure_engine(&default_data_dir())
        .await
        .map_err(|err| err.to_string())?;
    match roundtrip(&dir, Command::UseExisting { role, path })
        .await
        .map_err(|err| err.to_string())?
    {
        Outcome::Packages { packages } => Ok(packages
            .first()
            .map(|package| format!("{} {}", package.version, package.path))
            .unwrap_or_else(|| "recorded".to_string())),
        other => Err(format!("unexpected engine reply: {other:?}")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            engine_status,
            engine_setting,
            put_setting,
            list_packages,
            list_catalog,
            install_package,
            use_existing
        ])
        .run(tauri::generate_context!())
        .expect("error while running Stackyard");
}
