# Package and platform verification

## Stage 8A update — 4 November 2026

Windows PHP 8.5.12-1 NTS VS17 x64 and Composer 2.11.1 matched their publisher SHA256 pins and launched. Both now have Windows catalog entries. Managed relocation and real project/profile/Composer/Apache proxy checks are recorded in [S8A-INTEGRATE](evidence/S8A-INTEGRATE.md); exact URLs, digests and CLI/CGI ABI evidence are in [S8A-ARTIFACTS](evidence/S8A-ARTIFACTS.md). No system PHP configuration was modified.

The PHP memcached Windows ZIP has no established publisher digest and remains outside the extension catalog. An unofficial native Windows Memcached 1.6.8 ZIP matched its GitHub publisher asset SHA256 and passed set/get, but failed occupied-port refusal; it is excluded from managed installation. See [S8A-MEMCACHED](evidence/S8A-MEMCACHED.md). An explicit endpoint probe is separate from both installing that server and loading the PHP client.

Prior Mac Node/Python/uv/pnpm payload and launch checks from 3 November are preserved in [S0-MAC-PACKAGES](evidence/S0-MAC-PACKAGES.md). The user confirmed no Mac is currently available. PHP/extensions/Memcached and integrated Mac checks remain pending; this Windows run does not replace them. The following Stage 0 snapshot retains its original dates and statements.

Checked 31 October 2026. Source and metadata research is recorded below. Stage 0 then downloaded and launched the Windows payloads named in the integration section. The Mac was not connected to this execution environment. A Windows launch does not pass the matching macOS artifact. Publisher checksums were compared for the Windows downloads; Authenticode and PGP signatures were not checked.

Additional requirement: [DEPENDENCY_MANAGEMENT.md](DEPENDENCY_MANAGEMENT.md) specifies npm/pnpm/pip/uv and PHP/Composer/extensions. Windows pnpm and pip smoke checks passed in disposable directories. PHP-extension and Memcached native artifact tests remain pending.

## Target machines and evidence

- Windows: read-only OS inspection confirmed Windows 10 Pro, build 19045 (22H2), x64.
- Existing Node: `C:\Program Files\nodejs\node.exe` executed successfully and reported v24.13.0, x64, win32.
- Existing Python: `C:\Python\Python312\python.exe` executed successfully and reported CPython 3.12.3, AMD64. Imports of ssl, sqlite3, and venv succeeded; SQLite reported 3.45.1. This is a reuse candidate, not a recommendation to freeze that patch version.
- Mac: user-confirmed M1 Pro, 16 GB RAM, macOS 14.5; native arm64 target. Runtime tests remain to be run there.
- The user confirmed Rust has never been installed. Rust/Cargo setup is required before building the application. Managed uv 0.13.6 ran from the Stage 0 artifact directory; it is not installed on the system PATH. The VC++ runtime `v14.51.36247.00` is installed. The C++ compiler toolchain and WebView2 have not yet been inspected.

Public Node checksum metadata and GitHub release APIs for uv/Python were fetched successfully. The pre-download snapshot remains in [verification-evidence.json](verification-evidence.json). Windows payload comparisons are in [S0-WIN-RUNTIMES](evidence/S0-WIN-RUNTIMES.md) and [S0-WIN-SERVERS](evidence/S0-WIN-SERVERS.md). macOS metadata that was re-fetched without a download is in [S0-MAC-PACKAGES](evidence/S0-MAC-PACKAGES.md). Signing keys were not checked.

## Component decisions

