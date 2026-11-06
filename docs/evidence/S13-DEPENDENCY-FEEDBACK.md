# Dependency task feedback

6 November 2026. Windows 10 Pro build 19045 x64, Rust 1.99.1 and WebView2
156.0.4314.58. This change addresses dependency feedback within Stage 13.2.

A read-only Activity query against the running engine showed the reported
install in the cancelled state and two completed List installed requests.
The Projects tab showed neither terminal state nor manager output. Engine
review also found that version checks and virtual environment creation ran
outside the package command timeout, and inherited log handles could keep
the engine waiting after the direct command exited.

The Dependencies tab now shows the selected manager, its working directory,
the recognized install inputs and safe direct package declarations before
install. The package manager still resolves transitive dependencies. The
same tab shows the current phase, live output, terminal result and
cancellation. Results use the current invocation while the scoped log
retains its history. Listing a Python scope without an environment returns
an empty result without creating the environment.

Dependency owner wait and package commands have 180-second limits. Tool
version probes have a 10-second limit per attempt and scoped Python
environment creation has a 90-second limit. On Windows, a focused test
held a simulated output reader open and verified that the bounded wait
stopped a real descendant through the command's JobObject.

## Windows desktop checks

The custom-protocol Tauri build used an isolated engine, a disposable
WebView2 profile, and project paths with Cyrillic characters and spaces.

| Fixture | Result |
| --- | --- |
| Node 24.22.2 and bundled npm 11.19.1 | A locked local package installed and appeared in the inline list. An empty project showed the empty state. A marker from an earlier operation remained absent from the current result. |
| pnpm 12.16.1 | A locked local package installed and appeared in the inline list. The Next.js preview showed its direct declarations and lockfile. A separate run of the larger Next.js fixture reached linking and reported Windows access denied while creating a symlink. |
| Python 3.12.3, pip 24.0 and Flask 3.1.3 | Listing before install showed the empty state and left the environment absent. Installing from the local wheelhouse created the environment, and the next list included Flask. |
| npm script held open for 60 seconds | The running panel showed the script's output. Cancel produced a terminal state and re-enabled the dependency controls. |
| WebView at 1440, 768 and 390 px | The panel fit at each width without uncaught frontend exceptions. |

The engine retains a bounded recent output excerpt for each task and writes
command output to the scoped dependency log. Native macOS checks are pending.

`pnpm --dir apps/desktop test` passed four tests, and
`pnpm --dir apps/desktop run build` and the native custom-protocol build
passed. Strict workspace Clippy and formatting checks for the changed Rust
files passed.

The workspace regression run and the engine library rerun recorded 162
distinct passing tests. The two existing Windows junction tests received
access denied during fixture setup. The mixed scope fixture hit its
30-second test deadline inside unlocked `pnpm install`, also in an isolated
offline rerun. The latest task output identified the running command before
pnpm had emitted output. The locked pnpm desktop check above passed.
