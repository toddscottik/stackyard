Title: Project manifests, process ownership, and dependency commands
Status: accepted
Context and concrete problem:
Stage 3 needs to start and stop project processes without putting machine paths or live process state in a shareable file, and without letting dependency tools rewrite a project onto a different package manager.
Decision:
A project keeps its intent in `stackyard.toml` at the project root: service id, runtime role, argument array, relative working directory, readiness path, restart policy, and an optional manager. The engine database stores the project root, the selected runtime or tool, and the observed pid, process group, port, and log path. Python virtual environments live under the data directory `environments/<project>`.
Unix children are started in a new process group and stopped with that group. Windows children are assigned to a job object that ends the tree when the engine handle closes. After an engine crash, a recorded pid is adopted only when its OS creation identity still matches; anything else is left untouched and is not started again automatically. Engine stop stops managed project processes.
npm and pnpm are chosen from an explicit manager, `packageManager`, and lockfiles. Both lockfiles without an explicit manager is an error. pip runs as `<venv python> -m pip`. A `uv.lock` selects uv and is not rewritten by pip. Dependency work is cancellable and reports partial `node_modules` or virtual environments instead of promising a rollback.
Alternatives considered:
Store the manifest only in SQLite. Start project commands through a shell. Install Python dependencies into the selected system interpreter.
Consequences and compatibility impact:
Imported projects need a `stackyard.toml`. Existing package manifests and lockfiles stay in the project. Schema version is 3. API version stays 1 because the desktop and CLI ship with the engine. Windows job behavior and macOS process groups are not yet evidenced on those machines.
Evidence / tests:
`cargo test -p stackyard-core -p stackyard-engine -p stackyard-cli` on Linux x86_64, plus the fixture smoke in `docs/evidence/S3-PROJECTS.md`.
Supersedes (if any):
None.
