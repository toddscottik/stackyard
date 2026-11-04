# Development stages and agent handoff

Original plan: 31 October 2026. Updated: 4 November 2026. Stages 0–8 implemented; target acceptance gaps are recorded separately.

This is the execution plan. Where it differs from PRODUCT_PLAN.md or PRODUCT_PLAN_REVIEW.md, use this document for initial scope and ordering. Those documents retain research and broader product ideas. Read PACKAGE_VERIFICATION.md before selecting binary packages.

## Current position and corrective follow-up — 4 November 2026

Stages 0–8 already have implementations; consult IMPLEMENTATION_STATUS.md for evidence and outstanding platform acceptance. They are the baseline for further work. The remaining stages are Stage 8A (PHP/extensions), Stages 9–17 (bounded workflow and acceptance work), and Stage 18 (public release). Historical setup statements below describe the original plan, not current machine state.

DeepSeek's [desktop proposal](UI_REWORK_PROPOSAL.md) is a focused correction to the existing application, principally Stages 3–4 plus missing desktop access to Stage 6 capabilities. See [the review](UI_REWORK_REVIEW.md). It does not restart the roadmap.

The user confirmed runnable **Node HTTP, Next.js, Flask, and FastAPI starters** for the first usable release. A folder plus manifest is insufficient. Add creation and ordinary-folder import to the existing engine/CLI/desktop; reuse its package installer, dependency operations, supervisor, Apache, and databases. Mixed Node/Python projects need independent dependency scopes. These requirements supersede import-only acceptance and manifest-only creation in earlier text.

Agreed order: Stage 8A → Stages 9–15 (specific domain and desktop workflows) → Stage 16 (usability/recovery) → Stage 17 (target acceptance) → Stage 18 (public release). Stage dependencies below permit independent work without requiring every stage to be sequential. Complete PHP/Composer, extension downloads/profiles, Apache/PHP integration, and Memcached behavior first, then build the reworked flows around those working capabilities alongside Node/Python. Use engine/CLI and minimal desktop controls to validate 8A; reserve the broader UI redesign for the rework. Keep platform checks active throughout; public release waits for required target acceptance. Define only missing contracts; reuse completed foundations.

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

## Stage 9 — Define project scopes and launch contracts

**Depends on:** Stage 8A implementation and existing Stages 0–8.

**Outcome:** A shared engine/CLI contract that represents Node, Python, and PHP projects without one project-wide package manager controlling every service.

**Tasks:**

- 9.1 Audit existing manifest, protocol, store migrations, dependency tasks, and 8A profiles. Record what is reused and the minimum schema/API changes in one decision note; define request/result examples before UI work.
- 9.2 Implement dependency scopes with stable IDs, contained working directories, runtime/tool selections, environment ownership, manager, and lockfile authority. Services reference scopes; PHP extension profiles stay distinct from Composer dependencies. Preserve legacy single-scope projects through an explicit migration/default.
- 9.3 Separate process state, endpoint presence, readiness mode, and observed HTTP status. Support HTTP checks, TCP checks without HTTP paths, and workers without endpoints. Preserve legacy readiness semantics; new profiles write their choice explicitly. Reject Apache proxy targets without HTTP endpoints.
- 9.4 Define structured validation/error results and a pure preview contract. Preview accepts a draft, reports missing runtimes/tools and unresolved auto ports, and does not execute commands, create environments, or download files.

**Acceptance:** Load legacy manifests and reopen migrated state without lost selections. Run a disposable pnpm frontend and pip/uv API as independent scopes; a dependency action in one leaves the other unchanged. Exercise a worker, redirecting HTTP app, strict health endpoint, missing tool, invalid scope reference, and escaping directory. CLI and desktop boundary types agree.

**Implementation area / ownership:** Core manifest/dependency/project models, protocol, store migrations, engine launch resolution, CLI; coordinator owns shared DTOs and lockfiles.

**Handoff / boundary:** Document the schema/API examples, migration behavior, scope ownership, and fixtures for Stages 10–15. No broad UI redesign or starter generation in this stage.

## Stage 10 — Build the shared desktop workspace and activity layer

**Depends on:** Stage 9 contracts.

**Outcome:** A maintainable desktop shell that preserves existing actions and can host the new workflows.

**Tasks:**

