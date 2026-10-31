# Package and platform verification

Checked 31 October 2026. This is source/metadata verification plus local runtime probes. Downloaded package payloads have not been installed or tested in this review. The Mac was described by the user and is not connected to this execution environment. Do not treat a published artifact as a passed installation test.

Additional requirement: [DEPENDENCY_MANAGEMENT.md](DEPENDENCY_MANAGEMENT.md) specifies npm/pnpm/pip/uv and PHP/Composer/extensions. Their workflow and PHP-extension/Memcached native artifact tests are pending; the earlier runtime metadata evidence does not verify these additional capabilities.

## Target machines and evidence

- Windows: read-only OS inspection confirmed Windows 10 Pro, build 19045 (22H2), x64.
- Existing Node: `C:\Program Files\nodejs\node.exe` executed successfully and reported v24.13.0, x64, win32.
- Existing Python: `C:\Python\Python312\python.exe` executed successfully and reported CPython 3.12.3, AMD64. Imports of ssl, sqlite3, and venv succeeded; SQLite reported 3.45.1. This is a reuse candidate, not a recommendation to freeze that patch version.
- Mac: user-confirmed M1 Pro, 16 GB RAM, macOS 14.5; native arm64 target. Runtime tests remain to be run there.
- The user confirmed Rust has never been installed. Rust/Cargo setup is required before building the application. uv was not found through the current shell's command lookup; its presence elsewhere is unverified. C++ Build Tools and WebView2 have not yet been inspected.

Public Node checksum metadata and GitHub release APIs for uv/Python were fetched successfully. Exact observations are recorded in [verification-evidence.json](verification-evidence.json). Those digests have not yet been compared with downloaded payloads or authenticated using publisher signing keys.

## Component decisions

| Component | Verified distribution path | Target assessment | Development decision |
| --- | --- | --- | --- |
| Node.js | Official v24 index lists Windows x64 ZIP and macOS arm64 tar archives; checksum manifest fetched | v24 documentation lists Windows 10 and macOS 13.5+ technical minimums, with an additional OS-lifecycle qualification | Start with Node 24 as a candidate family; preserve project version requirements and prove downloads on both machines |
| Python | Astral python-build-standalone release API lists CPython 3.12.16 install-only archives for both targets, including digests | Artifacts found; their actual minimum OS, libraries, and package behavior still need execution checks | Managed CPython 3.12 candidate; use uv for project environments and dependency operations |
| uv | Release 0.13.6 has Windows x64 and macOS arm64 archives and recorded digests | Official policy lists Windows 10+ and macOS 13+ | Suitable candidate helper; pin it separately from Python and redirect storage into app-owned directories |
| Apache / Windows | Apache Lounge lists httpd 2.4.69 Win64 VS18 ZIP, checksums, and PGP signature | Distributor explicitly lists Windows 10; VC++ runtime prerequisite | Candidate download provider; validate modules, config, VC++ runtime, HTTP, and shutdown before approving |
| Apache / macOS | Apache source builds are possible; Homebrew has an httpd formula | Current formula's bottle list does not include Sonoma. No self-contained macOS 14.5 arm64 archive approved by this review | Packaging gate: validate a suitable distribution or build/package a relocatable arm64 artifact. Existing Apache may unblock development but does not satisfy managed-download acceptance |
| MySQL / Windows | Official 8.4 download page lists a winx64 ZIP (8.4.13 observed) and signature | Package listed; actual Windows 10 launch and initialization untested | MySQL 8.4 candidate family, with disposable data directory tests |
| MySQL / macOS | Official downloads/archives are the intended source | Current platform policy follows latest two macOS releases; this does not establish 14.5 compatibility. macOS selector could not be retrieved in this review | Open gate: obtain a specific compatible arm64 package and verify it on 14.5; do not guess an archive URL or silently substitute MariaDB |
| PostgreSQL | PostgreSQL directs users to EDB installers and binary archives on Windows/macOS | macOS table lists PostgreSQL 18 on 13+ arm64. Windows table lists tested Server editions, not an explicit Windows 10 certification | PostgreSQL 18 candidate unless a real project needs an older major; resolve exact archive/checksum and test on both targets |
| SQLite | Official tools page lists win-x64 and osx-arm64 archives; Python's embedded SQLite works locally | No standalone server required; newest CLI's 14.5 compatibility untested | Initial integration uses project SQLite files and a validated backup API. Optional CLI is a separate package |
| Local HTTPS helper | mkcert releases document macOS arm64 and Windows binaries | Published route found; no helper payload tested | Candidate helper for Stage 5; root trust is an explicit setup step, not an implicit download action |

