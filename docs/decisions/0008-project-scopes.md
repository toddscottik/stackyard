# Project scopes, endpoint readiness and pure preview

Status: **proposed; preparatory Stage 9.1 draft**, 4 November 2026. Stage 8A is still partial. This note does not accept Stage 9, implement a migration, or change the current manifest/API. The coordinator must review and freeze these examples before assigning 9.2–9.4 writes.

## Context and concrete problem

The working engine, package installer, process groups, dependency execution and PHP profile probes are reused. A mixed Node/Python/PHP application needs separate dependency ownership without adding another supervisor or package architecture.

Audit of the current checkout (concurrent Stage 8A edits may continue after this snapshot):

| Current source | Existing contract to preserve | Required change |
| --- | --- | --- |
| `crates/core/src/manifest.rs` | Schema 1; project `manager`; service runtime, argv, relative cwd, dependency order, restart, `port`, `ready_path`, timeout; static/proxy site | Normalize schema 1 and proposed schema 2 into one internal representation with explicit scopes and optional endpoints |
| `crates/core/src/deps.rs` | Ecosystem command arrays, lockfile/manager checks, contained directories, manifest fingerprint | `DepRequest.scope` currently means directory; add stable scope identity; manager/lockfile root/environment must resolve per scope |
| `crates/core/src/project.rs` | Project import, package resolution, command rewrite, containment, app-owned venv, service views | Resolve runtime/tools/environment via service scope; retain project/global selection fallback |
| `crates/core/src/store.rs` | Database schema 6; `selections(project_id, role)`; selection history; project roots and process identity | Add scope-specific selections/environment binding/history and observed readiness without destructive rewrites |
| `crates/engine/src/control.rs` | Cancellable dependency/exec tasks with process-group execution and bounded logs | Resolve one scope for each action; prevent concurrent writers to the same dependency/environment owner |
| `crates/engine/src/supervisor.rs` | Cancellation, rollback, restart, recorded identity and job/process-group cleanup | Stop allocating a port and issuing HTTP for every service; dispatch typed readiness |
| `crates/core/src/apache.rs`, `crates/engine/src/front.rs` | One shared Apache configuration; `upstream_for` currently accepts any service port | Require declared HTTP endpoint, independent of readiness method |
| `crates/core/src/protocol.rs` | API 1; dependency actions use `scope` directory; `Response.error` is optional string | Add explicit scoped commands, structured diagnostics and a preview result; preserve legacy request meaning |
| `crates/core/src/php.rs`, `php_project.rs` | Exact PHP identity/artifacts; candidate ini probe and atomic active pointer; project profile under `php/profiles/<project>` | Retain profile as extension activation, separate from Composer dependency scope; add scoped binding only where independently selected PHP runtimes require it |
| `apps/desktop/src/api.ts`, Tauri adapters, CLI | Handwritten boundary types, dependency `--scope DIR` | Add explicit `scope_id` contract and synchronize adapters; no broad UI work in Stage 9 |

Important limits: `plan_dependency` is not pure. It can create a pip venv, probe pnpm versions, and invoke PHP identity/profile verification through Composer planning. `Store::open` applies migrations. Neither may run inside a side-effect-free preview. Current readiness reserves a loopback port for every process and accepts only HTTP 2xx, including legacy defaults `ready_path = "/"`, timeout 15 seconds. `ServiceView.state` and port currently cannot express a healthy worker or HTTP status separately. Current dependency tasks do not establish a lock per stable scope.

## Proposed decision

### Shareable schema 2

Declare scopes with stable identifiers, contained relative working directories, one runtime role, manager and lockfile authority. Services reference a scope; runtime is derived from it in the normalized internal model. A command's working directory is relative to project root and must be within its owning scope directory. Dependency actions target the scope's lockfile root; package subdirectories within an npm/pnpm workspace are an explicit package selector rather than independent manager/environment owners.

```toml
schema = 2
name = "mixed-app"

[[scope]]
id = "frontend"
directory = "web"
runtime = "node"
manager = "pnpm"
lockfile_root = "web"

[[scope]]
id = "backend"
directory = "api"
runtime = "python"
manager = "pip"
lockfile_root = "api"
environment = "managed"

[[service]]
id = "web"
scope = "frontend"
command = ["pnpm", "dev", "--host", "127.0.0.1", "--port", "{port}"]
working_directory = "web"
endpoint = { protocol = "http", port = "auto" }
readiness = { kind = "http", path = "/", statuses = [200, 301, 302, 307, 308], timeout_secs = 15 }

[[service]]
id = "api"
scope = "backend"
command = ["python", "-m", "uvicorn", "app:app", "--host", "127.0.0.1", "--port", "{port}"]
working_directory = "api"
endpoint = { protocol = "http", port = "auto" }
readiness = { kind = "http", path = "/health", statuses = [200], timeout_secs = 15 }

[[service]]
id = "worker"
scope = "backend"
command = ["python", "worker.py"]
working_directory = "api"
readiness = { kind = "process", grace_ms = 500 }

[site]
service = "web"
```