- 10.1 Split App.tsx into shell/navigation and feature panels. Add project selection and workspace tabs with remembered selection; preserve existing import/start/stop/install/database/site actions during migration.
- 10.2 Add typed resource hooks with loading, local error, refresh/invalidation, and one polling owner per resource/task. Depend on stable IDs; prevent duplicate submissions and stale responses when changing projects.
- 10.3 Unify existing install/dependency activity by operation ID, with progress, output, cancel/retry capabilities, and results surviving navigation. Add engine status/query support only where existing operations lack it; reconnect must reconcile with engine state rather than fabricate success.
- 10.4 Tail logs using offsets, bounded buffers, rotation/truncation handling, follow/pause, and final output after exit. Map lifecycle buttons to valid engine states and keep failures beside their originating action.

**Acceptance:** Import an existing fixture, start it, navigate away/back while installing dependencies, reconnect the UI, read appended logs, restart, and stop. Verify cancellation, engine-down retry, final crash output, rotated logs, stale response handling, and that existing database/site actions remain available. Add focused UI tests at the invoke boundary and a real desktop smoke; stubs do not prove process behavior.

**Implementation area / ownership:** Desktop app/api/feature modules and narrowly scoped engine operation queries. Establish file ownership before workers split App.tsx.

**Handoff / boundary:** Publish component/resource/operation contracts and a parity checklist. New application recipes and database feature expansion belong to later stages.

## Stage 11 — Create runnable projects from the desktop

**Depends on:** Stages 9–10 and approved runtime/tool artifacts.

**Outcome:** Create project produces working source and a runnable application for Node HTTP, Next.js, Flask, and FastAPI.

**Tasks:**

- 11.1 Define versioned recipes: ID/version, source files or pinned generator/arguments, dependency inputs, required components, launch profile, and supported target evidence. Use the existing artifact policy; never resolve an unbounded latest generator.
- 11.2 Implement engine plan/apply plus CLI entry points. Plan previews target paths, files, downloads, commands, and next steps. Apply rechecks target contents/fingerprints, refuses unsafe overwrites, writes atomically where possible, and records partial outcomes. Cancellation preserves pre-existing files and never deletes unrelated data.
- 11.3 Deliver Node HTTP first as a complete desktop flow: native folder picker → name/recipe → runtime/manager choice (download or existing) → review → generate → install → start → open URL/logs. Include back/edit validation and a useful next action on failure.
- 11.4 Add Next.js, Flask, and FastAPI recipes using the same contract. Generate actual source, dependency declarations and manifest; do not count an empty folder or manifest as a starter. Report network/cache requirements and support retry without duplicating files or project records.

**Acceptance:** On disposable paths create each starter entirely through the desktop, install, receive the expected response, inspect logs, exercise supplied hot reload, restart, and stop its process tree. Test managed and existing runtimes, invalid names, spaces/Unicode paths, nonempty targets, changed files after preview, interrupted generation/download/install, and safe retry. Record exact recipe/runtime/dependency versions; unsupported Mac artifacts remain visible.

**Implementation area / ownership:** Recipe definitions, creation engine/core/CLI, desktop create feature, fixtures. Separate recipe content ownership from shared creation contracts.

**Handoff / boundary:** Provide four versioned starter fixtures and per-target results. PHP's existing-app workflow is integrated in Stage 13; additional PHP framework generators are not implied.

## Stage 12 — Import applications and edit their configuration

**Depends on:** Stages 9–11.

**Outcome:** Existing applications can be added without hand-writing stackyard.toml, and their configuration can be changed safely afterward.

**Tasks:**

- 12.1 Add bounded read-only detection of package metadata, scripts, lockfiles, Python entry points, Composer/PHP roots, and conventional workspace directories. Skip generated directories, cap reads, and prevent symlink/junction escapes. Show evidence/confidence; unknown or unsupported managers lead to explicit/manual setup.
- 12.2 Offer Next.js, Vite, Express, Flask, and FastAPI launch suggestions; respect existing scripts, packageManager and lockfiles. Separate npm/pnpm argument forwarding and host/port rules, and never assume an Express application reads PORT. PHP detection uses 8A capabilities and proposes a document root for review.
- 12.3 Provide editable service/scope/site forms with pure validation and preview. Preserve existing manifests, comments where practical, and unrelated files; apply only against the previewed fingerprint. Provide open-in-editor/reload for unsupported advanced fields.
- 12.4 Add stopped-project forget and safe reload behavior. Specify how owned scope/selections/bindings are detached; preserve source, secrets needed by retained resources, and database data. Refuse forgetting while project operations are active.

**Acceptance:** Import ordinary Next.js, Flask/FastAPI and PHP folders without a manifest, plus a mixed Next.js/FastAPI workspace. Review suggestions, save and run; legacy manifests still import. Test malformed metadata, conflicting lockfiles, unsupported managers, unknown entry points, fixed-port apps, concurrent edits and junctions. Forget a stopped project and prove files and database data remain.

