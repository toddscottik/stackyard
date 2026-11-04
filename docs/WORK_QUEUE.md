# Work queue

The coordinator owns this file. Status is separate from stage completion. Claim a task by setting an owner and status before assigning writes. Stage 0 Windows probes and integration were completed on 31 October 2026. Mac execution remains blocked on the M1 Pro.

| ID | Status / owner | Scope and writable paths | Acceptance / dependency |
| --- | --- | --- | --- |
| S0-WIN-RUNTIMES | Done / coordinator | `scripts/verification/windows/runtimes.ps1`, `docs/evidence/S0-WIN-RUNTIMES.md` | Windows 10 x64 accepted 31 October 2026. Managed Node 24.22.2, CPython 3.12.16, uv 0.13.6, and pnpm 12.16.1 matched publisher digests and launched. Local Node 24.13.0 and Python 3.12.3 were reused unchanged. macOS launch remains pending |
| S0-WIN-SERVERS | Done / coordinator | `scripts/verification/windows/servers.ps1`, `docs/evidence/S0-WIN-SERVERS.md` | Windows 10 x64 accepted 31 October 2026. Apache 2.4.69, MySQL 8.4.13, and PostgreSQL 18.6 launched on loopback against disposable data. No Windows service was registered. MySQL integrity is MD5-only. Postgres is not in the upstream Windows 10 certification table |
| S0-MAC-PACKAGES | Blocked / coordinator | `scripts/verification/macos/`, `docs/evidence/S0-MAC-PACKAGES.md` | Research accepted 31 October 2026. Execution is blocked on the M1 Pro. Apache and MySQL archive URLs are unresolved. The PostgreSQL Mac zip has no published digest |
| S0-INTEGRATE | Done / coordinator | `docs/PACKAGE_VERIFICATION.md`, `docs/IMPLEMENTATION_STATUS.md`, this queue | Windows combinations accepted or limited as recorded in the package verification integration section. Mac checks remain pending or blocked. Fixture convention recorded there |
| S1-BOOTSTRAP | Review / coordinator | `Cargo.toml`, `rust-toolchain.toml`, `crates/core/`, `crates/engine/`, `crates/cli/`, `apps/desktop/`, `.github/workflows/ci.yml`, `README.md`, `docs/evidence/S1-BOOTSTRAP.md`, and the status files | Windows skeleton reviewed 1 November 2026. CLI and desktop share one engine. Mac desktop open, Mac toolchain, and a live second Windows user connection remain pending |
| S2-PACKAGES | Review / coordinator | `catalog/`, `crates/core/`, `crates/engine/`, `crates/cli/`, `apps/desktop/`, `Cargo.toml`, `Cargo.lock`, `docs/evidence/S2-PACKAGES.md`, and the status files | Windows review 1 November 2026. Managed Node 24.22.2 and CPython 3.12.16 coexist with existing Python 3.12.3. uv 0.13.6 and pnpm 12.16.1 are in tool storage. Mac catalog entries remain out until a Mac payload check |
| S3-PROJECTS | Review / coordinator | `crates/core/`, `crates/engine/`, `crates/cli/`, `fixtures/`, `Cargo.toml`, `Cargo.lock`, `docs/evidence/S3-PROJECTS.md`, `docs/decisions/0002-project-runner.md`, and the status files | Linux x86_64 review 1 November 2026. Project processes, npm/pnpm/pip commands, and the four fixtures ran here. Windows 10 and macOS 14.5 process launches remain pending |
| S4-DESKTOP | Review / coordinator | `apps/desktop/`, `crates/core/src/project.rs`, `docs/evidence/S4-DESKTOP.md`, and the status files | Linux review 1 November 2026. Projects, components, settings, logs, and dependency actions call the engine. Tray hide and Quit policy are in the shell. A live Windows or macOS window is pending |
| S5-APACHE | Review / coordinator | `crates/core/`, `crates/engine/`, `crates/cli/`, `apps/desktop/`, `catalog/packages.json`, `fixtures/`, `docs/evidence/S5-APACHE.md`, `docs/decisions/0003-apache-sites.md`, and the status files | Linux review 1 November 2026. Shared Apache, static rewrite, proxy, and generated HTTPS ran against distro Apache 2.4.58. Windows catalog pin was not launched again. macOS Apache, system trust, and ports 80/443 remain pending |
| S6-DATABASES | Review / coordinator | `crates/core/`, `crates/engine/`, `crates/cli/`, `apps/desktop/`, `catalog/packages.json`, `docs/evidence/S6-DATABASES.md`, `docs/decisions/0004-databases.md`, and the status files | Linux review 1 November 2026. Distro MySQL 8.0.46, PostgreSQL 16.15, and SQLite 3.45.1 kept a row across restart and restore. Windows catalog PostgreSQL was not launched again. MySQL has no SHA256 catalog entry. macOS archives and OS keychains remain pending |
| S7-UPDATES | Review / coordinator | `crates/core/`, `crates/engine/`, `crates/cli/`, `apps/desktop/`, `catalog/packages.json`, `docs/evidence/S7-UPDATES.md`, `docs/decisions/0005-updates.md`, and the status files | Linux review 1 November 2026. Notices, one-project switch, cleanup, external drift, and interrupted-install recovery passed. No second digest-checked Node or Python payload was downloaded. macOS pins remain out |
| S8-PERSONAL | Review / coordinator | `crates/core/`, `crates/engine/`, `crates/cli/`, `apps/desktop/`, `docs/SETUP.md`, `docs/KNOWN_LIMITATIONS.md`, `docs/evidence/S8-PERSONAL.md`, `docs/decisions/0006-personal-release.md`, and the status files | Linux review 1 November 2026. Diagnostics redaction, npm and pip fixtures, and pnpm Next.js with the requested pnpm 12.16.1 ran here. Windows 10 and macOS 14.5 acceptance, the user's projects, and sleep/wake remain open |

