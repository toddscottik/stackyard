# S3-PROJECTS

Date: 1 November 2026. Host: Linux x86_64, kernel 6.12.94+, rustc 1.99.1. This is not Windows 10 x64 and not macOS 14.5 arm64.

## What ran

`cargo test -p stackyard-core -p stackyard-engine -p stackyard-cli` passed.

The engine tests covered two projects on loopback ports with different runtime selections, stopping one without stopping the other, a file-backed reload, a failed command whose stderr is stored on the service, rollback after a later service fails, cancellation while a service is still starting, an occupied port left alone, a crash loop limited to three attempts (`restart_max = 2`), child-process cleanup, and an engine `SIGKILL` that left an unrelated process alive and did not start a second copy of the orphan.

Dependency tests covered npm versus pnpm lockfile selection, a conflicting pair of lockfiles, scope paths that stay inside the project, an npm `file:` install, and a pip install into one virtual environment that the system interpreter and a second project could not import.

## Fixture smoke

Node v22.23.4 and Python 3.12.3 were adopted with `use-existing`. Data directories were under `/tmp`.

| Fixture | Tool | Result |
| --- | --- | --- |
| `fixtures/node-http` | npm 10.9.9 `ci` | Served `node-http`, then `reloaded` after `message.txt` changed |
| `fixtures/flask-app` | venv pip 24.0, Flask 3.1.3 | Served `flask-ok` from the app-owned environment |
| `fixtures/fastapi-app` | venv pip, FastAPI 0.162.0, uvicorn 0.54.0 `--reload` | Response changed from `{"ok":true}` to `{"ok":"reloaded"}` |
| `fixtures/next-app` | pnpm 12.16.1, Next 16.4.3 | Homepage contained `stackyard-next`. `pnpm-lock.yaml` is committed |

uv was not installed. Detection of `uv.lock` is unit-tested only.

## Not evidenced here

Windows job-object launches, macOS process groups, and the Mac package catalog. A Windows `cargo check` on this machine stopped in the `ring` and `zstd-sys` build scripts because the MSVC C compiler is not installed; it did not type-check the engine.