**Implementation area / ownership:** Detection/profile core, project configuration/forget commands, desktop import and service editor, fixtures.

**Handoff / boundary:** Document supported detection evidence and manual fallback. No arbitrary code execution during detection and no silent manager conversion.

## Stage 13 — Unify components, dependencies and PHP extensions

**Depends on:** Stages 8A and 9–12.

**Outcome:** Global Components and each project's Stack expose the same working runtime/tool/server and extension capabilities.

**Tasks:**

- 13.1 Group components by runtime/tool/server; show source, version, target, path, use-by references, catalog availability and install status. Add discovery/Browse/validation, side-by-side installation, and actionable removal protection.
- 13.2 In each dependency scope expose manager/runtime selection and install/add/remove/selected-update/list/recover with workspace context, output and cancellation. Respect npm/pnpm, pip/uv and Composer lockfile semantics; show changes to declarations versus environment-only changes honestly.
- 13.3 Wire the 8A extension inventory, compatible downloads, enable/disable and per-project profiles into PHP's Stack panel. Show ABI incompatibility before activation, apply candidate profiles through existing probes, and report affected PHP worker restarts.
- 13.4 Present runtime updates, tool updates, dependency updates and extension updates separately. Show Memcached as a server/endpoint and the PHP memcached extension as its client; do not merge them into one install.

**Acceptance:** Install two runtime versions and switch one scope without changing another. Reference an existing installation without modifying it; refuse removal while in use. Exercise pnpm, pip/uv and Composer operations. Give two PHP projects different extension profiles; verify CLI and HTTP, reject an incompatible artifact while preserving the working profile, and perform Memcached set/get.

**Implementation area / ownership:** Desktop Components/Stack features; reuse package, update, dependency and 8A commands. Add only missing typed adapters.

**Handoff / boundary:** Record managed/existing capability matrices by target and unresolved artifact gaps. This stage exposes 8A behavior rather than rebuilding its installers.

## Stage 14 — Complete project database workflows

**Depends on:** Stages 9–10 and 12; existing Stage 6 providers.

**Outcome:** A project can provision or attach a database and use its credentials without manually finding secret files.

**Tasks:**

- 14.1 Separate fleet instance administration from project connection tasks. Expose create instance, start/stop, create database/user, attach existing, SQLite selection, and probe. Show download/version/data-path review and shared-instance impact; remove implicit global-default changes.
- 14.2 Expose deliberate credential copy/reveal and per-service connection/environment mappings. Define variable collisions and precedence, encode credentials in driver-specific URLs, and handle missing/stopped connections explicitly. Shareable manifests contain no secret values.
- 14.3 Offer previewed .env output without overwriting unrelated entries. Keep resolved credentials out of application-control errors/diagnostics; document that application-emitted logs can contain secrets and apply the existing redaction policy.
- 14.4 Wire backup/restore and SQLite consistent backups with file dialogs. Restore into a fresh/disposable target; detach preserves server/data. Instance deletion and database drop remain deferred unless separately scoped with data-safety acceptance.

**Acceptance:** Using disposable MySQL/PostgreSQL, create a database/user, launch a fixture reading its mapped connection, perform CRUD, restart, backup, restore to a new target and verify content. Test SQLite backup/restore, passwords with URL-reserved characters, variable conflicts, unavailable server, shared-instance stop impact, external attach/detach, and secrets absent from diagnostics.

**Implementation area / ownership:** Desktop database fleet/project panels, connection mapping storage/launch integration, existing database command adapters.

**Handoff / boundary:** Return credential/mapping ownership rules and backup/restore evidence for all three database types. Database major migrations remain unsupported.

## Stage 15 — Complete sites, HTTPS and desktop settings

**Depends on:** Stages 9–10 and 12; 8A PHP integration and existing Stage 5.

**Outcome:** Project Site and global Web/Data settings provide the real routing and host-integration controls.

**Tasks:**

- 15.1 Build project Site configuration and publish/unpublish with correct HTTP/HTTPS URLs and ports, static/PHP document roots or HTTP service targets, Apache state and shared restart impact.
- 15.2 Expose front ports and explicit hosts/trust operations with previews, permission boundaries and reversal. Preserve unrelated entries/certificates; distinguish named-directory certificate tests from actual OS/browser trust.
- 15.3 Finish runtime-aware project terminal/editor/folder/log-folder actions using the selected scope and environment. Custom project commands remain non-elevated and use engine argument arrays.
- 15.4 Group General, Web, Data and About settings: existing tray/quit policy, open/copy data paths, scoped cleanup preview, diagnostics save, versions and catalog revision. Data relocation and start-at-login are deferred unless separately specified.