This example is proposed syntax, not an executable fixture today. `environment = "managed"` describes ownership, not a filesystem path; Node/Composer dependencies stay with their ecosystem in the scope directory. Only Python scopes may set that field initially. New profiles write explicit managers and readiness; an importer may report unresolved manager choice until lockfile conflicts are resolved. Scope IDs use existing lowercase identifier rules and never change merely because a directory is renamed. A manager must match runtime. `lockfile_root` is project-relative, contains the scope directory, and remains within the project; reject conflicting managers owning the same canonical lockfile root. Reject unknown fields, duplicate IDs, missing scope references, paths escaping via `..`, absolute/prefix paths and existing symlinks/junctions outside the project. Existing canonical containment helpers can be reused with explicit prefix checks.

Machine package IDs, paths, environment locations, PHP profile revisions, credentials and observed ports remain outside this file. Selected runtime and tool IDs belong to local scope bindings. A project may have a Composer scope and a Node scope, each with its own manager. PHP extensions are not Composer packages; a PHP scope refers to an independently validated extension profile through local state. Do not replace `PhpIdentity` or lower its ABI checks.

### Minimal local-state additions and migration

Use one coordinator-owned migration after the final Stage 8A schema version is known (currently 6; proposed next migration 7). Keep `selections` and history intact as legacy/project/global defaults. Add `scope_selections(project_id, scope_id, role, package_id)` with composite primary key; `scope_environments(project_id, scope_id, owner, path)` for local environment binding; and scope-specific selection history. Validate scope existence and package role/target in the shared service before storing a selection. Package-removal/reference checks must include scoped selections and active PHP profiles, and runtime switching must revalidate profiles for every affected PHP scope before changing any binding.

Resolution order: exact scope binding, existing project role selection, existing global default. Never copy the default value into every scope merely by listing or previewing; inherited defaults must continue to behave as inherited defaults. A scope override affects only that scope. Existing `select_package(project_id, role, ...)` remains the project-default operation until its callers are migrated deliberately. Record the resolved selection origin in views.

New Python scopes allocate environments under `environments/<project>/scopes/<scope>` only during an intentional dependency operation. Bind an existing schema-1 Python environment to its legacy scope at its original `environments/<project>` path. Do not move it: venv launchers may contain absolute paths. Do not silently adopt an arbitrary project `.venv`. Removing a scope preserves its environment/data until a separate explicit cleanup action.

Legacy manifest migration is an in-memory adapter, not an automatic file rewrite:

1. A single-runtime schema-1 manifest normalizes to scope `default` rooted at `.`. Retain exact manager detection/lockfile conflict behavior, selected runtime/tool fallback and environment path. All old services reference `default`.
2. A mixed-runtime schema-1 manifest normalizes to deterministic `legacy-node`, `legacy-python`, `legacy-php` scopes at `.` for the roles it actually uses. The old project manager binds only its compatible runtime; scopes without a resolved manager remain explicitly unresolved for dependency actions. Do not infer permission to install additional dependency sets. Services can still launch with their original runtime selections/environment. If the old mixed manifest has no manager, retain its dependency-choice error until the user chooses scopes; loading must not silently choose one manager for all roles.
3. Preserve each legacy service's HTTP endpoint/auto-port, readiness path, 2xx acceptance, timeout, restart and dependency order exactly. No redirects become ready unless schema 2 explicitly permits them. Preserve legacy cwd behavior even where it differs from stricter new scope containment.
4. The existing directory-valued `scope`/`--scope DIR` remains a legacy adapter; do not reinterpret `"api"` from a path into an ID. New requests/CLI use `scope_id`/`--scope-id`. For schema 2, legacy requests work only when a single unambiguous owner contains the requested directory; otherwise return `scope_required`. Never pick the first scope.

Migration/reopen runs transactionally and preserves package IDs, selection history, process identity records and PHP `active.json` pointers. Existing DB processes with a historical port do not prove new readiness; initialize new observation fields as unknown on reopening. Retain old project profile as the legacy PHP binding, without rewriting/copying artifacts. For initial schema-2 projects, one project PHP profile may be explicitly shared by PHP scopes only when all use the compatible selected identity; selecting independent identities requires a new profile binding, not blind reuse. Freeze the minimal profile-binding API before worker assignment.

