# Stage 11 project creation evidence

6 November 2026. Windows 10 Pro build 19045 x64, Rust 1.99.1, and WebView2
156.0.4314.58. Tests used disposable Stackyard data directories and project
paths with spaces and Cyrillic characters. Native macOS 14.5 arm64 acceptance
is pending.

## Desktop results

The native Tauri app was built with its embedded frontend. WebView2 test
automation drove the desktop controls and selected a directory through the
Windows native dialog. The tests used the real per-user engine, component
installer and dependency commands.

| Version 1 recipe | Runtime and dependencies | Windows result |
| --- | --- | --- |
| Node HTTP | Managed Node 24.22.2, bundled npm 11.19.1, no registry dependencies | Generated, installed, served the expected response, showed logs, reloaded after a source edit, restarted with a different PID and stopped serving |
| Next.js | Same managed Node, managed pnpm 12.16.1, Next 16.4.3, React and React DOM 19.3.1 | Completed the same flow using the frozen lockfile and a populated pnpm store |
| Flask | Recorded Python 3.12.3, scoped pip 24.0, Flask 3.1.3 | Completed the same flow using wheels fetched from PyPI |
| FastAPI | Recorded Python 3.12.3, scoped pip 24.0, FastAPI 0.162.0, Uvicorn 0.54.0 | Generated, installed, served the expected response and showed logs. Uvicorn detected a source edit and hung during worker restart. The revised launcher builds, with native reload and restart acceptance pending |

The final run refused an invalid name, an occupied folder and a destination
created after review. Existing files survived and the stale review queued no
downloads. The managed Node download was cancelled after bytes arrived.
Cancellation left the destination absent, and applying the same plan again
downloaded and verified Node without installing an unrequested pnpm. Next.js
dependency installation was cancelled and retried with its generated source
intact. The engine went offline during that retry.

Next.js runs its installed CLI directly with explicit hostname and port
arguments. The previous pnpm argument forwarding caused Next.js to interpret
the hostname option as a directory. The FastAPI template now includes a small
development launcher that watches `app.py` and restarts the Uvicorn child
directly. This avoids relying on a Windows console broadcast for a single
worker. [Windows console control events](https://learn.microsoft.com/en-us/windows/console/generateconsolectrlevent)

## CLI and regression checks

- `stackyard create recipes` listed all four recipes. A catalog-based
  `create plan` against a fresh data path created neither an engine data
  directory nor a destination. `create apply PLAN.json --wait` and a repeat
  apply succeeded on the same source and project registration. The global
  runtime selection table stayed empty after the desktop and CLI flows.
- `pnpm --dir apps/desktop test` passed all four tests.
  `pnpm --dir apps/desktop run build` passed. The native review rendered at
  390, 768, and 1440 px without horizontal overflow.
- `cargo build -p stackyard-desktop -p stackyard-engine -p stackyard-cli
  --features tauri/custom-protocol --offline` and
  `cargo clippy --workspace --all-targets --offline -- -D warnings` passed.
- `cargo test --workspace --offline` with the existing Windows native
  fixture environment passed 119 tests before an IPC timeout in
  `native_php_document_roots_rewrite_post_upload_https_and_fail_closed`.
  The passing tests included recipe rendering, six core creation safety
  cases, and the real engine IPC registration and retry test.

## Open acceptance

The user reported an antivirus Trojan alert for the locally built
`target/debug/stackyard-engine.exe`. During the final desktop run the
engine exited, Windows denied a subsequent launch, and the executable was
absent when checked. Windows lists 360 Total Security as the registered
antivirus, and the user reported the label `Trojan.Generic.HgEAUCAA`.
The action taken by the scanner and its classification remain unverified.
Executable testing was paused, with antivirus settings left in place.

Once the detection is resolved, rerun the native desktop smoke, including
FastAPI's new `dev.py` reload, restart and process-tree stop, and rerun the
workspace suite with the Windows native fixtures enabled. Repeat the
desktop flow on macOS 14.5 arm64 when available.
