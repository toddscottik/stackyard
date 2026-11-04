# S8A-NATIVE-CACHE — verified Windows client and managed native cache

4 November 2026. Actual host: Windows 10 Pro 10.0.19045 build 19045, AMD64. Rust/Cargo 1.99.1. Worker scope: two verification scripts, the native PHP/cache integration test, this evidence; coordinator owns catalog, engine provider, protocol/store and stage status. No Git mutation, OS service, trust/hosts change, global PATH/configuration change, user project, real database or existing cache ownership change occurred.

## Result

Real PHP memcached 3.4.0 loads in isolated CLI and CGI profiles. Native add/set/get/delete passed against a task-owned loopback endpoint. The engine integration test then passed against the engine-managed native cache and a distinct external fixture endpoint, including HTTP, an actual official PHP 8.5.11 → 8.5.12 switch, isolated project profiles, failed native-load rollback, volatile cache restart and preserving the independent endpoint when managed cache stops. These are Windows results; no Mac execution occurred.

The previously checked original PECL ZIP remains without an established publisher checksum. The working route uses separately approved **ServBay** publisher archives with GitHub release-asset SHA256 metadata. This is third-party binary provenance, not a retroactively invented PECL checksum or an independent reproducible-build attestation.

## Publisher pins and provenance

Every payload matched its expected digest before extraction or execution. The extension script re-fetches official PHP SPDX or ServBay release metadata and checks that it still declares the pin before accepting cached bytes.

