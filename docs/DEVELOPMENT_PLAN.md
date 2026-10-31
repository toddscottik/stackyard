# Development stages and agent handoff

Date: 31 October 2026. Status: ready for staged implementation; application implementation has not started.

This is the execution plan. Where it differs from PRODUCT_PLAN.md or PRODUCT_PLAN_REVIEW.md, use this document for initial scope and ordering. Those documents retain research and broader product ideas. Read PACKAGE_VERIFICATION.md before selecting binary packages.

## Fixed scope for the first usable release

- Personal development first, public release later.
- Windows 10 Pro 22H2 x64 (build 19045); MacBook M1 Pro, 16 GB RAM, macOS 14.5 arm64.
- Existing-installation selection and app-managed downloads for Node.js, Python, Apache, MySQL, and PostgreSQL. SQLite is project data, not a daemon.
- Recipes: Next.js, generic Node HTTP server, Flask, FastAPI, static sites. Projects may combine services.
- Project dependencies: npm/pnpm for Node, pip for Python, and uv workflows where selected. Respect existing managers and lockfiles. See [DEPENDENCY_MANAGEMENT.md](DEPENDENCY_MANAGEMENT.md).
- Local domains/HTTPS, logs, project shells, start/stop/restart, version selection, update notifications, and database backup/restore.
- PHP, Composer, downloadable PHP extensions, and Memcached integration are a required follow-on capability in Stage 8A. Go, MariaDB, Caddy/nginx, Docker, tunnels, cloud sync, marketplace, and broad public OS support remain later work.

## Architecture decisions agents should preserve

- Tauri 2 + React/TypeScript/Vite desktop; Rust core, engine process, and CLI; SQLite for machine state; TOML project manifest plus runtime lockfile. Record tested toolchain/dependency versions during setup; do not use unbounded latest dependencies.
- One per-user engine owns process state. Desktop/CLI use a versioned local API over a user-restricted named pipe or Unix socket. The CLI must not create a second orchestration path. A tray close hides the window; explicit Quit stops managed work unless background mode was deliberately selected.
- Model Installation, ServiceInstance, Project, and ExternalConnection separately. Installed packages can serve multiple instances. Package and service state machines are separate.
- Model language-tool selection, project dependency operations, and native extension profiles explicitly. Ecosystem managers own package resolution/lockfiles; the app's binary catalog owns approved runtime/tool/extension artifacts. Installing a client library/extension does not install its server.
- Share one Apache listener with virtual hosts. Each application has its own process/port/environment. No mandatory extra router. An Apache restart has shared impact and must be presented accordingly.
- Keep manifest intent, artifact lockfile, local paths/secret references, and observed process state distinct. Generated configuration is derived. UI and CLI call the same commands and validation.
- Prefer structured arguments and explicit working directories/environment. Project lifecycle hooks are visible, intentional actions. App control APIs are not exposed to project web pages.
- Managed downloads install side by side into versioned app storage. Existing installations are referenced; attached running services remain externally owned. No global PATH replacement or automatic takeover of system services.
- Persist database data independently of installations. Uninstall/removal preserves data by default. Database major upgrades are engine-specific migrations, not runtime version switches.

Suggested code layout, to create in Stage 1:

```text
apps/desktop/          Tauri shell and React UI
crates/core/           models, commands, persistence, config validation
crates/engine/         IPC, orchestration, process ownership
crates/cli/            command-line client
crates/providers/      native runtime/server integrations
catalog/              validated artifact records and schema
fixtures/             small reproducible example projects
scripts/verification/ OS-specific smoke tests
docs/                 plans, decisions, evidence, handoffs
```

## Stage 0 — Prove packages and target compatibility

**Depends on:** existing verification report. This is the first development task.

**Deliver:** a small artifact matrix and repeatable Windows/macOS smoke scripts. Fetch and verify candidate packages in disposable app-like directories. Prove Node execution, Python virtual environments and imports, Apache HTTP/proxy modules, and MySQL/PostgreSQL initialization, query, restart, and clean shutdown. Choose exact versions from evidence and real project constraints. Check build-tool prerequisites separately from end-user runtime prerequisites.

