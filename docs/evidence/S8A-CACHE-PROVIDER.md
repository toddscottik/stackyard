# S8A-CACHE-PROVIDER — exclusive TCP transport for native Memcached

4 November 2026. Executed on Windows 10 Pro 10.0.19045 x64, Rust 1.99.1. The user deferred Mac testing; none occurred here.

## Delivered bounded provider

Worker files: `crates/engine/src/cache.rs`, `crates/engine/tests/cache_transport.rs`, this evidence. Shared schema 7 cache records, protocol/DTOs, exports, dispatch, dependency pins and catalog remain coordinator-owned. The implementation follows the coordinator's chosen compensation architecture for the Windows binary's unsafe raw TCP reuse; it does not substitute a simulated cache.

`Caches::new(data_dir) -> Arc<Caches>` provides `reconcile`, `create`, `start`, `stop`, `forget`, `list`, and `shutdown`. Existing approved installations and explicitly selected existing binaries are installation references; external endpoints remain connection-only probes and are never adopted or stopped. Creating an instance validates the package name/source/target/executable, preserves a package binding and starts an owned run. Installation/download is a separate operation. No global default selection is modified.

For each run the engine creates and **holds** a loopback TCP listener before launching Memcached. On Windows it sets `SO_EXCLUSIVEADDRUSE` before bind, with no preflight bind/drop window. The backend arguments are only:

```text
-s <exclusive random user-temp directory>/m.sock -a 700 -U 0 -m 16 -t 1
```

The backend never receives a TCP port/listener argument. It listens on a private short AF_UNIX socket; the engine relays bytes from its held TCP listener. The Unix-socket path is validated through pinned socket2 before launch. The new directory is created exclusively, prepared with existing app directory permissions, and marked with an unpredictable ownership token. Normal cleanup checks that token, removes only its own socket/marker, then attempts nonrecursive removal of the now-empty directory. An unexpected file prevents directory removal and is preserved. Reconcile/forget never infer filesystem ownership from a persisted socket path.

The relay is byte-transparent: at most 32 connections; two bounded directions per connection; fixed 16 KiB buffers; 200 ms I/O/connect timeouts with global/local stop checks; partial writes retained across timeouts; TCP half-close forwarded. Excess connections are closed. It never parses cache commands or generates cache responses. A native version/add/set/get/delete probe passes through the actual relay before publishing `running`.

The existing grouped-process implementation owns the backend. A monitor owns the process/run resources, observes unexpected exit, closes listener and all relayed connections, cleans its private socket, and persists `failed`. The provider retains only stop/join handles; the monitor does not retain the provider Arc, so dropping the provider stops owned children. Intentional stop/shutdown joins transport/process work and persists `stopped`; cached values are explicitly volatile. Shutdown can signal cancellation during startup; failed or cancelled starts release listeners/processes and retain the record for retry. Instance IDs are validated before constructing log paths under `data_dir/cache/<id>.log`.

Reconciliation compares recorded PID creation identity. A matching still-live process without an owned run handle is `orphaned`: start/stop/forget refuse takeover. A missing or reused PID becomes stopped without touching that process or unknown socket paths. Forget requires stopped state, removes metadata only and preserves the installation/logs/unrelated files.

## Native artifact and provenance

Reused verified unofficial publisher artifact: jefyt `memcached-1.6.8-win64-mingw.zip`, release `1.6.8_mingw_libressl`. SHA256:

```text
48ec62cef718f0d73698414b783c0e4a69821013553ca00afe0eed324eb5994b
```

