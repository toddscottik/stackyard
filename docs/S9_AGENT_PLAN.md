# Stage 9 worker execution plan

Closed: 5 November 2026 after coordinator integration and Windows acceptance. This records the bounded assignments; current contracts are in decisions/0008-project-scopes.md.

Coordinator owns schema/API decisions, store migrations, preview, adapters and final review. Workers use a fresh short context, read this file and only relevant code, do not delegate, and return concise findings rather than repeating documentation. No Git mutations. Preserve all interrupted work and the concurrent CI resolver tests in apache.rs/WORK_QUEUE.md.

Read repository AGENTS.md, Stage 9 of DEVELOPMENT_PLAN.md, the current WORK_QUEUE.md, and relevant dependency policy. Windows is available; Mac acceptance is pending. Existing Stage 8A native fixtures must keep working.

## Frozen types and ownership

`manifest.rs` already declares Manifest.schema/scopes; ScopeSpec(id,directory,runtime,manager,lockfile_root,environment); ServiceSpec.scope_id/endpoint/readiness plus legacy aliases. EnvironmentOwnership is Managed for Python only and Project for Node/PHP. EndpointProtocol is Http/Tcp/FastCgi (serialized fastcgi), EndpointSpec.port 0 means auto. ReadinessSpec is Http(path,statuses,timeout_secs), Tcp(timeout_secs), or Process(grace_ms). Retain these names.

Scope selection resolution is exact scope → project → global default; never persist inherited defaults merely by viewing. Schema 1 uses default for a single runtime and legacy-ROLE for mixed runtimes, existing project manager semantics, existing environment location. Schema 2 uses explicit scopes/managers, service references and explicit readiness. New Python environments are data/environments/PROJECT/scopes/SCOPE; no legacy venv moves. PHP profiles stay separate from Composer, using the legacy project key for schema 1 and `pLEN-PROJECT-SCOPE` for schema 2.

The coordinator supplies Store schema 8 and APIs: get_scope_selection(project,scope,role), select_scope_package(project,scope,role,package), scope_selections(project), get_scope_environment(project,scope), bind_scope_environment(project,scope,path), get_service_observation(project,service), upsert_service_observation(record). Records are already declared. Migration 8 is coordinator-owned and complete at handoff; workers do not edit store.rs.

## Worker A: manifest (Luna)

Write only crates/core/src/manifest.rs and docs/evidence/S9-MANIFEST.md. Finish the existing partial parser. Schema 2 syntax is the example in decisions/0008-project-scopes.md: [[scope]], [[service]] scope reference, endpoint table with auto/numeric port, readiness table. Reject top-level manager for schema 2, unknown fields, wrong managers, duplicate IDs, invalid scope references, escaping/prefix paths, conflicting owners, HTTP readiness on non-HTTP endpoints, process endpoints and port placeholders without endpoints. Python Project environments are unsupported. Schema 1 normalization must preserve existing behavior (PHP-CGI remains FastCgi/Tcp). Parser is pure: no filesystem, commands, environment creation or downloads. Add focused tests and document exact behavior; do not change the frozen public types without asking coordinator.

## Worker B: scope execution (Sol, medium)

Write only crates/core/src/scopes.rs, deps.rs, project.rs; crates/engine/src/control.rs; crates/engine/tests/scopes.rs; docs/evidence/S9-SCOPES.md. Reuse partial scopes.rs. Functions: resolve_scope_runtime(data,project,scope,role), resolve_scope_tool(...,name), select_for_scope(...,package), scope_environment_dir(...,schema), scope_profile_key(...,schema), validate_scope_ownership(root,manifest), resolve_dependency_scope(root,manifest,scope_id,directory). Validate canonical containment/junctions and actual lockfile authority. Preserve legacy --scope directory meaning.

Add DepRequest.scope_id:Option<&str>, preserve scope directory, choose per-scope runtime/tool/manager/environment. Add control::start_dependency_scoped and start_recovery_scoped, serialize conflicting dependency writers by canonical owner/environment, allow independent scopes. Launch uses scope selections; Launch.env_remove removes inherited PORT/FLASK_RUN_PORT for portless workers while preserving explicit service env. Coordinate this with readiness worker.

Views: ProjectView.scopes:Vec<ScopeView{id,directory,runtime,manager,manager_detail,selections}>; ServiceView adds scope_id, readiness_state, http_status, endpoint:Option<EndpointView{protocol,requested_port,resolved_port}>. Existing fields remain. Read observation state from Store; active endpoint exists only for an active process. Do not create environments in views.

Tests: real disposable Node/pnpm and Python/pip or uv scopes, dependency isolation, per-scope selection, legacy reopen/environment identity, invalid references/manager/tool and escaping directories. Exact native artifacts are already under .artifacts; no new downloads needed. Fixtures may support process logic but do not count as vendor runtime evidence.

## Worker C: readiness (Luna)

Write only crates/engine/src/readiness.rs, supervisor.rs, bin/fixture.rs; crates/core/src/apache.rs; crates/engine/tests/readiness.rs; docs/evidence/S9-READINESS.md. Reuse partial readiness.rs. Wait::{Ready,Cancelled,Exited} carries last http_status. Supervisor dispatches typed readiness, does not reserve/inject a port for workers, records pending/ready/failed/not_checked separately from process state. HTTP checks observe raw status without redirects; TCP only connects; process readiness requires survival through grace. Keep process groups, bounded restart/cancel, PHP reconfigure rollback and FastCGI behavior.

Store observation record fields: project_id,service_id,readiness_state,http_status. Publication must reject proxy-to-worker/TCP; PHP FastCGI handler stays separate. Preserve unrelated resolver CI tests in apache.rs. Add bounded fixture modes if needed (worker/env marker, raw TCP, exact HTTP status, delayed crash) without changing old modes. Tests cover worker/no PORT, TCP/no HTTP, redirect allowed explicitly, strict health failure retains status, crash final output, occupied port and legacy checks.

## Handoff/checks

Notify coordinator when types/API become available or blocked by another assigned file. Avoid repeatedly running the whole workspace during shared edits. Run focused checks after the coordinator says shared compilation is ready; final cargo test/clippy/UI build belong to coordinator. Return changed files, commands/results, unresolved issues and exact API deviations in a short handoff. Substantial evidence goes in your assigned S9 note.
