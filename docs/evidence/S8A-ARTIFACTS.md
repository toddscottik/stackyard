# S8A-ARTIFACTS — Windows PHP and Composer feasibility

Verified 4 November 2026 on Windows 10 Pro 10.0.19045, build 19045, AMD64, with 64-bit Windows PowerShell 5.1. This is artifact/probe evidence, not completion of Stage 8A or macOS acceptance.

## Scope and files

- `scripts/verification/windows/php.ps1`: no-write/no-download default Help; explicit Verify downloads only pinned publisher artifacts, checks SHA256 before extraction/execution, checks archive paths, launches CLI/CGI and Composer with isolated settings, and writes structured `result.json`.
- `docs/evidence/S8A-ARTIFACTS.md`: this handoff.
- Disposable payloads, generated profiles, Composer manifest/lock/vendor and raw logs remain in ignored `.artifacts/S8A-ARTIFACTS/`. The final reviewed result is `.artifacts/S8A-ARTIFACTS/fresh-check/result.json` (`failed_count: 0`, 15 passed rows, one blocked row and one pending row).

No global PATH, shared php.ini, OS service, existing runtime, project, trust store or hosts file was changed. Composer runs with private home/cache directories and network disabled; fixture package scripts/plugins are disabled. Build logging uses `phpinfo(INFO_GENERAL)` to avoid recording the environment.

## Approved Windows payloads

