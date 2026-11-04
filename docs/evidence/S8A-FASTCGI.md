# S8A-FASTCGI — native PHP document roots through Apache

4 November 2026. Target actually executed: Windows 10 Pro 10.0.19045 x64; Rust 1.99.1. macOS execution was explicitly deferred by the user and was not attempted.

## Scope and delivered behavior

Worker-owned changes are `crates/core/src/apache.rs`, `crates/engine/src/front.rs`, `crates/engine/tests/php_fastcgi.rs`, and this evidence. The coordinator supplied the shared `SiteKind::Php`/manifest and selected-runtime launch contracts; the PHP worker supplied TCP readiness for `php-cgi`. No catalog, protocol, store schema, shared status or Git mutation was performed by this worker.

PHP projects can publish a real document root served by Apache, with PHP scripts delegated to the selected project's separately supervised FastCGI process. This extends the existing single shared Apache design; it does not introduce a second launcher or use PHP's development HTTP server for this path. Example current manifest:

```toml
schema = 1
name = "php-document-root"
manager = "composer"

[[service]]
id = "php"
runtime = "php"
command = ["php-cgi", "-b", "127.0.0.1:{port}"]
port = 0
ready_timeout_secs = 5

[site]
handler = "php"
service = "php"
root = "public"
```

The core adds `Upstream::FastCgi { port, root }`. Publication state reuses the existing record with `mode = "php"`, the PHP service ID and document root in `static_dir`; no migration is needed. Apache's `proxy_fcgi_module` is required only when rendering a PHP publication, with an actionable error if the selected distribution lacks it. Its module is loaded before the generated virtual hosts. Static and HTTP proxy publications retain their existing behavior.

The generated PHP virtual host uses `DocumentRoot`, `DirectoryIndex index.php index.html`, directory-scoped case-insensitive `.php` `SetHandler`, `ProxyFCGIBackendType GENERIC`, and `AllowOverride FileInfo`. Normal Apache filename mapping and per-directory rewrite occur before the FastCGI request. `ProxyTimeout` is bounded at 30 seconds and FastCGI connection reuse stays off. Windows requires the trailing slash in `proxy:fcgi://127.0.0.1:<port>/` and a conditional `ProxyFCGISetEnvIf` removing the leading slash from the generated `/C:/...` `SCRIPT_FILENAME`. That correction was verified against a real Unicode project directory, rather than inferred from a rendered configuration.

Source protection: a stopped backend produces HTTP 503, never static PHP source. A missing auto-port backend renders an unavailable virtual host. PHP backup/alternate suffixes without an approved handler are denied. A persisted PHP publication edited into a static manifest fails closed on refresh until explicitly republished. Document roots must be existing directories whose canonical paths remain within the project; absolute/prefixed paths, parent traversal, missing roots and regular files are rejected. The existing shared candidate syntax check and rollback remain in use, so failed publication/config candidates preserve the working configuration.

## Verified native payloads

Existing approved task artifacts were reused; this worker did not download executable payloads.

| Component | Executed identity | Prior artifact evidence |
| --- | --- | --- |
| PHP CLI and sibling CGI | PHP 8.5.12, NTS Visual C++ 2022 x64, built 26 October 2026; CGI reports `cgi-fcgi` | [S8A-ARTIFACTS](S8A-ARTIFACTS.md), PHP 8.5.12-1 ZIP SHA256 `2a36a2edcb7a5b9252f638fd5e4cf84d7f6e066092654220dc0b63e5cf4e9723` |
| Apache | Apache/2.4.69 Win64, Apache Lounge VS18, built 1 October 2026; OpenSSL 3.5.9 | [S0-WIN-SERVERS](S0-WIN-SERVERS.md), ZIP SHA256 `77d715d0d9cee731e2251e7ddc2090a7a8e6d41fedef2e9827cd918593c5a652` |
| HTTPS probe | curl 8.22.0 Windows, libcurl 8.22.0, OpenSSL 3.5.9 | Executable bundled with the verified Apache distribution |

## Commands and observed results

From the repository root, PowerShell:

