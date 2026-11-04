# S8A-WINDOWS-REGRESSIONS

4 November 2026. Windows 10 Pro 10.0.19045 build 19045, AMD64. Rust/Cargo 1.99.1. This bounded regression slice changes test fixtures; it does not establish Stage 8A or macOS acceptance.

## Changes

- `crates/engine/tests/projects.rs`: the disposable npm project writes its own `.npmrc` with `install-links=true`. npm packs/copies the local `file:vendor/hello` dependency rather than requiring symlink privileges. The existing install assertion remains, with an additional assertion that the installed dependency is not a symlink. No global npm configuration, Windows privilege, Developer Mode, or project installation policy changes.
- `crates/engine/src/bin/fixture.rs`: the simulated `-m pip --version` returns its version before the install branch. Previously it wrote `pip-installed` into the engine crate's working directory during the tool-version probe. A focused regression checks successful version output and absence of this marker in a disposable working directory.
- `crates/engine/pip-installed`: inspected the generated untracked file, verified its exact contents were `installed`, then removed that single file. The final tests did not recreate it.
- `crates/engine/src/supervisor.rs`: temporarily instrumented for diagnosis, then restored exactly; no final diff or production lifecycle change.

## Reproduction and diagnosis

The initial focused `stop_kills_children_and_failure_names_the_error` passed, 1/1 in 3.51s. The initial full parallel project suite reproduced three failures: the npm EPERM symlink error, `fails` staying `starting` with no recorded pid, and `crasher` staying `starting` with no recorded pid; 4/7 passed, total 34.82s.

An instrumented full run then reproduced the `fails` timeout, 6/7 passed, 28.61s. Its startup worker entered `run_start`; the grouped-spawn completion trace appeared only after the test's eight-second status assertion had timed out. This trace does not isolate whether the delay was in launch resolution, process creation, or another pre-readiness operation. A missing pid in the stored `starting` row alone does not locate the delay, because the current implementation records pid when readiness succeeds.

More detailed tracing timed launch resolution and grouped spawning. Five subsequent complete instrumented suites passed 7/7; launch resolution was typically 3–7ms and spawning approximately 2–5ms. Three repeated instrumented logs remain in ignored `.artifacts/S8A-WINDOWS-REGRESSIONS/instrumented-{1,2,3}.log`. No evidence established a production state-transition, job-object, database-lock, antivirus, or process-creation root cause. Those possible explanations must not be reported as confirmed.

The original eight-second status assertion, child cleanup assertion, failure-message assertion, restart-count assertion and parallel test execution were preserved. No speculative supervisor patch or timeout increase was made. The intermittent lifecycle failure therefore remains an unresolved reproduction risk despite the passing final checks.

## Final validation

Commands ran from `D:\_work\laragon_alternative`:

```powershell
cargo test -p stackyard-engine --test projects --offline
cargo test -p stackyard-engine --test projects stop_kills_children_and_failure_names_the_error --offline
cargo test -p stackyard-engine --test projects npm_and_pip_keep_project_environments_separate --offline
rustfmt --edition 2021 crates/engine/tests/projects.rs crates/engine/src/bin/fixture.rs
git diff --check -- crates/engine/tests/projects.rs crates/engine/src/bin/fixture.rs crates/engine/src/supervisor.rs
```

- Final uninstrumented full suite: 8 passed, 0 failed, 0 ignored; 3.35s. This includes the new pip-version side-effect regression and the existing lifecycle, partial rollback, cancel, busy port, bounded restart, crash recovery and dependency-isolation checks.
- Final focused lifecycle: 1 passed in 3.13s.
- Final focused real npm/pip isolation: 1 passed in 1.80s.
- Formatting/diff whitespace checks passed. Supervisor's diff is empty. Generated crate-level `pip-installed` is absent.

