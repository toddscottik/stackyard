use std::fs::File;
use std::io::Read;
use std::path::Path;

use stackyard_core::create::{plan_create, ComponentChoice, CreatePlan, CreateRequest};
use stackyard_core::{ensure_engine, roundtrip, Command, Error, Result};

pub async fn run(data_dir: &Path, args: &[String]) -> Result<()> {
    match args {
        [action] if action == "recipes" => print_json(&stackyard_core::recipes::recipes()),
        [action, parent, name, recipe, manager, runtime, tool @ ..]
            if action == "plan" && tool.len() <= 1 =>
        {
            let request = CreateRequest {
                parent: parent.clone(),
                name: name.clone(),
                recipe: recipe.clone(),
                manager: manager.clone(),
                runtime: choice(runtime)?,
                tool: tool.first().map(|value| choice(value)).transpose()?,
            };
            print_json(&plan_create(data_dir, &request)?);
        }
        [action, file, flags @ ..]
            if action == "apply" && (flags.is_empty() || flags == ["--wait"]) =>
        {
            let plan = read_plan(file)?;
            let dir = ensure_engine(data_dir).await?;
            let started = roundtrip(&dir, Command::ApplyCreate { plan }).await?;
            super::print_outcome(&started);
            if !flags.is_empty() {
                super::wait_task(&dir, &started).await?;
            }
        }
        [action, id] if action == "status" || action == "cancel" => {
            let dir = ensure_engine(data_dir).await?;
            let command = if action == "status" {
                Command::DepStatus {
                    operation_id: id.clone(),
                }
            } else {
                Command::DepCancel {
                    operation_id: id.clone(),
                }
            };
            super::print_outcome(&roundtrip(&dir, command).await?);
        }
        _ => {
            return Err(Error::InvalidMessage(
                "Use create recipes, create plan PARENT NAME RECIPE MANAGER installed:ID|catalog:ID [TOOL], create apply PLAN_FILE [--wait], create status TASK_ID or create cancel TASK_ID".into(),
            ));
        }
    }
    Ok(())
}

fn choice(value: &str) -> Result<ComponentChoice> {
    match value.split_once(':') {
        Some((kind @ ("installed" | "catalog"), id)) if !id.is_empty() => Ok(ComponentChoice {
            kind: kind.into(),
            id: id.into(),
        }),
        _ => Err(Error::InvalidMessage(
            "Use installed:ID or catalog:ID for component choices".into(),
        )),
    }
}

fn read_plan(path: &str) -> Result<CreatePlan> {
    const LIMIT: u64 = 32_768;
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| Error::io("open reviewed creation plan", error))?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| Error::io("read reviewed creation plan", error))?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::InvalidMessage("Creation plan exceeds 32 KiB".into()));
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| Error::InvalidMessage(format!("Invalid creation plan: {error}")))
}

pub fn print_json(value: &impl serde::Serialize) {
    // These types are serialized with JSON-only fields.
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("creation JSON")
    );
}
