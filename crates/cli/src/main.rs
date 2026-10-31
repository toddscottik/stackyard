use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use localdev_core::{default_data_dir, ensure_engine, roundtrip};
use localdev_core::{Command, Outcome};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
    }
}

async fn run() -> localdev_core::Result<()> {
    let mut args = env::args().skip(1);
    let mut data_dir: Option<PathBuf> = None;
    let mut command: Vec<String> = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                let path = args.next().ok_or_else(|| {
                    localdev_core::Error::InvalidMessage("--data-dir needs a path".to_string())
                })?;
                data_dir = Some(PathBuf::from(path));
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            _ => command.push(arg),
        }
    }
    let data_dir = data_dir.unwrap_or_else(default_data_dir);
    match command.as_slice() {
        [] => {
            print_help();
            Ok(())
        }
        [cmd, rest @ ..] if cmd == "help" => {
            let _ = rest;
            print_help();
            Ok(())
        }
        [cmd] if cmd == "serve" => localdev_engine::serve(&data_dir),
        [cmd] if cmd == "status" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(&roundtrip(&dir, Command::Status).await?);
            Ok(())
        }
        [cmd] if cmd == "stop" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(&roundtrip(&dir, Command::Stop).await?);
            Ok(())
        }
        [cmd, key] if cmd == "get" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::GetSetting {
                        key: key.to_string(),
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd, key, value] if cmd == "put" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::PutSetting {
                        key: key.to_string(),
                        value: value.to_string(),
                    },
                )
                .await?,
            );
            Ok(())
        }
        other => Err(localdev_core::Error::InvalidMessage(format!(
            "unknown command {other:?}; run localdev --help"
        ))),
    }
}

fn print_help() {
    let default = default_data_dir();
    println!(
        "localdev controls the per-user engine.\n\n\
         Usage:\n  \
         localdev status\n  \
         localdev get KEY\n  \
         localdev put KEY VALUE\n  \
         localdev stop\n  \
         localdev serve\n\n\
         --data-dir PATH   state directory (default: {})\n\n\
         status, get, put, and stop start the engine when it is not running.\n\
         serve runs it in the foreground. A second serve attaches to nothing and exits\n\
         because the endpoint is already owned.",
        default.display()
    );
}

fn print_outcome(outcome: &Outcome) {
    match outcome {
        Outcome::Status {
            pid,
            data_dir,
            api_version,
            schema_version,
            last_event,
        } => {
            println!("pid: {pid}");
            println!("data_dir: {data_dir}");
            println!("api_version: {api_version}");
            println!("schema_version: {schema_version}");
            println!("last_event: {}", last_event.as_deref().unwrap_or("-"));
        }
        Outcome::Setting { key, value } => {
            println!("key: {key}");
            println!("value: {}", value.as_deref().unwrap_or("-"));
        }
        Outcome::Stopped => println!("stopped"),
    }
}
