# Implementation status

Updated: 4 November 2026 (planning; existing execution evidence retains its dates).

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
| 5: Apache/HTTPS | Partial. Linux x86_64 shared Apache, proxy, static rewrite, and generated HTTPS passed on 1 November 2026. Windows catalog pin is the Stage 0 zip and was not launched again. macOS Apache remains blocked | Launch the Windows catalog Apache on Windows 10. Keep macOS blocked until a relocatable arm64 archive exists. System trust and ports 80/443 stay pending |
| 6: databases | Partial. Linux x86_64 MySQL 8.0.46, PostgreSQL 16.15, and SQLite 3.45.1 passed on 1 November 2026. Windows catalog PostgreSQL was not launched again. MySQL has no SHA256 catalog entry. macOS database archives stay blocked | Launch the Windows catalog PostgreSQL on Windows 10. Leave MySQL out of the catalog until a SHA256 exists. Leave macOS blocked until an approved archive exists. Credential Manager and Keychain remain pending |
| 7: updates/recovery | Partial. Linux x86_64 notice, switch, cleanup, and interrupted-install checks passed on 1 November 2026. No second digest-checked Node or Python pin was downloaded | Download a newer checked Node or Python pin on Windows 10 and switch one project there. Leave macOS pins out until a payload check |
| 8: personal release | Partial. Linux x86_64 diagnostics, setup notes, and fixture smoke passed on 1 November 2026. Windows 10 and macOS 14.5 acceptance, the user's projects, and sleep/wake are open | Run the acceptance list on Windows 10 x64 and the M1 Pro running macOS 14.5. Measure startup there with an editor and browser. Do not treat the Linux sample as that gate |
| 8A: PHP/extensions | Not started | PHP/Composer, native extension profiles, Memcached server/client feasibility and integration |
| 9: Define project scopes and launch contracts | Not started | A shared engine/CLI contract that represents Node, Python, and PHP projects without one project-wide package manager controlling every service. |
| 10: Build the shared desktop workspace and activity layer | Not started | A maintainable desktop shell that preserves existing actions and can host the new workflows. |
| 11: Create runnable projects from the desktop | Not started | Create project produces working source and a runnable application for Node HTTP, Next.js, Flask, and FastAPI. |
| 12: Import applications and edit their configuration | Not started | Existing applications can be added without hand-writing stackyard.toml, and their configuration can be changed safely afterward. |
| 13: Unify components, dependencies and PHP extensions | Not started | Global Components and each project's Stack expose the same working runtime/tool/server and extension capabilities. |
| 14: Complete project database workflows | Not started | A project can provision or attach a database and use its credentials without manually finding secret files. |
| 15: Complete sites, HTTPS and desktop settings | Not started | Project Site and global Web/Data settings provide the real routing and host-integration controls. |
| 16: Finish desktop usability and failure recovery | Not started | All delivered workflows remain usable through keyboard, navigation, cancellation, engine reconnect and offline startup. |
| 17: Accept the integrated app on Windows and macOS | Not started | A release-candidate acceptance report for Windows 10 x64 and the M1 Pro running macOS 14.5 arm64. |
| 18: public release | Deferred; depends on 17 | Signing/notarization, distribution, updater, clean-machine release checks and license decisions |

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

## Stage 5 worker handoff

1 November 2026. Slice: S5-APACHE. Executed on Linux x86_64 (`uname` 6.12.94+, rustc 1.99.1, Apache 2.4.58 at `/usr/sbin/apache2`). This is not the Windows 10 or macOS 14.5 acceptance machine.

One generated Apache config serves every published `[site]`. `httpd -t` must pass before the live file is replaced. High ports only. A busy port is reported with other high ports and the occupant is left running. Certificates are generated in the data directory. Trust install and removal touch only `stackyard-ca.pem` in a directory the caller names. `hosts-write` rewrites only lines marked `# stackyard`. The Windows catalog now lists the Apache Lounge 2.4.69 zip from Stage 0. It was not downloaded here. macOS Apache stays out of the catalog.

Commands and outcomes on this machine:

- `cargo test --workspace --exclude stackyard-desktop` passed, including two static hosts, an `.htaccess` rewrite, proxy POST, redirect rewriting, a WebSocket upgrade, HTTPS checked with the generated CA, a refused candidate config, trust install/removal, and an occupied port.
- `cargo test -p stackyard-desktop --lib` passed. `npx tsc --noEmit` in `apps/desktop` passed.
- Chrome on the Vite page showed the Apache explanation and an Apache choice for Use existing. Publish needs the desktop engine, which that browser does not have.

Details: `docs/evidence/S5-APACHE.md` and `docs/decisions/0003-apache-sites.md`.

Next task: run the Windows catalog Apache on Windows 10 x64. Leave macOS Apache blocked until a relocatable arm64 build exists. Do not treat directory trust or this Linux `apache2` as the Windows or macOS trust and package checks.

## Stage 6 worker handoff

1 November 2026. Slice: S6-DATABASES. Executed on Linux x86_64 (`uname` 6.12.94+, rustc 1.99.1, MySQL 8.0.46 at `/usr/sbin/mysqld`, PostgreSQL 16.15 at `/usr/lib/postgresql/16/bin/postgres`, SQLite 3.45.1). This is not the Windows 10 or macOS 14.5 acceptance machine.