Resolve macOS Apache and MySQL packaging first on the Mac. If no appropriate Apache archive exists, build and package a relocatable arm64 distribution with its dependencies; end users should download the result. Record any build prerequisites. An existing installation can unblock later prototype work but cannot be reported as a working managed-download provider.

**Acceptance:** each approved artifact has an exact URL, digest/provenance, dependencies, target, launch evidence, and cleanup result. Test relocation and a path with spaces. Mark untested Mac items pending. Source listings and checksum metadata alone do not pass this stage. Independent Windows/Node/Python work can proceed while a specific Mac artifact is unresolved; full platform acceptance waits for it.

Stage 0 also needs pinned pnpm and pip environment smoke checks. Track PHP-extension and Memcached artifact feasibility separately for Stage 8A; unverified candidates must not be advertised as supported.

## Stage 1 — Establish the application foundation

**Depends on:** Stage 0 target/toolchain findings; independent of final database packaging.

Rust is confirmed not installed on the Windows machine. Begin with Rust/Cargo MSVC toolchain setup and inspection of C++ Build Tools/WebView2; install missing prerequisites through their supported installers under normal system-change approvals. Check the Mac toolchain separately. Stage 0 runtime-package probes can run without Rust.

**Deliver:** workspace, pinned toolchain/dependencies, minimal Tauri window, Rust models/storage, engine singleton, CLI IPC client, structured errors/events, and OS-specific app-data directories. Add schema versions and migrations from the start. Use a neutral temporary package name until branding is chosen.

**Acceptance:** desktop opens on both targets; CLI and desktop query the same engine; a second client does not duplicate it; settings survive restart; another local user cannot access its control endpoint. Add one CI build path per target and document local commands. A build on a newer OS does not replace testing on 14.5/Windows 10.

## Stage 2 — Select existing installations and install managed packages

**Depends on:** Stage 1 and approved Node/Python artifacts from Stage 0.

**Deliver:** provider capabilities, discovery/Browse, version/architecture probes, bundled catalog schema, download progress/cancel, verification, safe extraction, atomic install, install records, and per-project selection. Start with Node and Python, then reuse infrastructure for server packages. Pin uv and redirect its cache/install locations to app storage; disable surprise downloads for an explicit existing-runtime selection.

**Acceptance:** a user can select existing Python and download Node, or the reverse. Two versions coexist. A bad digest, interrupted download, unsafe archive path, or concurrent install cannot create a usable partial installation. Existing software remains untouched. Referenced packages cannot be accidentally removed. UI shows actionable source/version/path information.

Stage 2 additionally installs or locates a compatible pinned pnpm and ensures pip is available in selected app-created Python environments. Manager/tool storage is separate from project dependencies. Test the selected manager against its intended runtime; preserve external tools.

## Stage 3 — Run projects reliably

**Depends on:** Stage 2.

**Deliver:** manifest import/validation; process dependency ordering; readiness checks; bounded restart policy; log streaming/rotation; stop/restart; process-tree cleanup; shell/command execution in a selected service context. Add small fixtures for Next.js, generic Node, Flask, and FastAPI, with pinned dependencies. Existing projects retain their chosen commands/lockfiles. Introduce no database dependency in the first fixtures.

**Acceptance:** two projects using different runtime selections run together on loopback high ports; stop one without stopping the other; hot reload works; failed startup points to the actual error; child processes exit; ports are released. Crash/relaunch does not start duplicates or kill unrelated processes. Run with spaces and non-ASCII paths. Test cancellation and failure after only part of a project has started.

Stage 3 additionally implements dependency install/add/remove/selected-update/list commands through npm/pnpm and pip/uv as specified in DEPENDENCY_MANAGEMENT.md. Test pnpm versus npm lockfiles, Python environment isolation, workspace scope, and cancellation/failure output.

## Stage 4 — Make the desktop useful for daily work

**Depends on:** Stage 3. Thin UI controls may be added in earlier stages to exercise their features.

**Deliver:** Components, Projects, project/service details, logs, and settings. Expose Install / Use existing / Connect existing where applicable; add open URL/editor/folder/shell actions, tray controls, operation progress, and empty/error states. All controls use real engine behavior. Follow applicable UI skills when implementing this stage.

