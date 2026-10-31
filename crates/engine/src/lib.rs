//! Per-user engine process. Desktop and CLI are clients of [`serve`].

mod endpoint;

use std::path::Path;
use std::process;

use localdev_core::{check_api_version, Command, Outcome, Request, Response, API_VERSION};
use localdev_core::{prepare_data_dir, recv, send, Error, Result, Store};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

pub use localdev_core::Error as EngineError;

pub fn serve(data_dir: &Path) -> Result<()> {
    let data_dir = prepare_data_dir(data_dir)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|source| Error::io("create engine runtime", source))?;
    runtime.block_on(listen(data_dir))
}

async fn listen(data_dir: std::path::PathBuf) -> Result<()> {
    let store = Store::open(&data_dir)?;
    let mut endpoint = endpoint::Endpoint::bind(&data_dir)?;
    loop {
        let mut client = endpoint.accept().await?;
        let stop = match handle_client(&mut client, &store, &data_dir).await {
            Ok(stop) => stop,
            Err(err) => {
                eprintln!("engine client: {err}");
                false
            }
        };
        if stop {
            break;
        }
    }
    Ok(())
}

async fn handle_client<S>(stream: &mut S, store: &Store, data_dir: &Path) -> Result<bool>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let request: Request = {
        let mut reader = BufReader::new(&mut *stream);
        recv(&mut reader).await?
    };
    let response = match dispatch(&request, store, data_dir) {
        Ok(outcome) => Response {
            v: API_VERSION,
            id: request.id.clone(),
            result: Some(outcome),
            error: None,
        },
        Err(err) => Response {
            v: API_VERSION,
            id: request.id.clone(),
            result: None,
            error: Some(err.to_string()),
        },
    };
    send(&mut *stream, &response).await?;
    stream
        .shutdown()
        .await
        .map_err(|source| Error::io("close engine reply", source))?;
    Ok(matches!(request.cmd, Command::Stop) && response.error.is_none())
}

fn dispatch(request: &Request, store: &Store, data_dir: &Path) -> Result<Outcome> {
    check_api_version(request.v)?;
    match &request.cmd {
        Command::Status => Ok(Outcome::Status {
            pid: process::id(),
            data_dir: data_dir.display().to_string(),
            api_version: API_VERSION,
            schema_version: store.schema_version()?,
            last_event: store.last_event()?,
        }),
        Command::GetSetting { key } => Ok(Outcome::Setting {
            key: key.clone(),
            value: store.get(key)?,
        }),
        Command::PutSetting { key, value } => {
            store.put(key, value)?;
            Ok(Outcome::Setting {
                key: key.clone(),
                value: Some(value.clone()),
            })
        }
        Command::Stop => Ok(Outcome::Stopped),
    }
}
