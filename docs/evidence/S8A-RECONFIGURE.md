# S8A-RECONFIGURE — live PHP profiles and owned CGI readiness

4 November 2026. Executed on Windows 10 Pro build 19045 x64, Rust 1.99.1. macOS execution remains pending, as directed by the user.

## Delivered boundary

Changed worker-owned files:

- `crates/core/src/php.rs`
- `crates/engine/src/supervisor.rs`
- `crates/engine/tests/php_reconfigure.rs`
- This evidence note.

The coordinator owns routing, CLI/desktop waits, selected `php-cgi` sibling resolution, manifest/FastCGI Apache contracts, and shared status. This worker did not change those files or install a new runtime/dependency.

`Supervisor::reconfigure_php<T, F>(&Arc<Self>, project_id, apply)` snapshots the current profile and previously running, engine-owned PHP services. Starting, orphaned, exiting/unowned workers, shutdown, and overlapping lifecycle operations are rejected before applying anything. The candidate closure runs its existing separate-process ABI/extension load probes and promotes only a working candidate. Candidate failure leaves worker PIDs untouched.

After successful candidate validation, only those previously live PHP services are stopped and relaunched through the existing process-group/job owner, `bring_up`, readiness check, and monitor. Their actual observed ports become the restart ports, preserving Apache upstream addresses. Node/Python services and other projects are not stopped. A per-project lifecycle guard covers start/stop/profile rollout; normal shutdown still cleans up owned groups.

Old monitors publish their terminal state before releasing the live ownership entry. Rollout waits for this completion boundary before starting a replacement, preventing an old monitor from overwriting a new worker's state.

If worker readiness fails, rollout stops partial candidate workers and calls `php::rollback_php_profile(data_dir, project_id, previous)`. The helper locks the profile, validates project/generation containment and actual previous PHP extension loads, and atomically restores the prior immutable pointer. `None` restores the original unprofiled `-n` configuration. No extension artifact or profile generation is deleted. The supervisor then recovers only the original PHP services on their original ports. Errors distinguish successful recovery from failed pointer restoration or failed worker recovery and identify the next action.

`php::php_native_directories(profile)` validates artifact/dependency containment and returns deduplicated native dependency parent directories. `configure_php_command` prepends these directories to the subprocess PATH, respecting an explicit pending PATH override. Global PATH and native installations are unchanged. The coordinator integrates the same helper into project/service/Composer launch configuration.

PHP services whose selected command token is `php-cgi` use bounded TCP readiness rather than sending HTTP to a FastCGI listener. Runtime identity and extension loads continue to use the selected PHP CLI executable. FastCGI request correctness is verified separately in the native acceptance test.

## Native runtimes

- PHP **8.5.12 NTS x64 VS17**, the already verified `.artifacts/S8A-ARTIFACTS/php runtime/php.exe` and sibling `php-cgi.exe`. Provenance and archive digest remain in [S8A-ARTIFACTS.md](S8A-ARTIFACTS.md).
- Existing Node **v24.13.0**, `C:\Program Files\nodejs\node.exe`, referenced and left unchanged.
- Existing Python **3.12.3**, `C:\Python\Python312\python.exe`, referenced and left unchanged. The test uses only its standard-library HTTP server and creates no Python environment or dependencies.

Tests create disposable engine state and projects, including spaces and Cyrillic characters in project paths. They bind high loopback ports, use no OS service registration, and clean up owned processes and fixture data. The CGI test client sends real FastCGI BEGIN/PARAMS/STDIN records, reads bounded STDOUT/END_REQUEST records with socket deadlines, and observes the actual extension state returned by PHP-CGI.

## Commands and results

Executed in PowerShell at the repository root:

```powershell
$env:STACKYARD_TEST_PHP = (Resolve-Path '.artifacts/S8A-ARTIFACTS/php runtime/php.exe').Path
$env:STACKYARD_TEST_NODE = 'C:\Program Files\nodejs\node.exe'
$env:STACKYARD_TEST_PYTHON = 'C:\Python\Python312\python.exe'
cargo test -p stackyard-core --lib php:: -- --nocapture
cargo test -p stackyard-engine --test php_reconfigure -- --nocapture
cargo test -p stackyard-engine --test projects
rustfmt --edition 2021 --check crates/core/src/php.rs crates/engine/src/supervisor.rs crates/engine/tests/php_reconfigure.rs
cargo clippy -p stackyard-core -p stackyard-engine --all-targets -- -D warnings
```

Results:

- Core PHP tests: **8 passed, 0 failed**, 2.07 seconds, including real native load, rollback/deactivation, wrong-project restoration refusal, timeout/output cap, native PATH scoping and profile preservation checks.
- Native rollout acceptance: **1 passed, 0 failed**, 18.72 seconds.
- Existing engine project/lifecycle regressions: **8 passed, 0 failed**, 3.47 seconds.
- Formatting and core/engine all-target clippy: passed.

Later combined clippy reruns during coordinator cache integration reached an incomplete engine command match and then an in-progress cache-provider compilation error. Those shared-file changes are coordinator-owned; the successful combined lint above predates those concurrent additions. Core-only all-target clippy and owned-file formatting passed again after the final PHP edits. The coordinator must repeat the combined check after cache integration is complete.

The native rollout test demonstrates:

1. A mixed project runs actual PHP HTTP, PHP-CGI, Node, and Python services; a separate project runs two more PHP services.
2. Starting and deliberately injected orphan observation states reject profile changes without restarting workers. The orphan test restores only its disposable process observation; it does not adopt or kill an external process.
3. A validated candidate that cannot pass application readiness rolls back to the original unprofiled `-n` selection. PHP recovers on the original ports, with no phantom profile record.
4. Enabling/disabling OpenSSL while running changes only the affected project's two PHP PIDs. Their observed ports remain unchanged. Actual CLI, HTTP, and FastCGI requests agree on extension availability. Node/Python and the other project's PHP PIDs remain unchanged.
5. Unknown engine extension selection and a separate-process invalid native DLL probe preserve the prior profile and all worker PIDs.
6. When an application requires OpenSSL, disabling it passes the native candidate probe but fails application readiness. The prior profile is restored and both original PHP services recover successfully on their original ports; unrelated PIDs remain unchanged.
7. When application readiness fails under both candidate and previous profiles, the previous profile is still restored, the error explicitly reports failed worker recovery, partially recovered PHP workers stop and release their ports, and unrelated services remain alive.
8. Persistent project data is retained throughout.

When any required native-runtime variable is absent, the new integration test prints an explicit skip message and returns; a default harness pass without those variables is not native runtime evidence.

## Limits and coordinator follow-through

Profile rollout intentionally has a bounded service restart window; it is not zero-downtime switching. It preserves PHP upstream ports and does not restart Apache itself. The native FastCGI Apache/document-root/`.htaccess` acceptance belongs to [S8A-FASTCGI.md](S8A-FASTCGI.md). Full engine shutdown may stop all projects as usual.

Managed/native Memcached artifact acceptance belongs to the native-cache/provider slice; this worker supplies scoped dependency loader paths only. A compatible PHP patch identity remains only eligibility: a real extension load must pass before promotion. No new Mac execution or second PHP patch-runtime support is claimed here.

Next: coordinator reviews the combined interfaces, updates the earlier running-profile refusal regression, and runs combined PHP/FastCGI/native-cache and workspace acceptance with the required artifact paths.
