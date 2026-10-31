# S0-MAC-PACKAGES

Date fetched: 31 October 2026.

Named target: MacBook M1 Pro, 16 GB, macOS 14.5 arm64. That machine is available for later testing. It is not the machine that ran this task.

This task ran on Windows. It re-fetched publisher metadata and wrote a smoke script. It did not launch any macOS artifact, did not compare a downloaded Mac payload to its digest, and did not approve anything for the runnable catalog. Metadata re-fetched and launched are different states. Every execution cell below is pending.

Small checksum and HTML/JSON documents were fetched. Mac binaries were not downloaded. The Node tarball, uv archive, CPython archive, pnpm archive, and PostgreSQL zip were not saved to disk.

## Matrix

| Component | Target | URL | Digest and source | Execution | Packaging decision |
| --- | --- | --- | --- | --- | --- |
| Node.js 24.22.2 | macOS 14.5 arm64 | `https://nodejs.org/dist/v24.22.2/node-v24.22.2-darwin-arm64.tar.gz` | `aa9549349e47dffaa42dcf95d6e06ade8c4bdafc980979883b88767670df385a` from `https://nodejs.org/dist/v24.22.2/SHASUMS256.txt` | Pending: not run on macOS 14.5 | Metadata re-fetched from the immutable version directory. Payload not downloaded. Candidate only after the Mac checksum and launch probes. |
| uv 0.13.6 | macOS 14.5 arm64 | `https://github.com/astral-sh/uv/releases/download/0.13.6/uv-aarch64-apple-darwin.tar.gz` | `60fbd5c8a1ed681845eea28d9d622fcde03c51e91a43465b64a1e17db1ed3472` from `uv-aarch64-apple-darwin.tar.gz.sha256` and the matching `sha256.sum` line on the 0.13.6 release | Pending: not run on macOS 14.5 | Metadata re-fetched. Payload not downloaded (release API size 14235670 bytes). Candidate only after the Mac probes. |
| CPython 3.12.16 build 20261026 | macOS 14.5 arm64 | `https://github.com/astral-sh/python-build-standalone/releases/download/20261026/cpython-3.12.16%2B20261026-aarch64-apple-darwin-install_only_stripped.tar.gz` | `353d5f1b4fd2715e7053f12f068435d5bbad16b91e057006b5d01e0925709a7e` from release `SHA256SUMS` for `cpython-3.12.16+20261026-aarch64-apple-darwin-install_only_stripped.tar.gz` | Pending: not run on macOS 14.5 | Metadata re-fetched. Payload not downloaded (release API size 25035343 bytes). Candidate only after the Mac probes. |
| pnpm 12.16.1 | macOS 14.5 arm64 | `https://github.com/pnpm/pnpm/releases/download/v12.16.1/pnpm-darwin-arm64.tar.gz` | `6661212a6ec725634b0d3268e5bc9408e3152506dab5997891e38fa1ef22bf9c` from the GitHub release asset `digest` field on tag `v12.16.1` | Pending: not run on macOS 14.5 | Metadata re-fetched. The release asset list and notes contain no maintainer SHA256SUMS file. Payload not downloaded (API size 22403634 bytes). Candidate only after the Mac probes. |
| Apache httpd | macOS 14.5 arm64 | unresolved | unresolved | Pending: not run on macOS 14.5 | Blocked. No official relocatable arm64 archive. Packaging gate below. Homebrew bottles are not a managed-download provider. |
| MySQL 8.4 | macOS 14.5 arm64 | unresolved | unresolved | Pending: not run on macOS 14.5 | Blocked. Platform policy does not name 14.5. The downloads selector was not completed, so no tarball or DMG URL is recorded. MariaDB was not substituted. |
| PostgreSQL 18.6 | macOS 14.5 arm64 | `https://get.enterprisedb.com/postgresql/postgresql-18.6-5-osx-binaries.zip` | unresolved | Pending: not run on macOS 14.5 | Blocked. The publisher exposes this Mac OS X zip and no digest. The filename does not say arm64. Download stays disabled until a digest is published. |

## What was re-fetched

### Node.js

`https://nodejs.org/dist/v24.22.2/SHASUMS256.txt` contains:

