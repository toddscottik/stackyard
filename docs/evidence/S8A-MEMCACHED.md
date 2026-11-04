# S8A-MEMCACHED — endpoint probe and native server feasibility

4 November 2026. Worker scope: `crates/core/src/memcached.rs`, `scripts/verification/windows/memcached.ps1`, this evidence file. Coordinator owns module exports, engine/CLI/protocol/catalog and shared status. No service registration, global PATH change, external process adoption, WSL, Docker or Mac execution occurred.

## Result

The core provides an explicit numeric-IP TCP endpoint check, independent of server process ownership and PHP extension loading. `MemcachedEndpoint { address: IpAddr, port: u16 }`, `MemcachedProbe { endpoint, version, roundtrip_verified, cleanup_verified }`, `probe_memcached(endpoint, Duration) -> Result<MemcachedProbe>`, and `managed_memcached_unavailable(target)` are the integration API. An endpoint probe does not install or start a server. The coordinator must run the synchronous check outside an async executor's worker thread where applicable.

The probe validates its address/port and timeout (1 ms–30 seconds), uses an absolute deadline for connect/read/write, bounds each response line to 512 bytes and checks a fixed response sequence. It never allocates based on a server-declared value length. It generates a random 128-bit namespaced key, reserves with `add` to avoid replacing an existing key, then tests `set`/`get`, deletes that key and verifies absence. Only a confirmed reservation permits cleanup; a collision never triggers delete. A failure after confirmed reservation gets an additional bounded 250 ms cleanup attempt on a fresh connection. A possibly accepted key expires after 30 seconds. Arbitrary endpoint response contents are not reflected into errors. This is unauthenticated plain TCP text protocol, without TLS, SASL, DNS, arbitrary user keys or general cache-management commands.

On Windows 10 x64, an unofficial native Memcached 1.6.8 binary passed digest verification, launch from a path containing spaces, add/set/get/delete, process cleanup and port release. It then **failed occupied-port refusal**: a second process remained alive after five seconds and appeared as the listener on the already occupied port. The final repeatable script intentionally exits 1 on that failure. This artifact is unsuitable for managed lifecycle acceptance; no catalog inclusion is recommended. No PHP memcached extension was used by this worker; PHP client set/get acceptance remains separate.

## Native artifact research

