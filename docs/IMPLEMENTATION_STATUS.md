# Implementation status

Updated: 31 October 2026.

## Completed preparation

- Initialized local Git on `main`; added repository instructions, ignore/format conventions, a parallel-agent workflow, and a bounded initial work queue. No application implementation or runtime installation performed in this setup task.

- Product research and architectural review recorded.
- Personal-use scope and both target machines identified.
- Current Windows build/architecture and existing Node/Python execution checked.
- Published distribution sources checked; Node/uv/Python metadata and digests recorded.
- Ordered development stages and agent acceptance criteria written.
- Added explicit language-tool/project dependency management (npm/pnpm/pip/uv), plus PHP/Composer/extensions and Memcached acceptance in Stage 8A. No new provider implementation or artifact tests performed.

## Stage tracking

| Stage | Status | Next concrete work |
| --- | --- | --- |
| 0: package feasibility | Partial. Windows runtime and server probes accepted 31 October 2026. Mac metadata is recorded and every Mac launch is pending or blocked | Run `scripts/verification/macos/packages.sh` on the M1 Pro. Resolve macOS Apache and MySQL archives before approving those providers |
| 1: foundation | Partial. Windows skeleton runs as of 1 November 2026. Mac toolchain and desktop open are pending | Finish the Mac toolchain check and open the desktop there. Then Stage 2 package providers |
| 2: packages | Partial. Windows Node, Python, uv, and pnpm can be installed or referenced as of 1 November 2026. macOS packages are not in the catalog | Run the same install and selection flow on the M1 Pro after the Mac catalog pins are payload-checked |
| 3: project runner | Partial. Linux x86_64 process and dependency checks passed on 1 November 2026. Windows 10 and macOS 14.5 launches are pending | Run the same project and dependency flows on Windows 10 x64 and the M1 Pro |
| 4: desktop workflows | Partial. Desktop screens and tray policy are implemented as of 1 November 2026. A live window was not opened on Windows 10 or macOS 14.5 | Open the desktop on those machines and run one project through import, install, start, logs, restart, and stop |
| 5: Apache/HTTPS | Not started | Shared virtual hosts, proxying, trust setup |
| 6: databases | Not started | MySQL then PostgreSQL providers, SQLite and backup/restore |
| 7: updates/recovery | Not started | Side-by-side updates and interrupted-operation recovery |
| 8: personal release | Not started | Real-project acceptance on Windows and Mac |
| 8A: PHP/extensions | Not started | PHP/Composer, native extension profiles, Memcached server/client feasibility and integration |
| 9: public release | Deferred | Distribution/license decisions, release pipeline and expanded coverage |

## Open evidence gaps

- Windows Apache, MySQL, and PostgreSQL launches are accepted with two integrity limits: MySQL 8.4.13 was checked with the publisher MD5 sidecar because no SHA256 sidecar was published, and PostgreSQL 18.6 ran on Windows 10 even though the upstream table names Windows Server. Details: `docs/evidence/S0-WIN-SERVERS.md`.
- No execution on the Mac yet. User has confirmed testing availability. S0-MAC-PACKAGES re-fetched metadata from Windows on 31 October 2026 and left every launch check pending.
- No approved standalone Apache package for macOS 14.5 arm64. The Mac research found source and Windows binaries only, and no Homebrew Sonoma bottle. A relocatable arm64 archive is still a packaging gate.
- No selected/approved MySQL package for macOS 14.5 arm64. The downloads selector was not completed, so no archive URL is recorded.
- PostgreSQL on macOS: `postgresql-18.6-5-osx-binaries.zip` was observed with no published digest and no arm64 marker in the filename. The Windows x64 18.6 zip did launch on this machine.
- Rust 1.99.1 is installed with rustup on this Windows machine. WebView2 runtime 156.0.4314.50 and Visual Studio 2022 Community C++ tools were already present. The Mac Rust toolchain and Xcode Command Line Tools are still unchecked. Managed uv 0.13.6 remains an artifact-directory helper, not a system-shell install.
- Windows pnpm 12.16.1 and pip (`six==1.17.0` in a disposable venv) smoke checks passed. PHP extension and Memcached native artifacts remain unverified. macOS pnpm/pip execution remains pending.

## Handoff format for future work

Append: date; stage/slice; files changed; exact tests/commands; target OS/architecture; outcomes; unresolved issues; next task. Link detailed logs where needed. Do not mark an OS passed based only on compilation or vendor documentation.

## Stage 0 coordinator handoff

31 October 2026. Slice: S0-INTEGRATE after S0-WIN-RUNTIMES, S0-WIN-SERVERS, and S0-MAC-PACKAGES.

Files changed by the coordinator: `docs/WORK_QUEUE.md`, `docs/IMPLEMENTATION_STATUS.md`, `docs/PACKAGE_VERIFICATION.md`. Probe files remain `scripts/verification/windows/runtimes.ps1`, `scripts/verification/windows/servers.ps1`, `scripts/verification/macos/packages.sh`, and `docs/evidence/S0-*.md`. Downloads stay in gitignored `.artifacts/`.

