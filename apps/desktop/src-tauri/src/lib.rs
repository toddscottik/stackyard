use localdev_core::{default_data_dir, ensure_engine, roundtrip};
use localdev_core::{Command, Outcome};
use serde::Serialize;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            engine_status,
            engine_setting,
            put_setting
        ])
        .run(tauri::generate_context!())
        .expect("error while running Local Dev");
}