Managed instances live under the data directory, bind `127.0.0.1`, and keep their files when a project stops or a connection is detached. External connections have no process. SQLite is a project file copied with the backup API. `db migrate` is refused. Passwords stay in mode `0600` files and off the command line. The Windows catalog lists the PostgreSQL 18.6 zip from Stage 0. MySQL stays out because the publisher zip is MD5-only. macOS database archives stay out.

Commands and outcomes on this machine:

- `cargo test --workspace --exclude stackyard-desktop --offline` passed, including the MySQL, PostgreSQL, and SQLite restart and restore test.
- `cargo test -p stackyard-desktop --lib --offline` passed. `npx tsc --noEmit` in `apps/desktop` passed.
- Chrome on the Vite page showed the Databases section and MySQL and PostgreSQL under Use existing. Create needs the desktop engine, which that browser does not have.

Details: `docs/evidence/S6-DATABASES.md` and `docs/decisions/0004-databases.md`.

Next task: launch the Windows catalog PostgreSQL on Windows 10. Do not add MySQL to the catalog without a SHA256. Leave macOS MySQL and PostgreSQL blocked. Do not treat these distro 8.0 and 16 servers as the Windows 8.4 and 18.6 checks, and do not treat secret files as Credential Manager or Keychain.

## Stage 7 worker handoff

1 November 2026. Slice: S7-UPDATES. Executed on Linux x86_64 (`uname` 6.12.94+, rustc 1.99.1). This is not the Windows 10 or macOS 14.5 acceptance machine.

Refresh reads the bundled catalog and a news list with no download URL. A compatible runtime can be installed beside the current one and selected for one project. The previous selection is recorded and the other project is left alone. MySQL and PostgreSQL version changes stay on the migration error. An external file is not overwritten when its observed version differs. A missing launch file keeps the selection. An install journal left running loses only its incoming directory on the next engine start. Cleanup refuses a package a project still selects, including a running project. `deps recover` reinstalls from the lockfile and does not claim to roll scripts back. Application updates are a notice.

Commands and outcomes on this machine:

- `cargo test --workspace --exclude stackyard-desktop --offline` passed, including the update unit tests and `interrupted_install_is_cleared_and_a_cached_project_stays_listed`.
- `cargo test -p stackyard-desktop --lib --offline` passed. `npx tsc --noEmit` in `apps/desktop` passed.
- Chrome on the Vite page showed the Updates block. Check and cleanup need the desktop engine, which that browser does not have.

Details: `docs/evidence/S7-UPDATES.md` and `docs/decisions/0005-updates.md`.

Next task: add a second digest-checked Node or Python pin only after the payload is verified, then download it on Windows 10 and switch one project. Do not fetch an unchecked upstream release into the catalog. Leave macOS pins out until a payload check.

## Stage 8 worker handoff

1 November 2026. Slice: S8-PERSONAL. Executed on Linux x86_64 (`uname` 6.12.94+, rustc 1.99.1, Node v22.23.4, npm 10.9.9, pnpm 12.16.1, Python 3.12.3). This is not the Windows 10 or macOS 14.5 acceptance machine. `uv` is not installed here.

`diagnostics` lists projects, packages, and databases and strips secret file contents, private keys, and setting values. Setup is `docs/SETUP.md`. Limits are `docs/KNOWN_LIMITATIONS.md`. A fresh `stackyard status` returned in 0.106 seconds. The engine RSS was 11268 kB. Chrome was running; an editor was not. `fixtures/node-http` served `node-http` with npm. `fixtures/flask-app` served `flask-ok` from an app-owned pip environment. `fixtures/next-app` refused pnpm 10.33.4 because `package.json` asks for pnpm 12.16.1, then served HTML with pnpm 12.16.1 and `--frozen-lockfile`.

Commands and outcomes on this machine:

- `cargo test --workspace --exclude stackyard-desktop --offline` passed, including the diagnostics redaction test.
- `cargo test -p stackyard-desktop --lib --offline` passed. `npx tsc --noEmit` in `apps/desktop` passed.
- Chrome on the Vite page showed Settings → Diagnostics. Create needs the desktop engine, which that browser does not have.

Details: `docs/evidence/S8-PERSONAL.md` and `docs/decisions/0006-personal-release.md`.

Next task: run this acceptance list on Windows 10 x64 and the M1 Pro running macOS 14.5, including the user's projects, sleep and wake, and a startup measurement with an editor and browser. Do not mark Stage 8 passed from the Linux sample.

## Planning clarification — 4 November 2026

Stages 0–8 remain the implemented baseline; pending target acceptance is tracked separately. Stage 8A remains next. The former broad Stage 9 is split into Stages 9–17 with numbered tasks and acceptance in DEVELOPMENT_PLAN.md; public release is now Stage 18 and requires Stage 17 acceptance. Runnable Node, Next.js, Flask and FastAPI starters are delivered in Stage 11. See UI_REWORK_REVIEW.md and WORK_QUEUE.md. Planning only; no new implementation or runtime acceptance is claimed.