**Acceptance:** Serve concurrent PHP, Node and Python sites, including .htaccess, redirects, POST/upload, HMR/WebSockets and high-port URLs. Reject invalid candidate config while retaining working sites. Test occupied ports without killing their owner; verify actual OS/browser HTTPS trust and its removal on both targets. Show the selected runtime in the project terminal; cleanup protects active resources and diagnostics remain redacted.

**Implementation area / ownership:** Desktop Site/Settings and Tauri host-integration adapters; existing Apache/trust/hosts/open commands.

**Handoff / boundary:** Record privileged effects and reversal checks separately from ordinary UI tests. Missing target trust or package checks stay pending.

## Stage 16 — Finish desktop usability and failure recovery

**Depends on:** Stages 10–15.

**Outcome:** All delivered workflows remain usable through keyboard, navigation, cancellation, engine reconnect and offline startup.

**Tasks:**

- 16.1 Review every empty/loading/error/disabled state; name the next action and affected object. Confirm destructive consequences at the relevant action; remove duplicate lifecycle controls and dead affordances.
- 16.2 Complete native copy/save/picker behavior, keyboard traversal/shortcuts, focus return, screen-reader labels, non-color status cues and reduced motion. Verify 1024×700 through larger windows without clipping.
- 16.3 Bundle licensed fonts and remove remote rendering dependencies. Preserve offline startup for installed projects; explain uncached dependency/component downloads.
- 16.4 Exercise cross-feature operation recovery: navigation/reconnect/relaunch during downloads, dependency tasks, creation and configuration changes. Reconcile from actual engine state, preserve partial results and provide safe retry. Add focused regression tests for observed failures.

**Acceptance:** Complete create/import/start/stop and extension/database tasks using keyboard; verify focus/error placement, copy actions, minimum-size layout and offline rendering. Cancel/retry and reconnect across each long operation; no duplicate process/install or false success. Preserve the chosen tray/background shutdown policy.

**Implementation area / ownership:** Cross-feature desktop integration and narrowly scoped recovery fixes; follow applicable UI skills during implementation.

**Handoff / boundary:** Provide a UI regression checklist and actual screenshots/window evidence. Dark theme/command palette are optional enhancements after required flows, not release blockers by themselves.

## Stage 17 — Accept the integrated app on Windows and macOS

**Depends on:** Stages 8A and 9–16; approved artifacts for every advertised target capability.

**Outcome:** A release-candidate acceptance report for Windows 10 x64 and the M1 Pro running macOS 14.5 arm64.

**Tasks:**

- 17.1 Build a matrix linking each stage's acceptance to executable fixtures/manual steps, exact versions, OS/architecture and evidence. Reuse existing Stage 0–8 evidence only for what it actually proves.
- 17.2 Run fresh-data and upgrade-from-existing-state scenarios on both targets, including four starters, ordinary imports, mixed scopes, managed/existing components, PHP/Composer/extensions/Memcached, database restore and Apache HTTPS/HMR.
- 17.3 Exercise real user projects, sleep/wake, spaces/Unicode paths, occupied ports, offline cached startup, engine crash/relaunch, close/tray/quit and preserving files/data. Measure startup and idle/active memory with editor/browser open.
- 17.4 Fix acceptance blockers in their owning stage, update setup/known limitations and rerun affected cases. Maintain a clear list of unsupported combinations; neither Linux tests nor a newer macOS CI runner replace these target runs.

**Acceptance:** Every required capability has passing evidence on its advertised target. Missing required Mac packages/launch/trust checks keep this stage open. No destructive data regression, ownership takeover, secret export or duplicate engine/process remains. The report identifies exact builds, recipe/catalog revisions and any user-approved scope exclusions.

**Implementation area / ownership:** Verification scripts, fixtures, evidence, setup/limitations; fixes assigned to feature owners rather than one unbounded QA rewrite.

**Handoff / boundary:** Record the accepted release candidate and explicit public-release readiness. Publication is not performed by this stage.
## Stage 18 — Prepare a public release and extend coverage

**Depends on:** Stage 17 accepted, including Stage 8A and Stages 9–16 integration on Windows 10/macOS 14.5, plus user decisions on distribution/license.

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

### Next task to assign

> Implement the next incomplete Stage 8A slice from IMPLEMENTATION_STATUS.md. Begin with PHP/extension/Memcached artifact feasibility and existing provider contracts; preserve existing Stage 0–8 behavior. After 8A, start task 9.1 and follow declared dependencies. Dispatch one numbered task with exact writable paths and acceptance rather than the whole rework.
