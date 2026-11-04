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
    roundtrip_within(data_dir, cmd, CALL_TIMEOUT).await
}

/// Same as [`roundtrip`], with a caller-chosen wait. Database initialization
/// can outlast the short status timeout.
pub async fn roundtrip_within(
    data_dir: &Path,
    cmd: EngineCommand,
    limit: Duration,
) -> Result<Outcome> {
    let data_dir = prepare_data_dir(data_dir)?;
    let request = Request {
        v: API_VERSION,
        id: next_request_id(),
        cmd,
    };
    let response = timeout(limit, call(&data_dir, &request))
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
        return Err(Error::OperationFailed(message));
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
        // The server finishes one request before it calls ConnectNamedPipe again.
        // A client that opens in that gap gets ERROR_PIPE_BUSY (231). A gap with
        // no instance at all is ERROR_FILE_NOT_FOUND (2). Both clear on the next try.
        let mut last_error = None;
        let client = 'open: {
            for _ in 0..50 {
                match ClientOptions::new().open(&pipe) {
                    Ok(client) => break 'open client,
                    Err(err) if matches!(err.raw_os_error(), Some(2 | 231)) => {
                        last_error = Some(err);
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                    Err(err) => return Err(Error::io("connect to engine pipe", err)),
                }
            }
            return Err(Error::io(
                "connect to engine pipe",
                last_error.unwrap_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::NotFound, "engine pipe did not appear")
                }),
            ));
        };
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
        // Replacing stdio alone does not stop std::process::Command from
        // inheriting other Windows handles, including captured shell pipes.
        // The stable toolchain has no Command API to disable inheritance.
        windows_background::spawn(&command)
    }
    #[cfg(not(windows))]
    {
        command
            .spawn()
            .map_err(|source| Error::io("start engine process", source))?;
        Ok(())
    }
}

#[cfg(windows)]
mod windows_background {
    use std::ffi::{c_void, OsStr};
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    use super::{Command, Error, Result};

    #[repr(C)]
    struct StartupInfo {
        size: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        x: u32,
        y: u32,
        x_size: u32,
        y_size: u32,
        x_chars: u32,
        y_chars: u32,
        fill_attribute: u32,
        flags: u32,
        show_window: u16,
        reserved_size: u16,
        reserved_bytes: *mut u8,
        stdin: *mut c_void,
        stdout: *mut c_void,
        stderr: *mut c_void,
    }

    #[repr(C)]
    struct ProcessInfo {
        process: *mut c_void,
        thread: *mut c_void,
        process_id: u32,
        thread_id: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateProcessW(
            application: *const u16,
            command_line: *mut u16,
            process_security: *const c_void,
            thread_security: *const c_void,
            inherit_handles: i32,
            flags: u32,
            environment: *const c_void,
            directory: *const u16,
            startup: *const StartupInfo,
            process: *mut ProcessInfo,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    pub(super) fn spawn(command: &Command) -> Result<()> {
        // engine_command sets only an explicit executable and argument array.
        // Null environment/directory inherit the current user's values.
        let mut application = wide(command.get_program())?;
        application.push(0);
        let mut line = quote_argument(command.get_program())?;
        for argument in command.get_args() {
            line.push(b' ' as u16);
            line.extend(quote_argument(argument)?);
        }
        line.push(0);
        // The Win32 command-line length includes its terminating NUL.
        if line.len() > 32_767 {
            return Err(Error::io(
                "start engine process",
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "engine command line exceeds Windows limit",
                ),
            ));
        }
        // SAFETY: both C structs consist of integer/pointer fields; zero is a
        // valid initial value. All strings remain alive through CreateProcessW.
        let mut startup: StartupInfo = unsafe { std::mem::zeroed() };
        startup.size = std::mem::size_of::<StartupInfo>() as u32;
        let mut process: ProcessInfo = unsafe { std::mem::zeroed() };
        // CREATE_NO_WINDOW matches the previous launch. No BREAKAWAY or job
        // flags are used. FALSE prevents every inheritable parent handle from
        // reaching the engine, not merely the three standard handles.
        let created = unsafe {
            CreateProcessW(
                application.as_ptr(),
                line.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                0,
                0x0800_0000,
                ptr::null(),
                ptr::null(),
                &startup,
                &mut process,
            )
        };
        if created == 0 {
            return Err(Error::io(
                "start engine process",
                io::Error::last_os_error(),
            ));
        }
        // SAFETY: CreateProcessW returned success and owns these two handles.
        // Closing them does not terminate the per-user background process.
        unsafe {
            CloseHandle(process.thread);
            CloseHandle(process.process);
        }
        Ok(())
    }

    fn wide(value: &OsStr) -> Result<Vec<u16>> {
        let value: Vec<u16> = value.encode_wide().collect();
        if value.contains(&0) {
            return Err(Error::io(
                "start engine process",
                io::Error::new(io::ErrorKind::InvalidInput, "engine argument contains NUL"),
            ));
        }
        Ok(value)
    }

    /// Windows CRT argv quoting: double backslashes before quotes and before
    /// the closing quote, while preserving all other UTF-16 units exactly.
    fn quote_argument(value: &OsStr) -> Result<Vec<u16>> {
        let mut quoted = vec![b'"' as u16];
        let mut slashes = 0;
        for unit in wide(value)? {
            if unit == b'\\' as u16 {
                slashes += 1;
                continue;
            }
            let count = if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            };
            quoted.extend(std::iter::repeat_n(b'\\' as u16, count));
            quoted.push(unit);
            slashes = 0;
        }
        quoted.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
        quoted.push(b'"' as u16);
        Ok(quoted)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn quotes_windows_arguments_without_losing_utf16_or_backslashes() {
            for (input, expected) in [
                ("", "\"\""),
                ("plain", "\"plain\""),
                ("пример path", "\"пример path\""),
                ("C:\\space dir\\", "\"C:\\space dir\\\\\""),
                ("say \"hello\"", "\"say \\\"hello\\\"\""),
                ("slash\\\"quote", "\"slash\\\\\\\"quote\""),
            ] {
                assert_eq!(
                    quote_argument(OsStr::new(input)).unwrap(),
                    expected.encode_utf16().collect::<Vec<_>>()
                );
            }
            assert!(quote_argument(OsStr::new("bad\0argument")).is_err());
        }
    }
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
        "stackyard-engine.exe"
    } else {
        "stackyard-engine"
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
    let is_cli = file_name.starts_with("stackyard")
        && !file_name.starts_with("stackyard-engine")
        && !file_name.starts_with("stackyard-desktop");
    if is_cli {
        let mut command = Command::new(&current);
        command.arg("serve").arg("--data-dir").arg(data_dir);
        return Ok(command);
    }
    Err(Error::EngineBinaryMissing)
}
