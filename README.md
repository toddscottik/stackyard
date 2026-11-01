# Local development environment

A planned desktop application for managing native development runtimes and services on Windows 10 and macOS. Personal use first: Next.js, Node.js, Flask, FastAPI, Apache, MySQL, PostgreSQL, and SQLite, with existing installations or managed downloads.

The desktop is named Stackyard. Start development with these documents:

Repository working rules are in [AGENTS.md](AGENTS.md). For parallel development, use the [agent workflow](docs/AGENT_WORKFLOW.md) and [work queue](docs/WORK_QUEUE.md). Downloaded packages and disposable test state belong under ignored `.artifacts/` directories.

1. [Development stages and agent handoff](docs/DEVELOPMENT_PLAN.md) — current execution scope, stage dependencies, deliverables, acceptance checks, and task prompt.
2. [Implementation status](docs/IMPLEMENTATION_STATUS.md) — completed preparation and the next task.
3. [Package verification](docs/PACKAGE_VERIFICATION.md) — sources, local checks, candidate artifacts, and unresolved compatibility gates.
   [Dependency management](docs/DEPENDENCY_MANAGEMENT.md) specifies pnpm/pip workflows and PHP/Composer/native extensions, including Memcached server/client separation.
4. [Product plan](docs/PRODUCT_PLAN.md) and [review](docs/PRODUCT_PLAN_REVIEW.md) — research and broader roadmap. The development plan takes precedence for current implementation scope.

Stage 0 recorded Windows package probes. macOS package execution is still pending on the M1 Pro. Stage 4 is the desktop: Projects, Components, and Settings call the engine. Closing the window hides it; Quit stops managed projects unless background mode is on. Linux UI evidence is in `docs/evidence/S4-DESKTOP.md`. Windows and macOS window checks are still pending.

Stage 5 publishes projects through one Apache. A `[site]` table proxies a service or serves a directory. The listener stays on high loopback ports. Evidence is in `docs/evidence/S5-APACHE.md`.

Stage 6 adds MySQL, PostgreSQL, and SQLite. A managed instance has its own data directory and is not stopped when a project stops. An external connection has no process. Evidence is in `docs/evidence/S6-DATABASES.md`.

Stage 7 checks the bundled catalog for compatible runtime updates and installs them beside the current version. Switching one project leaves the others. Database versions, external tools, project dependencies, and application updates stay on separate paths. Evidence is in `docs/evidence/S7-UPDATES.md`.

Stage 8 adds setup steps, a redacted diagnostics report, and the personal-release limits. The Windows 10 and macOS 14.5 acceptance run is still open. See [setup](docs/SETUP.md), [known limitations](docs/KNOWN_LIMITATIONS.md), and `docs/evidence/S8-PERSONAL.md`.

Development commands, from the repository root:

```text
cargo test --workspace
cargo run -p stackyard-cli -- status
cargo run -p stackyard-cli -- serve
cargo run -p stackyard-cli -- stop
cargo run -p stackyard-cli -- catalog
cargo run -p stackyard-cli -- install node-24.22.2-windows-x64
cargo run -p stackyard-cli -- use-existing python "C:\Python\Python312\python.exe"
cargo run -p stackyard-cli -- import PATH
cargo run -p stackyard-cli -- start PROJECT
cargo run -p stackyard-cli -- stop-project PROJECT
cargo run -p stackyard-cli -- deps install PROJECT
cargo run -p stackyard-cli -- publish PROJECT
cargo run -p stackyard-cli -- sites
cargo run -p stackyard-cli -- apache restart
cargo run -p stackyard-cli -- db list
cargo run -p stackyard-cli -- db create mysql
cargo run -p stackyard-cli -- sqlite PROJECT data/app.sqlite
cargo run -p stackyard-cli -- diagnostics
cargo run -p stackyard-cli -- updates
cargo run -p stackyard-cli -- cleanup
cargo run -p stackyard-cli -- deps recover PROJECT
npm --prefix apps/desktop install
npm --prefix apps/desktop run dev
```

The product name is Stackyard. Node projects default to pnpm; an existing npm lockfile stays on npm. The engine state defaults to `%LOCALAPPDATA%\stackyard` on Windows and `~/Library/Application Support/stackyard` on macOS. A previous `%LOCALAPPDATA%\localdev` directory is renamed on first launch. Tests use disposable directories. Rust is installed with rustup during Stage 1; the pinned toolchain is `rust-toolchain.toml`. WebView2 and Visual Studio 2022 C++ tools were already present on the Windows 10 machine. The Mac still needs its own Rust and Xcode Command Line Tools check. A build on a newer macOS runner does not replace testing on macOS 14.5.