| Component | Verified distribution path | Target assessment | Development decision |
| --- | --- | --- | --- |
| Node.js | Windows x64 zip 24.22.2 downloaded from the immutable `v24.22.2` directory and checked against `SHASUMS256.txt`. macOS arm64 tarball URL and digest re-fetched; payload not downloaded | Launched on this Windows 10 x64 machine, including a path with spaces and a relocated tree. macOS 14.5 launch is pending | Accept Node 24.22.2 for Windows managed-download work. Keep the macOS arm64 artifact pending. Preserve each project's own Node requirement |
| Python | Astral CPython 3.12.16 install-only stripped archive, build 20261026, for Windows x64, checked against release `SHA256SUMS`. macOS arm64 digest re-fetched; payload not downloaded | Windows launch imported ssl, sqlite3, and venv, then pip-installed `six==1.17.0` in a disposable venv. Existing `C:\Python\Python312` 3.12.3 still runs and was not modified | Accept this Windows CPython 3.12.16 archive for managed-download work. macOS launch remains pending |
| uv | Release 0.13.6 Windows x64 zip checked against `sha256.sum`. macOS arm64 digest re-fetched; payload not downloaded | `uv 0.13.6` printed its version from the artifact directory with Python downloads disabled. It did not create a project environment | Pin uv 0.13.6 separately from Python. Windows version check passed. A uv-managed project workflow remains later work |
| Apache / Windows | Apache Lounge `httpd-2.4.69-261002-Win64-VS18.zip`, SHA256 from the publisher checksum file. PGP signature was not checked | HTTP and proxy modules, two loopback virtual hosts, proxy to a script-owned upstream, restart, and clean shutdown on this Windows 10 machine. VC++ runtime `v14.51.36247.00` was already installed and was not installed by the probe | Accept this Windows build for later Apache provider work. Do not treat the unchecked PGP signature as verified |
| Apache / macOS | ASF publishes source, not a relocatable arm64 archive. Homebrew's 31 October 2026 bottle list has no Sonoma bottle and uses `/opt/homebrew/Cellar` | No macOS binary was downloaded or launched | Packaging gate stays open. An existing Mac Apache install does not satisfy managed-download acceptance |
| MySQL / Windows | CDN `mysql-8.4.13-winx64.zip`. The publisher MD5 sidecar matched. A SHA256 sidecar returned HTTP 404, and the HTML download page returned HTTP 403 | Disposable data directory on `127.0.0.1:13306`, query, restart, shutdown, and nonempty-directory refusal. No Windows service was registered. The system `mysql` service stayed Stopped | Accept MySQL 8.4.13 for Windows disposable provider work, with MD5-only integrity recorded as a limitation |
| MySQL / macOS | Official downloads and archives were checked. No macOS arm64 tarball or DMG URL was obtained | Not launched | Open gate. Do not guess an archive URL or substitute MariaDB |
| PostgreSQL | EDB `postgresql-18.6-5-windows-x64-binaries.zip` with a publisher SHA256 sidecar. The Mac zip URL has no published digest | Windows 10 launch, query, restart, and shutdown succeeded on disposable data. postgresql.org lists Postgres 18 as tested on Windows Server, not Windows 10. macOS execution was not run | Accept the Windows 18.6 zip for later provider work and keep the Windows 10 certification gap visible. The Mac zip stays blocked until a digest exists |
| SQLite | Official tools page lists win-x64 and osx-arm64 archives; Python's embedded SQLite works locally | No standalone server required; newest CLI's 14.5 compatibility untested | Initial integration uses project SQLite files and a validated backup API. Optional CLI is a separate package |
| Local HTTPS helper | mkcert releases document macOS arm64 and Windows binaries | Published route found; no helper payload tested | Candidate helper for Stage 5; root trust is an explicit setup step, not an implicit download action |

