# S8A-PROFILES — PHP ABI and activation profiles

4 November 2026. Target executed: Windows 10 Pro build 19045, x64. Rust 1.99.1. macOS 14.5 arm64 execution remains pending.

## Scope and API

`crates/core/src/php.rs` owns serialized `PhpIdentity`, `PhpExtensionArtifact`, `PhpExtensionKind`, and `PhpProfile` types. The coordinator exposes this module and integrates engine/CLI clients. No dependency or package installer was introduced by this slice.

- `probe_php(executable)` executes the selected binary with `-n`, removing inherited PHP configuration variables. A bounded `phpinfo(INFO_GENERAL)`/constant probe observes PHP version, PHP/Zend APIs, thread safety, debug mode, compiler, architecture, target, and both extension build identities. PE/ELF/thin Mach-O header cross-checks reject contradictory CPU architecture observations. Universal/unknown headers use the runtime observation; this is not Mac execution evidence.
- `php_abi_compatible` and `compatible_extensions` require the same PHP major/minor and every observed ABI/build field. Patch version differences remain eligible only with a subsequent actual native load probe. Claimed artifact metadata alone never activates a profile.
- `apply_php_profile` accepts already installed/verified artifact records, validates names and canonical package containment, verifies referenced native dependency files, and writes only under `data_dir/php/profiles/<project-id>/`. Bundled and downloaded library inventory remains separate from activation. External installation/library files are never edited.
- Candidate generations contain `php.ini` and an explicit empty scan directory. PHP must load the candidate ini, report no additional scanned ini, successfully exit without startup diagnostics, and report every intended native extension. Only then is `active.json` atomically replaced. Windows uses `MoveFileExW` with replace-existing/write-through; other platforms use same-directory rename. Prior generations remain intact. An OS-owned exclusive lock releases automatically on process death; the harmless `.lock` file does not need stale-lock cleanup.
- `read_php_profile` verifies generation containment. `verify_php_profile(executable, profile)` rechecks actual runtime ABI, generated ini integrity, empty scan directory, and actual native loads without promoting anything. Call before PHP launches and runtime switches.
- `configure_php_unprofiled` provides `-n`. `php_profile_args`/`configure_php_command` provide `-c` and isolated scan environment; apply profile controls after project environment overrides. CLI and CGI can consume these settings. CGI/Apache acceptance belongs to the coordinator/artifact slice.

Each PHP process has a 10 second deadline and each captured output stream has a 256 KiB cap. Probe stdin is closed. Timeout, nonzero exit, unreadable output, excessive output, native startup warnings, wrong ini, and missing requested modules fail explicitly. Child pipe readers cannot hold the caller past the probe deadline.

## Native artifact used

Read-only verified artifact supplied by S8A-ARTIFACTS: `.artifacts/S8A-ARTIFACTS/php runtime/php.exe`, PHP **8.5.12 NTS x64 VS17**. Its observed identity is PHP API `20250925`, Zend API `420250925`, `API20250925,NTS,VS17`, `API420250925,NTS,VS17`, compiler `Visual C++ 2022`, debug false, target `x86_64-pc-windows-msvc`. Publisher archive provenance/digest is recorded in [S8A-ARTIFACTS.md](S8A-ARTIFACTS.md); this worker did not download or approve a separate binary.

## Checks and demonstrated behavior

Executed in PowerShell from the repository:

```powershell
$env:STACKYARD_TEST_PHP = (Resolve-Path '.artifacts/S8A-ARTIFACTS/php runtime/php.exe').Path
cargo test -p stackyard-core --lib php:: -- --nocapture
```

Result: **8 passed, 0 failed**, including two opt-in tests that actually launched the verified native PHP. Latest recorded run after OS-lock, bounded-record and patch-ABI compatibility changes completed in 1.82 seconds.

Demonstrated:

- Every ABI dimension is checked; a patch-compatible identity is eligible while a minor/build/thread/API mismatch is refused.
- Invalid extension names, ini interpolation/line/quote injection, duplicate selection, and package path/dependency escape are rejected.
- One project's actual PHP reports OpenSSL loaded while another project's actual PHP reports it absent, using the same executable and separate profiles.
- A claimed matching-ABI artifact containing invalid native bytes fails its separate PHP load and leaves the prior active JSON and ini byte-for-byte unchanged. A mismatched thread-safety claim is rejected before loading.
- Successful replacement updates the active pointer while preserving the old ini. Missing promotion source leaves the active pointer untouched. Competing profile operations are refused, and releasing the OS handle allows a new operation.
- Unexpected scan files and modified generated ini are rejected on revalidation.
- A sleeping PHP process is killed after a 100 ms test deadline; a 300,000-byte output exceeds the 256 KiB cap and is refused; exit code 3 is refused.

Core tests without `STACKYARD_TEST_PHP` exercise pure logic only; the two native tests return early, so their harness status alone is not runtime evidence.

