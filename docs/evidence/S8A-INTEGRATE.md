# Stage 8A integration — Windows PHP and isolated profiles

4 November 2026. Windows 10 Pro 10.0.19045 x64; Rust 1.99.1; PHP 8.5.12 NTS VS17 x64; Composer 2.11.1; Apache Lounge 2.4.69 VS18; Windows curl 8.13.0. This is a partial Stage 8A handoff, not completion of the development plan or Mac acceptance.

Resumed Windows acceptance is now complete: [native PHP/cache](S8A-NATIVE-CACHE.md), [guarded provider](S8A-CACHE-PROVIDER.md), [FastCGI](S8A-FASTCGI.md), and [live rollout](S8A-RECONFIGURE.md). A real engine network download/activation/set-get smoke also passed. The initial-slice gaps below are retained as historical handoff context; current status is in IMPLEMENTATION_STATUS.md. Mac execution remains pending and does not block independent Stage 9 implementation.

## Delivered behavior and files

- `catalog/packages.json` adds publisher-SHA256-checked Windows PHP and Composer. The existing verified installer handles PHP ZIP and a narrowly supported raw Composer PHAR format. Both install into owned versioned storage; real relocation into spaces/Unicode paths passed. Existing PHP/Composer references remain immutable.
- `crates/core/src/php.rs` owns observed ABI identity, native artifact compatibility, bounded probes and immutable isolated project ini profiles. Failed/mismatched native loads preserve the active pointer; OS locks release on crashes. Patch compatibility requires matching minor/API/build fields and an actual load probe. A second native PHP patch runtime was not downloaded, so its end-to-end switch remains pending.
- `php_project.rs`, manifest/dependency/project/selection code, engine dispatch and CLI wire PHP/Composer, inventory and explicit profile replacement. Empty profiles disable optional shared modules; selected profiles supply the same configuration to CLI and HTTP. Active profiles protect referenced package roots from cleanup. Composer install/add/remove/selected-update/list reuse the existing cancellable executor; successful mutations run `check-platform-reqs`. The validated manifest and app-owned home/cache are explicit; inherited vendor/bin overrides are removed. Existing global Composer plugins/config are not imported; `COMPOSER_AUTH` can provide registry credentials through the environment.
- Tauri invokes expose the same typed operations. The dedicated Components/Stack interface is still Stage 13; no new visible UI workflow or live desktop window acceptance is claimed here.
- `memcached.rs` provides a bounded numeric-IP endpoint version/set/get/cleanup probe. Its seven protocol tests use disposable listeners. Real native-server findings are separate in S8A-MEMCACHED. `catalog/php-extensions.json` is deliberately empty: future entries require publisher SHA256, exact target/build metadata and contained native dependency paths. No unverified memcached DLL is installed.
- `fixtures/php-http` is a small hand-authored reproducible PHP/Composer sample with lockfile. It is not a Stage 11 recipe. `crates/core/tests/php_packages.rs` and `crates/engine/tests/php.rs` exercise real approved artifacts when their explicit environment variables are supplied.

## Actual checks

Artifact provenance and initial CLI/CGI execution: [S8A-ARTIFACTS](S8A-ARTIFACTS.md). Profile and full native engine evidence: [S8A-PROFILES](S8A-PROFILES.md). Native server rejection: [S8A-MEMCACHED](S8A-MEMCACHED.md). Existing Windows fixture fixes and startup handle regression: [S8A-WINDOWS-REGRESSIONS](S8A-WINDOWS-REGRESSIONS.md).

The reviewed Apache ZIP was rehashed before reuse: `77d715d0d9cee731e2251e7ddc2090a7a8e6d41fedef2e9827cd918593c5a652`, matching the existing catalog/publisher pin. Native integration found and fixed two Stage 5 Windows gaps: use the supported default mutex mechanism and load DSO modules, including log_config/mime, before their directives. Both project hosts then passed actual HTTP and generated-CA HTTPS checks with different OpenSSL profiles. Curl used explicit `--cacert`/`--resolve`; Windows additionally used `--ssl-no-revoke` because the disposable CA has no CRL. No insecure TLS bypass, OS trust installation or hosts modification occurred. This does not prove actual OS/browser trust.

