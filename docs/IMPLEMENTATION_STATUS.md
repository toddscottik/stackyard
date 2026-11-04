# Implementation status

Updated: 5 November 2026 (Stage 9 Windows implementation and acceptance; historical execution retains its dates).

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
| 0: package feasibility | Partial. Windows probes accepted 31 October. Mac Node/Python/uv/pnpm payload and native launch evidence recorded 3 November in S0-MAC-PACKAGES | Resolve Mac server artifacts; preserve the accepted runtime pins. No Mac is available in the current session |
| 1: foundation | Partial. Windows skeleton runs as of 1 November 2026. Mac toolchain and desktop open are pending | Finish the Mac toolchain check and open the desktop there. Then Stage 2 package providers |
| 2: packages | Partial. Windows packages supported; Mac Node/Python/uv/pnpm entered the catalog after 3 November payload/launch checks | Complete Mac desktop selection/relocation/cleanup checks; basic payload launches do not prove the full workflow |
| 3: project runner | Partial. Linux x86_64 process and dependency checks passed on 1 November 2026. Windows 10 and macOS 14.5 launches are pending | Run the same project and dependency flows on Windows 10 x64 and the M1 Pro |
| 4: desktop workflows | Partial. Desktop screens and tray policy are implemented as of 1 November 2026. A live window was not opened on Windows 10 or macOS 14.5 | Open the desktop on those machines and run one project through import, install, start, logs, restart, and stop |
| 5: Apache/HTTPS | Partial. Linux x86_64 shared Apache, proxy, static rewrite, and generated HTTPS passed on 1 November 2026. Windows catalog pin is the Stage 0 zip and was not launched again. macOS Apache remains blocked | Launch the Windows catalog Apache on Windows 10. Keep macOS blocked until a relocatable arm64 archive exists. System trust and ports 80/443 stay pending |
| 6: databases | Partial. Linux x86_64 MySQL 8.0.46, PostgreSQL 16.15, and SQLite 3.45.1 passed on 1 November 2026. Windows catalog PostgreSQL was not launched again. MySQL has no SHA256 catalog entry. macOS database archives stay blocked | Launch the Windows catalog PostgreSQL on Windows 10. Leave MySQL out of the catalog until a SHA256 exists. Leave macOS blocked until an approved archive exists. Credential Manager and Keychain remain pending |
| 7: updates/recovery | Partial. Linux x86_64 notice, switch, cleanup, and interrupted-install checks passed on 1 November 2026. No second digest-checked Node or Python pin was downloaded | Download a newer checked Node or Python pin on Windows 10 and switch one project there. Leave macOS pins out until a payload check |
| 8: personal release | Partial. Linux x86_64 diagnostics, setup notes, and fixture smoke passed on 1 November 2026. Windows 10 and macOS 14.5 acceptance, the user's projects, and sleep/wake are open | Run the acceptance list on Windows 10 x64 and the M1 Pro running macOS 14.5. Measure startup there with an editor and browser. Do not treat the Linux sample as that gate |
| 8A: PHP/extensions | Windows deliverables/native acceptance completed 4 November: PHP patches/Composer, FastCGI, live profiles, memcached extension and guarded native server. Mac pending | Repeat native checks on macOS 14.5 when available; continue independent Stage 9 now |
| 9: Define project scopes and launch contracts | Windows implementation/acceptance complete 5 November: schema 2 scopes, migration 8, typed readiness, pure preview and client adapters. Mac pending | Repeat native scope/readiness checks on macOS 14.5 when available; Stage 10 desktop workspace is ready to begin |
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
- Mac runtime payload/launch evidence exists from 3 November in `docs/evidence/S0-MAC-PACKAGES.md`. This session has no Mac access (user confirmed 4 November). Integrated desktop/server/PHP checks remain pending.
- No approved standalone Apache package for macOS 14.5 arm64. The Mac research found source and Windows binaries only, and no Homebrew Sonoma bottle. A relocatable arm64 archive is still a packaging gate.
- No selected/approved MySQL package for macOS 14.5 arm64. The downloads selector was not completed, so no archive URL is recorded.
- PostgreSQL on macOS: `postgresql-18.6-5-osx-binaries.zip` was observed with no published digest and no arm64 marker in the filename. The Windows x64 18.6 zip did launch on this machine.
- Rust 1.99.1 is installed with rustup on this Windows machine. WebView2 runtime 156.0.4314.50 and Visual Studio 2022 Community C++ tools were already present. Mac Rust 1.99.1 is recorded in the 3 November package evidence; Xcode Command Line Tools and desktop build/open acceptance remain pending. Managed uv 0.13.6 remains an artifact-directory helper, not a system-shell install.
- Windows PHP 8.5.11/8.5.12 and Composer 2.11.1 have publisher SHA256 checks and native launches. ServBay's publisher-verified archive supplies memcached 3.4.0; the missing PECL checksum was not invented. Memcached 1.6.8 raw TCP remains forbidden; its guarded private-socket provider passed native exclusivity/lifecycle and PHP set/get. Mac PHP/cache execution remains pending. Prior Mac pnpm/pip runtime evidence is retained.

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

