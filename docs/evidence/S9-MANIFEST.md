# Stage 9 manifest parser evidence

Date: 4 November 2026
Slice: Worker A, schema 1 normalization and schema 2 manifest parsing
Target used: Windows 10 x64 (build not re-probed in this slice)

## Implemented behavior

- Schema 1 remains the legacy format. It normalizes services into `default` for a single runtime, or `legacy-<runtime>` scopes for mixed runtimes, keeps the compatible top-level manager association, and retains the existing project-relative working-directory and service readiness defaults. A `php-cgi` service still normalizes to a FastCGI endpoint with TCP readiness.
- Schema 2 requires one or more `[[scope]]` records. Each scope has a unique lowercase ID, a relative directory, a lockfile root that contains that directory, a runtime, and a required runtime-compatible manager. Python environments are managed; Python `environment = "project"` is rejected. Node and PHP use project ownership, and explicit managed environments are Python-only.
- Schema 2 services refer to a known scope. Runtime is derived from that scope. Endpoints accept `http`, `tcp`, or `fastcgi` and a numeric high port or `"auto"`; readiness accepts `http`, `tcp`, or `process` with bounded settings. HTTP readiness requires an HTTP endpoint; TCP requires an endpoint; process readiness forbids an endpoint. `{port}` is rejected without an auto endpoint.
- Unknown TOML fields remain rejected. Duplicate scope/service IDs, manager mismatches, conflicting managers for one lockfile root, path escapes/prefix paths, invalid service references, top-level schema 2 managers, and proxy sites targeting non-HTTP endpoints are rejected. The parser remains pure and performs no filesystem access beyond the separate `load_manifest` reader.

## Validation

- `cargo check -p stackyard-core` — passed on Windows 10 x64.
- `cargo test -p stackyard-core manifest::tests --lib` — blocked during test compilation by a concurrent Worker B API addition: `crates/core/src/php_project.rs:363` initializes `DepRequest` without the newly required `scope_id`. No parser test failure was reached. Coordinator should resolve the shared compilation issue, then rerun this focused command.

Follow-up contract correction: schema 2 now requires a manager and validates the lockfile-root containment direction (lockfile root contains scope directory). `cargo test -p stackyard-core manifest::tests --lib` passed after this correction: 6 passed, 0 failed, on Windows 10 x64.

The evidence reflects parser behavior and compilation only; it is not Windows or macOS runtime acceptance. macOS 14.5 arm64 was not available for this slice.