[Publisher release](https://github.com/jefyt/memcached-windows/releases/tag/1.6.8_mingw_libressl), [versioned asset](https://github.com/jefyt/memcached-windows/releases/download/1.6.8_mingw_libressl/memcached-1.6.8-win64-mingw.zip), and [versioned release API digest provenance](https://api.github.com/repos/jefyt/memcached-windows/releases/tags/1.6.8_mingw_libressl). Prior download/hash/path checks and the raw-listener failure remain documented in [S8A-MEMCACHED](S8A-MEMCACHED.md). This worker did not redownload the binary. The actual backend returned `VERSION 1.6.8` through the relay. This is still an old unofficial native port; the transport compensation does not establish upstream Windows support or current-version parity.

## Exact validation

PowerShell from the repository root:

```powershell
rustfmt --edition 2021 crates/engine/src/cache.rs crates/engine/tests/cache_transport.rs
cargo test -p stackyard-engine --lib cache::tests --offline -- --nocapture
$env:STACKYARD_TEST_MEMCACHED = (Get-ChildItem -LiteralPath '.artifacts/S8A-MEMCACHED' -Recurse -Filter memcached.exe | Select-Object -First 1).FullName
cargo test -p stackyard-engine --test cache_transport --offline -- --nocapture
cargo clippy -p stackyard-engine --all-targets --offline -- -D warnings
git diff --check -- crates/engine/src/cache.rs crates/engine/tests/cache_transport.rs docs/evidence/S8A-CACHE-PROVIDER.md
```

Results:

- Four unit transport tests passed on Windows. These use real OS sockets and a controlled echo peer, not a Memcached server: exclusive bind refuses an SO_REUSEADDR contender; 96 KiB binary data and half-close are preserved; the 33rd connection is refused; shutdown closes 32 in-flight connections within two seconds and permits port rebind; private-directory cleanup preserves unknown files.
- Seven integration tests passed, exit 0, final run 5.12 seconds. Three require `STACKYARD_TEST_MEMCACHED`: full native lifecycle, provider Drop, and shutdown during native startup. Missing executable configuration explicitly skips native evidence; ordinary transport fixtures cannot replace it.
- All-target engine Clippy with `-D warnings` passed, exit 0.
- Scoped whitespace check passed.

The integration suite demonstrated:

1. An occupied unrelated loopback listener is preserved before any backend spawn. Its actual accepted connection still returns its external owner marker. A failed instance record remains, can be stopped/forgotten, and the package is retained. Low ports are rejected.
2. A deliberately unsuitable executable exits on native server arguments; failed startup releases its reserved port, clears observed ownership, preserves one retry record and permits explicit stop/forget. This is failure evidence only, not a server emulation.
3. Runtime/target/missing-package mismatches and escaping instance IDs are rejected. Reconcile/forget leave an unknown file path and a reused unrelated PID untouched.
4. Two real private-socket Memcached instances run under a Unicode/spaces data directory with different held TCP ports. Both pass native version/set/get. A raw SO_REUSEADDR socket **and the actual unsafe raw-TCP Memcached binary** fail to steal the engine's exclusive port; the original relay still answers correctly.
5. A manager with no owned run handles reports the live recorded process as orphaned and refuses start/stop/forget; the original owned server remains reachable.
6. Stopping one instance closes an in-flight client and removes only its owned socket directory. Restart reuses the same port and loses that cache's volatile value while the second instance retains its own value.
7. Killing only the fixture-owned backend produces `failed`, clears PID/socket observations, closes the relay port and cleans the private socket. The other instance remains reachable. Retry on the same record/port succeeds.
8. Provider shutdown during native startup leaves either a cancelled failed record or a ready-then-stopped record, with no PID/socket/listener remaining. Dropping the last provider Arc also stops an owned ready process, demonstrating no monitoring ownership cycle.

No OS service, hosts file, trust store, global PATH, existing external endpoint or unrelated data was changed. Tests use disposable fixture directories; failure teardown stops owned processes and may retain only diagnostic fixture files.

## Remaining integration and limits

The coordinator must integrate the managed installation/catalog/CLI flow and the real selected PHP memcached extension client test. The native tests here explicitly reference an already verified binary; they do not claim that this worker implemented or revalidated download/extraction. PHP client compatibility, per-project extension profiles, and Apache PHP transport have separate evidence owners. Other-user access to the short temp socket directory was not tested with a second Windows account. Mac launch and native artifact acceptance remain pending.

Primary transport references: [Microsoft socket exclusivity documentation](https://learn.microsoft.com/en-us/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse) explains why the option must be applied before bind; [socket2 0.6.5 source](https://github.com/rust-lang/socket2/tree/v0.6.5) supplies the pinned native socket API. The implementation uses existing Windows process-job ownership rather than invoking a shell or registering a daemon service.
