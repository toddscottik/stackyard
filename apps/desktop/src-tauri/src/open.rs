use std::path::{Path, PathBuf};
use std::process::Command;

pub fn quit_stops_projects(background_mode: Option<&str>) -> bool {
    background_mode != Some("on")
}

pub fn loopback_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/")
}

pub fn accept_editor(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    let value = if value.is_empty() { "code" } else { value };
    let allowed = value.chars().all(|c| {
        !c.is_control() && !matches!(c, '&' | '|' | ';' | '<' | '>' | '`' | '$' | '\n' | '\r')
    });
    if !allowed || value.len() > 260 {
        return Err("editor must be a program name or path, without shell syntax".to_string());
    }
    Ok(value.to_string())
}

pub fn shell_program(root: &Path) -> (String, Vec<String>) {
    if cfg!(windows) {
        (
            "cmd.exe".to_string(),
            vec!["/D".to_string(), "/K".to_string()],
        )
    } else if cfg!(target_os = "macos") {
        (
            "open".to_string(),
            vec![
                "-a".to_string(),
                "Terminal".to_string(),
                root.display().to_string(),
            ],
        )
    } else if Path::new("/usr/bin/gnome-terminal").is_file() {
        (
            "gnome-terminal".to_string(),
            vec![format!("--working-directory={}", root.display())],
        )
    } else {
        ("x-terminal-emulator".to_string(), Vec::new())
    }
}

pub fn folder_program(path: &Path) -> (String, Vec<String>) {
    if cfg!(windows) {
        ("explorer".to_string(), vec![path.display().to_string()])
    } else if cfg!(target_os = "macos") {
        ("open".to_string(), vec![path.display().to_string()])
    } else {
        ("xdg-open".to_string(), vec![path.display().to_string()])
    }
}

pub fn spawn_detached(program: &str, args: &[String], cwd: Option<&Path>) -> Result<(), String> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let child = command.spawn().map_err(|err| {
        format!(
            "could not start {program} for {}: {err}",
            cwd.unwrap_or(Path::new(program)).display()
        )
    })?;
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });
    Ok(())
}

pub fn canonical_project_root(root: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(root);
    path.canonicalize()
        .map_err(|err| format!("could not open {}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_keeps_projects_only_when_background_mode_is_on() {
        assert!(quit_stops_projects(None));
        assert!(quit_stops_projects(Some("off")));
        assert!(!quit_stops_projects(Some("on")));
    }

    #[test]
    fn editor_rejects_shell_syntax_and_defaults_to_code() {
        assert_eq!(accept_editor("").expect("default"), "code");
        assert_eq!(accept_editor("  code ").expect("trim"), "code");
        assert!(accept_editor("code & calc").is_err());
        assert!(accept_editor("code\nrm").is_err());
    }

    #[test]
    fn loopback_url_stays_on_localhost() {
        assert_eq!(loopback_url(4312), "http://127.0.0.1:4312/");
    }

    #[test]
    fn shell_arguments_do_not_include_a_shell_string() {
        let root = Path::new("/tmp/my projects");
        let (program, args) = shell_program(root);
        assert!(!program.contains(' '));
        assert!(args.iter().all(|arg| !arg.contains('&')));
        if cfg!(windows) {
            assert_eq!(program, "cmd.exe");
            assert_eq!(args, ["/D", "/K"]);
        } else if cfg!(target_os = "macos") {
            assert_eq!(program, "open");
            assert!(args.iter().any(|arg| arg == "Terminal"));
        } else {
            assert!(program == "gnome-terminal" || program == "x-terminal-emulator");
        }
        let _ = root;
    }
}
