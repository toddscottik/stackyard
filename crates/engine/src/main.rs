use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
    }
}

fn run() -> stackyard_core::Result<()> {
    let mut args = env::args().skip(1);
    let mut data_dir = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "stackyard-engine runs the per-user engine.\n\n\
                     --data-dir PATH   state directory (default: the per-user stackyard directory)\n\
                     --help            show this text"
                );
                return Ok(());
            }
            "--data-dir" => {
                data_dir = Some(args.next().ok_or_else(|| {
                    stackyard_core::Error::InvalidMessage("--data-dir needs a path".to_string())
                })?);
            }
            other => {
                return Err(stackyard_core::Error::InvalidMessage(format!(
                    "unknown argument {other:?}"
                )));
            }
        }
    }
    let dir = data_dir
        .map(PathBuf::from)
        .unwrap_or_else(stackyard_core::default_data_dir);
    stackyard_engine::serve(&dir)
}
