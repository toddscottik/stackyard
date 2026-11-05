# Stage 10 desktop workspace acceptance

6 November 2026 on Windows 10 Pro 10.0.19045 x64. Rust 1.99.1, WebView2 156.0.4314.58, and verified Node 24.22.2. macOS 14.5 arm64 acceptance remains pending.

## Component and resource contracts

- App owns the connection, project list, packages, catalog, Apache and database resources. A passive probe observes connection state. Startup and Retry connection explicitly request engine startup. Each resource serializes reads and discards results invalidated by a mutation or connection change.
- Shell renders navigation. Projects owns import and project lifecycle actions, with a per-project submission guard. The Overview, Dependencies, Logs and Site tabs restore the last choice for each project. The WebView profile stores the selected project ID and tab names.
- Projects, Components, Databases, Settings and Activity are separate panels. Dependency controls support explicit scope IDs and the existing directory-based flow. Lifecycle controls follow the current process state. Action errors appear with the relevant control.
- Activity uses one app-level poll for bounded engine snapshots. Install and dependency entries retain their engine IDs, state and output across navigation. The engine keeps running entries and the last 64 completions per board. The desktop rechecks omitted tracked IDs before changing their status. A running entry missing after an engine restart becomes interrupted. Confirmed results already observed stay visible in the open window.
- Cancel uses the existing operation commands. Retry uses the input captured by this desktop session. Database creation waits for confirmed package installation before selecting the server package.
- Logs uses a byte offset and opaque cursor token through the additive FollowLogs command. The token detects replacement and truncate-and-regrow using file metadata and a cursor anchor. The viewer preserves a bounded text buffer, serializes reads, follows or pauses, and drains final bytes after a process exits. CLI log reads keep their prior contract.

## Windows acceptance

| Check | Result |
| --- | --- |
| `cargo test --workspace --offline` | 133 reported passes with the existing Windows native artifact gates enabled. Includes activity snapshot limits, rotation and UTF-8 boundaries, scoped services, and native PHP regressions. |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | Passed |
| `pnpm --dir apps/desktop test` | Three invoke boundary tests passed. They cover stale and overlapping project reads, disconnect invalidation, and probe, activity and log command arguments. |
| `pnpm --dir apps/desktop run build` | Passed, including TypeScript |
| `cargo build -p stackyard-desktop -p stackyard-engine -p stackyard-cli --features tauri/custom-protocol --offline` | Built the native desktop, engine and CLI used for acceptance |
| Production browser inspection at 1440, 768 and 390 px | Zero failures. The desktop offline sample retains whitespace in the empty project pane. Monospace is used for paths, IDs, manifest and logs. |

The native smoke opened the compiled Tauri window with isolated WebView and engine state under `.artifacts/S10/`. It used a disposable copy of the npm fixture with a local dependency, an offline install delay, and loopback HTTP routes that emit test log lines. The native-control runtime could not initialize on this host because its Windows sandbox helper failed to refresh. The smoke drove the compiled app's loopback WebView2 debugging target and exercised real invokes, engine IPC and child processes.

1. Imported the fixture from an empty profile, selected it, started Node and verified its loopback response. A fresh-profile run exposed an import selection race with the initial empty project list. The corrected run passed.
2. Started dependency installation, navigated to Activity and back to the remembered Dependencies tab, cancelled the running operation, and retried it to confirmed success.
3. Paused Logs, appended a line through the service, and verified it appeared on resume. Rotated the disposable file and verified the new generation replaced the previous display.
4. Restarted and observed a replacement service PID. Triggered a crash and read its final line after exit. Started again, stopped, and confirmed the port was closed.
5. Checked the Site publish control, recorded an actual SQLite file for the fixture, saved an isolated setting, and exercised arrow-key tab navigation. The settings draft survived project polling.
6. Stopped the engine with a dependency operation in progress. The window reported offline until Retry connection. The old operation became interrupted, and the earlier confirmed success remained visible.
7. Reloaded the WebView. The selected project and Logs tab were restored, and the final crash line remained readable. The run recorded no uncaught WebView exceptions.

The isolated engine, desktop process, and preview servers were stopped after verification. The smoke scripts and screenshots are under ignored `.artifacts/S10/`. The native smoke covered local dependencies and SQLite. Managed server downloads and Apache publishing retain their existing invoke paths and prior provider evidence.

## Parity and handoff

- Projects retains import, project runtime selection, start, cancel start, restart, stop, and open folder, editor, shell and HTTP URL.
- Dependencies retains install, list, add, remove and update. Activity shows progress, results and retry where the request is available.
- Site retains publish, unpublish, open and shared Apache restart.
- Components retains managed installs, existing runtime registration, update notices and cleanup.
- Databases retains install and create, start and stop, attach and detach, and SQLite. Settings retains preferences and diagnostics.

Stage 11.1 can define the first recipe contract using the workspace and activity interfaces. Native macOS smoke remains open. See the [development plan](../DEVELOPMENT_PLAN.md).