### Process, endpoint and readiness are separate

Use `EndpointSpec = None | { protocol: Http | Tcp, port: Auto | Fixed }` and `ReadinessSpec = Process { grace_ms } | Tcp { timeout_secs } | Http { path, statuses, timeout_secs }`. Endpoints bind loopback by default. A process without an endpoint gets no port reservation or injected `PORT`/`FLASK_RUN_PORT`; reject `{port}` substitution for such commands. Avoid changing intentional user-provided environment variables as a side effect of representing a worker.

HTTP readiness requires an HTTP endpoint. TCP readiness may check either declared TCP or HTTP endpoint, but makes no claim about HTTP health. A worker has no endpoint and process readiness means only that its owned process survives the configured bounded grace period; later exit still follows existing lifecycle/restart policy. This is not a semantic application-health guarantee. Reject TCP readiness without an endpoint and HTTP-only fields on other readiness kinds. Dependent services start after the selected readiness condition, reusing existing rollback/cancellation.

Read HTTP status directly with bounded reads/deadlines; do not follow redirects. An explicit status allowlist can accept a redirecting app, while `/health` with `[200]` remains strict. Keep the observed status even on failure. New views carry `process_state`, optional declared/resolved endpoint, `readiness_state` and optional `http_status`, separate from detail text. Preserve the existing `state` alias during client migration. Stopped processes have no active endpoint even if a declared fixed port is known. Apache accepts only a declared HTTP endpoint, regardless of whether readiness is HTTP or TCP; reject proxy-to-worker/TCP during validation and publication, not after generating a bad config.

### Proposed API examples

Keep the API 1 envelope for additive operations; if maintainers choose a breaking removal, bump API once with all bundled clients. Never silently change the meaning of existing JSON `scope`.

```json
{"v":1,"id":"r1","cmd":{"op":"dep_install_scoped","project_id":"mixed-app","scope_id":"backend"}}
```

```json
{"v":1,"id":"r1","result":{"kind":"task","id":"task-42","label":"deps install mixed-app/backend","state":"running","message":"running","project_id":"mixed-app","scope_id":"backend"}}
```

Add/remove/update/list/recover receive the same explicit identity. Reuse cancellation and operation IDs. Include `scope_id`, manager, manifest/lockfile owner and environment identity in structured task metadata instead of relying on label parsing. Serialize mutating jobs by canonical dependency owner and environment, not merely by service ID; workspace siblings can share one owner. Independent frontend/backend owners may run concurrently. Do not claim durable task history from the current in-memory `TaskBoard` (Stage 10 addresses activity persistence/reconnect gaps).

```json
{"v":1,"id":"r2","cmd":{"op":"select_scope_package","project_id":"mixed-app","scope_id":"backend","role":"python","package_id":"python-verified-pin"}}
```

The package ID is illustrative local state, not a proposed catalog pin. A project view adds `scopes` and each service's `scope_id`, selection origin and typed readiness/endpoint fields; the legacy manager summary can remain empty with `manager_detail = "multiple scopes"` for old clients. CLI JSON uses Rust snake_case; Tauri adapters retain their existing explicit camelCase mapping and must be tested against the same DTO examples.

### Pure preview and structured diagnostics

Implement a separate pure evaluator, for example `preview_project(draft, context) -> PreviewResult`, where immutable context contains already recorded package selections/identities, bounded project-file metadata/content snapshots and known profile metadata. Build that context using read-only filesystem/database access outside the evaluator; never open/create a store with migrations during preview. An unimported draft must not create a project or engine data directory. No launch/probe, venv creation, dependency execution, download, port bind, file write or environment mutation is permitted.

```json
{"v":1,"id":"r3","cmd":{"op":"preview_project","root":"D:/projects/mixed-app","manifest_text":"schema = 2\nname = \"mixed-app\"\n...","selection_overrides":[{"scope_id":"backend","role":"python","package_id":"python-verified-pin"}]}}
```

```json
{"v":1,"id":"r3","result":{"kind":"project_preview","can_apply":false,"scopes":[{"id":"backend","manager":"pip","runtime_package_id":null,"environment_state":"not_created"}],"services":[{"id":"api","endpoint":{"protocol":"http","requested_port":"auto","resolved_port":null},"readiness":{"kind":"http","path":"/health","statuses":[200]}}],"diagnostics":[{"code":"runtime_missing","severity":"error","field":"scope.backend.runtime","scope_id":"backend","message":"Select a Python runtime for backend."},{"code":"auto_port_unresolved","severity":"info","field":"service.api.endpoint.port","service_id":"api","message":"A port will be chosen at start."}]}}
```

