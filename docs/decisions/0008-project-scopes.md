# Project scopes, endpoints, readiness and preview

Status: accepted Stage 9 implementation decision, 4 November 2026. Windows evidence is separate; Mac execution remains pending.

## Audit and decision

Reuse shared Rust services, store, dependency planners, package providers, PHP ABI/profile probes, process groups and supervisor. Schema 2 adds scopes; schema 1 normalizes in memory without rewriting its manifest. The per-user engine remains the single process owner.

| Existing boundary | Addition |
| --- | --- |
| Service runtime/project manager | Stable scopes with runtime, manager, directory and lockfile owner |
| Project/global selections | Exact scope bindings; resolution scope -> project -> global without copying defaults |
| Directory-valued dependency scope | Explicit scope_id commands; existing directory meaning retained |
| Store schema 7 | Transactional migration 8: scope selections/history/environments and readiness observations |
| Immutable PHP profiles/active pointer | Legacy project key preserved; schema 2 uses p<project-length>-<project>-<scope> |
| Port and HTTP check for every process | Optional endpoint, explicit readiness, separate observed status |
| String errors/handwritten clients | Optional structured error detail, pure preview, synchronized client adapters |

## Shareable schema

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
working_directory = "web"
command = ["node", "server.js"]
endpoint = { protocol = "http", port = "auto" }
readiness = { kind = "http", path = "/", statuses = [200, 302], timeout_secs = 15 }

[[service]]
id = "worker"
scope = "backend"
working_directory = "api"
command = ["python", "worker.py"]
readiness = { kind = "process", grace_ms = 500 }

[site]
service = "web"
```

The example requires the source files named in commands; it is not a generated starter. Schema 2 requires explicit compatible managers and readiness. Python uses managed environments; Node/PHP retain project-owned ecosystem dependencies. Machine package IDs, executable paths, resolved ports, credentials and PHP profiles stay in local state. PHP extensions remain separate from Composer dependencies.

Lockfile roots CONTAIN scope directories, including an appropriate workspace ancestor. Canonical paths define authority. Reject escapes/junctions outside project, conflicting owners, competing npm/pnpm locks and lockfiles beneath a different declared owner. Npm ancestor workspace detection is conservative: declare that ancestor as owner rather than assume membership from its patterns. Mutating dependency jobs serialize by canonical lockfile owner and managed environment; independent owners can run concurrently. No implicit manager conversion occurs.

## Compatibility and migration

Migration 8 retains packages, project/global selections/history, process identity and all existing data. New readiness is unknown/not checked rather than inferred from a historical port. Scope bindings have separate history and participate in removal protection. Active PHP pointers protect referenced binaries/extensions even after manifest edits.

Schema 1 single-runtime manifests normalize to default; mixed manifests use legacy-node, legacy-python, legacy-php. The original manager applies only to its compatible runtime. Unresolved mixed dependency choices remain explicit errors. Commands, cwd, order, restart, 2xx HTTP readiness and PHP-CGI FastCGI/TCP behavior remain. No automatic file rewrite occurs.

Legacy Python environments stay at environments/<project>; new environments use environments/<project>/scopes/<scope>. Bindings are recorded by intentional dependency operations. Views and previews do not create environments or freeze inherited selections. Removing a scope preserves its data. PHP scope profiles are independent: selection verifies the active profile before writing a binding. Project/global PHP changes validate all affected inheriting scopes and preserve exact overrides.

## Endpoints and readiness

An endpoint is optional, or protocol http/tcp/fastcgi plus an auto/fixed port. Auto normalizes to zero. Endpoints are loopback; fixed ports are unprivileged. Portless workers get no reservation or inherited PORT/FLASK_RUN_PORT; explicit service environment values remain. Port placeholders require an endpoint.

Readiness is HTTP with path/status allowlist/timeout, TCP with timeout, or process survival through a grace period. HTTP reads raw status without following redirects; failed checks retain last observed status. TCP makes no HTTP-health claim. Process readiness requires no endpoint. Lifecycle and readiness are separate; inactive services have no active resolved port. Apache proxy and Open URL require HTTP endpoints; PHP FastCGI handling stays distinct.

## Additive API and boundaries

API version 1 remains. Existing directory-valued scope is unchanged:

```json
{"v":1,"id":"r1","cmd":{"op":"dep_install_scoped","project_id":"mixed-app","scope_id":"backend"}}
```

Install/add/remove/update/list/recover have scoped variants. Selection infers role from the checked package record:

```json
{"v":1,"id":"r2","cmd":{"op":"select_scope_package","project_id":"mixed-app","scope_id":"backend","package_id":"local-python-id"}}
```

Scoped PHP status/apply/install-extension use the same IDs. Legacy PHP commands resolve one unambiguous PHP scope or report scope_required. CLI: select-scope PROJECT SCOPE PACKAGE; deps install PROJECT --scope-id SCOPE; php profile PROJECT --scope-id SCOPE EXTENSION... . Legacy --scope DIR remains a directory and cannot combine with --scope-id.

Project views add scopes and per-service scope_id, readiness, readiness_state, http_status and endpoint with requested/resolved ports. Existing fields remain. Selection origin identifies inheritance; source retains package provenance. Tauri project-view fields map explicitly to camelCase; the nested readiness union and preview retain core snake_case DTOs verbatim in TypeScript. Tasks retain operation ID/label/state/message; durable activity and richer task-resource metadata are Stage 10.

Errors retain error for old clients and add optional error_detail with code, severity, field, optional scope/service IDs and message. Preview returns that diagnostic shape in a list. Invalid manifest diagnostics do not echo draft values.

## Pure preview

preview_project(text, context, overrides) is deterministic over immutable snapshots. Read-only collect_context supplies package files, inherited selections, canonical directories and bounded metadata. Store::open_read_only never creates/migrates/retargets state. CLI preview ROOT [DRAFT_FILE] and Tauri preview_project bypass autostart; the engine also exposes PreviewProject.

```json
{"root":"PROJECT_ROOT","manifest_text":"DRAFT_TOML","selection_overrides":[]}
```

Results contain can_apply, scopes, services and diagnostics. Missing runtimes/tools, invalid overrides/owners and metadata conflicts prevent apply. Auto ports remain unresolved with informational diagnostics; fixed ports are not reserved/probed. can_apply means no known error in the snapshot, not a runtime health guarantee. Draft/package metadata limits are 64 KiB. Preview never invokes dependency planning, version/PHP probes, downloads or environment creation. Apply/launch rechecks files, versions, PHP profile/ABI and ports.

## Evidence and downstream boundary

See [manifest evidence](../evidence/S9-MANIFEST.md), [scope evidence](../evidence/S9-SCOPES.md), [readiness evidence](../evidence/S9-READINESS.md) and [integration evidence](../evidence/S9-INTEGRATE.md). Acceptance includes native pnpm/Python isolation, migration/reopen, worker/TCP/redirect/strict health, invalid references/tools/paths, pure preview and adapter serialization. Mac acceptance remains pending.

This extends [the project runner decision](0002-project-runner.md) while retaining process ownership/cancellation. Stage 10 builds the desktop workspace and activity layer. Import detection, recipes, configuration apply and broad UI changes remain later stages. A second launcher/store was rejected because it duplicates existing ownership.