Commands from the repository root (artifact preparation is the explicit PHP verification script):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verification/windows/php.ps1 -Action Verify
$env:STACKYARD_TEST_PHP = (Resolve-Path '.artifacts/S8A-ARTIFACTS/php runtime/php.exe').Path
$env:STACKYARD_TEST_COMPOSER = (Resolve-Path '.artifacts/S8A-ARTIFACTS/composer.phar').Path
$env:STACKYARD_TEST_PHP_ARCHIVE = (Resolve-Path '.artifacts/S8A-ARTIFACTS/php-8.5.12-1-nts-Win32-vs17-x64.zip').Path
$env:STACKYARD_TEST_APACHE = (Resolve-Path '.artifacts/S0-WIN-SERVERS/space path/apache/Apache24/bin/httpd.exe').Path
$env:STACKYARD_TEST_CURL = 'C:/Windows/System32/curl.exe'
$env:COMPOSER_DISABLE_NETWORK = '1'
cargo test --workspace --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
pnpm --dir apps/desktop run build
git diff --check
```

The final canonical workspace test run passed with every PHP/Composer/Apache variable above set: 62 core tests, real managed PHP/PHAR installation and corrupt-digest rejection, desktop library tests, captured-client autostart regression, engine lifecycle, native PHP project acceptance, eight project process/dependency cases and interrupted-install recovery. The PHP engine test passed in 16.14 seconds in that run; all tests exited 0. Lint and the desktop TypeScript/Vite production build passed. Tests gated to Linux or Mac did not run on Windows; the test name `checked_macos_archives_install_and_launch` is a no-op here and is not Mac evidence. If artifact variables are absent, native PHP tests report not executed rather than establishing support.

A separate real CLI smoke used an app-owned engine/data directory and a copied PHP fixture under `.artifacts/S8A-INTEGRATE/`. It imported the fixture, referenced approved PHP/Composer, enabled OpenSSL, installed dependencies/created the lockfile, started HTTP and checked JSON, stopped, enabled mbstring, restarted and checked that module, then stopped the project and engine. The generated lockfile was retained in the fixture. Initial autostart exposed Windows inherited-handle capture; the deterministic regression and scoped fix are recorded in S8A-WINDOWS-REGRESSIONS. After rebuilding, the complete CLI smoke passed with automatic engine startup in 5.41 seconds; no separately started engine was needed. The worker's independent captured PowerShell check returned in 2.07 seconds with the same engine PID surviving client exit, then stopped it through scoped IPC.

No user project/database/runtime was adopted implicitly. No Windows service, global PATH, trust store or hosts entry was changed. Downloads, logs, data and generated certificates remain ignored under `.artifacts/` or disposable test directories. A nonpassing Memcached verification script is retained intentionally as evidence of a rejected artifact.

## Remaining acceptance and next assignment

Stage 8A remains partial: obtain a publisher-verified PHP memcached artifact or a tested verified build path, and a native managed Memcached implementation that safely refuses occupied ports; demonstrate PHP client set/get against managed and explicit endpoints. Add Apache FastCGI/document-root integration and affected-PHP-worker-only profile restart; profile changes currently require a stopped project. Download and test a second compatible PHP patch. Mac PHP/extensions/Memcached and integrated acceptance remain pending because the user confirmed no Mac is available now.

Stage 9.1 has a source-audited [proposal](../decisions/0008-project-scopes.md), with schema/API examples and migration boundaries. It does not implement scopes, readiness or pure preview. After the remaining 8A provider work, freeze that contract and assign 9.2 with one owner for manifest/store/protocol and disjoint readiness/preview modules. Stages 10–18 remain open; public distribution is deferred.