Node evidence: [official release index](https://nodejs.org/dist/latest-v24.x/) and [v24 platform policy](https://github.com/nodejs/node/blob/v24.x/BUILDING.md). The Windows technical minimum alone is not a promise of upstream support on an OS outside its lifecycle policy.

Python/uv evidence: [standalone distribution project](https://github.com/astral-sh/python-build-standalone), [Python release API](https://api.github.com/repos/astral-sh/python-build-standalone/releases/latest), [uv release API](https://api.github.com/repos/astral-sh/uv/releases/latest), and [uv platform policy](https://docs.astral.sh/uv/reference/policies/platforms/).

Apache evidence: [Apache Lounge](https://www.apachelounge.com/download/) and [Homebrew httpd formula](https://formulae.brew.sh/formula/httpd). Homebrew bottles should not be treated as freely relocatable archives independent of their prefix/dependency graph.

MySQL evidence: [8.4 downloads](https://dev.mysql.com/downloads/mysql/8.4.html), [platform policy](https://www.mysql.com/support/supportedplatforms/database.html), and [archives](https://downloads.mysql.com/archives/community/). Published archives do not by themselves establish current compatibility or maintenance suitability.

PostgreSQL evidence: [Windows packages](https://www.postgresql.org/download/windows/), [macOS packages](https://www.postgresql.org/download/macosx/), and [EDB binary archives](https://www.enterprisedb.com/download-postgresql-binaries). The Windows x64 18.6 zip and SHA256 sidecar were resolved and checked during S0-WIN-SERVERS. The Mac zip URL was observed and has no published digest.

SQLite and HTTPS evidence: [SQLite downloads](https://sqlite.org/download.html) and [mkcert releases](https://github.com/FiloSottile/mkcert/releases). SQLite's current Mac CLI artifacts are described as unsigned; evaluate delivery/trust explicitly rather than automatically removing quarantine metadata.

## Application frameworks and desktop prerequisites

Next.js belongs in each project's Node dependencies; Flask and FastAPI belong in each project's Python environment. They are recipes, not global server installations. Preserve existing dependency lockfiles and commands. Current Next.js documentation lists Node 20.9+; Flask supports Python 3.9+; FastAPI documents running an ASGI server such as Uvicorn. Test pinned sample versions instead of inferring compatibility for all historical projects. Sources: [Next.js installation](https://nextjs.org/docs/pages/getting-started/installation), [Flask installation](https://flask.palletsprojects.com/en/stable/installation/), [FastAPI server execution](https://fastapi.tiangolo.com/deployment/manually/).

Tauri remains a plausible shell. Windows development requires Rust, C++ Build Tools, and WebView2; macOS desktop development requires Rust and suitable Xcode Command Line Tools. macOS minimum deployment settings are configurable, but dependencies/sidecars also need validation. See [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) and [macOS bundle configuration](https://v2.tauri.app/distribute/macos-application-bundle/).

## Stage 0 integration

Coordinator review on 31 October 2026. Windows commands were rechecked against retained files: the relocated Node binary printed `v24.22.2`, the relocated Python binary printed `3.12.16`, and the Node, Apache, MySQL, and PostgreSQL archives matched the digests in the evidence files. Probe ports `18080`, `18081`, `13306`, `15432`, and `18099` were free. Services `Apache2.4` and `mysql` were Stopped.

| Combination | Decision |
| --- | --- |
| Windows Node 24.22.2 zip, existing Node 24.13.0 | Accepted for managed download and reuse |
| Windows CPython 3.12.16 archive, existing Python 3.12.3 | Accepted for managed download and reuse |
| Windows uv 0.13.6 | Accepted as a pinned helper version check only |
| Windows pnpm 12.16.1 and disposable pip | Accepted as smoke checks, not as a Next.js or uv project fixture |
| Windows Apache Lounge httpd 2.4.69 VS18 | Accepted for later provider work |
| Windows MySQL 8.4.13 zip | Accepted for later provider work; integrity is publisher MD5 |
| Windows PostgreSQL 18.6 EDB zip | Accepted for later provider work on this Windows 10 machine; upstream certification table does not name Windows 10 |
| macOS Node, uv, CPython, and pnpm | Metadata only. Launch pending on the M1 Pro |
| macOS Apache | Blocked. No relocatable arm64 archive |
| macOS MySQL | Blocked. No resolved archive URL |
| macOS PostgreSQL 18.6 zip | Blocked. URL observed, digest unpublished, architecture not proven |

Disposable fixture convention for later probes: help is the default and does not download; downloads and data stay in gitignored `.artifacts/<task-id>/`; the evidence note is `docs/evidence/<task-id>.md`; loopback high ports only; no Windows service registration. New scripts should write `result.json` with `task`, `os`, `arch`, `failed_count`, and `results`. The runtime probe already uses that name. The server probe wrote `results.json`.

## What verification establishes

Windows 10 x64 has a tested managed-download path for Node, CPython, a uv version check, pnpm and pip smoke checks, Apache, MySQL, and PostgreSQL. Existing Node 24.13.0 and Python 3.12.3 were executed and left unchanged. macOS 14.5 arm64 has checksum pins for Node, uv, CPython, and pnpm, and no passed launch. Apache and MySQL on that Mac still have no approved archive. No empty checksum or guessed URL is approved for the runnable catalog.
