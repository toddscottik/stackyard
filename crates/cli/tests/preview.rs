use std::{fs, process::Command};

#[test]
fn cold_cli_preview_does_not_start_engine_or_create_state() {
    let root = stackyard_core::disposable_dir("cli-preview");
    fs::create_dir_all(&root).unwrap();
    let draft="schema=2\nname='draft'\n[[scope]]\nid='api'\ndirectory='.'\nruntime='python'\nmanager='pip'\n[[service]]\nid='jobs'\nscope='api'\ncommand=['python','jobs.py']\nreadiness={kind='process',grace_ms=100}\n";
    fs::write(root.join("stackyard.toml"), draft).unwrap();
    let state = root.join("absent-engine");
    let output = Command::new(env!("CARGO_BIN_EXE_stackyard"))
        .arg("--data-dir")
        .arg(&state)
        .arg("preview")
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["can_apply"], false);
    assert_eq!(value["scopes"][0]["id"], "api");
    assert!(value["services"][0]["endpoint"].is_null());
    assert!(value["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"] == "runtime_missing"));
    assert!(!state.exists());
    assert_eq!(
        fs::read_to_string(root.join("stackyard.toml")).unwrap(),
        draft
    );
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}