`aa9549349e47dffaa42dcf95d6e06ade8c4bdafc980979883b88767670df385a  node-v24.22.2-darwin-arm64.tar.gz`

That matches the earlier observation from the floating `latest-v24.x` index. This task used the immutable `v24.22.2` directory. A HEAD request to the tarball URL on 31 October 2026 returned HTTP 200, `Content-Length: 53068959`, `Last-Modified: Tue, 27 Oct 2026 23:38:09 GMT`. The body was not downloaded, so this hash is the publisher checksum, not a hash of a local payload.

### uv

GitHub release tag `0.13.6` lists `uv-aarch64-apple-darwin.tar.gz` (14235670 bytes) and `uv-aarch64-apple-darwin.tar.gz.sha256`. The sidecar file and `sha256.sum` both contain `60fbd5c8a1ed681845eea28d9d622fcde03c51e91a43465b64a1e17db1ed3472`. The tarball was not downloaded.

### CPython

Release `20261026` lists `cpython-3.12.16+20261026-aarch64-apple-darwin-install_only_stripped.tar.gz` (25035343 bytes) and `SHA256SUMS` (142210 bytes). The checksum line is `353d5f1b4fd2715e7053f12f068435d5bbad16b91e057006b5d01e0925709a7e`. The tarball was not downloaded.

### pnpm

`https://api.github.com/repos/pnpm/pnpm/releases/tags/v12.16.1` reports tag `v12.16.1`, published `2026-10-27T15:28:26Z`. Asset `pnpm-darwin-arm64.tar.gz` is 22403634 bytes. Its API `digest` is `sha256:6661212a6ec725634b0d3268e5bc9408e3152506dab5997891e38fa1ef22bf9c`. The asset list has no checksum file, and the release notes contain no digest. The tarball was not downloaded. On this Windows machine, the script's awk extractor reproduced that same digest from the saved API body. That check did not download or run pnpm.

### Apache

Fetched 31 October 2026:

- `https://httpd.apache.org/download.cgi` — stable release 2.4.69 (2026-10-01). Files offered are source archives `httpd-2.4.69.tar.bz2` and `httpd-2.4.69.tar.gz`, plus a pointer to third-party Windows builds. No macOS binary.
- `https://downloads.apache.org/httpd/httpd-2.4.69.tar.gz.sha256` — `c551b9861e26f146f57e1b1765e69c89a60b3db0ec252ccab0d36283487016bb *httpd-2.4.69.tar.gz`. This is the source archive, not a macOS build.
- `https://www.apachelounge.com/download/` — Windows VS18 binaries only (`httpd-2.4.69-261002-Win64-VS18.zip` and Win32). The page says those builds run on Windows Vista through Windows 11 and listed Windows Server editions. No macOS archive.
- `https://formulae.brew.sh/api/formula/httpd.json` — `generated_date` 2026-10-31, stable 2.4.69. Bottle keys: `arm64_golden_gate`, `arm64_tahoe`, `arm64_sequoia`, `arm64_linux`, `x86_64_linux`. There is no `arm64_sonoma` or `sonoma` bottle. Each macOS bottle records cellar `/opt/homebrew/Cellar`. Declared dependencies: `apr`, `apr-util`, `brotli`, `libnghttp2`, `openssl@4`, `pcre2`, plus `libxcrypt` and `libxml2` from macOS.
- `https://docs.brew.sh/Support-Tiers` — as of September 2026, Apple Silicon Tier 1 is macOS Golden Gate 27, Tahoe 26, and Sequoia 15. Tier 3 includes Sonoma 14. macOS 14.5 is Sonoma.

No relocatable official arm64 archive for macOS 14.5 was found. No archive URL is invented here.

### MySQL

Fetched 31 October 2026:

