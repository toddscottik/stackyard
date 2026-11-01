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

async fn deps_command(dir: &std::path::Path, args: &[String]) -> stackyard_core::Result<()> {
    let (scope, positional) = split_scope(args)?;
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
    print_outcome(&roundtrip(&dir, command).await?);
    Ok(())
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
