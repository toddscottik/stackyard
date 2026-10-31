# Local development environment

A planned desktop application for managing native development runtimes and services on Windows 10 and macOS. Personal use first: Next.js, Node.js, Flask, FastAPI, Apache, MySQL, PostgreSQL, and SQLite, with existing installations or managed downloads.

Application implementation has not started. Start development with these documents:

Repository working rules are in [AGENTS.md](AGENTS.md). For parallel development, use the [agent workflow](docs/AGENT_WORKFLOW.md) and [work queue](docs/WORK_QUEUE.md). Downloaded packages and disposable test state belong under ignored `.artifacts/` directories.

1. [Development stages and agent handoff](docs/DEVELOPMENT_PLAN.md) — current execution scope, stage dependencies, deliverables, acceptance checks, and task prompt.
2. [Implementation status](docs/IMPLEMENTATION_STATUS.md) — completed preparation and the next task.
3. [Package verification](docs/PACKAGE_VERIFICATION.md) — sources, local checks, candidate artifacts, and unresolved compatibility gates.
   [Dependency management](docs/DEPENDENCY_MANAGEMENT.md) specifies pnpm/pip workflows and PHP/Composer/native extensions, including Memcached server/client separation.
4. [Product plan](docs/PRODUCT_PLAN.md) and [review](docs/PRODUCT_PLAN_REVIEW.md) — research and broader roadmap. The development plan takes precedence for current implementation scope.

First task: Stage 0 package feasibility and disposable runtime tests on Windows 10 x64 and macOS 14.5 arm64. See the development plan for the copyable agent prompt.

Development prerequisites: Git and Node/npm are available on the current Windows machine. Rust/Cargo must be installed during Stage 1; C++ Build Tools and WebView2 still need inspection. Check the Mac's Rust and Xcode Command Line Tools separately. Stage 0 runtime probes do not need Rust. No application build/test commands exist yet; Stage 1 will document them here.