Actual tools: existing Node v24.13.0, npm 11.6.2 at `C:\Program Files\nodejs`. The test resolves `python3.exe` through the WindowsApps alias; its actual interpreter reports Python 3.9.13 at the Microsoft Store Python 3.9 package path. This differs from existing `C:\Python\Python312\python.exe` (3.12.3). The isolated test installs its generated wheel through the chosen disposable venv and confirms the marker module is absent from the selected base interpreter and other project environment. No global Python package was installed.

## Handoff

The npm privilege-dependent fixture and pip version-probe side effect are corrected. Current Windows focused and full project checks pass without changing lifecycle assertions. The coordinator owns canonical workspace validation and shared status updates. If the lifecycle timeout recurs, retain precise phase timing before making a production change; the original intermittent failure is not claimed fixed. macOS execution remains pending.

## Captured CLI autostart regression

A later real CLI smoke exposed a separate deterministic Windows defect: the CLI process exited after autostarting its engine, but a capturing PowerShell pipeline did not receive EOF. This is distinct from the intermittent embedded-engine lifecycle timeout above.

`std::process::Command` on Windows inherits every inheritable parent handle by default. Replacing stdin/stdout/stderr with null does not prevent unrelated shell pipeline handles from reaching a long-lived child. Rust 1.99.1's handle-inheritance control and raw startup attribute APIs remain nightly-only. [Pinned Rust CommandExt documentation](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html#tymethod.inherit_handles)

The coordinator approved a Windows-only launch adapter in `crates/core/src/client.rs`: call `CreateProcessW` with an explicit executable, UTF-16 argv quoting and `bInheritHandles=FALSE`; preserve `CREATE_NO_WINDOW`, current user/environment, inherited working directory, and existing job associations. No BREAKAWAY flag, nightly feature, dependency, privilege, or project-process-group change is introduced. Native process/thread handles are closed after successful startup. Invalid NUL arguments and Windows command lines exceeding 32,767 UTF-16 units fail before launch. This is a narrow host-integration detail within the existing per-user engine architecture. [CreateProcessW handle semantics](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw), [Windows creation flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags)

`crates/engine/tests/cli_startup.rs` runs the client autostart path in a captured subprocess. The baseline implementation failed after its fixed 15-second deadline: the client exited, but captured stdout/stderr remained open. After the fix, the same scenario passes while the engine remains responsive. The final regression also creates an extra inheritable pipe, checks EOF on all three capture channels, copies the engine executable into a path containing spaces/Unicode, uses a data path containing spaces/Unicode, verifies the same reported engine pid through IPC, and stops only that disposable engine. Cleanup is scoped to the unique test endpoint. One test entry is the subprocess helper; the other is the actual autostart scenario.

The quoting unit check covers empty arguments, spaces/Unicode, trailing backslashes, embedded quotes, a backslash before a quote, and NUL rejection. It passed on this Windows x64 host.

Final checks for this addition:

```powershell
cargo build -p stackyard-cli -p stackyard-engine --offline
cargo test -p stackyard-core quotes_windows_arguments --offline
cargo test -p stackyard-engine --test cli_startup --offline
cargo test -p stackyard-engine --test projects --offline
cargo clippy -p stackyard-core -p stackyard-engine --all-targets --offline -- -D warnings
```

All passed: quoting 1/1; autostart harness 2/2 in 3.19s; project suite 8/8 in 3.28s, including child-job cleanup; scoped Clippy clean. Existing project ownership behavior remains covered by the unchanged assertions.

Real rebuilt CLI verification also passed: an independently spawned hidden PowerShell process captured two piped `stackyard --data-dir <disposable> status` calls and printed `CAPTURED_PIPELINE_DONE`; stdout/stderr closed and the PowerShell process returned in approximately 2.07s under a 20-second deadline. Both calls reported pid 23804. A third status after the PowerShell process exited returned the same pid, then scoped `stop` succeeded. Script and raw logs remain under ignored `.artifacts/S8A-WINDOWS-REGRESSIONS/captured PowerShell пример/`. No coordinator-owned stuck session or process was killed. The change fixes captured output ownership; it does not establish freedom from the earlier intermittent lifecycle delay or Mac runtime support.
