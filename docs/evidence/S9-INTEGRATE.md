# Stage 9 integration evidence

Coordinator acceptance: 5 November 2026 (work began 4 November).
Host: Windows 10 Pro 10.0.19045, x64; rustc 1.99.1 (58cf8f9c3 2026-10-13), x86_64-pc-windows-msvc. No Mac is connected; macOS 14.5 arm64 execution remains pending.

## Delivered behavior

- Schema 2 stable scopes own compatible managers, contained cwd/lockfile roots, local runtime/tool bindings and managed Python environments. Services resolve their scope both in supervisor preflight and launch. Canonical dependency owners serialize conflicting writers; independent scopes remain isolated.
- Schema 1 normalizes without manifest rewrites. Migration 8 preserves package IDs, project/global selections/history, process identity and PHP active pointers. The reopen test starts from the exact schema-7 tables and verifies preservation, read-only opening without migration, scope selection/environment reopen, and original environment bytes/path.
- Endpoint presence, declared readiness, process state, readiness observation and raw HTTP status are separate. Portless workers remove inherited port values while retaining explicit ones. TCP does no HTTP probing. Redirects need explicit allowed statuses; strict health failures retain the observed code. Proxy publication/Open URL require HTTP; PHP FastCGI remains separate.
- Scope-specific PHP profiles are distinct from Composer. Scoped rollout restarts only the selected scope, preserves unrelated workers, retains listener ports, and rolls back failures. Package cleanup checks all active pointers even after a manifest edit. Inherited PHP switches validate every affected scope before changing selection.
- Pure snapshot preview has structured diagnostics for missing packages/tools, invalid overrides, paths/metadata/manager conflicts and unresolved auto ports. It does not reserve ports, execute commands, create environments or download. Read-only CLI/Tauri entry points bypass engine autostart. Draft and package metadata reads are capped at 64 KiB.
- Engine/CLI/Tauri/TypeScript expose scoped dependency, selection and PHP commands. Project views retain existing fields and add scopes, selection origin, readiness and typed endpoint/status fields. The adapter serialization test checks the camelCase mapping; preview DTOs remain snake_case verbatim.

## Validation

Commands from repository root, against the combined working tree:

| Check | Outcome |
| --- | --- |
| cargo test --workspace --offline | Passed with all listed native Windows gates enabled; 128 reported passes in the combined run |
| cargo clippy --workspace --all-targets --offline -- -D warnings | Passed |
| pnpm --dir apps/desktop run build | Passed, including TypeScript |
| cargo test -p stackyard-cli --test preview --offline | Passed; an unimported draft creates no engine data directory and leaves the source untouched |
| cargo test -p stackyard-desktop --lib --offline | Passed; includes scope/endpoint/readiness/status adapter serialization |
| cargo test -p stackyard-core --lib --offline | Passed, including parser, pure preview and schema-7 migration/reopen |
| Native scope and readiness acceptance | See S9-SCOPES.md and S9-READINESS.md; native gates enabled |
| Formatting/diff/local documentation links | Checked before commit |

The full test command enables environment-gated vendor acceptance; a zero-execution Mac/Unix-only suite is not evidence for those targets. The working tree includes a separately preserved CI resolver-test portability change; production behavior for that change is unchanged. No CI service run or desktop window smoke is claimed by this stage.

## Native inputs and reproduction

Previously publisher-verified cached artifacts were reused; no new artifact downloads or dependency pins were introduced.

| Gate | Cached input / actual version |
| --- | --- |
| STACKYARD_TEST_SCOPE_NODE | .artifacts/S2-PACKAGES/packages/node/24.22.2/node-v24.22.2-win-x64/node.exe; v24.22.2 |
| STACKYARD_TEST_SCOPE_PNPM | .artifacts/S2-PACKAGES/tools/pnpm/12.16.1/pnpm.exe; 12.16.1 |
| STACKYARD_TEST_SCOPE_PYTHON | .artifacts/S2-PACKAGES/packages/python/3.12.16+20261026/python/python.exe; 3.12.16 |
| STACKYARD_TEST_PHP / STACKYARD_TEST_COMPOSER | .artifacts/S8A-ARTIFACTS/php runtime/php.exe (8.5.12-1); composer.phar (2.11.1) |
| STACKYARD_TEST_APACHE / STACKYARD_TEST_CURL | Verified Apache 2.4.69 httpd.exe under S0-WIN-SERVERS; Windows System32 curl.exe |
| STACKYARD_TEST_NODE / STACKYARD_TEST_PYTHON | Existing system Node and Python used by the earlier mixed-runtime regression fixtures |
| STACKYARD_TEST_MEMCACHED | Verified Memcached 1.6.8 executable under S8A-MEMCACHED, used only through the guarded engine provider |
| PHP/extension/server archive gates | PHP 8.5.11/8.5.12 archives, ServBay 8.5.11/8.5.12 extension archives, Memcached 1.6.8 archive already recorded in Stage 8A evidence |

Set these variables to absolute local paths, and COMPOSER_DISABLE_NETWORK=1. Archive variables are STACKYARD_TEST_PHP_ARCHIVE, STACKYARD_TEST_PHP8511_ARCHIVE, STACKYARD_TEST_PHP8512_ARCHIVE, STACKYARD_TEST_MEMCACHED_EXTENSION8511_ARCHIVE, STACKYARD_TEST_MEMCACHED_EXTENSION8512_ARCHIVE and STACKYARD_TEST_MEMCACHED_SERVER_ARCHIVE. Then run the workspace command above. For the narrower new native acceptance run cargo test -p stackyard-engine --test scopes --test readiness --offline. Missing gate variables explicitly skip vendor checks rather than establish target support.

The coordinator's ignored full gate runner/log are .artifacts/S9-INTEGRATE/native-tests.ps1 and workspace-tests.log. Detailed fixture behavior/provenance is in [scope evidence](S9-SCOPES.md), [readiness evidence](S9-READINESS.md), [manifest evidence](S9-MANIFEST.md), [Stage 8A native cache](S8A-NATIVE-CACHE.md) and [provider evidence](S8A-CACHE-PROVIDER.md).

## Review corrections and remaining work

Review corrected reversed lockfile-root containment, omitted schema-2 manager/Python ownership checks, ignored invalid preview overrides, portless PHP rollout, original PHP listener-port preservation, and project-wide runtime preflight that bypassed exact scope selections. The live pnpm/Python acceptance deliberately uses scope-only bindings to prevent a project default from masking that last defect. Readiness fixture reads were corrected after the strict lint cleanup exposed a fixed-length request-read deadlock; final tests use actual received byte counts.

Mac native scopes/readiness/PHP remain pending. Native uv is not claimed; the accepted Python branch is pip. Earlier platform/desktop gaps remain in IMPLEMENTATION_STATUS.md. Stage 10.1 is next; recipes/import editing/resource workflows remain in their planned later stages. The frozen schema, request examples, migration and preview limits are in [Decision 0008](../decisions/0008-project-scopes.md).