Commands reviewed, not re-run in full: `runtimes.ps1 -Action Verify` and `servers.ps1 -Action Verify` on Windows 10 Pro build 19045 x64, both previously exit 0. Spot-checks: relocated Node `v24.22.2`, relocated Python `3.12.16`, and matching SHA256 or MD5 for the Node, Apache, MySQL, and PostgreSQL archives.

Outcome: Windows combinations in the package verification integration table are accepted with the limitations recorded there. macOS launches, macOS Apache, macOS MySQL, and the unsigned Mac PostgreSQL zip stay pending or blocked. Stage 0 is not fully complete.

Next task: S1-BOOTSTRAP. Inspect the C++ build tools and WebView2, install Rust/Cargo through its normal installer, and add the workspace skeleton. Mac package execution stays a separate blocked task.

## Stage 1 coordinator handoff

1 November 2026. Slice: S1-BOOTSTRAP on Windows 10 Pro build 19045 x64.

Rustup 1.29.2 installed the stable toolchain. `rustc 1.99.1 (58cf8f9c3 2026-10-13)` is pinned in `rust-toolchain.toml`. WebView2 runtime `156.0.4314.50` and Visual Studio 2022 Community with the C++ tools component were already installed. The first `rustup-init` attempt returned Windows access denied; a following `rustup toolchain install stable` succeeded.

Commands and outcomes:

- `cargo test --workspace --exclude localdev-desktop` passed, including schema reopen, a second engine refusing the same data directory, settings surviving restart, and a Windows pipe DACL check that does not grant Everyone, Authenticated Users, or Administrators.
- `cargo clippy --workspace --exclude localdev-desktop --all-targets -- -D warnings` passed. `cargo clippy -p localdev-desktop -- -D warnings` passed.
- `npm --prefix apps/desktop run build` passed.
- `cargo check -p localdev-desktop` passed after adding `apps/desktop/src-tauri/icons/icon.ico`.
- CLI smoke against `.artifacts/S1-BOOTSTRAP/cli-smoke`: two `status` calls reported pid 74132; after `stop` and a new `status`, pid 89508 and `last_event` was `setting_put display_name`.
- `npm --prefix apps/desktop run dev` launched `localdev-desktop.exe` with window title `Local Dev`. `localdev status` then reported the same engine pid, 91424, and data directory `%LOCALAPPDATA%\localdev`.

The product name is now Stackyard. The Stage 1 records below used the placeholder name `localdev`. Tray hide and explicit Quit policy are still Stage 4. A second Windows account was not used to try the pipe. GitHub Actions has not been run. macOS 14.5 has not built or opened the desktop.

## Stage 2 coordinator handoff

1 November 2026. Slice: S2-PACKAGES on Windows 10 Pro build 19045 x64.

The runnable catalog is `catalog/packages.json`. Windows Node 24.22.2, CPython 3.12.16, uv 0.13.6, and pnpm 12.16.1 downloaded into `.artifacts/S2-PACKAGES`, matched by the installer digest check, and launched. Existing Python 3.12.3 was recorded and left at `C:\Python\Python312\python.exe`. Removing that reference was refused. Details: `docs/evidence/S2-PACKAGES.md`.

Next task: payload-check the macOS Node, Python, uv, and pnpm pins on the M1 Pro and add them to the catalog only after that. Server packages stay later.

## Stage 3 coordinator handoff

1 November 2026. Slice: S3-PROJECTS. Executed on Linux x86_64 (`uname` 6.12.94+, rustc 1.99.1). This is not the Windows 10 or macOS 14.5 acceptance machine.

Shareable project intent is `stackyard.toml`. The engine database stores the project root and observed process state. Unix services run in their own process group. Windows code assigns a job object that ends the tree when the handle closes; that path was not launched here.

Commands and outcomes on this machine:

- `cargo test -p stackyard-core -p stackyard-engine -p stackyard-cli` passed, including two projects on different runtimes, hot reload, child cleanup, partial start rollback, cancel, occupied port, bounded restart, crash recovery that leaves an unrelated process alive, npm versus pnpm lockfile choice, and pip environment isolation.
- CLI smoke with Node v22.23.4 and Python 3.12.3: `fixtures/node-http` installed with npm 10.9.9 and served `node-http`, then `reloaded`. `fixtures/flask-app` installed Flask 3.1.3 into an app-owned virtual environment and served `flask-ok`. `fixtures/fastapi-app` installed FastAPI 0.162.0 and uvicorn 0.54.0; `--reload` changed the response from `{"ok":true}` to `{"ok":"reloaded"}`.
- `fixtures/next-app` installed with pnpm 12.16.1 and served a page containing `stackyard-next` on `127.0.0.1`. The generated `pnpm-lock.yaml` is committed. uv was not installed here, so `uv sync` was not executed; lockfile detection is covered by a unit test.

Details: `docs/evidence/S3-PROJECTS.md`.

Next task: repeat the Stage 3 project and dependency smoke on Windows 10 x64 and macOS 14.5 arm64. Mac package catalog pins stay blocked on a payload check.