## Stage 8A coordinator handoff — 4 November 2026

Target: Windows 10 Pro build 19045 x64, Rust 1.99.1, PHP 8.5.12 NTS VS17, Composer 2.11.1, Apache Lounge 2.4.69 VS18, Windows curl 8.13.0. Three disjoint workers handled approved artifacts, profile services/native acceptance, and Memcached feasibility; the coordinator reviewed and integrated shared interfaces. The artifact worker also corrected Windows fixture/startup regressions. No Mac is available now (user confirmed); earlier Mac runtime evidence is retained.

Changed areas: core PHP/extension/Memcached modules; manifest, package/dependency/selection and archive adapters; engine routing/environment handling/Apache config; CLI and Tauri invokes; Windows probes; PHP HTTP fixture and lockfile; integration tests and documentation. No root dependencies or lockfile versions changed. Details and exact commands: [S8A-INTEGRATE](evidence/S8A-INTEGRATE.md), [S8A-ARTIFACTS](evidence/S8A-ARTIFACTS.md), [S8A-PROFILES](evidence/S8A-PROFILES.md), [S8A-MEMCACHED](evidence/S8A-MEMCACHED.md), [S8A-WINDOWS-REGRESSIONS](evidence/S8A-WINDOWS-REGRESSIONS.md). Architecture: [0007](decisions/0007-php-profiles.md).

Demonstrated: publisher-digest-checked managed PHP and Composer relocation, existing reference, two isolated CLI/HTTP extension profiles, failed/incompatible load rollback, offline local Composer dependency install/platform checks/lockfile reuse, persistent project data through stop/restart and engine restart, real Apache HTTP and generated-CA HTTPS proxying, busy-port preservation and owned-process cleanup. Fixed Windows Apache mutex/module-order defects and a reproducible engine-autostart inherited-handle hang; captured PowerShell now returns while the same per-user engine stays alive. No trust/hosts/PATH/OS-service changes or user projects/data were used.

Validation: `cargo test --workspace --offline` passed with real PHP/Composer/Apache variables enabled, including 62 core tests, native package/project tests and new captured-client startup regression. `cargo clippy --workspace --all-targets --offline -- -D warnings`, `pnpm --dir apps/desktop run build`, `git diff --check`, JSON/TOML and new relative-document links passed. The full real CLI PHP workflow with automatic engine startup passed in 5.41 seconds. Native Memcached verification intentionally failed occupied-port refusal; its candidate is excluded from the catalog, not counted as passing managed support. Initial intermittent cold/concurrent project-test startup timeouts remain documented; later complete/focused runs passed without weakening lifecycle assertions.

Remaining: verified PHP memcached artifact/build plus safe native managed Memcached and real PHP client set/get; Apache FastCGI/document-root integration; affected-worker-only profile restart; second native PHP patch switch; Mac launches and integrated desktop acceptance. The extension download catalog stays empty until integrity/ABI/native dependency checks pass. Stage 8A is partial and Stages 10–18 remain open.

Next task: finish the missing 8A native provider slice using a verified build/artifact route, then review/freeze the preparatory [9.1 scope contract](decisions/0008-project-scopes.md) and assign 9.2–9.4. The 9.1 audit/examples are a proposal, not implemented scopes or migration acceptance. Do not mark the development plan complete from this handoff.

## Resumed Stage 8A Windows acceptance — 4 November 2026