| Artifact | Exact publisher URL | SHA256 | Provenance |
| --- | --- | --- | --- |
| PHP 8.5.12-1, NTS VS17 x64 ZIP | `https://downloads.php.net/~windows/releases/archives/php-8.5.12-1-nts-Win32-vs17-x64.zip` | `2a36a2edcb7a5b9252f638fd5e4cf84d7f6e066092654220dc0b63e5cf4e9723` | [PHP Windows downloads](https://www.php.net/downloads.php?os=windows), x64 NTS entry, published 26 October 2026 |
| Composer 2.11.1 PHAR | `https://getcomposer.org/download/2.11.1/composer.phar` | `6f1b891e1d4927d62740276c540fb490e47689e19e102c49e8da849794882f8c` | [Composer version history](https://getcomposer.org/download/) and [versioned checksum sidecar](https://getcomposer.org/download/2.11.1/composer.phar.sha256sum) |

Both observed payload digests exactly matched the pins before extraction or execution. Verify re-fetches the PHP publisher page and versioned Composer checksum before accepting cached payloads. A publisher page that no longer contains the PHP pin fails closed for provenance review. The PHP archive's `-1` packaging suffix remains in the artifact identity; the executable reports PHP `8.5.12`.

The upstream Windows download instructions identify NTS for CLI/FastCGI and require the Visual C++ runtime. This machine already had suitable runtime dependencies; no redistributable was installed. The runtime launch proves this machine's combination only. [PHP Windows downloads](https://www.php.net/downloads.php?os=windows)

## Demonstrated Windows behavior

`php.exe -n -v` and `php-cgi.exe -n -v` both succeeded from a directory containing a space (`php runtime`). They report PHP 8.5.12, built 26 October 2026 09:51:07, NTS Visual C++ 2022 x64, Zend Engine 4.5.12. `php.exe -n -r 'phpinfo(INFO_GENERAL);'` records:

| Identity field | Observed value |
| --- | --- |
| PHP API / PHP Extension | `20250925` |
| Zend Extension | `420250925` |
| PHP Extension Build | `API20250925,NTS,VS17` |
| Zend Extension Build | `API420250925,NTS,VS17` |
| Debug Build | `no` |
| Thread Safety | `disabled` |
| Compiler / architecture | Visual C++ 2022 / x64 |

The per-fixture `bundled.ini` points `extension_dir` to the verified runtime's `ext` directory and enables `mbstring`, `openssl`, `curl`, and `pdo_sqlite`. CLI reports all four loaded. CGI, launched with explicit CGI request environment, returns CGI response headers plus JSON with `sapi: cgi-fcgi` and all four loaded. The base CLI `-n` profile reports those optional modules disabled. This proves direct CGI execution; it does not prove Apache/FastCGI transport, HTTP routing or managed worker lifecycle.

Composer reports `Composer version 2.11.1 2026-10-30 14:52:10`. It is always invoked as the verified PHP executable with `-c <fixture-profile> <versioned-phar>`, never a Composer wrapper or PHP found on global PATH. The disposable fixture requires PHP `~8.5.0`, `ext-json`, `ext-mbstring`, and `ext-openssl`, with Packagist disabled:

- `install --no-interaction --no-plugins --no-scripts --no-progress`: exit 0; creates a lock file and autoload files on the fresh run; repeat install verifies the existing lock file and exits 0.
- `check-platform-reqs --no-interaction --no-plugins --no-scripts`: exit 0; PHP and all required extensions succeed.
- The same check under a second profile enabling OpenSSL/curl but omitting mbstring exits 2 and identifies `ext-mbstring` as missing.

This checks tool execution, fixture lock/autoload generation and platform enforcement. No third-party Composer library was downloaded; library resolution/install/update and project integration remain coordinator acceptance work.

Failure checks passed: an invalid ini yields PHP's dynamic-library load warning (PHP can still exit 0, so exit status alone is insufficient); corrupt artifact bytes fail SHA256 validation; synthetic traversal (`../escape.txt`), absolute (`/absolute.txt`) and drive-qualified (`C:/drive.txt`) ZIP paths are rejected before extraction; an ArtifactRoot escaping the assigned fixture is rejected before directory creation. Help with an absent custom ArtifactRoot leaves that directory absent.

## External memcached extension: blocked on integrity

The official [PECL memcached 3.4.0 Windows page](https://pecl.php.net/package/memcached/3.4.0/windows) links a PHP 8.5 NTS x64 VS17 artifact:

`https://downloads.php.net/~windows/pecl/releases/memcached/3.4.0/php_memcached-3.4.0-8.5-nts-vs17-x64.zip`

The publisher [release directory](https://downloads.php.net/~windows/pecl/releases/memcached/3.4.0/) lists that ZIP, but no checksum/signature sidecars. The extension's [GitHub v3.4.0 release](https://github.com/php-memcached-dev/php-memcached/releases/tag/v3.4.0) has no uploaded assets in the official API metadata fetched during this task; its source release does not provide a digest for the Windows ZIP. No expected SHA256 was established. The ZIP was therefore not downloaded, extracted, loaded or added to an approved runnable catalog. A self-computed hash would not fill the publisher-integrity gap.

The filename describes the desired PHP minor/thread-safety/compiler/architecture match, but no binary ABI or native dependency load was demonstrated. Upstream documents libmemcached, zlib compression, and optional SASL dependencies. Windows compatibility also requires the matching debug/build settings and dependent libraries. [PHP memcached installation](https://www.php.net/manual/en/memcached.installation.php), [Windows extension compatibility](https://www.php.net/manual/en/install.pecl.windows.php)

PHP's guide recommends PIE as PECL's replacement. PIE was researched as a provider option but was not installed or executed; no automatic source compilation or unrestricted extension bootstrap is approved by this evidence. [PHP Windows extension guide](https://www.php.net/manual/en/install.pecl.windows.php), [official PIE repository](https://github.com/php/pie)

No real PHP memcached set/get occurred. Native Memcached service feasibility is owned separately by S8A-MEMCACHED, and a connection-only endpoint does not close the managed-server gate.

## macOS 14.5 arm64: pending/unapproved

PHP's official macOS guidance directs users to third-party package managers such as Homebrew and MacPorts; it does not establish a standalone PHP binary approved for this app. [PHP macOS packages](https://www.php.net/manual/en/install.macosx.packages.php)

The [Homebrew PHP formula](https://formulae.brew.sh/formula/php) and [official formula API](https://formulae.brew.sh/api/formula/php.json), fetched 4 November 2026, report PHP 8.5.12 revision 1. Bottle keys are `arm64_golden_gate`, `arm64_tahoe`, `arm64_sequoia`, `arm64_linux`, and `x86_64_linux`; `arm64_sonoma` is absent. macOS bottles declare `/opt/homebrew/Cellar` and a substantial dependency graph. Those newer-OS bottles are not approved as relocatable macOS 14.5 packages. A bottle checksum for another OS is not evidence of target compatibility.

No matching standalone macOS 14.5 arm64 PHP/memcached binary pair with verified native dependencies was established in the checked publisher sources. No Mac launch occurred on this Windows host. Existing installation inspection or an explicit, tested packaging/build route on the M1 Pro remains required; do not mark Mac runtime, extension or Composer checks passed from this research.

## Commands and handoff

Repository-relative commands run from `D:\_work\laragon_alternative`:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verification/windows/php.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verification/windows/php.ps1 -Action Verify
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verification/windows/php.ps1 -Action Verify -ArtifactRoot D:\_work\laragon_alternative\.artifacts\S8A-ARTIFACTS\fresh-check
git diff --check -- scripts/verification/windows/php.ps1 docs/evidence/S8A-ARTIFACTS.md
```

Final full Verify exited 0. PowerShell's parser reported no syntax errors. Help's absent-root check and the rejected escaping-root invocation also passed. No Rust/UI source changed in this worker slice, so Cargo/desktop builds were not used as artifact evidence.

Coordinator inputs: approved PHP and Composer URLs/digests above; retained executable paths `.artifacts/S8A-ARTIFACTS/php runtime/php.exe` and `php-cgi.exe`; bundled extension path `php runtime/ext`; exact ABI table above. No shared-interface edits requested.

Next: integrate approved Windows PHP/Composer into the package/provider contracts, validate real project dependency operations and Apache/profile lifecycle, obtain trustworthy memcached extension integrity metadata or approve an explicit verified build route, and execute the target checks on macOS 14.5 arm64. Stage 8A remains partial until its native client/server and target acceptance gates pass.