**Acceptance:** import a Next.js or Python project, choose its runtime, install dependencies through an intentional action, start it, inspect logs, restart, and stop it entirely through the UI. Keyboard navigation and readable errors work. Closing the main window preserves tray access; explicit Quit follows the documented shutdown policy. No mock status is presented as a running service.

Stage 4 additionally exposes project dependency actions, selected manager/version, manifest/environment change results, and logs. Scope actions to the selected project/environment; never imply that a dependency update is a runtime update.

## Stage 5 — Add Apache, domains, and trusted HTTPS

**Depends on:** Stages 3–4 and approved Apache artifacts on the tested target.

**Deliver:** Apache installation/reference provider, virtual hosts, static serving, reverse proxy to Node/Python, WebSockets, configurable HTTP/HTTPS ports, and certificate helper integration. Start with project.localhost and tested resolver behavior; add explicit project.test host entries only where needed. Use high ports first; add scoped OS setup for 80/443 and trust installation. Preserve unrelated host entries and trust certificates.

**Acceptance:** concurrent Next.js and FastAPI/Flask sites work through distinct local HTTPS domains, including hot reload, POST/upload requests, host/scheme forwarding, and redirects. Unknown hosts do not expose another project's files. A bad config change retains working sites. Restart impact is clear. Test a static .htaccess rewrite fixture now; full PHP handling is deferred. Unavailable ports produce choices, not termination of another app. Certificate trust installation and removal work on both actual machines.

## Stage 6 — Add databases and persistence

**Depends on:** Stages 2–4 and approved database artifacts; Apache is not required for database-provider work.

**Deliver:** MySQL first, then PostgreSQL: install/reference, instance initialization, explicit data directory, loopback binding, credentials, readiness, start/stop, create project database/user, connection details, and logical backup/restore. Support external endpoint connections separately. Add SQLite path selection and consistent backup through a validated API; no fake SQLite service process. Store credentials through OS facilities and redact them from logs/exports.

**Acceptance:** fixtures perform CRUD against all three database types. Data survives restart. Restore backups into fresh disposable databases and verify content. Stopping one project preserves a shared instance needed elsewhere; stopping/removing a project never shuts down a connection-only external database. Runtime removal never removes database files. Initialization refuses an existing nonempty data directory. Major-version migration remains explicitly unsupported until separately implemented.

## Stage 7 — Add controlled updates and recovery

**Depends on:** Stages 2–6.

**Deliver:** catalog refresh, compatible update detection, side-by-side runtime installation, explicit project switching, retained previous configuration, cleanup of unused packages, and interrupted-operation reconciliation. Distinguish upstream release news from installable packages. External tools receive informative update notices without being overwritten. App updates and runtime updates are separate flows.

**Acceptance:** download a new Node/Python version, switch only one project, and leave other selections intact. Recover from failed health checks and interrupted installs. Cached projects still run offline. Packages needed for active projects cannot be deleted. Database major versions never enter the generic switch path. Record compatibility limits when an external installation changes independently.

Stage 7 additionally separates language-tool updates from project dependency updates. Reinstallation from recorded manifests is a recovery operation, not a claim that package scripts can be rolled back atomically.

## Stage 8 — Validate the complete personal-use release

**Depends on:** Stages 0–7 completed for both targets.

**Deliver:** runnable private Windows/macOS builds, setup instructions, diagnostics export with redaction, basic regression tests, and an acceptance report. Test the user's real projects alongside the fixtures. Keep optional tools such as Mailpit out unless actually needed for those projects.

**Acceptance:** on Windows 10 and the M1 Pro/14.5 machine, use an existing runtime, download another component, run Next.js plus a Python API, use MySQL/PostgreSQL/SQLite, switch runtimes, restart the app, and restore a backup. Verify sleep/wake, Unicode paths, occupied ports, offline startup, clean shutdown, and removal preserving projects/data. Measure startup time and idle/active memory with editor/browser present; report measurements rather than inventing a target. Deliver a precise known-limitations list. This is the first personal release gate.