- `https://www.mysql.com/support/supportedplatforms/database.html` — macOS row is “macOS (Latest 2 major releases)”, architecture `arm64`, and 8.4 LTS is marked supported on that row. The page does not name macOS 14, 14.5, or Sonoma. It says Debian, macOS, and Windows client follow a latest-two-releases policy.
- The Homebrew support-tiers page above names Golden Gate 27, Tahoe 26, Sequoia 15, and Sonoma 14 as of September 2026. Under that version list, 14.5 is not one of the latest two major macOS releases. MySQL’s own page still does not say “14.5” either way.
- `https://dev.mysql.com/downloads/mysql/8.4.html` — title “MySQL Community Server 8.4.13 LTS” and a “Select Operating System” control. The default listing is Windows x64 only (`mysql-8.4.13-winx64.msi`, `mysql-8.4.13-winx64.zip`, and the debug ZIP), with MD5 checksums. No macOS arm64 tarball or DMG appeared in that listing.
- `https://downloads.mysql.com/archives/community/` — default listing is Windows packages for `mysql-26.7.0`, not an 8.4 macOS archive.
- `https://dev.mysql.com/doc/refman/8.4/en/macos-installation.html` — says macOS packages exist as a DMG installer and a tar archive, and points OS support back at the platforms page. It does not give an arm64 URL.
- `https://dev.mysql.com/doc/refman/8.4/en/macos-installation-pkg.html` — examples are `mysql-8.4.13-macos-10.13-x86_64.pkg` and `mysql-8.4.13-macos10.15-x86_64.dmg`. Those names are x86_64 and cite macOS 10.13 and 10.15. They are manual examples, not a resolved 14.5 arm64 archive, and they are not recorded as download URLs.

Direct curl from this Windows host to `mysql.com` and `dev.mysql.com` returned HTTP 403 from Akamai (“Technical Difficulties”, incident `0.98071002.1793475726.dc90bf1` at 31 October 2026 19:42:06 GMT). A later page fetch through the research reader succeeded for the URLs above and still showed the Windows default. The in-session browser could not open a tab, so the operating-system selector was not completed. No macOS arm64 tarball or DMG URL is recorded.

### PostgreSQL

Fetched 31 October 2026:

- `https://www.postgresql.org/download/macosx/` — EDB installers are tested on Postgres 18 for macOS 13+ (amd64) and 13+ (arm64). The page says they generally work on newer macOS versions as well. 14.5 is inside the “13+” range for that installer matrix. The page also lists Postgres.app, Homebrew, MacPorts, and Fink, which are not the EDB binary archive.
- `https://www.enterprisedb.com/download-postgresql-binaries` — “Binaries from installer Version 18.6” has two links: Windows x86-64 `https://sbp.enterprisedb.com/getfile.jsp?fileid=1260609` and Mac OS X `https://sbp.enterprisedb.com/getfile.jsp?fileid=1260613`. The HTML contains no sha256, checksum, arm64, or macos string for that row.
- HEAD of the Mac OS X link followed one redirect to `https://get.enterprisedb.com/postgresql/postgresql-18.6-5-osx-binaries.zip`, HTTP 200, `Content-Type: application/zip`, `Content-Length: 448233137`, `Last-Modified: Mon, 05 Oct 2026 07:57:32 GMT`. Response headers include an S3 ETag and no sha256. The body was not downloaded.
- `https://get.enterprisedb.com/postgresql/` returned S3 `AccessDenied`, so sibling names and checksum files were not listed.

The zip URL is real. Its digest is unpublished, and the filename is `osx`, not arm64. The script records the URL and refuses to download it.

## Apache packaging gate

A managed download for this target would be a new archive that does not exist yet. Building it was not started.

The archive would need to contain a relocatable macOS arm64 `httpd` built from the ASF 2.4.69 source (`https://dlcdn.apache.org/httpd/httpd-2.4.69.tar.gz`, publisher sha256 above), with deployment target 14.5. Libraries that must travel inside the prefix, because the current Homebrew formula does not treat httpd as self-contained, are APR, APR-util, PCRE2, OpenSSL, nghttp2, and brotli. Modules the later product stages need from that build are `mod_proxy`, `mod_proxy_http`, `mod_proxy_wstunnel`, `mod_ssl`, `mod_http2`, `mod_rewrite`, and `mod_headers`, plus the core set required to serve and proxy (`mpm_event`, `mod_alias`, `mod_dir`, `mod_mime`, `mod_log_config`). Mach-O install names have to be relative to the package (`@loader_path` or `@rpath`). A Homebrew cellar path, `/opt/homebrew`, or `/usr/local` does not qualify. The finished archive needs its own published digest.