## Engine acceptance extension

`crates/engine/tests/php.rs` adds an explicit opt-in engine test requiring both `STACKYARD_TEST_PHP` and `STACKYARD_TEST_COMPOSER`. It prints a skip message when either path is absent. Disposable engine state, local Composer path packages, Unicode/spaces project directories, loopback servers, and occupied-port listeners are created by the test and removed on teardown. No external runtime file is changed.

Executed:

```powershell
$env:STACKYARD_TEST_PHP = (Resolve-Path '.artifacts/S8A-ARTIFACTS/php runtime/php.exe').Path
$env:STACKYARD_TEST_COMPOSER = (Resolve-Path '.artifacts/S8A-ARTIFACTS/composer.phar').Path
$env:COMPOSER_DISABLE_NETWORK = '1'
cargo test -p stackyard-engine --test php -- --nocapture
cargo clippy -p stackyard-core -p stackyard-engine --all-targets -- -D warnings
```

Result: **1 passed, 0 failed**, 6.98 seconds. Clippy passed. The native engine test demonstrated:

- Engine IPC import, reference, and selection of verified PHP 8.5.12 and Composer 2.11.1, using project paths containing spaces and Cyrillic characters.
- Separate profiles expose OpenSSL to the first project's actual CLI and HTTP server while the second project's actual CLI and HTTP server report it absent.
- Composer installs a local path repository with network disabled, writes `vendor/autoload.php` under the intended project, executes its autoloaded PHP function, and runs `check-platform-reqs`. Repeating install preserves `composer.lock` bytes. The second project fails with a missing OpenSSL diagnostic and gets no autoloader; the first project remains installed.
- Unknown extension selection, ABI mismatch, and unloadable native library retain the active profile. A running project's profile change is refused. An ini override following a `-S` address is refused.
- Stopping/restarting one PHP project leaves the other responding. Project data survives project restart and full engine stop/restart; recorded profiles survive and revalidate.
- A separate occupied-port listener remains owned and reachable after project startup is refused. The Composer lockfile remains unchanged.

The test exposed two integration defects, fixed by the coordinator: plain Windows scope paths are required by Composer, and inherited vendor/bin overrides must be removed rather than set to empty strings. App-owned Composer home/cache and explicit scoped manifest selection are now exercised by this workflow.

Optional Apache coverage requires `STACKYARD_TEST_APACHE` and explicit `STACKYARD_TEST_CURL`. The coordinator rechecked the retained Apache Lounge 2.4.69 archive SHA256 against the approved catalog/publisher value `77d715d0d9cee731e2251e7ddc2090a7a8e6d41fedef2e9827cd918593c5a652` before execution. The binary was `.artifacts/S0-WIN-SERVERS/space path/apache/Apache24/bin/httpd.exe`; the TLS client was Windows system curl **8.13.0**, Schannel.

Executed with the PHP/Composer/offline environment above plus:

```powershell
$env:STACKYARD_TEST_APACHE = (Resolve-Path '.artifacts/S0-WIN-SERVERS/space path/apache/Apache24/bin/httpd.exe').Path
$env:STACKYARD_TEST_CURL = 'C:\Windows\System32\curl.exe'
cargo test -p stackyard-engine --test php -- --nocapture
rustfmt --edition 2021 --check crates/core/src/php.rs crates/engine/tests/php.rs
cargo clippy -p stackyard-core -p stackyard-engine --all-targets -- -D warnings
```

Result: **1 passed, 0 failed**, 17.00 seconds including the full PHP/Composer workflow and real Apache. Format and clippy checks passed. Both host-routed HTTP and generated-CA HTTPS returned the intended OpenSSL-enabled/disabled project responses and persistent project data. HTTPS used `--cacert`, `--resolve`, and a five second maximum transfer time. The Windows-only `--ssl-no-revoke` flag avoids a revocation lookup for the disposable private CA, which publishes no revocation service; CA-chain and hostname verification remain enabled. System trust and hosts files were not changed.

Removing the first Apache site leaves the second responding; removing the final site releases both high loopback Apache ports. The test exposed two existing Windows Apache generation defects, fixed by the coordinator: the unsupported Unix file mutex mechanism and module directives emitted before their `LoadModule` declarations. This establishes **Apache reverse proxy to a profiled PHP built-in HTTP server**, not FastCGI, `mod_php`, or PHP `.htaccess` behavior.

## Limits and next work

Immutable artifact installation and shared engine/CLI adapters are coordinator work. This module does not download extension binaries or claim native Memcached availability. Native dependency presence is checked and the loader proves the selected profile; it does not implement a linker dependency scanner. There is no automatic deletion of old profile generations. macOS PHP/compiler/extension launch, Mac atomic rename and flock execution, and a second PHP patch-version executable load remain pending. Next: coordinator review, combined workspace checks, and explicit reporting of the remaining platform/artifact gates.
