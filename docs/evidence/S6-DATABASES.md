# S6-DATABASES evidence

Date: 1 November 2026.

Target: Linux x86_64, `uname` 6.12.94+. This is not the Windows 10 or macOS 14.5 acceptance machine. `rustc 1.99.1`. Servers were the distro packages already on the machine, recorded with `use-existing`:

- MySQL `8.0.46-0ubuntu0.24.04.4`, `/usr/sbin/mysqld`, client `/usr/bin/mysql`
- PostgreSQL `16.15 (Ubuntu 16.15-0ubuntu0.24.04.1)`, `/usr/lib/postgresql/16/bin/postgres`
- SQLite `3.45.1`

They were not downloaded and they are not catalog entries. The system MySQL and PostgreSQL services were not started. `/var/lib/mysql` and `/var/lib/postgresql` were not used.

## Commands

```text
cargo test --workspace --exclude stackyard-desktop --offline
cargo test -p stackyard-desktop --lib --offline
npx tsc --noEmit
```

The first two exited 0. `npx tsc --noEmit` in `apps/desktop` exited 0. The integration test is `mysql_postgres_and_sqlite_keep_a_row_across_restart_and_restore` in `crates/engine/tests/databases.rs`. It finished in 7.23s inside the workspace run.

## What the test showed

- `db migrate` failed with a message containing `not supported`.
- A nonempty directory containing `marker.txt` was refused, and the marker was still `keep`.
- MySQL and PostgreSQL each came up on `127.0.0.1` with a free high port. Status showed `secret_path` as `[redacted]` and state `running`.
- Each instance received two project databases, `mydb`/`myother` and `pgdb`/`pgother`. A logical backup restored into a new name, and the restored database contained the row `kept`.
- Stop and start left that row in place. `DbProbe` reported `kept`.
- Detach of the second project's database left the instance `running` and left `{data_dir}/databases/{id}/data` on disk.
- `StopProject` for `alpha` left both the MySQL and PostgreSQL instances `running`.
- SQLite `data/app.sqlite` was created inside the project, backed up, and restored. The restore message contained `verified`.
- An external attach used the password `super-secret-value`. The debug text of the status view did not contain that password. The binding was `external` with an empty `instance_id`. Detach did not stop the managed instances.

## Desktop

Chrome on the Vite page showed a Databases section: managed instances, connect existing, and SQLite. The password field is `type="password"`. Components lists MySQL and PostgreSQL under Use existing. The engine was not in that browser, so Create and Connect stayed unavailable there.

## Not shown here

- The Windows catalog entry `postgres-18.6-windows-x64` is the EDB zip and SHA256 `e2246ba91d22345bc3d017586c09ede52d9df180b1eeb480f050445f1cad84e2` already checked on Windows 10 in `docs/evidence/S0-WIN-SERVERS.md`. This machine did not download it again.
- MySQL 8.4.13 remains MD5-only, so it is not in the runnable catalog. No Windows MySQL was launched in this stage.
- macOS has no approved MySQL archive, and the macOS PostgreSQL zip has no published digest. Neither was launched.
- Passwords are mode `0600` files under the data directory. Windows Credential Manager and macOS Keychain are not wired up.
- The distro servers are MySQL 8.0.46 and PostgreSQL 16.15, not the Windows catalog's PostgreSQL 18.6.
