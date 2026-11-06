# Built-in starter recipe pins

Recipe definitions are source fixtures, version 1. They use no unbounded
scaffolder or `latest` resolution.

| Starter | Manager | Dependency pins | Existing evidence |
| --- | --- | --- | --- |
| `node-http` | npm | No registry dependencies; lockfile v3 | npm 10.9.9 install and HTTP/reload smoke on Linux x86_64: [`S8-PERSONAL`](../../docs/evidence/S8-PERSONAL.md), [`S3-PROJECTS`](../../docs/evidence/S3-PROJECTS.md) |
| `node-http` | pnpm 12.16.1 | No registry dependencies; lockfile v9 | pnpm 12.16.1 package launch on Windows x64 and macOS arm64: [`S0-WIN-RUNTIMES`](../../docs/evidence/S0-WIN-RUNTIMES.md), [`S0-MAC-PACKAGES`](../../docs/evidence/S0-MAC-PACKAGES.md); this starter's pnpm lock is source-only |
| `next-app` | pnpm 12.16.1 | Next 16.4.3, React 19.3.1, React DOM 19.3.1 | Install and HTTP smoke on Linux x86_64: [`S8-PERSONAL`](../../docs/evidence/S8-PERSONAL.md); committed lock copied byte-for-byte from `fixtures/next-app/pnpm-lock.yaml` |
| `flask-app` | pip | Flask 3.1.3 | Install and HTTP smoke on Linux x86_64: [`S8-PERSONAL`](../../docs/evidence/S8-PERSONAL.md), [`S3-PROJECTS`](../../docs/evidence/S3-PROJECTS.md) |
| `fastapi-app` | pip | FastAPI 0.162.0, Uvicorn 0.54.0 | Install and HTTP/reload smoke on Linux x86_64: [`S3-PROJECTS`](../../docs/evidence/S3-PROJECTS.md) |

The managed Node 24.22.2, Python 3.12.16, and pnpm 12.16.1 catalog artifacts
have Windows x64 evidence in [`S0-WIN-RUNTIMES`](../../docs/evidence/S0-WIN-RUNTIMES.md); Node,
Python, and pnpm have macOS 14.5 arm64 payload evidence in
[`S0-MAC-PACKAGES`](../../docs/evidence/S0-MAC-PACKAGES.md).
Windows desktop checks passed for Node HTTP with npm, Next.js with pnpm,
Flask with pip, and FastAPI with pip. macOS starter checks remain pending.
See [Stage 11 evidence](../../docs/evidence/S11-CREATE.md).
