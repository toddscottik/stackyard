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
| S8A-PHP | Windows accepted / coordinator; Mac pending | Windows PHP patches/Composer, FastCGI, live PHP-only rollout, verified memcached extension and guarded native server completed | Full native gates and actual engine catalog download/set-get passed; Mac checks stay pending |
| S9-CONTRACTS | Preparatory audit / coordinator | 9.1 proposal in decisions/0008-project-scopes.md; 9.2–9.4 not implemented | Review/freeze contracts against 8A; retain its artifact/platform gates |
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

## Stage 8A assignments — 4 November 2026

| Slice | Owner | Exclusive writable paths | Acceptance |
| --- | --- | --- | --- |
| S8A-ARTIFACTS | Reviewed / artifact worker | scripts/verification/windows/php.ps1; docs/evidence/S8A-ARTIFACTS.md | PHP/Composer publisher digests and CLI/CGI launches passed; memcached integrity blocked, Mac pending |
| S8A-PROFILES | Reviewed / profile worker | crates/core/src/php.rs; crates/engine/tests/php.rs; docs/evidence/S8A-PROFILES.md | Native ABI/profile rollback/isolation and PHP/Composer/Apache HTTP+HTTPS integration passed on Windows |
| S8A-MEMCACHED | Reviewed, managed blocked / cache worker | crates/core/src/memcached.rs; scripts/verification/windows/memcached.ps1; docs/evidence/S8A-MEMCACHED.md | Seven protocol tests passed; native Windows candidate failed occupied-port refusal and was cleaned up/excluded |
| S8A-WINDOWS-REGRESSIONS | Reviewed / artifact worker | crates/engine/tests/projects.rs; crates/engine/src/bin/fixture.rs; crates/core/src/client.rs; crates/engine/tests/cli_startup.rs; docs/evidence/S8A-WINDOWS-REGRESSIONS.md | Copied npm fixture/read-only pip version fixes passed; deterministic autostart baseline failure now passes with no inherited pipe handles; real CLI workflow passed |
| S9.1-AUDIT | Proposed / cache worker | docs/decisions/0008-project-scopes.md | Source audit and JSON/TOML examples checked; no schema migration or runtime acceptance claimed |
| S8A-INTEGRATE | coordinator | Shared models/protocol/store/engine/CLI/catalog/manifests/status and integration fixtures | Review workers; wire PHP/Composer/profiles; validate combined behavior and retain target gaps |

## Resumed Stage 8A Windows completion — 4 November 2026

The user explicitly directed continued implementation without waiting for Mac access. Mac execution stays pending for the user's later check. Complete independent Windows work; artifact research failures are inputs to provider/build work, not a reason to stop.

| Slice | Owner | Exclusive writable paths | Acceptance |
| --- | --- | --- | --- |
| S8A-FASTCGI | Reviewed / FastCGI worker | crates/core/src/apache.rs; crates/engine/src/front.rs; crates/engine/tests/php_fastcgi.rs; docs/evidence/S8A-FASTCGI.md | Native Apache PHP document roots/.htaccess/POST/HTTPS through selected profiled php-cgi; no source leakage and invalid config rollback |
| S8A-RECONFIGURE | Reviewed / PHP worker | crates/core/src/php.rs; crates/engine/src/supervisor.rs; crates/engine/tests/php_reconfigure.rs; docs/evidence/S8A-RECONFIGURE.md | Probe before promotion; restart only affected PHP services on same ports; rollback failed activation/restart; PHP-CGI TCP readiness |
| S8A-NATIVE-CACHE | Reviewed / Native artifact worker | scripts/verification/windows/php-memcached.ps1; scripts/verification/windows/memcached-native.ps1; docs/evidence/S8A-NATIVE-CACHE.md | Resolve publisher verification/build route for real PHP memcached and safe native server; no guessed pins, no execution before approved provenance/digest |
| S8A-COMPLETE-INTEGRATE | coordinator | Shared models/protocol/manifest/project/php_project/catalog/CLI/exports/status; new engine cache provider/test files | Freeze interfaces, native server safety, package/profile operations, review and combined native acceptance |

| S8A-CACHE-PROVIDER | Reviewed / cache worker | crates/engine/src/cache.rs; crates/engine/tests/cache_transport.rs; docs/evidence/S8A-CACHE-PROVIDER.md | Native private socket/exclusive TCP, crash/retry/cancel/ownership/volatile restart passed |

Windows Stage 8A now has complete native provider acceptance; no Mac pass is implied. Continue Stage 9 without waiting for the deferred target run. See the resumed handoff in IMPLEMENTATION_STATUS.md.
