use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use stackyard_core::{default_data_dir, ensure_engine, roundtrip, roundtrip_within};
use stackyard_core::{Command, Outcome};

mod create_cli;

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
        [cmd, args @ ..] if cmd == "create" => create_cli::run(&data_dir, args).await,
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
                &roundtrip_within(
                    &dir,
                    Command::UseExisting {
                        role: role.to_string(),
                        path: path.to_string(),
                    },
                    std::time::Duration::from_secs(45),
                )
                .await?,
            );
            Ok(())
        }
        [cmd, project, package_id] if cmd == "select" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip_within(
                    &dir,
                    Command::SelectPackage {
                        project_id: project.to_string(),
                        package_id: package_id.to_string(),
                    },
                    std::time::Duration::from_secs(45),
                )
                .await?,
            );
            Ok(())
        }
        [cmd, project, scope, package] if cmd == "select-scope" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip_within(
                    &dir,
                    Command::SelectScopePackage {
                        project_id: project.clone(),
                        scope_id: scope.clone(),
                        package_id: package.clone(),
                    },
                    std::time::Duration::from_secs(45),
                )
                .await?,
            );
            Ok(())
        }
        [cmd, root] if cmd == "preview" => preview_command(&data_dir, root, None),
        [cmd, root, draft] if cmd == "preview" => preview_command(&data_dir, root, Some(draft)),
        [cmd, path] if cmd == "import" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::ImportProject {
                        path: path.to_string(),
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd] if cmd == "projects" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(&roundtrip(&dir, Command::ListProjects).await?);
            Ok(())
        }
        [cmd, id] if cmd == "project" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::ProjectStatus {
                        project_id: id.to_string(),
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd, id] if cmd == "start" => {
            let dir = ensure_engine(&data_dir).await?;
            roundtrip(
                &dir,
                Command::StartProject {
                    project_id: id.to_string(),
                },
            )
            .await?;
            wait_project(&dir, id).await
        }
        [cmd, id] if cmd == "stop-project" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::StopProject {
                        project_id: id.to_string(),
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd, id] if cmd == "restart" => {
            let dir = ensure_engine(&data_dir).await?;
            roundtrip(
                &dir,
                Command::RestartProject {
                    project_id: id.to_string(),
                },
            )
            .await?;
            wait_project(&dir, id).await
        }
        [cmd, id] if cmd == "cancel" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::CancelStart {
                        project_id: id.to_string(),
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd, id, service] if cmd == "logs" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::ReadLogs {
                        project_id: id.to_string(),
                        service_id: service.to_string(),
                        offset: 0,
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd, id, service, rest @ ..] if cmd == "exec" => {
            let dir = ensure_engine(&data_dir).await?;
            let args = if rest.first().map(String::as_str) == Some("--") {
                rest[1..].to_vec()
            } else {
                rest.to_vec()
            };
            let started = roundtrip(
                &dir,
                Command::Exec {
                    project_id: id.to_string(),
                    service_id: service.to_string(),
                    args,
                },
            )
            .await?;
            wait_task(&dir, &started).await
        }
        [cmd] if cmd == "sites" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(&roundtrip(&dir, Command::FrontStatus).await?);
            Ok(())
        }
        [cmd, id] if cmd == "publish" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::PublishProject {
                        project_id: id.to_string(),
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd, id] if cmd == "unpublish" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::UnpublishProject {
                        project_id: id.to_string(),
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd, sub] if cmd == "apache" && sub == "restart" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(&roundtrip(&dir, Command::RestartFront).await?);
            Ok(())
        }
        [cmd, http, https] if cmd == "ports" => {
            let dir = ensure_engine(&data_dir).await?;
            let http_port = http.parse().map_err(|_| {
                stackyard_core::Error::InvalidMessage(format!("{http} is not a port"))
            })?;
            let https_port = https.parse().map_err(|_| {
                stackyard_core::Error::InvalidMessage(format!("{https} is not a port"))
            })?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::SetFrontPorts {
                        http_port,
                        https_port,
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd, sub, path] if cmd == "trust" && (sub == "install" || sub == "remove") => {
            let dir = ensure_engine(&data_dir).await?;
            let command = if sub == "install" {
                Command::TrustInstall {
                    directory: path.to_string(),
                }
            } else {
                Command::TrustRemove {
                    directory: path.to_string(),
                }
            };
            print_outcome(&roundtrip(&dir, command).await?);
            Ok(())
        }
        [cmd, rest @ ..] if cmd == "db" => db_command(&data_dir, rest).await,
        [cmd, rest @ ..] if cmd == "cache" => cache_command(&data_dir, rest).await,
        [cmd, project, path] if cmd == "sqlite" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::SqliteUse {
                        project_id: project.to_string(),
                        path: path.to_string(),
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd, path] if cmd == "hosts-write" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::WriteHosts {
                        path: path.to_string(),
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd] if cmd == "diagnostics" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(&roundtrip(&dir, Command::Diagnostics).await?);
            Ok(())
        }
        [cmd, path] if cmd == "diagnostics" => {
            let dir = ensure_engine(&data_dir).await?;
            let outcome = roundtrip(&dir, Command::Diagnostics).await?;
            let Outcome::Diagnostics { report } = &outcome else {
                print_outcome(&outcome);
                return Ok(());
            };
            std::fs::write(path, report)
                .map_err(|source| stackyard_core::Error::io("write diagnostics report", source))?;
            println!("wrote {}", path);
            Ok(())
        }
        [cmd] if cmd == "updates" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(&roundtrip(&dir, Command::RefreshUpdates).await?);
            Ok(())
        }
        [cmd] if cmd == "cleanup" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(&roundtrip(&dir, Command::CleanupUnused).await?);
            Ok(())
        }
        [cmd, id, version] if cmd == "note-external" => {
            let dir = ensure_engine(&data_dir).await?;
            print_outcome(
                &roundtrip(
                    &dir,
                    Command::NoteExternal {
                        package_id: id.to_string(),
                        observed_version: version.to_string(),
                    },
                )
                .await?,
            );
            Ok(())
        }
        [cmd, rest @ ..] if cmd == "php" => {
            let dir = ensure_engine(&data_dir).await?;
            php_command(&dir, rest).await
        }
        [cmd, address, port] if cmd == "memcached-probe" => {
            let dir = ensure_engine(&data_dir).await?;
            let endpoint = stackyard_core::memcached::MemcachedEndpoint {
                address: address.parse().map_err(|_| {
                    stackyard_core::Error::InvalidMessage(
                        "Memcached needs a numeric IP address".into(),
                    )
                })?,
                port: port.parse().map_err(|_| {
                    stackyard_core::Error::InvalidMessage("Memcached needs a port".into())
                })?,
            };
            print_outcome(&roundtrip(&dir, Command::MemcachedProbe { endpoint }).await?);
            Ok(())
        }
        [cmd, rest @ ..] if cmd == "deps" => {
            let dir = ensure_engine(&data_dir).await?;
            deps_command(&dir, rest).await
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

async fn php_command(dir: &std::path::Path, args: &[String]) -> stackyard_core::Result<()> {
    let (scope_id, args) = take_scope_id(args)?;
    let command = match args.as_slice() {
        [action, project] if action == "status" => Command::PhpStatus { project_id: project.clone() },
        [action, project, extensions @ ..] if action == "profile" => Command::PhpApplyProfile { project_id: project.clone(), extensions: extensions.to_vec() },
        [action, project, id] if action == "install-extension" => Command::PhpInstallExtension { project_id: project.clone(), catalog_id: id.clone() },
        _ => return Err(stackyard_core::Error::InvalidMessage("usage: stackyard php status PROJECT | profile PROJECT [EXTENSION...] | install-extension PROJECT CATALOG_ID".into())),
    };
    let command = match (scope_id, command) {
        (Some(scope_id), Command::PhpStatus { project_id }) => Command::PhpStatusScoped {
            project_id,
            scope_id,
        },
        (
            Some(scope_id),
            Command::PhpApplyProfile {
                project_id,
                extensions,
            },
        ) => Command::PhpApplyProfileScoped {
            project_id,
            scope_id,
            extensions,
        },
        (
            Some(scope_id),
            Command::PhpInstallExtension {
                project_id,
                catalog_id,
            },
        ) => Command::PhpInstallExtensionScoped {
            project_id,
            scope_id,
            catalog_id,
        },
        (_, command) => command,
    };
    let outcome = roundtrip_within(dir, command, std::time::Duration::from_secs(120)).await?;
    if let Outcome::Operation { id, .. } = &outcome {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(600);
        loop {
            let status = roundtrip(
                dir,
                Command::InstallStatus {
                    operation_id: id.clone(),
                },
            )
            .await?;
            if let Outcome::Operation { state, message, .. } = &status {
                if state != "running" {
                    print_outcome(&status);
                    if state != "succeeded" {
                        return Err(stackyard_core::Error::OperationFailed(message.clone()));
                    }
                    break;
                }
            }
            if tokio::time::Instant::now() >= deadline {
                let _ = roundtrip(
                    dir,
                    Command::CancelInstall {
                        operation_id: id.clone(),
                    },
                )
                .await;
                return Err(stackyard_core::Error::OperationFailed(
                    "extension download timed out and cancellation was requested".into(),
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    } else {
        print_outcome(&outcome);
    }
    Ok(())
}

async fn cache_command(data_dir: &std::path::Path, args: &[String]) -> stackyard_core::Result<()> {
    let command = match args {
        [] => Command::CacheList,
        [action] if action == "list" => Command::CacheList,
        [action,package] if action == "create" => Command::CacheCreate {package_id:package.clone(),port:0},
        [action,package,port] if action == "create" => Command::CacheCreate {package_id:package.clone(),port:port.parse().map_err(|_|stackyard_core::Error::InvalidMessage("cache port must be numeric".into()))?},
        [action,id] if action == "start" => Command::CacheStart {instance_id:id.clone()},
        [action,id] if action == "stop" => Command::CacheStop {instance_id:id.clone()},
        [action,id] if action == "forget" => Command::CacheForget {instance_id:id.clone()},
        _ => return Err(stackyard_core::Error::InvalidMessage("usage: stackyard cache list | create PACKAGE_ID [PORT] | start ID | stop ID | forget ID".into())),
    };
    let dir = ensure_engine(data_dir).await?;
    print_outcome(&roundtrip_within(&dir, command, std::time::Duration::from_secs(30)).await?);
    Ok(())
}

async fn deps_command(dir: &std::path::Path, args: &[String]) -> stackyard_core::Result<()> {
    let (scope_id, args) = take_scope_id(args)?;
    if scope_id.is_some() && args.iter().any(|a| a == "--scope") {
        return Err(stackyard_core::Error::InvalidMessage(
            "choose --scope DIR or --scope-id ID, not both".into(),
        ));
    }
    if let Some(scope_id) = scope_id {
        let command=match args.as_slice() {
            [a,p] if a=="install"=>Command::DepInstallScoped{project_id:p.clone(),scope_id},
            [a,p] if a=="list"=>Command::DepListScoped{project_id:p.clone(),scope_id},
            [a,p] if a=="recover"=>Command::DepRecoverScoped{project_id:p.clone(),scope_id},
            [a,p,package] if a=="add"=>Command::DepAddScoped{project_id:p.clone(),scope_id,package:package.clone()},
            [a,p,package] if a=="remove"=>Command::DepRemoveScoped{project_id:p.clone(),scope_id,package:package.clone()},
            [a,p,package] if a=="update"=>Command::DepUpdateScoped{project_id:p.clone(),scope_id,package:package.clone()},
            _=>return Err(stackyard_core::Error::InvalidMessage("scoped dependency action needs install/list/recover PROJECT or add/remove/update PROJECT PACKAGE".into())),
        };
        return wait_task(dir, &roundtrip(dir, command).await?).await;
    }
    let (scope, positional) = split_scope(&args)?;
    let started = match positional.as_slice() {
        [action, id] if action == "install" => {
            roundtrip(
                dir,
                Command::DepInstall {
                    project_id: id.clone(),
                    scope,
                },
            )
            .await?
        }
        [action, id, package] if action == "add" => {
            roundtrip(
                dir,
                Command::DepAdd {
                    project_id: id.clone(),
                    package: package.clone(),
                    scope,
                },
            )
            .await?
        }
        [action, id, package] if action == "remove" => {
            roundtrip(
                dir,
                Command::DepRemove {
                    project_id: id.clone(),
                    package: package.clone(),
                    scope,
                },
            )
            .await?
        }
        [action, id, package] if action == "update" => {
            roundtrip(
                dir,
                Command::DepUpdate {
                    project_id: id.clone(),
                    package: package.clone(),
                    scope,
                },
            )
            .await?
        }
        [action, id] if action == "recover" => {
            roundtrip(
                dir,
                Command::DepRecover {
                    project_id: id.clone(),
                    scope,
                },
            )
            .await?
        }
        [action, id] if action == "list" => {
            roundtrip(
                dir,
                Command::DepList {
                    project_id: id.clone(),
                    scope,
                },
            )
            .await?
        }
        [action, id] if action == "cancel" => {
            print_outcome(
                &roundtrip(
                    dir,
                    Command::DepCancel {
                        operation_id: id.clone(),
                    },
                )
                .await?,
            );
            return Ok(());
        }
        _ => {
            return Err(stackyard_core::Error::InvalidMessage(
                "usage: stackyard deps install|add|remove|update|list|recover|cancel ..."
                    .to_string(),
            ));
        }
    };
    wait_task(dir, &started).await
}

fn take_scope_id(args: &[String]) -> stackyard_core::Result<(Option<String>, Vec<String>)> {
    let mut scope = None;
    let mut positional = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--scope-id" {
            if scope.is_some() {
                return Err(stackyard_core::Error::InvalidMessage(
                    "--scope-id was repeated".into(),
                ));
            }
            scope = Some(
                iter.next()
                    .ok_or_else(|| {
                        stackyard_core::Error::InvalidMessage("--scope-id needs an ID".into())
                    })?
                    .clone(),
            );
        } else {
            positional.push(arg.clone());
        }
    }
    Ok((scope, positional))
}

fn preview_command(
    data_dir: &std::path::Path,
    root: &str,
    draft: Option<&String>,
) -> stackyard_core::Result<()> {
    use std::io::Read;
    let path = draft
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(root).join("stackyard.toml"));
    let file = std::fs::File::open(path)
        .map_err(|e| stackyard_core::Error::io("read preview draft", e))?;
    let mut text = String::new();
    file.take(65537)
        .read_to_string(&mut text)
        .map_err(|e| stackyard_core::Error::io("read bounded preview draft", e))?;
    let store = stackyard_core::Store::open_read_only(data_dir)?;
    let result = stackyard_core::preview::preview_request(
        &stackyard_core::preview::PreviewRequest {
            root: root.into(),
            manifest_text: text,
            project_id: None,
            selection_overrides: Vec::new(),
        },
        store.as_ref(),
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&result)
            .map_err(|e| stackyard_core::Error::InvalidMessage(e.to_string()))?
    );
    Ok(())
}

fn split_scope(args: &[String]) -> stackyard_core::Result<(String, Vec<String>)> {
    let mut scope = String::new();
    let mut positional = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--scope" {
            scope = iter
                .next()
                .ok_or_else(|| {
                    stackyard_core::Error::InvalidMessage("--scope needs a directory".to_string())
                })?
                .clone();
        } else {
            positional.push(arg.clone());
        }
    }
    Ok((scope, positional))
}

