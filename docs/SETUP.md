# Setup

Stackyard is a personal local development manager for Windows 10 x64 and macOS 14.5 arm64. This file is how to build and run the private tree. It is not a public installer.

## Machines

The acceptance machines are Windows 10 x64 and a MacBook with an M1 Pro running macOS 14.5. A build on this repository's Linux agent, or on a newer macOS runner, does not replace those checks. The known limits are in [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md).

## Build

Install Rust with rustup. The pinned toolchain is `rust-toolchain.toml`. The desktop also needs Node.js and the platform webview libraries (WebView2 on Windows, the system web view on macOS).

From the repository root:

```text
cargo test --workspace
cargo run -p stackyard-cli -- status
npm --prefix apps/desktop install
npm --prefix apps/desktop run dev
```

`status` starts the per-user engine if it is not already running. `stop` stops managed projects, the shared Apache, and managed database instances.

## Data on this machine

The engine state defaults to `%LOCALAPPDATA%\stackyard` on Windows and `~/Library/Application Support/stackyard` on macOS. A previous `%LOCALAPPDATA%\localdev` directory is renamed on first launch. Pass `--data-dir` to use another directory. Tests use disposable directories and do not write to the default location.

Package files live under `packages/` and `tools/`. Database files live under `databases/`. Passwords live under `secrets/` and are not printed by `diagnostics`.

## First project

Record a runtime you already have, or install a catalog package. Recording does not copy or upgrade that installation.

```text
cargo run -p stackyard-cli -- use-existing python "C:\Python\Python312\python.exe"
cargo run -p stackyard-cli -- import PATH
cargo run -p stackyard-cli -- deps install PROJECT
cargo run -p stackyard-cli -- start PROJECT
cargo run -p stackyard-cli -- diagnostics
```

`deps install` follows the project's lockfile. `deps update` changes that project's dependencies only. It does not switch the runtime. `diagnostics` prints a redacted report, or writes it when given a path.

Closing the desktop window hides it. Quit stops managed work unless Settings has background mode on.
