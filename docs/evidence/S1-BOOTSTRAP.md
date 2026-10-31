# S1-BOOTSTRAP

Date: 1 November 2026.

Target: Windows 10 Pro, build 19045, x64. macOS was not used.

## Toolchain

- rustup 1.29.2
- rustc 1.99.1 (58cf8f9c3 2026-10-13), pinned in `rust-toolchain.toml`
- WebView2 runtime 156.0.4314.50, already installed
- Visual Studio 2022 Community with the MSVC toolset, already installed at `C:\Program Files\Microsoft Visual Studio\2022\Community`
- Node v24.13.0 and npm 11.6.2, already installed

The first `rustup-init.exe -y` returned Windows error `0x80070005` (access denied) and left rustup without a default toolchain. `rustup toolchain install stable --profile minimal --component rustfmt --component clippy` then installed 1.99.1.

## What runs

The placeholder product name is Local Dev. The Cargo package name is `localdev`. Desktop and CLI call one engine over a user-only named pipe. State is SQLite schema version 1 in the data directory. Tests use disposable directories. The desktop and a normal CLI `status` use `%LOCALAPPDATA%\localdev`.

## Commands

```text
cargo test --workspace --exclude localdev-desktop
cargo clippy --workspace --exclude localdev-desktop --all-targets -- -D warnings
cargo clippy -p localdev-desktop -- -D warnings
npm --prefix apps/desktop run build
cargo check -p localdev-desktop
cargo run -q -p localdev-cli -- --data-dir .artifacts/S1-BOOTSTRAP/cli-smoke status
npm --prefix apps/desktop run dev
```

`cargo test` passed. Clippy passed with warnings denied. The frontend production build finished. `cargo check -p localdev-desktop` passed after `apps/desktop/src-tauri/icons/icon.ico` was added.

CLI smoke, same disposable directory:

- first `status`: pid 74132, schema 1, `last_event: -`
- second `status`: pid 74132
- `put display_name Ada`, then `stop`
- next `status`: pid 89508, `last_event: setting_put display_name`

Desktop launch: process `localdev-desktop` pid 76156, window title `Local Dev`. A following `localdev status` reported engine pid 91424 and data directory `C:\Users\ptiev\AppData\Local\localdev`, the same process already running for that window.

The pipe DACL test reads the security descriptor and fails if it contains Everyone (`S-1-1-0`), Authenticated Users (`S-1-5-11`), or Administrators (`S-1-5-32-544`). It expects the current user's `S-1-5-21-` sid. No second Windows account logged in and attempted a connection.

## Not done

- macOS toolchain, tests, and window
- GitHub Actions execution
- tray hide and explicit Quit behavior
- a live cross-user denial of the pipe