| Artifact | SHA256 | Publisher source |
| --- | --- | --- |
| Official PHP 8.5.11 NTS VS17 x64 | `0ea96e0d2b9b737a6036f05cf4e95c49313faa6d0f27bd97edb2742503f0c043` | [Official SPDX](https://downloads.php.net/~windows/releases/archives/php-8.5.11-nts-Win32-vs17-x64.zip.spdx.json), package `php` 8.5.11, `SHA256` checksum |
| Official PHP 8.5.12-1 NTS VS17 x64 | `2a36a2edcb7a5b9252f638fd5e4cf84d7f6e066092654220dc0b63e5cf4e9723` | [Official SPDX](https://downloads.php.net/~windows/releases/archives/php-8.5.12-1-nts-Win32-vs17-x64.zip.spdx.json); prior S8A-ARTIFACTS launch evidence |
| ServBay PHP 8.5.11 portable x64 NTS, containing memcached 3.4.0 | `8011ec199c0b714e2c9a15b0355abe0e93117e1e09f46f486cff4463a6cb9348` | [Publisher release API](https://api.github.com/repos/ServBay/PHP-Windows-Portable/releases/tags/php-8.5.11-x64-nts), asset `php-8.5.11-windows-x64.zip` |
| ServBay PHP 8.5.12 portable x64 NTS, containing memcached 3.4.0 | `b2e21593123c4e53930b8afb4a6686e4e9f9d8481caccff43c545679992e58c9` | [Publisher release API](https://api.github.com/repos/ServBay/PHP-Windows-Portable/releases/tags/php-8.5.12-x64-nts), asset `php-8.5.12-windows-x64.zip` |
| jefyt Memcached 1.6.8 native Windows x64 ZIP | `48ec62cef718f0d73698414b783c0e4a69821013553ca00afe0eed324eb5994b` | [Publisher versioned release API](https://api.github.com/repos/jefyt/memcached-windows/releases/tags/1.6.8_mingw_libressl), asset `memcached-1.6.8-win64-mingw.zip`; raw TCP limitation retained below |

Exact payload URLs:

```text
https://downloads.php.net/~windows/releases/archives/php-8.5.11-nts-Win32-vs17-x64.zip
https://downloads.php.net/~windows/releases/archives/php-8.5.12-1-nts-Win32-vs17-x64.zip
https://github.com/ServBay/PHP-Windows-Portable/releases/download/php-8.5.11-x64-nts/php-8.5.11-windows-x64.zip
https://github.com/ServBay/PHP-Windows-Portable/releases/download/php-8.5.12-x64-nts/php-8.5.12-windows-x64.zip
https://github.com/jefyt/memcached-windows/releases/download/1.6.8_mingw_libressl/memcached-1.6.8-win64-mingw.zip
```

ServBay's [8.5.11 build](https://github.com/ServBay/PHP-Windows-Portable/actions/runs/35782227019) and [8.5.12 build](https://github.com/ServBay/PHP-Windows-Portable/actions/runs/39616547329) report success and repository head `5f5af83f797ae89a7187ddca87492cf8064d68d7`. The [workflow at that commit](https://github.com/ServBay/PHP-Windows-Portable/blob/5f5af83f797ae89a7187ddca87492cf8064d68d7/.github/workflows/build.yml) actually invokes `ServBay/php-windows-builder/extension@master`, a vendor fork of the PHP builder, rather than a pinned official action revision. The [extension configuration](https://github.com/ServBay/PHP-Windows-Portable/blob/5f5af83f797ae89a7187ddca87492cf8064d68d7/extensions.default.json) selects `https://github.com/php-memcached-dev/php-memcached`, ref `v3.4.0`, libraries `libmemcached,zlib`. The upstream release API identifies source commit `ce37962769fc7b25f9905c4b23fa53df92447a32`. This task did not rebuild those archives or verify a signing/attestation chain. Mutable build-action references remain a supply-chain limitation even though the consumed archive bytes are now pinned.

The alternative explicit source-build route was investigated but not needed after finding approved publisher-digest archives. Installed tooling includes VS2022 x64 `cl.exe` toolsets through 14.35.32215 and the VS bundled CMake/Ninja. The official PECL build log identifies `phpize`, VS17, `libmemcached.lib`, PHP headers/import library and a release NTS x64 build. No unverified build helper, library or compiler was executed, and no toolchain was installed globally. PIE's [documentation](https://php.github.io/pie/) requires prebuilt Windows DLLs; it does not itself provide a checksum for the original PECL archive or a Windows source-build fallback.

## Layout, native dependencies and actual compatibility

Both ServBay ZIPs have the relevant files at:

```text
ext/php_memcached.dll
libmemcached.dll
libhashkit.dll
```

Other files in the whole vendor ZIP, including its PHP executable, are not used as the selected runtime. The approved whole ZIP is the verified extension package; no guessed digest for a separately repacked subset is supplied. Catalog descriptor details are `library = ext/php_memcached.dll`, extension kind `extension`, native dependencies `libmemcached.dll` and `libhashkit.dll` relative to the package root.

`dumpbin.exe` 14.35.32216.1 inspected the imports without executing the DLLs:

- `php_memcached.dll`: selected PHP's `php8.dll`, `libmemcached.dll`, VCRUNTIME140, UCRT and Kernel32.
- `libmemcached.dll`: `libhashkit.dll`, Winsock, MSVCP140, VCRUNTIME140/VCRUNTIME140_1, UCRT and Kernel32.
- `libhashkit.dll`: VCRUNTIME140, UCRT and Kernel32.

Native dependency search is scoped to child processes; the selected official PHP directory remains first. No DLL is copied into the runtime or a system directory. Existing VC++ runtime components were sufficient on this machine; no redistributable installation was performed.

Both official runtime probes report x64, NTS, Visual C++ 2022, debug disabled, PHP API/extension `20250925`, Zend API `420250925`, PHP extension build `API20250925,NTS,VS17`, Zend build `API420250925,NTS,VS17`. PHP 8.5.11 reports build date 22 September 2026 13:51:38. The approved 8.5.12-1 archive reports PHP 8.5.12.

| Official selected PHP | ServBay memcached build | CLI | CGI |
| --- | --- | --- | --- |
| 8.5.11 | built with 8.5.11 | Loaded 3.4.0 | Loaded 3.4.0 |
| 8.5.12 | built with 8.5.12 | Loaded 3.4.0 | Loaded 3.4.0 |
| 8.5.12 | built with 8.5.11, retained during patch update | Loaded 3.4.0 | Loaded 3.4.0 |
| 8.5.11 | built with 8.5.12 | Rejected: actual native load fails | Not approved |

The last failure is a real compatibility limit despite equal nominal API/build markers. The newer extension imports `smart_str_ferealloc@@24`, absent from official 8.5.11's exports. The integration test attempts that candidate and verifies the working profile stays unchanged. Compatibility metadata alone is insufficient; the real load probe remains required.

`--ri memcached` reports extension 3.4.0 with libmemcached-awesome 1.1.4 and PHP serialization. SASL, session-handler, igbinary, JSON, msgpack and zstd support are disabled in these builds. No claim of those optional features is made.

## Native server route and ownership

The jefyt 1.6.8 raw TCP listener remains unsuitable for managed use because S8A-MEMCACHED demonstrated occupied-port reuse. It is not silently re-approved in that mode. The coordinator's implemented provider starts this native binary only on a private app-owned AF_UNIX socket and holds an exclusive loopback TCP listener in the engine for the entire run, relaying bytes. The server process does not own or bind the managed TCP port. Provider socket exclusivity, lifecycle and admission checks are owned by the coordinator/cache worker and require their separate evidence.

This worker's engine acceptance uses that real provider. It does not substitute a simulated Memcached implementation. A separate raw native server is launched solely as an explicit disposable external-endpoint fixture on another high loopback port, with UDP disabled; a local Child guard owns and terminates only that fixture process. It is never registered/adopted as an engine-managed cache. No existing user endpoint was stopped.

The standalone extension check also used an explicitly owned temporary raw server PID 12408 on `127.0.0.1:46111` only for client feasibility. Random keys expire after 30 seconds; add reserves before set, get validates, delete verifies cleanup. Process termination and port release passed. This check does not establish raw TCP server safety.

## Files and validation

- `scripts/verification/windows/php-memcached.ps1`: default Help performs no writes/downloads. Verify checks publisher metadata, payload SHA256 and archive containment, creates fresh isolated profiles, probes CLI/CGI and forward/reverse compatibility, and optionally tests an explicitly selected high loopback endpoint. Results/logs stay under `.artifacts/S8A-NATIVE-CACHE/`.
- `scripts/verification/windows/memcached-native.ps1`: default Help performs no writes/downloads. Verify checks five existing archives against the fixed publisher pins, supplies explicit paths to the Rust native test and refuses a skipped test. It writes `.artifacts/S8A-NATIVE-CACHE/native-provider/result.json` and bounded-run logs. It reuses the engine provider rather than introducing a second lifecycle implementation.
- `crates/engine/tests/php_memcached.rs`: opt-in real Windows native acceptance using the five explicit archive variables below. Missing variables produce a visible skip, not native compatibility evidence. The selected files are installed through `install_reader`, so the engine installer rechecks integrity in disposable state.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verification/windows/php-memcached.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verification/windows/php-memcached.ps1 -Action Verify -Port 46111
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verification/windows/memcached-native.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verification/windows/memcached-native.ps1 -Action Verify
cargo test -p stackyard-engine --test php_memcached --offline -- --nocapture
cargo clippy -p stackyard-engine --tests --offline -- -D warnings
```

The direct Cargo invocation received these process-local variables pointing at the five verified ZIPs; the native wrapper sets them itself:

```text
STACKYARD_TEST_PHP8511_ARCHIVE
STACKYARD_TEST_PHP8512_ARCHIVE
STACKYARD_TEST_MEMCACHED_EXTENSION8511_ARCHIVE
STACKYARD_TEST_MEMCACHED_EXTENSION8512_ARCHIVE
STACKYARD_TEST_MEMCACHED_SERVER_ARCHIVE
```

Outcomes:

- Standalone extension/client Verify: exit 0, `failed_count: 0`, 17 passed rows, no pending client check when explicit Port was supplied. It distinguishes missing extension (exit 42) from unavailable endpoint (a separate failed operation). CLI/CGI load and forward-patch evidence is real executable output.
- Full direct engine native test: 1 passed, 0 failed, 20.48s. An initial test draft incorrectly called Start after Create, which already starts atomically; the test was corrected to the observed contract.
- Native wrapper Verify: exit 0, `failed_count: 0`, five artifact checks and real native acceptance passed. Raw output remains in `native-provider/acceptance.*.log`.
- Full test checks CLI/HTTP set/get with memcached enabled, another project without it, retention of the valid old DLL after selecting actual 8.5.12, unchanged peer pid/runtime, rejection of the newer DLL on 8.5.11 without profile replacement, separate external native set/get, external survival after managed stop, distinct stopped-cache diagnostics, loss of volatile managed data after restart, recovered HTTP roundtrip, and unchanged persistent project data.
- Help with an absent custom ArtifactRoot left that directory absent for both scripts. PowerShell parser checks passed. A deliberately corrupt newer-extension ZIP was rejected by the native wrapper before Cargo/native acceptance execution; its separate `negative-digest/result.json` records the expected failure.
- Scoped Clippy passed after correcting one test-only owned-path comparison. Rustfmt completed for the new test. No Mac test was run. The coordinator runs the combined workspace gates.

## Handoff

The approved older extension package id is `php-memcached-3.4.0-php8.5.11-windows-x64`; its exact tested bytes work on both selected official PHP patches. Coordinator-owned catalog metadata must keep ServBay provenance, native dependency roots and optional-feature limits explicit. The newer vendor package is a verified negative reverse-load fixture rather than proof of universal patch compatibility. Managed/native Windows client acceptance now has real engine evidence; final integrated Stage8A status remains the coordinator's decision. Mac runtime/package/extension/server checks remain pending for the target machine.