The abbreviated `manifest_text` above illustrates the envelope; executable acceptance fixtures must send the complete TOML example. Preview reports unknown/unprobed artifact identities honestly and reports fixed-port availability as unchecked; neither type of port is reserved. Execution revalidates containment, selection, profiles and ports because snapshots can become stale. Missing files are structured errors/warnings rather than a reason to create placeholder directories. Redact secret environment values from result/errors.

Add `diagnostics: Vec<Diagnostic>` with stable `code`, `severity`, `field`, optional scope/service IDs and human `message`; reserve codes including `scope_unknown`, `scope_required`, `path_outside_project`, `manager_conflict`, `runtime_missing`, `tool_missing`, `profile_incompatible`, `endpoint_required`, `http_endpoint_required`, `port_unchecked`. Preserve `Response.error: String` as a readable legacy summary and add optional `error_detail` for operational failures; preview validation failures are an ordinary structured result with `can_apply = false`. Do not expose internal filesystem values or command output as an error code.

## Minimum implementation order and file ownership

1. Coordinator freezes DTOs/schema examples and migration contract in `manifest.rs`, `protocol.rs`, `store.rs`, exports, Tauri adapter and CLI parsing boundaries. Store schema/migration is one owner. Tests reopen a schema-6 fixture with exact runtime/tool selections/history/environment/profile references preserved.
2. Scope worker owns new `crates/core/src/scopes.rs` and focused scope fixtures/tests; coordinator delegates changes to existing `deps.rs`/`project.rs` together because manager, environment and launch resolution must agree. `control.rs` gets one owner for executor scope routing/concurrency. This worker does not edit supervisor/readiness files.
3. Readiness worker owns new `crates/engine/src/readiness.rs`, disposable worker/TCP/redirect/health fixtures and focused tests after shared enums are frozen. Coordinator owns integrating it into `supervisor.rs`, `apache.rs` and `front.rs` (or explicitly assigns those files together). Do not concurrently assign `supervisor.rs` to dependency and readiness workers.
4. Preview worker owns new `crates/core/src/preview.rs` and pure input/output tests, taking the frozen scope/readiness types as inputs; coordinator alone wires protocol dispatch and read-only context acquisition. This work can overlap step 3 once shared types exist.
5. CLI/desktop boundary verification may use dedicated contract fixtures after the DTO freeze. Existing `apps/desktop/src/api.ts`, `App.tsx`, `src-tauri/src/lib.rs` and engine dispatch `lib.rs` each need one named owner; large UI redesign remains Stage 10. No worker independently updates shared status/catalog/manifests/lockfiles.

Concrete filenames for new modules above are proposed boundaries, not declarations that those files already exist. Downstream recipe workers can later own disjoint fixture folders (`fixtures/node-http`, `next-app`, `flask-app`, `fastapi-app`) only after runnable creation contracts in Stage 11; this draft does not authorize creating starters now.

## Acceptance and evidence

This note was checked against the source files listed above on Windows 10 x64. It contains **no implementation/runtime acceptance** for Stage 9. Documentation validation is relative-path existence and `git diff --check`; compilation and application tests are not claimed for a proposal.

Required future checks: schema-1 single/mixed manifests; schema-6 state reopen without lost/default-frozen selections; pnpm frontend plus pip/uv backend with independent environments/lockfiles; npm/pnpm workspace owner conflict; manager/tool/runtime missing; scope reference/cwd escape including symlink/junction; background worker without port; TCP service without HTTP path; redirecting HTTP accepted only by explicit statuses; strict health mismatch retains observed status; proxy-to-worker/TCP refusal; duplicate/cancelled dependency jobs; PHP profile revalidation before scoped runtime switch. Pure preview must run against an unimported draft and read-only fixture with spawn/download/write/bind hooks that fail if invoked; snapshot output/state must be identical before/after, including nonexistent engine data path. Target execution on both Windows 10 x64 and macOS 14.5 arm64 remains required.

## Alternatives considered and compatibility impact

Do not use a project-wide manager for a mixed app, reinterpret directory strings as scope IDs, auto-create environments during preview, move old venvs, or treat all listeners as HTTP. A second launcher/store would duplicate existing ownership and is rejected. Add normalized scope models and a small local-state extension to the existing shared services instead.

If accepted, this extends the scope/readiness portions of [0002-project-runner.md](0002-project-runner.md); process ownership, cancellation, dependency ecosystem authority and no-shell launch decisions remain. No earlier decision is superseded by this preparatory draft alone.