```powershell
rustfmt --edition 2021 crates/core/src/apache.rs crates/engine/src/front.rs crates/engine/tests/php_fastcgi.rs
cargo test -p stackyard-core apache::tests --offline
cargo clippy -p stackyard-core -p stackyard-engine --all-targets --offline -- -D warnings
$env:STACKYARD_TEST_PHP = (Resolve-Path '.artifacts/S8A-ARTIFACTS/php runtime/php.exe').Path
$env:STACKYARD_TEST_APACHE = (Resolve-Path '.artifacts/S0-WIN-SERVERS/space path/apache/Apache24/bin/httpd.exe').Path
$env:STACKYARD_TEST_CURL = (Resolve-Path '.artifacts/S0-WIN-SERVERS/space path/apache/Apache24/bin/curl.exe').Path
cargo test -p stackyard-engine --test php_fastcgi --offline -- --nocapture
git diff --check -- crates/core/src/apache.rs crates/engine/src/front.rs crates/engine/tests/php_fastcgi.rs docs/evidence/S8A-FASTCGI.md
```

- Nine core Apache tests passed, including real directory containment failures and generated FastCGI virtual-host isolation.
- Core/engine all-target Clippy completed with `-D warnings`, exit 0.
- Native FastCGI acceptance passed, exit 0, one test, final run 22.10 seconds. This test is environment-gated: a run without the three explicit executable paths prints `SKIPPED` and provides no runtime acceptance.
- Scoped whitespace check passed.

The native test imported two disposable projects under paths containing spaces; one included Cyrillic characters. Both selected the same PHP installation and used separate project profiles: OpenSSL enabled for the first, disabled for the second. It started both `php-cgi` workers on loopback assigned high ports and published both Apache virtual hosts. Assertions covered:

1. HTTP and generated-CA HTTPS return the intended project, `cgi-fcgi` SAPI, OpenSSL profile and correct `SCRIPT_FILENAME` basename.
2. Directory index resolves `index.php`; `.htaccess` rewrites `/front` to `index.php?route=rewritten`, preserving method/body.
3. URL-encoded POST and HTTPS multipart upload deliver the posted field and exact uploaded fixture bytes.
4. Apache serves a static asset; uppercase `.PHP` executes; `backup.php.bak` returns 403 without source; traversal does not expose a project file outside `public`.
5. Invalid Apache candidate text fails syntax validation and retains the prior live file byte-for-byte. Publishing a missing document root fails and retains that same file and the other project's working response.
6. Editing a persisted PHP publication into a static manifest and refreshing Apache produces 503 without source; restoring its PHP manifest restores routing.
7. Stopping the first PHP worker returns 503 over both HTTP and HTTPS without the PHP source marker or `<?php`; the second project still executes with its own profile.
8. Stopping the remaining worker and unpublishing both sites releases both Apache ports. Engine/test teardown owns all processes and disposable files; failed assertions still stop owned processes and retain only task-local diagnostic fixtures.

The initial native attempts exposed an invalid handler URL authority, then a drive-prefix `SCRIPT_FILENAME` mismatch. Those were fixed and the full acceptance was rerun successfully. Temporary FastCGI trace logging used for diagnosis was removed; generated production logging remains `LogLevel info`.

## Limits and handoff

No hosts file, OS trust store, global PATH or Windows service was changed. curl supplies the generated private CA directly and resolves fixture hostnames explicitly. This worker does not claim native Memcached client/server acceptance or selective profile-restart acceptance; those have separate owners. Newly published sites with assigned ports are published after their worker reaches readiness, as in the existing proxy workflow; automatic publication refresh after a changed auto-port is a coordinator lifecycle concern. macOS remains pending and has no launch evidence from this task.

Coordinator next: review the four files, integrate the shared manifest/CGI launcher and reconfiguration work, and run the combined workspace checks/native acceptance. This worker's native fixture proves Apache/FastCGI transport and source protection on the current Windows machine, not that every PHP framework or extension combination is supported.

Primary protocol/configuration source: [Apache mod_proxy_fcgi documentation](https://httpd.apache.org/docs/2.4/mod/mod_proxy_fcgi.html), especially handler-based filename mapping, backend type and environment-variable overrides. The implementation keeps process ownership with the existing engine because mod_proxy_fcgi does not start the backend.