async fn wait_project(dir: &std::path::Path, id: &str) -> stackyard_core::Result<()> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(90);
    loop {
        let outcome = roundtrip(
            dir,
            Command::ProjectStatus {
                project_id: id.to_string(),
            },
        )
        .await?;
        let stackyard_core::Outcome::Project { project } = &outcome else {
            print_outcome(&outcome);
            return Ok(());
        };
        if project
            .services
            .iter()
            .any(|service| service.state == "starting")
        {
            if tokio::time::Instant::now() > deadline {
                return Err(stackyard_core::Error::OperationFailed(format!(
                    "project {id} stayed in starting"
                )));
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            continue;
        }
        print_outcome(&outcome);
        if project
            .services
            .iter()
            .any(|service| service.state == "failed")
        {
            return Err(stackyard_core::Error::OperationFailed(format!(
                "project {id} failed to start"
            )));
        }
        return Ok(());
    }
}

async fn wait_task(dir: &std::path::Path, started: &Outcome) -> stackyard_core::Result<()> {
    let Outcome::Task { id, .. } = started else {
        print_outcome(started);
        return Ok(());
    };
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(180);
    loop {
        let status = roundtrip(
            dir,
            Command::DepStatus {
                operation_id: id.clone(),
            },
        )
        .await?;
        if let Outcome::Task { state, .. } = &status {
            if state != "running" {
                print_outcome(&status);
                if state != "succeeded" {
                    return Err(stackyard_core::Error::OperationFailed(format!(
                        "task {state}"
                    )));
                }
                return Ok(());
            }
        }
        if tokio::time::Instant::now() > deadline {
            return Err(stackyard_core::Error::OperationFailed(
                "task stayed running".to_string(),
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
}

async fn db_command(data_dir: &std::path::Path, args: &[String]) -> stackyard_core::Result<()> {
    let dir = ensure_engine(data_dir).await?;
    let command = match args {
        [] => Command::DbList,
        [op] if op == "list" => Command::DbList,
        [op, engine] if op == "create" => Command::DbCreate {
            engine: engine.to_string(),
            port: 0,
        },
        [op, engine, port] if op == "create" => Command::DbCreate {
            engine: engine.to_string(),
            port: port.parse().map_err(|_| {
                stackyard_core::Error::InvalidMessage(format!("{port} is not a port"))
            })?,
        },
        [op, id] if op == "start" => Command::DbStart {
            instance_id: id.to_string(),
        },
        [op, id] if op == "stop" => Command::DbStop {
            instance_id: id.to_string(),
        },
        [op, id, project, name] if op == "add" => Command::DbAdd {
            instance_id: id.to_string(),
            project_id: project.to_string(),
            name: name.to_string(),
        },
        [op, id, database, path] if op == "backup" => Command::DbBackup {
            instance_id: id.to_string(),
            database: database.to_string(),
            destination: path.to_string(),
        },
        [op, id, database, source, into] if op == "restore" => Command::DbRestore {
            instance_id: id.to_string(),
            database: database.to_string(),
            source: source.to_string(),
            into: into.to_string(),
        },
        [op, project, id] if op == "detach" => Command::DbDetach {
            project_id: project.to_string(),
            connection_id: id.to_string(),
        },
        _ => {
            return Err(stackyard_core::Error::InvalidMessage(
                "usage: stackyard db list|create|start|stop|add|backup|restore|detach".to_string(),
            ))
        }
    };
    print_outcome(&roundtrip_within(&dir, command, std::time::Duration::from_secs(180)).await?);
    Ok(())
}

fn print_help() {
    println!("Scoped dependency/PHP operations accept --scope-id ID; legacy --scope DIR retains its path meaning.");
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
         stackyard remove PACKAGE_ID\n  \
         stackyard import PATH\n  \
         stackyard projects\n  \
         stackyard project ID\n  \
         stackyard start ID\n  \
         stackyard stop-project ID\n  \
         stackyard restart ID\n  \
         stackyard cancel ID\n  \
         stackyard logs ID SERVICE\n  \
         stackyard exec ID SERVICE -- ARG...\n  \
         stackyard deps install ID [--scope DIR]\n  \
         stackyard deps add ID PACKAGE [--scope DIR]\n  \
         stackyard deps remove ID PACKAGE [--scope DIR]\n  \
         stackyard deps update ID PACKAGE [--scope DIR]\n  \
         stackyard deps list ID [--scope DIR]\n  \
         stackyard deps recover ID [--scope DIR]\n  \
         stackyard deps cancel TASK\n  \
         stackyard preview ROOT [DRAFT_FILE]\n  \
         stackyard create recipes\n  \
         stackyard create plan PARENT NAME RECIPE MANAGER installed:ID|catalog:ID [TOOL]\n  \
         stackyard create apply PLAN_FILE [--wait]\n  \
         stackyard create status TASK_ID | create cancel TASK_ID\n  \
         stackyard select-scope PROJECT SCOPE_ID PACKAGE_ID\n  \
         stackyard php status PROJECT\n  \
         stackyard php profile PROJECT [EXTENSION...]\n  \
         stackyard php install-extension PROJECT CATALOG_ID\n  \
         stackyard memcached-probe NUMERIC_IP PORT\n  \
         stackyard cache list\n  \
         stackyard cache create PACKAGE_ID [PORT]\n  \
         stackyard cache start INSTANCE\n  \
         stackyard cache stop INSTANCE\n  \
         stackyard cache forget INSTANCE\n  \
         stackyard diagnostics [PATH]\n  \
         stackyard updates\n  \
         stackyard cleanup\n  \
         stackyard note-external PACKAGE VERSION\n  \
         stackyard sites\n  \
         stackyard publish ID\n  \
         stackyard unpublish ID\n  \
         stackyard apache restart\n  \
         stackyard ports HTTP HTTPS\n  \
         stackyard trust install DIRECTORY\n  \
         stackyard trust remove DIRECTORY\n  \
         stackyard hosts-write PATH\n  \
         stackyard db list\n  \
         stackyard db create mysql|postgres [PORT]\n  \
         stackyard db start ID\n  \
         stackyard db stop ID\n  \
         stackyard db add ID PROJECT NAME\n  \
         stackyard db backup ID DATABASE PATH\n  \
         stackyard db restore ID DATABASE SRC INTO\n  \
         stackyard db detach PROJECT ID\n  \
         stackyard sqlite PROJECT PATH\n\n\
         --data-dir PATH   state directory (default: {})\n\n\
         status, get, put, stop, and package commands start the engine when it is not running.\n\
         serve runs it in the foreground. A second serve exits because the endpoint is already owned.\n\
         use-existing records a path and does not copy or modify that installation.",
        default.display()
    );
}

fn print_outcome(outcome: &Outcome) {
    match outcome {
        Outcome::Recipes { recipes } => create_cli::print_json(recipes),
        Outcome::CreatePlan { plan } => create_cli::print_json(plan),
        Outcome::Activity { operations, tasks } => {
            for item in operations.iter().chain(tasks) {
                print_outcome(item);
            }
        }
        Outcome::FollowLogs { text, .. } => print!("{text}"),
        Outcome::ProjectPreview { preview } => println!(
            "{}",
            serde_json::to_string_pretty(preview)
                .unwrap_or_else(|_| "preview serialization failed".into())
        ),
        Outcome::Caches { instances } => {
            if instances.is_empty() {
                println!("caches: none");
            }
            for instance in instances {
                println!(
                    "cache: {} {} 127.0.0.1:{} {} pid={} {}",
                    instance.id,
                    instance.package_id,
                    instance.port,
                    instance.state,
                    instance
                        .pid
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "-".into()),
                    instance.detail
                );
            }
        }
        Outcome::Php { view } => {
            println!(
                "PHP {} {} {}",
                view.identity.version, view.identity.target, view.identity.extension_build
            );
            println!(
                "profile: {}",
                view.profile
                    .as_ref()
                    .map(|p| p.revision.as_str())
                    .unwrap_or("isolated -n (no shared extensions)")
            );
            for artifact in &view.inventory {
                let enabled = view.profile.as_ref().is_some_and(|profile| {
                    profile.extensions.iter().any(|item| item.id == artifact.id)
                });
                println!(
                    "extension: {} {} {}",
                    artifact.name,
                    artifact.version,
                    if enabled { "enabled" } else { "disabled" }
                );
            }
            println!("{}", view.limitations);
        }
        Outcome::Memcached { probe } => println!(
            "Memcached {} {}:{} set_get={} cleanup={}",
            probe.version,
            probe.endpoint.address,
            probe.endpoint.port,
            probe.roundtrip_verified,
            probe.cleanup_verified
        ),
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
            total,
            message,
        } => {
            println!("operation: {id}");
            println!("catalog_id: {catalog_id}");
            println!("state: {state}");
            println!("bytes: {bytes}");
            println!("total: {total}");
            println!("message: {message}");
        }
        Outcome::Project { project } => print_project(project),
        Outcome::Projects { projects } => {
            if projects.is_empty() {
                println!("none");
            }
            for project in projects {
                print_project(project);
            }
        }
        Outcome::Logs {
            project_id,
            service_id,
            offset,
            next_offset,
            text,
        } => {
            println!("project: {project_id}");
            println!("service: {service_id}");
            println!("offset: {offset}");
            println!("next_offset: {next_offset}");
            println!("{text}");
        }
        Outcome::Task {
            id,
            label,
            state,
            message,
        } => {
            println!("task: {id}");
            println!("label: {label}");
            println!("state: {state}");
            println!("{message}");
        }
        Outcome::Front {
            state,
            pid,
            http_port,
            https_port,
            restart_impact,
            ca_path,
            resolves_localhost,
            sites,
        } => {
            println!(
                "apache: {state} pid={}",
                pid.map(|pid| pid.to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
            println!("http_port: {http_port}");
            println!("https_port: {https_port}");
            println!("ca: {ca_path}");
            println!(
                "localhost_resolves: {}",
                if *resolves_localhost { "yes" } else { "no" }
            );
            println!("{restart_impact}");
            if sites.is_empty() {
                println!("sites: none");
            }
            for site in sites {
                println!(
                    "site: {} {} {} {} {}",
                    site.project_id, site.host, site.mode, site.http_url, site.https_url
                );
            }
        }
        Outcome::Databases { view } => {
            println!("{}", view.message);
            if view.instances.is_empty() {
                println!("instances: none");
            }
            for instance in &view.instances {
                println!(
                    "instance: {} {} 127.0.0.1:{} {}",
                    instance.id, instance.engine, instance.port, instance.state
                );
            }
            for binding in &view.bindings {
                println!(
                    "binding: {} {} {} external={}",
                    binding.project_id,
                    binding.connection_id,
                    binding.database_name,
                    binding.external
                );
            }
            for file in &view.sqlite {
                println!("sqlite: {} {}", file.project_id, file.path);
            }
        }
        Outcome::Diagnostics { report } => {
            println!("{report}");
        }
        Outcome::Updates { notices } => {
            if notices.is_empty() {
                println!("updates: none");
            }
            for notice in notices {
                println!("{}: {} — {}", notice.kind, notice.subject, notice.detail);
            }
        }
    }
}

fn print_project(project: &stackyard_core::ProjectView) {
    println!("project: {} {}", project.id, project.root);
    for service in &project.services {
        println!(
            "service: {} {} pid={} port={} {}",
            service.id,
            service.state,
            service
                .pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "-".to_string()),
            service
                .port
                .map(|port| port.to_string())
                .unwrap_or_else(|| "-".to_string()),
            service.detail
        );
    }
}
