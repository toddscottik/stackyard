# Stage 9 scope execution evidence

4 November 2026. Worker B. Windows NT 10.0.19045 x64; Rust 1.99.1, x86_64-pc-windows-msvc. macOS execution remains pending.

## Changed behavior

Assigned files: `crates/core/src/scopes.rs`, `deps.rs`, `project.rs`; `crates/engine/src/control.rs`; `crates/engine/tests/scopes.rs`; this note. Store migration, protocol/adapters, preview, PHP profile adapters and shared exports are coordinator-owned.

- Runtime/tool resolution uses exact scope selection, project selection, then global default. Reads never copy inherited values into scope bindings. Selection validates scope, role, target, executable presence, active scope processes and PHP profile compatibility.
- Schema 1 retains normalized IDs and `environments/PROJECT`; schema 2 uses `environments/PROJECT/scopes/SCOPE`. Intentional Python dependency planning records the original/resolved environment path. Existing virtual environments are not relocated. PHP scope profile keys use `pLEN-PROJECT-SCOPE`, with schema 1 retaining the project key.
- Existing directory-valued `scope` remains a directory; `scope_id` selects a declared identity. Schema 2 dependency commands execute at the declared lockfile owner. Canonical containment rejects escaping directories/junctions, conflicting owners, competing npm/pnpm lockfiles even with an explicit schema-2 manager, nested competing lockfiles and linked dependency metadata escaping its owner. Ancestor pnpm workspace discovery is rejected when it would select another owner. Ancestor npm workspace metadata is conservatively rejected unless its root is declared as owner; npm workspace pattern membership is not inferred.
- Dependency jobs hold canonical owner/environment writer keys, wait cancellably on conflicts and allow independent owners to proceed. Keys canonicalize existing ancestors for not-yet-created environments. Scoped logs use distinct names. Recovery retains its package-script limitation message.
- Service launch resolves its scope runtime, pnpm, managed Python environment and PHP profile. Portless launches use `Launch.env_remove` for inherited `PORT`/`FLASK_RUN_PORT`; explicit service values survive. Views add scopes/selections and endpoint/readiness/status fields without creating environments; stopped services expose no active endpoint. Existing view fields remain.
- uv list explicitly targets the scope environment interpreter; uv project operations retain the selected base interpreter and managed environment variables. Native uv execution was not part of this slice.

## Commands and results

Approved artifacts already present under `.artifacts/S2-PACKAGES` were reused; no runtime downloads were performed:

| Artifact | Actual version | Relative executable |
| --- | --- | --- |
| Node | v24.22.2 | `packages/node/24.22.2/node-v24.22.2-win-x64/node.exe` |
| pnpm | 12.16.1 | `tools/pnpm/12.16.1/pnpm.exe` |
| Python | 3.12.16 | `packages/python/3.12.16+20261026/python/python.exe` |

Set `STACKYARD_TEST_SCOPE_NODE`, `STACKYARD_TEST_SCOPE_PNPM`, and `STACKYARD_TEST_SCOPE_PYTHON` to these absolute executable paths before the native test command. Without those variables, the two native gates report a skip and do not establish native acceptance.

- `cargo test -p stackyard-engine --test scopes --offline -- --nocapture`: six tests passed with all three native variables enabled. The engine IPC `DepInstallScoped` path installed a local pnpm dependency and a locally generated Python wheel into different scopes. Both selected-runtime launches imported their installed dependency. Each action left the other scope manifest fingerprint unchanged. Adding an npm lock beside the real generated pnpm lock failed before command execution. No project `.venv` was adopted or created. Reopening retained exact scope selections and the managed binding.
- The same suite verifies a pre-existing schema-1 virtual environment remains at its original identity/path after binding and reopen; absent bindings/environments remain absent during project views; invalid scope/tool/manager and nested lockfile ownership fail; a real Windows junction outside the project is refused; explicit worker PORT survives while inherited FLASK_RUN_PORT is removed.
- `cargo test -p stackyard-engine --lib scope_writer_tests --offline -- --nocapture`: one test passed, showing an independent owner can acquire while a shared owner waits, a waiting writer cancels, and dropping the owner releases the key.
- `cargo test -p stackyard-core deps::tests --offline`: four existing dependency regression tests passed, including lockfile commands and legacy directory scopes.
- `cargo clippy -p stackyard-engine --test scopes --offline -- -D warnings`: passed.
- Assigned Rust modules were formatted with `rustfmt --edition 2021`. `git diff --check` passed apart from Git's existing CRLF normalization notice for coordinator-owned WORK_QUEUE.md.

The first native run exposed a Python wheel fixture quoting error; it was corrected and subsequent native runs passed. Fixtures testing selection validation use the test executable and do not count as vendor runtime evidence. The Node/pnpm/Python gate above uses the actual verified vendor artifacts.

## Remaining boundaries

Native PHP scope profile behavior is integrated and tested by the coordinator/readiness acceptance rather than claimed by this slice. macOS runtime/dependency/junction-equivalent acceptance remains pending. Preview and migration/reopen preservation of older DB schemas remain coordinator checks. Workspace-wide tests, all-targets Clippy and desktop build are coordinator checks. Next: integrated Stage 9 review, keeping Stage 8A native gates enabled.


## Supervised native frontend/API acceptance — 5 November 2026

Extended the existing native test without changing the six-test count. Same verified Windows x64 Node 24.22.2, pnpm 12.16.1 and Python 3.12.16 artifacts and environment variables listed above.

After the short-lived import/portless-environment checks, the test rewrites the disposable manifest with HTTP auto endpoints and explicit HTTP 200 readiness. A real Node loopback HTTP server requires the installed local pnpm package; a Python HTTPServer imports the installed wheel and reads a persistent disposable data marker. Runtime and tool bindings remain scope-only, with no project/global runtime selections.

`StartProject` IPC now launches both through the engine supervisor. The test waits for both process state `running` and readiness `ready`, verifies distinct resolved ports and exact dependency-backed HTTP 200 payloads, then repeats `DepInstallScoped(frontend)` while both applications are live. The backend keeps its PID, port, HTTP payload, dependency manifest fingerprint and data marker; the frontend still serves its installed dependency payload. `StopProject` IPC succeeds and bounded connection checks prove both listeners close.

The first expanded run exposed a real preflight regression: `Supervisor::start` still used project-level `resolve_runtime`, rejecting valid scope-only bindings. The coordinator corrected that call to `resolve_scope_runtime` before the passing run. No fallback selection was added to hide the defect.

Validation: `cargo test -p stackyard-engine --test scopes --offline -- --nocapture`, with all native variables enabled, passed **6/6** in **8.92 seconds**. `rustfmt --check --edition 2021 crates/engine/tests/scopes.rs` and scoped `git diff --check` passed. No downloads, global host changes, user projects or persistent user data were used. macOS acceptance remains pending.
