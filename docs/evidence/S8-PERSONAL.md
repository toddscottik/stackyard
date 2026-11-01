# S8-PERSONAL evidence

Date: 1 November 2026.

This is not the personal-release acceptance run. The acceptance machines are Windows 10 x64 and the M1 Pro running macOS 14.5. This run is Linux x86_64, `uname` 6.12.94+, `rustc 1.99.1`. No editor was running. Chrome was running. Sleep and wake were not tested. The user's own projects were not in this workspace.

## Commands

```text
cargo test --workspace --exclude stackyard-desktop --offline
cargo test -p stackyard-desktop --lib --offline
npx tsc --noEmit
```

All three exited 0. The new test is `export_redacts_secrets_and_keeps_a_unicode_project_path` in `crates/core/src/diagnostics.rs`. It imports a project under a `café` directory, redacts a password and a private key, lists the setting key without the setting value, and removes a managed runtime while a database marker file stays.

## Startup on this machine

Debug build `target/debug/stackyard`, fresh data directory `/tmp/s8-measure`:

- `stackyard status` returned in 0.106 seconds, pid 62486, schema 6.
- Engine `VmRSS` and `VmHWM` were 11268 kB at the status reply and again 2 seconds later. `VmSize` was 289412 kB.
- An earlier sample with Chrome running was 11204 kB RSS. The process list showed Chrome and did not show `code` or Cursor.

These numbers are this Linux VM. They are not a target, and they are not the Windows or macOS measurement with an editor and browser.

## Fixtures on this machine

Data directory `/tmp/s8-personal`, then a second directory for Next after the pnpm pin was corrected.

- Node v22.23.4, Python 3.12.3, and pnpm were recorded with `use-existing`. They were not copied.
- `fixtures/node-http` installed with npm 10.9.9 (`npm ci`, lockfile unchanged) and served `node-http` on `127.0.0.1:40381`. Stop left the port unused by that process.
- `fixtures/flask-app` installed Flask 3.1.3 with pip 24.0 into `/tmp/s8-personal/environments/flask-app` and served `flask-ok` on `127.0.0.1:38701`.
- The first Next attempt failed: `package.json` asks for `pnpm@12.16.1` and the selected tool was 10.33.4. Stackyard did not switch the manager or rewrite the lockfile.
- A second data directory recorded corepack pnpm 12.16.1. `pnpm install --frozen-lockfile` installed Next 16.4.3, React 19.3.1, and React DOM 19.3.1. The app served HTML on `127.0.0.1:38233`.
- `uv` is not installed here, so no uv fixture ran.
- `stackyard diagnostics /tmp/s8-diagnostics.txt` wrote a report that names the projects and the limitations list. `host` is `unsupported` because this is not Windows or macOS.

## Desktop

Chrome on the Vite page showed Settings → Diagnostics, including the redaction sentence and a disabled Create button. The engine is not in that browser.

## Not accepted here

Windows 10 and macOS 14.5 builds, the user's projects, sleep and wake, a second digest-checked runtime download, catalog MySQL, macOS archives, Credential Manager, and Keychain. The list is `docs/KNOWN_LIMITATIONS.md`.