Stage 8 must exercise a pnpm-based Next.js fixture and a pip-managed Python environment, alongside npm and uv fixtures where supported. Verify dependency updates stay within the selected project and respect its manager/lockfiles.

## Stage 8A — Add PHP, Composer, extensions, and Memcached

**Depends on:** Stages 2, 3, 5, and 7 plus PHP/extension/service artifact feasibility. Scheduled after the first Node/Python personal release; public distribution is not a prerequisite.

**Deliver:** PHP installation/reference and Apache integration; Composer project operations; extension inventory, download/install/enable/disable, per-project ini profiles, and compatibility filtering. Include memcached as the first external-extension use case, with a separate Memcached service provider or explicit existing endpoint. Verify native service and extension artifact availability on both targets before promising parity. Use approved binary providers/PIE where validated. Honor the detailed ownership and compatibility rules in DEPENDENCY_MANAGEMENT.md.

**Acceptance:** Composer install/checks run using selected PHP; CLI and HTTP load the intended extensions; incompatible artifacts never replace working profiles; two projects can differ in enabled extensions; PHP updates revalidate the extension set. A real PHP memcached client performs set/get against a managed local server and an existing endpoint. Unsupported target combinations remain explicit. A connection-only fallback does not count as completing managed server installation.

## Stage 9 — Prepare a public release and extend coverage

**Depends on:** Stage 8 and user decisions on distribution/license.

**Deliver:** signed release pipeline/updater, macOS notarization, package provenance and notices, catalog signing/rotation, documented OS/version policy, clean-machine tests, and contribution/support documentation. Publish an explicit capability matrix including Stage 8A status; do not advertise PHP/extensions before their acceptance checks pass. Expand one provider at a time: Go, then other languages/services based on demand. Add Linux/Intel Mac support only with actual package and runtime evidence.

**Acceptance:** a clean target machine can install, run, update, and uninstall with verified packages and preserved user data. A new provider passes the same install/launch/stop/update contract. Publication and hosting are separate user decisions; this plan does not itself publish anything.

## Agent execution and handoff rules

1. Read this plan, PACKAGE_VERIFICATION.md, DEPENDENCY_MANAGEMENT.md, and IMPLEMENTATION_STATUS.md. Inspect the current checkout and any applicable AGENTS.md before editing.
2. Work on the requested stage or its next incomplete slice. Keep changes reviewable; do not implement the whole roadmap in one task or redesign agreed interfaces without recording why.
3. State dependencies and the specific acceptance checks being delivered. Build on real providers; mocks may support tests but cannot establish runtime/platform completion.
4. Test only in app-owned/disposable paths unless the user explicitly selects existing data. Respect normal tool approvals for system changes. Do not auto-elevate imported commands or replace unrelated installations.
5. Run meaningful unit tests for state/config logic and integration tests for lifecycle, downloads, and persistence. Capture real OS/version/architecture and exact commands with results. Check the UI in the actual app/browser when implementing it.
6. Update IMPLEMENTATION_STATUS.md with completed slices, tests, limitations, and next work. A stage remains partial if required Mac testing is missing. Continue independent work when one platform is unavailable, and clearly report the outstanding gate.
7. End with a short handoff: changed files, demonstrated behavior, tests, unresolved issues, and the exact next task. No performance/support claim without evidence.

### Reusable task prompt

> Implement Stage N of docs/DEVELOPMENT_PLAN.md, starting with the next incomplete slice in docs/IMPLEMENTATION_STATUS.md. Read docs/PACKAGE_VERIFICATION.md and applicable repository instructions. Preserve the agreed architecture and scope. Complete the stage's deliverables and relevant acceptance checks, update implementation status, and report evidence and the next task. If a target machine is unavailable, implement/test what is independent and leave that target's checks explicitly pending.

### First task to assign

> Execute Stage 0. Start with the Windows Node/Python managed-download and reuse checks, then Apache/MySQL/PostgreSQL disposable smoke tests. Prepare the corresponding Mac checks. Resolve macOS 14.5 Apache and MySQL artifact selection on the M1 Pro before approving their managed providers. Produce tested artifact records and a clear pass/pending matrix. Do not build the full product UI in this task.
