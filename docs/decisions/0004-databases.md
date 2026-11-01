Title: Managed databases stay outside the package tree
Status: accepted
Context and concrete problem:
Stage 6 needs MySQL, PostgreSQL, and SQLite without treating a database file as a project process, and without stopping a server that other projects still use. MySQL 8.4.13's Windows zip publishes an MD5 sidecar and no SHA256. macOS has no approved MySQL archive, and the macOS PostgreSQL zip has no published digest. This Linux machine has no login keychain.
Decision:
A managed instance is one MySQL or PostgreSQL process per engine and port. Its data directory is `{data_dir}/databases/{engine}-{port}/data`, which is outside package storage. Initialization refuses a nonempty data directory before the server is invoked. The server binds `127.0.0.1` on a high port (13306 and 15432 unless another free port in the next 20 is chosen). Passwords are random, written to mode `0600` files under `{data_dir}/secrets/`, and sent to the client on stdin or in a defaults file. They are not placed on the command line, and status replaces the secret path with `[redacted]`. An external connection is a binding with no instance and no process. Detach deletes that binding. Stopping or removing a project does not stop a database. Removing a runtime deletes only an app-owned directory under `packages/` or `tools/`. SQLite is a file chosen inside the project and copied with the SQLite backup API. There is no SQLite service process. `db migrate` returns an error. The Windows catalog gains the PostgreSQL 18.6 EDB zip and SHA256 already checked in Stage 0. MySQL stays out of the catalog until a SHA256 exists. macOS database archives stay out.
Alternatives considered:
Put the data directory inside the downloaded package so removing the package deletes it. Guess a SHA256 for the MySQL zip. Start a SQLite daemon so the UI can show a process. Store passwords in the Linux secret service on this machine and leave Windows Credential Manager and macOS Keychain for later without a shared path.
Consequences and compatibility impact:
Schema version is 5. API version stays 1. Credential Manager and Keychain are not used yet; the secret files are the store on every OS in this build. Windows catalog PostgreSQL 18.6 was not launched again here. Windows MySQL, macOS MySQL, and macOS PostgreSQL remain pending. A major-version migration is unsupported.
Evidence / tests:
`cargo test --workspace --exclude stackyard-desktop` on Linux x86_64, including `crates/engine/tests/databases.rs` against distro MySQL 8.0.46 and PostgreSQL 16.15. Details: `docs/evidence/S6-DATABASES.md`.
Supersedes (if any):
None.
