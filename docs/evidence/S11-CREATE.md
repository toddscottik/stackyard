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
| FastAPI | Managed CPython 3.12.16 build 20261026, scoped pip 26.2.2, FastAPI 0.162.0, Uvicorn 0.54.0 | Completed the same flow. Editing `app.py` replaced the Uvicorn worker while the launcher PID and port stayed stable. Desktop Restart changed the launcher PID and Stop closed the endpoint |

The desktop refused an invalid name, an occupied folder and a destination
created after review. Existing files survived and the stale review queued no
downloads. An earlier managed Node download was cancelled after bytes arrived.
Cancellation left the destination absent, and applying the same plan again
downloaded and verified Node without installing an unrequested pnpm. Next.js
dependency installation was cancelled and retried with its generated source
intact. The engine went offline during that retry. A later isolated desktop
run passed all four lifecycle flows without an uncaught WebView exception.

Next.js runs its installed CLI directly with explicit hostname and port
arguments. The previous pnpm argument forwarding caused Next.js to interpret
the hostname option as a directory. The initial FastAPI launcher detected a
source edit and hung waiting for the Uvicorn worker to exit. The template now
includes a small development launcher that watches `app.py` and restarts the
Uvicorn child directly. This avoids relying on a Windows console broadcast
for a single worker. [Windows console control events](https://learn.microsoft.com/en-us/windows/console/generateconsolectrlevent)

## Managed Python timeout

The user reported Windows socket error 10060 while reading the Python
archive. A fresh HTTPS download from Astral's release returned 22,014,416
bytes and matched catalog SHA256
`1f2ab53880fc67ea323398c6388e05af28addc99a8e389209f08a51fdff4b149`.
The isolated desktop then downloaded the same archive during FastAPI
generation, verified it and recorded the exact Python package on the
project's `app` scope. The global selection table stayed empty.
[Astral CPython build 20261026](https://github.com/astral-sh/python-build-standalone/releases/tag/20261026)

The package installer now retries a transient connection or response failure
once within the same Activity operation. A new transfer resets its byte
counters and starts with a fresh payload. The idle read timeout is 45 seconds.
The Windows loopback regression stalls the first response after sending part
of the archive and verifies that the second response installs, that the
byte count reflects one complete archive and that partial files are removed.

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
- `cargo test --workspace --offline` with Windows native fixtures enabled
  passed 142 tests in the resumed run. This included the new transfer retry,
  recipe rendering, six creation safety cases and the engine IPC registration
  test. The native PHP FastCGI test that timed out in the earlier run passed
  on the rerun.

## Earlier interruption and open acceptance

The user reported an antivirus Trojan alert for the locally built
`target/debug/stackyard-engine.exe`. During the final desktop run the
engine exited, Windows denied a subsequent launch, and the executable was
absent when checked. Windows lists 360 Total Security as the registered
antivirus, and the user reported the label `Trojan.Generic.HgEAUCAA`.
The action taken by the scanner and its classification remain unverified.
Executable testing paused at that point. The resumed run used a separate
Cargo output directory, engine state, WebView profile and project parent
while the user's running app remained separate.

Repeat the desktop flow on macOS 14.5 arm64 when available.