Node evidence: [official release index](https://nodejs.org/dist/latest-v24.x/) and [v24 platform policy](https://github.com/nodejs/node/blob/v24.x/BUILDING.md). The Windows technical minimum alone is not a promise of upstream support on an OS outside its lifecycle policy.

Python/uv evidence: [standalone distribution project](https://github.com/astral-sh/python-build-standalone), [Python release API](https://api.github.com/repos/astral-sh/python-build-standalone/releases/latest), [uv release API](https://api.github.com/repos/astral-sh/uv/releases/latest), and [uv platform policy](https://docs.astral.sh/uv/reference/policies/platforms/).

Apache evidence: [Apache Lounge](https://www.apachelounge.com/download/) and [Homebrew httpd formula](https://formulae.brew.sh/formula/httpd). Homebrew bottles should not be treated as freely relocatable archives independent of their prefix/dependency graph.

MySQL evidence: [8.4 downloads](https://dev.mysql.com/downloads/mysql/8.4.html), [platform policy](https://www.mysql.com/support/supportedplatforms/database.html), and [archives](https://downloads.mysql.com/archives/community/). Published archives do not by themselves establish current compatibility or maintenance suitability.

PostgreSQL evidence: [Windows packages](https://www.postgresql.org/download/windows/), [macOS packages](https://www.postgresql.org/download/macosx/), and [EDB binary archives](https://www.enterprisedb.com/download-postgresql-binaries). The retrieved archive page did not expose the per-platform artifact links, so exact URLs/digests remain unresolved.

SQLite and HTTPS evidence: [SQLite downloads](https://sqlite.org/download.html) and [mkcert releases](https://github.com/FiloSottile/mkcert/releases). SQLite's current Mac CLI artifacts are described as unsigned; evaluate delivery/trust explicitly rather than automatically removing quarantine metadata.

## Application frameworks and desktop prerequisites

Next.js belongs in each project's Node dependencies; Flask and FastAPI belong in each project's Python environment. They are recipes, not global server installations. Preserve existing dependency lockfiles and commands. Current Next.js documentation lists Node 20.9+; Flask supports Python 3.9+; FastAPI documents running an ASGI server such as Uvicorn. Test pinned sample versions instead of inferring compatibility for all historical projects. Sources: [Next.js installation](https://nextjs.org/docs/pages/getting-started/installation), [Flask installation](https://flask.palletsprojects.com/en/stable/installation/), [FastAPI server execution](https://fastapi.tiangolo.com/deployment/manually/).

Tauri remains a plausible shell. Windows development requires Rust, C++ Build Tools, and WebView2; macOS desktop development requires Rust and suitable Xcode Command Line Tools. macOS minimum deployment settings are configurable, but dependencies/sidecars also need validation. See [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) and [macOS bundle configuration](https://v2.tauri.app/distribute/macos-application-bundle/).

## What verification establishes

There is a credible distribution path for the requested stack. Node/Python reuse is proven locally at the version/import level, and candidate Node/uv/Python artifact metadata has been retrieved. Managed Apache and MySQL delivery on macOS 14.5 remain specific feasibility risks. End-to-end downloaded-package acceptance is pending for all components.

The first implementation stage must turn these candidates into tested artifacts. It must record exact URL, payload digest, provenance verification, OS/architecture, dependency requirements, installation command/layout, launch result, cleanup result, and remaining limitations. No empty checksum or guessed URL may enter the runnable catalog.