Build prerequisites on the Mac, for that later packaging task:

- Xcode Command Line Tools so `clang`, `make`, the macOS 14.5 SDK, and `install_name_tool` can target arm64.
- The ASF 2.4.69 source tarball and the library sources above, built into the same prefix.
- Disk under an app-owned or disposable directory.

An existing Apache on the M1 Pro, including a Homebrew install, can be referenced later as an external installation for prototype work. This task did not inspect any Mac install. An existing install does not satisfy managed-download acceptance.

Homebrew’s current bottles are rejected for the same reason: absolute cellar `/opt/homebrew/Cellar`, a dependency graph, and no Sonoma bottle in the 31 October 2026 formula response. Bottle URLs are intentionally omitted from the smoke script.

## Smoke script

`scripts/verification/macos/packages.sh`

- Help is the default. It does not download, extract, or launch.
- Artifact root defaults to `<repo>/.artifacts/S0-MAC-PACKAGES` (gitignored). `--artifact-root` overrides it. The directory is created only after the Darwin arm64 / macOS 14.5 check.
- `checksum-compare` re-fetches the publisher checksum or the pnpm release-asset digest, requires it to match the 31 October 2026 pin, then downloads and hashes the payload. `--only` accepts `node`, `uv`, `python`, or `pnpm`.
- `probe-node`, `probe-python`, `probe-uv`, and `probe-pnpm` extract only after that check. Node’s launch probe binds `127.0.0.1` on `--port` (default 18080) or the next free high port, prints the address, and closes the socket. Python creates a venv in the artifact root and imports `ssl` and `sqlite3`. uv and pnpm print their versions with cache and home redirected into the artifact root.
- `pnpm-install --project PATH` uses that pinned binary and a store under the artifact root.
- The script refuses to claim success unless `uname` is Darwin arm64 and `sw_vers` ProductVersion is `14.5` or `14.5.x`.
- It does not call `launchctl`, does not edit `/etc/hosts`, and does not use `sudo`.
- `probe-apache` and `probe-mysql` exit 4 with the URL unresolved. `probe-postgres` exits 3 because the zip has no published digest. Those commands do not listen.
- Header comments state that macOS execution has not been run and that verification on the M1 Pro is pending.
- Probe stdout includes `catalog=not-approved`. A future Mac probe log is still not catalog acceptance.

## Checks run from Windows

These checks did not run on macOS 14.5. They are not a macOS pass.

| Command | Host | Exit | Outcome |
| --- | --- | --- | --- |
| `"C:\Program Files\Git\bin\bash.exe" -n scripts/verification/macos/packages.sh` | Windows, GNU bash 5.2.12(1)-release (x86_64-pc-msys) | 0 | Syntax accepted by that bash. macOS `/bin/bash` 3.2 was not executed. The script avoids bash-4-only syntax. |
| `packages.sh` (no args) | Git bash, `MINGW64_NT-10.0-19045 x86_64` | 0 | Help text. No download. |
| `packages.sh checksum-compare` | same | 2 | `result=refused`, reason `uname is not Darwin arm64`. Artifact root was not created. |
| `packages.sh probe-node` | same | 2 | Same refusal. Node was not downloaded. |
| `packages.sh probe-apache` | same | 4 | `url=unresolved`, packaging gate text. |
| `packages.sh probe-mysql` | same | 4 | `url=unresolved`. |
| `packages.sh probe-postgres` | same | 3 | URL printed, `digest=unresolved`, no download. |

## Limitations

- No process was started on the MacBook M1 Pro. Launch, relocation, paths with spaces, occupied ports, and cleanup are pending on macOS 14.5 arm64.
- Publisher checksum files were fetched. Payload bytes were not hashed locally. Signatures and publisher keys were not checked.
- pnpm’s digest is GitHub’s release-asset digest, not a checksum file published inside the pnpm release.
- MySQL’s macOS archive URL stays unresolved until the operating-system selector can be completed.
- PostgreSQL’s zip URL is known and its digest is not. The archive was not opened, so its CPU architecture is unverified.
- Apache’s managed-download archive does not exist. The packaging gate is a later build task.
