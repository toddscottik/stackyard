# S5-APACHE evidence

Date: 1 November 2026.

Target: Linux x86_64, `uname` 6.12.94+. This is not the Windows 10 or macOS 14.5 acceptance machine. `rustc 1.99.1`. Apache was the distro package already on the machine after `apache2 -v` reported `Apache/2.4.58 (Ubuntu)`, binary `/usr/sbin/apache2`. It was recorded with `use-existing`. It was not downloaded, and it is not a catalog entry. The system Apache service was not started and `/etc/apache2` was not edited.

`python3` `socket.getaddrinfo("notes.localhost", 80)` returned `EAI_NONAME`. `stackyard.localhost` is reported as not resolving. Hosts lines are written only by `hosts-write`, and only lines ending in `# stackyard` are replaced.

## Command

```text
cargo test --workspace --exclude stackyard-desktop
```

Exit code 0. The new integration test is `shared_apache_isolates_sites_and_keeps_a_bad_config_off_the_live_one` in `crates/engine/tests/front.rs`.

## What the test showed

- Trust install wrote `stackyard-ca.pem` into a disposable directory. Trust remove deleted that file only after its fingerprint matched. A neighboring `other.pem` stayed.
- Setting the HTTP port to 18190 while another socket held it returned an error that named the port and said the listener was not stopped. The holding socket was still bound afterward.
- Four published sites shared one Apache on a free high HTTP port and its HTTPS pair: two static roots, one `.htaccess` rewrite (`/hello` returned `rewrite-ok`), and one proxy.
- `Host: site-b.localhost` requesting `secret-a.txt` was 404 and did not return `site-a-only`.
- A POST body `hello-post` came back from the proxy. A `302` from the upstream was rewritten away from the upstream port and named `proxy.localhost`.
- A WebSocket upgrade through the proxy received `101`.
- HTTPS to `rewrite.localhost` with `curl --cacert` on the generated CA returned `rewrite-ok`.
- A candidate config `Listen nope` was refused by `httpd -t`. The live site still returned `site-a-only`.
- `hosts-write` kept `10.9.9.9 kept.example` and added marked `.localhost` lines.

`cargo test -p stackyard-desktop --lib` passed, including the site URL check. `npx tsc --noEmit` in `apps/desktop` passed. Chrome on the Vite page showed the Apache explanation and an Apache choice under Use existing. The engine was not in that browser, so Publish stayed unavailable there.

## Not shown here

- The Windows catalog entry `apache-2.4.69-windows-x64` is the Apache Lounge zip and SHA256 already checked on Windows 10 in `docs/evidence/S0-WIN-SERVERS.md`. This machine did not download it again. The PGP signature remains unchecked, as in that evidence.
- macOS has no approved Apache archive. No macOS `httpd` was launched.
- `certutil` and `security add-trusted-cert` were not executed. This build refuses to change the system trust store and asks for a directory instead.
- Ports 80 and 443 are rejected. No privileged port helper was installed.
- Next.js, Flask, and FastAPI were not started behind Apache in this run. Their manifests now have a `[site]` proxy. The proxy checks above used a local HTTP listener on the declared port.