All workers may read existing documentation. Runtime caches and test data belong under `.artifacts/<task-id>/` and stay out of Git. Root instructions, root dependency manifests/lockfiles, shared APIs, PACKAGE_VERIFICATION.md, IMPLEMENTATION_STATUS.md, and this queue remain coordinator-owned unless explicitly reassigned.

`scripts/verification/windows/runtimes.ps1`, `scripts/verification/windows/servers.ps1`, and `scripts/verification/macos/packages.sh` exist. Each provides help, explicit paths or ports, non-destructive defaults, and structured results. Do not treat missing software or a compiler as a passed check.

## Remaining work — 4 November 2026

Stage 8A is next. Stages 0–8 remain the implemented baseline; their pending target checks are retained. The former broad Stage 9 is replaced by Stages 9–17. Detailed tasks, acceptance, ownership boundaries and handoffs are in DEVELOPMENT_PLAN.md.

| ID | Status / owner | Bounded scope | Dependency |
| --- | --- | --- | --- |
| S8A-PHP | Next / unassigned | PHP, Composer, extension profiles/downloads, Apache integration and Memcached | Existing provider infrastructure and artifact feasibility |
| S9-CONTRACTS | Waiting / unassigned | Define project scopes and launch contracts; execute tasks 9.1–9.4 and its acceptance checks | Stage 8A implementation and existing Stages 0–8. |
| S10-DESKTOP | Waiting / unassigned | Build the shared desktop workspace and activity layer; execute tasks 10.1–10.4 and its acceptance checks | Stage 9 contracts. |
| S11-CREATE | Waiting / unassigned | Create runnable projects from the desktop; execute tasks 11.1–11.4 and its acceptance checks | Stages 9–10 and approved runtime/tool artifacts. |
| S12-IMPORT | Waiting / unassigned | Import applications and edit their configuration; execute tasks 12.1–12.4 and its acceptance checks | Stages 9–11. |
| S13-COMPONENTS | Waiting / unassigned | Unify components, dependencies and PHP extensions; execute tasks 13.1–13.4 and its acceptance checks | Stages 8A and 9–12. |
| S14-DATABASES | Waiting / unassigned | Complete project database workflows; execute tasks 14.1–14.4 and its acceptance checks | Stages 9–10 and 12; existing Stage 6 providers. |
| S15-WEB | Waiting / unassigned | Complete sites, HTTPS and desktop settings; execute tasks 15.1–15.4 and its acceptance checks | Stages 9–10 and 12; 8A PHP integration and existing Stage 5. |
| S16-USABILITY | Waiting / unassigned | Finish desktop usability and failure recovery; execute tasks 16.1–16.4 and its acceptance checks | Stages 10–15. |
| S17-ACCEPTANCE | Waiting / unassigned | Accept the integrated app on Windows and macOS; execute tasks 17.1–17.4 and its acceptance checks | Stages 8A and 9–16; approved artifacts for every advertised target capability. |
| S18-PUBLIC | Deferred / unassigned | Signing, notarization, verified distribution/updater and clean-machine release tests | Stage 17 and distribution/license decisions |

A stage is a sequence of small assignments, not one instruction to implement everything concurrently. Before dispatch, choose one numbered task and assign exact writable paths, inputs, output examples, checks, evidence and a handoff under AGENT_WORKFLOW.md. Coordinator owns schema/protocol/lockfiles/status; no workers are assigned here.

After Stage 9 contracts, independent owners can prepare recipe content, desktop resource infrastructure and platform package checks. Dependency order still governs integration. Stages 13–15 may overlap after their declared dependencies, using disjoint files; shared App.tsx/protocol edits must have one owner.
