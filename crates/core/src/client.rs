use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use tokio::io::BufReader;
use tokio::time::timeout;

use crate::error::{Error, Result};
#[cfg(windows)]
use crate::paths::pipe_path;
use crate::paths::prepare_data_dir;
#[cfg(unix)]
use crate::paths::socket_path;
use crate::protocol::{
    check_api_version, next_request_id, Command as EngineCommand, Outcome, Request, Response,
    API_VERSION,
};
use crate::transport::{recv, send};

const ATTEMPTS: u32 = 50;
const ATTEMPT_PAUSE: Duration = Duration::from_millis(100);
const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// Connect to the engine for `data_dir`, starting it when the endpoint is down.
pub async fn ensure_engine(data_dir: &Path) -> Result<PathBuf> {
    let data_dir = prepare_data_dir(data_dir)?;
    if ping(&data_dir).await.is_ok() {
        return Ok(data_dir);
    }
    spawn_engine(&data_dir)?;
    for _ in 0..ATTEMPTS {
        if ping(&data_dir).await.is_ok() {
            return Ok(data_dir);
        }
        tokio::time::sleep(ATTEMPT_PAUSE).await;
    }
    Err(Error::Timeout {
        data_dir: data_dir.clone(),
    })
}

pub async fn roundtrip(data_dir: &Path, cmd: EngineCommand) -> Result<Outcome> {
    let data_dir = prepare_data_dir(data_dir)?;
    let request = Request {
        v: API_VERSION,
        id: next_request_id(),
        cmd,
    };
    let response = timeout(CALL_TIMEOUT, call(&data_dir, &request))
        .await
        .map_err(|_| Error::Timeout {
            data_dir: data_dir.clone(),
        })??;
    check_api_version(response.v)?;
    if response.id != request.id {
        return Err(Error::ResponseMismatch {
            request_id: request.id,
            response_id: response.id,
        });
    }
    if let Some(message) = response.error {
        return Err(Error::InvalidMessage(message));
    }
    response
        .result
        .ok_or_else(|| Error::InvalidMessage("engine reply had no result".to_string()))
}

async fn ping(data_dir: &Path) -> Result<()> {
    let request = Request {
        v: API_VERSION,
        id: next_request_id(),
        cmd: EngineCommand::Status,
    };
    let response = timeout(CALL_TIMEOUT, call(data_dir, &request))
        .await
        .map_err(|_| Error::Timeout {
            data_dir: data_dir.to_path_buf(),
        })??;
    if response.error.is_some() || response.result.is_none() {
        return Err(Error::InvalidMessage(
            "engine status probe failed".to_string(),
        ));
    }
    Ok(())
}

async fn call(data_dir: &Path, request: &Request) -> Result<Response> {
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ClientOptions;
        let pipe = pipe_path(data_dir);
        let client = ClientOptions::new()
            .open(&pipe)
            .map_err(|source| Error::io("connect to engine pipe", source))?;
        let (reader, mut writer) = tokio::io::split(client);
        let mut reader = BufReader::new(reader);
        send(&mut writer, request).await?;
        recv(&mut reader).await
    }
    #[cfg(unix)]
    {
        use tokio::net::UnixStream;
        let socket = socket_path(data_dir);
        let stream = UnixStream::connect(&socket)
            .await
            .map_err(|source| Error::io("connect to engine socket", source))?;
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        send(&mut writer, request).await?;
        recv(&mut reader).await
    }
}

fn spawn_engine(data_dir: &Path) -> Result<()> {
    let mut command = engine_command(data_dir)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW. The engine is a background process of this user.
        command.creation_flags(0x0800_0000);
    }
    command
        .spawn()
        .map_err(|source| Error::io("start engine process", source))?;
    Ok(())
}

fn engine_command(data_dir: &Path) -> Result<Command> {
    if let Some(path) = std::env::var_os("LOCALDEV_ENGINE") {
        let mut command = Command::new(path);
        command.arg("--data-dir").arg(data_dir);
        return Ok(command);
    }
    let current =
        std::env::current_exe().map_err(|source| Error::io("locate current executable", source))?;
    let dir = current.parent().ok_or(Error::EngineBinaryMissing)?;
    let sibling_name = if cfg!(windows) {
        "localdev-engine.exe"
    } else {
        "localdev-engine"
    };
    let sibling = dir.join(sibling_name);
    if sibling.is_file() {
        let mut command = Command::new(sibling);
        command.arg("--data-dir").arg(data_dir);
        return Ok(command);
    }
    let file_name = current
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let is_cli = file_name.starts_with("localdev")
        && !file_name.starts_with("localdev-engine")
        && !file_name.starts_with("localdev-desktop");
    if is_cli {
        let mut command = Command::new(&current);
        command.arg("serve").arg("--data-dir").arg(data_dir);
        return Ok(command);
    }
    Err(Error::EngineBinaryMissing)
}