The user directed continued work without waiting for tomorrow's Mac check. Remaining Windows providers now have real native evidence: [FastCGI](evidence/S8A-FASTCGI.md), [PHP-only rollout/rollback](evidence/S8A-RECONFIGURE.md), [native extension/client](evidence/S8A-NATIVE-CACHE.md), [guarded managed server](evidence/S8A-CACHE-PROVIDER.md). [Decision 0009](decisions/0009-guarded-native-cache.md) records provenance and transport/ownership boundaries.

Target Windows 10 Pro build 19045 x64, Rust 1.99.1: official PHP 8.5.11/8.5.12, Composer 2.11.1, Apache 2.4.69, ServBay-built memcached 3.4.0/libmemcached-awesome 1.1.4/libhashkit, native jefyt Memcached 1.6.8. The engine holds an exclusive loopback TCP listener and relays bytes to a private native socket; direct backend TCP mode is never used. Schema 7 preserves existing state and adds cache bindings protected from package cleanup. Pinned socket2 0.6.5 already existed transitively; it is now a direct transport dependency.

Canonical cargo test --workspace --offline passed with every native gate enabled: 64 core tests, real package install, seven native cache lifecycle cases, FastCGI document roots/rewrites/POST/upload/HTTPS/fail-closed (21.99 s), PHP memcached CLI+HTTP managed/external set/get and actual patch update (20.64 s), mixed-runtime PHP-only rollout/rollback/recovery (18.29 s), and earlier lifecycle/startup/recovery suites. Workspace Clippy all-targets -D warnings, desktop production build, catalog/link/diff checks passed. Changed diagnostics export test also passed after adding cache summaries; values/private markers are not exported.

An actual CLI/engine network smoke downloaded the extension and server through catalog URLs, verified publisher digests before extraction, activated the extension, created a guarded native instance, performed PHP set/get, stopped/forgot that instance and stopped its isolated engine. It passed; ignored evidence is .artifacts/S8A-CACHE-BUILD/download-result.json. No global PATH, OS service, hosts/trust entry, user project or persistent database changed.

Windows Stage 8A deliverables are accepted. Mac execution remains pending for the user's target run. No user input is needed to continue Stage 9; freeze its contract and implement 9.2–9.4 while retaining target evidence boundaries. Dedicated desktop Stack controls remain Stage 13.
## Stage 9 coordinator handoff - 5 November 2026

Stage 9 source and Windows acceptance are complete. [Decision 0008](decisions/0008-project-scopes.md) freezes the schema/API and migration behavior. [Integration evidence](evidence/S9-INTEGRATE.md) links the worker checks and exact native gates. Mac execution remains pending; it did not block independent source work.

Changed: shared manifest/scope/dependency/project/PHP services; migration 8 and read-only store opening; pure preview and structured error detail; supervisor/readiness/Apache validation; scoped engine/CLI/Tauri/TypeScript commands; disposable tests and documentation. Schema 1 is normalized without rewriting manifests or moving environments. Scope selections inherit project/global choices without freezing defaults. Scope-specific PHP rollout preserves unrelated workers and observed listener ports.

Validation on Windows 10 Pro 10.0.19045 x64, Rust 1.99.1: cargo test --workspace --offline passed with all cached native Windows gates enabled (128 reported passes); cargo clippy --workspace --all-targets --offline -- -D warnings and pnpm --dir apps/desktop run build passed. The coordinator checked cold CLI preview, migration/reopen preservation, Tauri serialization, native PHP portless rollout and existing PHP/FastCGI/cache regressions. Native pnpm/Python live-service acceptance is detailed in S9-SCOPES.md. Missing Mac and older stage desktop/target checks remain open, without a whole-project completion claim.

Workers used fresh bounded contexts: two Luna workers for parser/readiness, one Sol at medium reasoning for scope execution. The coordinator owned shared interfaces, migrations, preview, adapters, review and status. No new package downloads, global PATH, service, trust or hosts changes were required. Independent CI resolver-test changes were preserved separately.

Next task: Stage 10.1, split the desktop shell/navigation and feature panels while preserving existing actions; then resource/activity ownership and Stage 10 acceptance. Mac Stage 9 checks can run independently when the user's machine is available.