The [upstream installation guide](https://docs.memcached.org/serverguide/) says Windows builds have no official support and recommends Homebrew on Mac. The [upstream download page](https://memcached.org/downloads) listed source `memcached-1.6.45.tar.gz`, rather than an official Windows executable. Source compilation is not an implicit fallback.

### Windows feasibility artifact (unofficial)

The [jefyt publisher repository](https://github.com/jefyt/memcached-windows) describes a native MinGW port without WSL/Cygwin. The [release](https://github.com/jefyt/memcached-windows/releases/tag/1.6.8_mingw_libressl), published 28 February 2026, identifies upstream base 1.6.8, statically linked LibreSSL 4.2.1/libevent 2.1.12, and [AppVeyor build 53617011](https://ci.appveyor.com/project/jefty/memcached-windows/builds/53617011). Reproducible-build claims were not independently verified; signatures were not checked. This old server base differs from upstream 1.6.45.

- Artifact: [memcached-1.6.8-win64-mingw.zip](https://github.com/jefyt/memcached-windows/releases/download/1.6.8_mingw_libressl/memcached-1.6.8-win64-mingw.zip).
- Expected and observed SHA256: `48ec62cef718f0d73698414b783c0e4a69821013553ca00afe0eed324eb5994b`.
- Digest provenance: publisher GitHub release asset `digest` in the [GitHub release API](https://api.github.com/repos/jefyt/memcached-windows/releases/latest). The moving API lookup resolved the immutable release/artifact above; the script pins that URL and digest.
- The coordinator approved this artifact for disposable feasibility verification, not automatic catalog inclusion. Archive paths were validated before extraction/execution. Nothing was downloaded or executed from unverified candidates.

The direct launch used executable plus arguments `-l 127.0.0.1 -p <high port> -U 0 -m 16 -t 1`, a task-owned working directory, a hidden window and redirected task-local logs. It did not daemonize or register a Windows service. The script terminates only its own process and checks the port can be rebound. It retains the verified zip and disposable extracted directory in ignored `.artifacts/S8A-MEMCACHED/`.

### macOS 14.5 arm64 candidate (not executed)

The [Homebrew formula](https://formulae.brew.sh/formula/memcached) and [publisher JSON](https://formulae.brew.sh/api/formula/memcached.json) list 1.6.45 with `arm64_sonoma` bottle, cellar `:any`, and dependencies `libevent` and `openssl@3`. Recorded metadata:

- Bottle URL: `https://ghcr.io/v2/homebrew/core/memcached/blobs/sha256:60b574b8eee009f0332f3f9baddbd00ee050f19126c5d69a356fda88f40b8d72`.
- SHA256: `60b574b8eee009f0332f3f9baddbd00ee050f19126c5d69a356fda88f40b8d72`.
- JSON generated date observed: 1 October 2026.

Metadata does not prove arbitrary-prefix relocation, dependency closure or launch on 14.5. No Mac payload was downloaded. A selected existing Homebrew installation/endpoint is independent of a Stackyard-managed package; silently invoking `brew install` or `brew services` is not implemented.

## Validation

Actual host: Windows 10 Pro 10.0.19045 x64. `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.

- Initial `./scripts/verification/windows/memcached.ps1` before occupied-port coverage: exit 0. Download SHA256 matched before extraction; real native server returned `VERSION 1.6.8` on `127.0.0.1:37665`; add/set/get/delete/get passed; owned process exited and port rebind passed. Path contained `native space`. This script uses a real native server, not a protocol fixture.
- Final `./scripts/verification/windows/memcached.ps1`: exit 1, **occupied-port acceptance failed** after preceding roundtrip/cleanup checks. Reproduced across multiple fresh ports. Last run: original PID 9392, contender PID 21512; contender remained alive after five seconds; `Get-NetTCPConnection -LocalPort 38731 -State Listen` reported `127.0.0.1:38731`, owning PID 21512. The script stopped both owned processes in `finally` and confirmed port rebind. Do not infer a proven cause from this observation; Windows socket reuse is a hypothesis. No external listener was touched.
- `cargo test -p stackyard-core memcached --offline`: exit 0, seven tests passed. Tests use local Rust TCP fixtures for exact command sequence, cleanup after invalid/oversized VALUE header, reservation collision without deleting, 512-byte response limit, malformed CRLF, server error, empty version, silent endpoint timeout, closed port and input validation. These fixtures do not establish native server or PHP extension support.
- `rustfmt --edition 2021 crates/core/src/memcached.rs`: completed.
- `cargo clippy -p stackyard-core --all-targets --offline -- -D warnings`: initial run blocked by contemporaneous shared PHP changes (`php_project.rs` type complexity and a `php.rs` cloned slice warning), with no Memcached warning. Coordinator owns combined lint resolution.
- Earlier focused builds encountered transient shared coordinator edits (missing PHP module/Composer arms); these were resolved before the successful seven-test run.

Protocol references: [basic text protocol](https://docs.memcached.org/protocols/basic/) and [upstream full protocol](https://raw.githubusercontent.com/memcached/memcached/master/doc/protocol.txt). Cleanup never calls `flush_all`.

## Handoff and remaining gates

Coordinator: keep the failing Windows candidate out of the runnable catalog, wire the endpoint DTO/result through existing engine/CLI boundaries, and expose unavailable reasons honestly. Resolve the occupied-port behavior using a separately verified suitable native artifact before implementing managed launch. A managed lifecycle must reuse shared supervision and remain distinct from an external endpoint. A real selected PHP memcached extension still needs set/get against both the managed native server and an explicitly selected existing endpoint. Mac artifact/dependency/relocation and actual Mac execution remain pending. No native managed-server parity claim is made.
