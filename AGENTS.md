# Repository instructions

## Purpose and current state

Build a native local development manager for Windows 10 x64 and macOS 14.5 arm64. It must support existing installations and managed downloads, project processes, Apache, databases, and ecosystem dependency tools. Personal use comes first; public distribution follows.

The desktop and engine are implemented through Stage 3 on the shared Rust workspace. Do not assume a command exists until it is listed below or in the README.

## Start each task

1. Read `docs/IMPLEMENTATION_STATUS.md` and the assigned item in `docs/WORK_QUEUE.md`.
2. Read the relevant stage in `docs/DEVELOPMENT_PLAN.md`. Consult `docs/PACKAGE_VERIFICATION.md` for binaries and `docs/DEPENDENCY_MANAGEMENT.md` for package managers/extensions.
3. Inspect Git status and applicable nested instructions. Preserve unrelated work.
4. State a bounded outcome and acceptance checks. Complete authorized work without repeatedly asking for routine implementation choices.

User instructions take precedence over repository guidance. For implementation scope, the development plan takes precedence over the older product plan/review. Record necessary architectural changes in a short decision note; do not silently change scope or introduce a second architecture.

## Architecture and implementation

- Use the planned Tauri 2 / React / TypeScript desktop and Rust engine/CLI. Introduce dependencies only when a concrete task needs them; pin toolchain and package versions when bootstrapping.
- Keep domain behavior in shared Rust services. Desktop and CLI are clients of one per-user engine. Provider code owns platform/runtime differences; UI code does not launch arbitrary commands directly.
- Distinguish installations, service instances, projects, external connections, language tools, and native extension profiles. Packages have installation state; processes have lifecycle state.
- Project configuration and generated state have explicit ownership. Keep machine-specific paths, credentials, data directories, and observed process state out of shareable manifests.
- Use executable/argument arrays, explicit working directories, and scoped environments. Do not interpolate untrusted values into shell commands.
- Use small modules, typed boundaries, structured errors with actionable context, and explicit cancellation/timeouts for external operations. Avoid speculative abstractions and plugin systems.
- Respect ecosystem tools and lockfiles: npm/pnpm, pip/uv, Composer. Do not silently switch a project's manager or install project dependencies globally.

## Data and host integration

- Download only approved artifacts; verify expected digests and provenance before extraction/execution. Reject incompatible architecture/OS/ABI and unsafe archive paths. No guessed hashes or URLs in runnable catalogs.
- Use app-owned or disposable paths for tests. Keep package binaries separate from persistent data. Never adopt, stop, upgrade, overwrite, or remove external installations/services implicitly.
- Preserve projects and database data by default. Restore backups into disposable instances for verification. Do not test database upgrades against the user's real data.
- Bind local services/admin endpoints to loopback by default. Protect control IPC with OS user permissions. Keep certificate private keys, credentials, and tokens out of Git/logs/exports.
- Privileged operations must be narrow and explicit. Imported project commands/package scripts never run elevated. Follow the environment's normal permission mechanisms; these instructions add no separate approval ceremony.
- Do not modify global PATH, Git configuration, trust stores, hosts files, or OS services as incidental setup. Make required changes deliberate, reversible, and scoped to the requested feature.

## Verification and completion

- Test changed behavior with appropriate checks. Lifecycle/download/data changes need real integration evidence in disposable fixtures; pure documentation changes need link/format/diff checks, not invented application tests.
- Record the actual OS, architecture, artifact versions, commands, and results. Source metadata, successful compilation, and mocks do not prove runtime support. Missing Mac checks stay pending.
- Exercise failure paths relevant to the task: occupied ports, interrupted downloads, process exit, invalid config, and persistent data as applicable.
- Once source exists, use the repository's documented lint/format/type/build/test commands. Add and document these in Stage 1 rather than claiming nonexistent commands pass.
- Finish with changed files, demonstrated behavior, validation, limitations, and the next task. The coordinator updates shared status/queue; workers return a handoff.

## Git and collaboration

- Keep changes scoped and reviewable. Stage explicit task files, inspect the staged diff, and commit when requested or part of the assigned workflow. Never discard unrelated changes or rewrite published history.
- Commit dependency lockfiles and small reproducible fixtures. Exclude downloads, extracted runtimes, databases, secrets, build output, and machine-local state.
- For authorized parallel work, follow `docs/AGENT_WORKFLOW.md`. Assign non-overlapping write ownership. In a shared checkout, workers must not switch branches, stage, commit, merge, reset, or stash.
- The coordinator owns shared manifests/lockfiles, core interfaces, root instructions, and shared status files unless explicitly reassigned. Workers propose changes to those files rather than racing to edit them.
- Delegate independent work only when parallel agent work is requested/authorized. This file does not require spawning agents for every task. Do not recursively delegate without a coordinator-assigned scope.

## Current commands

Repository inspection: `git status --short --branch`, `git diff --check`, `git diff --stat`.

Canonical checks are `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`. Stage 0 verification scripts remain the package probes. The desktop UI build is `npm --prefix apps/desktop run build`.
