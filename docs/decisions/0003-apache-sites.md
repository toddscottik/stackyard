Title: Shared Apache, generated configuration, and a local CA
Status: accepted
Context and concrete problem:
Stage 5 needs one HTTP listener for many projects, with HTTPS, without editing a system Apache install and without downloading mkcert. The mkcert payload was never checked. macOS still has no approved Apache archive. On this Linux machine, names under `.localhost` do not resolve.
Decision:
One app-owned `httpd` reads only configuration generated under the data directory. `httpd -t` must accept a candidate before it replaces the live file. A refused candidate leaves the previous sites in place. Publishing comes from a `[site]` table in `stackyard.toml`: proxy one service, or serve a relative directory with `AllowOverride FileInfo` so Apache reads `.htaccess`. The default host is `{slug}.localhost`. A `.test` name is used only when the manifest sets it. The listener binds `127.0.0.1` on high ports, 18080 and 18443 unless changed. Ports below 1024 are refused. A busy port returns other high ports and does not stop the program already listening. Certificates are a CA and leaves generated in the data directory. This build does not change the system trust store. Trust install and removal copy only `stackyard-ca.pem` into a directory the user names, and removal checks a fingerprint first. `hosts-write` is a separate command and rewrites only lines marked `# stackyard`. The Windows catalog includes the Apache Lounge 2.4.69 zip already checked in Stage 0. macOS stays out of the catalog. Restarting Apache is reported as interrupting every published site.
Alternatives considered:
Download mkcert and install it into the system trust store. Edit the system `httpd.conf`. Bind ports 80 and 443 in this stage. Put a guessed macOS Apache URL in the catalog.
Consequences and compatibility impact:
Schema version is 4. A project can be static, with a `[site]` root and no `[[service]]`. Existing manifests without `[site]` still import. API version stays 1. Windows Apache launch of the catalog zip, macOS Apache, system trust installation, and ports 80 and 443 remain pending.
Evidence / tests:
`cargo test --workspace --exclude stackyard-desktop` on Linux x86_64, including `crates/engine/tests/front.rs` against Apache 2.4.58 at `/usr/sbin/apache2`. Details: `docs/evidence/S5-APACHE.md`.
Supersedes (if any):
None.
