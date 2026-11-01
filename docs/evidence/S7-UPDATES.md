# S7-UPDATES evidence

Date: 1 November 2026.

Target: Linux x86_64, `uname` 6.12.94+. This is not the Windows 10 or macOS 14.5 acceptance machine. `rustc 1.99.1`. The bundled catalog's host list on this machine is empty, so refresh did not offer a Windows package as an install. No new Node, Python, pnpm, or uv archive was downloaded. The Stage 0 pins were not changed.

## Commands

```text
cargo test --workspace --exclude stackyard-desktop --offline
cargo test -p stackyard-desktop --lib --offline
npx tsc --noEmit
```

All three exited 0. The new tests are in `crates/core/src/updates.rs` and `crates/engine/tests/updates.rs`.

## What the tests showed

- A bundled news item is reported as news. It is not given a download URL. Node 24.23.0 beside 24.22.2 is installable. Node 25 and Python 3.13 are news, not automatic switches. PostgreSQL 19 beside 18 is a database notice and is not switched.
- Two projects selected Node 24.22.2. Switching only the first to 24.23.0 left the second on 24.22.2. The history row names the previous package, and that package directory is still recorded.
- Switching PostgreSQL 18.6 to 19.1 returns the migration error. The selection stays 18.6.
- A package selected by a project whose service is `running` cannot be removed. The error says a running project still uses it.
- An external Python file was changed on disk and noted as observed version 3.12.4. The notice says the file was not overwritten. The new bytes stayed, and the recorded version stayed 3.12.3.
- A journal row left `running`, plus a partial file under `incoming/`, was cleared on reconcile. A finished package file outside `incoming/` stayed.
- A missing launch file produced a health notice. The project selection was unchanged.
- After an engine start cleared that partial download, importing a project and refreshing still listed the project. Refresh also reported the application-update notice, the news item, and the dependency notice. It did not need a network call.

## Desktop

Chrome on the Vite page showed the Updates block on Components: the beside-install explanation, Check updates, and Remove unused packages. Those buttons stay disabled in the browser because it has no engine.

## Not shown here

- No second digest-checked Node or Python package was added, so a real download of a newer runtime was not run. The fixture packages cover the switch.
- Windows catalog PostgreSQL 18.6 and the Windows runtime pins were not launched again.
- Application updates are not downloaded. There is no updater.
- `deps recover` uses the existing lockfile install and labels it as recovery. It was not executed against a live npm or pip project in this run; the command path is the Stage 3 installer with that label.
