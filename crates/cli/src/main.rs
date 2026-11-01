use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use stackyard_core::{default_data_dir, ensure_engine, roundtrip};
use stackyard_core::{Command, Outcome};

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

async fn run() -> stackyard_core::Result<()> {
    let mut args = env::args().skip(1);
    let mut data_dir: Option<PathBuf> = None;
    let mut command: Vec<String> = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                let path = args.next().ok_or_else(|| {
                    stackyard_core::Error::InvalidMessage("--data-dir needs a path".to_string())
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
        [cmd] if cmd == "serve" => stackyard_engine::serve(&data_dir),
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
        [cmd] if cmd == "catalog" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(&roundtrip(&dir, Command::ListCatalog).await?);
            Ok(())
        }
        [cmd] if cmd == "packages" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(&roundtrip(&dir, Command::ListPackages).await?);
            Ok(())
        }
        [cmd] if cmd == "discover" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(&roundtrip(&dir, Command::Discover).await?);
            Ok(())
        }
        [cmd, id] if cmd == "install" => {
            let dir = ensure_engine(&data_dir).await?;
            let started = roundtrip(
                &dir,
                Command::StartInstall {
                    catalog_id: id.to_string(),
                },
            )
            .await?;
            let Outcome::Operation {
                id: operation_id, ..
            } = &started
            else {
                print_outcome(&started);
                return Ok(());
            };
            let operation_id = operation_id.clone();
            loop {
                let status = roundtrip(
                    &dir,
                    Command::InstallStatus {
                        operation_id: operation_id.clone(),
                    },
                )
                .await?;
                if let Outcome::Operation { state, .. } = &status {
                    if state != "running" {
                        print_outcome(&status);
                        if state != "succeeded" {
                            return Err(stackyard_core::Error::InvalidMessage(format!(
                                "install {state}"
                            )));
                        }
                        return Ok(());
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
        [cmd, role, path] if cmd == "use-existing" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::UseExisting {
                        role: role.to_string(),
                        path: path.to_string(),
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd, project, package_id] if cmd == "select" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::SelectPackage {
                        project_id: project.to_string(),
                        package_id: package_id.to_string(),
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd, id] if cmd == "remove" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::RemovePackage {
                        package_id: id.to_string(),
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
        other => Err(stackyard_core::Error::InvalidMessage(format!(
            "unknown command {other:?}; run stackyard --help"
        ))),
    }
}

fn print_help() {
    let default = default_data_dir();
    println!(
        "stackyard controls the per-user engine.\n\n\
         Usage:\n  \
         stackyard status\n  \
         stackyard get KEY\n  \
         stackyard put KEY VALUE\n  \
         stackyard stop\n  \
         stackyard serve\n  \
         stackyard catalog\n  \
         stackyard packages\n  \
         stackyard discover\n  \
         stackyard install CATALOG_ID\n  \
         stackyard use-existing ROLE PATH\n  \
         stackyard select PROJECT PACKAGE_ID\n  \
         stackyard remove PACKAGE_ID\n\n\
         --data-dir PATH   state directory (default: {})\n\n\
         status, get, put, stop, and package commands start the engine when it is not running.\n\
         serve runs it in the foreground. A second serve exits because the endpoint is already owned.\n\
         use-existing records a path and does not copy or modify that installation.",
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
        Outcome::Catalog { packages } => {
            for package in packages {
                println!(
                    "{} {} {} {}",
                    package.id, package.version, package.target, package.url
                );
            }
        }
        Outcome::Packages { packages } => {
            if packages.is_empty() {
                println!("none");
            }
            for package in packages {
                println!(
                    "{} {} {} {} {}",
                    package.id, package.source, package.version, package.path, package.launch
                );
            }
        }
        Outcome::Discovered { items } => {
            if items.is_empty() {
                println!("none");
            }
            for item in items {
                println!("{} {}", item.role, item.path);
            }
        }
        Outcome::Operation {
            id,
            catalog_id,
            state,
            bytes,
            message,
        } => {
            println!("operation: {id}");
            println!("catalog_id: {catalog_id}");
            println!("state: {state}");
            println!("bytes: {bytes}");
            println!("message: {message}");
        }
    }
}
