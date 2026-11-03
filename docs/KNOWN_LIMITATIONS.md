# Known limitations

This is the personal-release list as of 1 November 2026. It is not a claim that the Windows 10 and macOS 14.5 acceptance run has passed.

## Acceptance machines

- No Stage 8 run has been done on Windows 10 x64 or on the M1 Pro running macOS 14.5. Sleep and wake, a live desktop window, and startup time with an editor and browser on those machines are not measured.
- The user's own projects were not in this workspace, so they were not part of the run.
- macOS Node 24.22.2, CPython 3.12.16, uv 0.13.6, and pnpm 12.16.1 are in the catalog after a payload check on macOS 14.5 arm64. Apache, MySQL, and PostgreSQL are still not installable on macOS.
- macOS Apache has no approved archive. macOS MySQL has no approved archive. The macOS PostgreSQL zip has no published digest.

## Packages

- The runnable catalog has the Stage 0 Windows pins plus the macOS arm64 Node, CPython, uv, and pnpm pins checked on 3 November 2026. No second digest-checked Node or Python version has been added, so a newer runtime was not downloaded.
- MySQL 8.4.13 is in the Windows catalog. The publisher ships an MD5 sidecar and no SHA256, so that entry is checked with the published MD5.
- The Windows Apache and PostgreSQL catalog entries were checked in Stage 0 and were not launched again from the catalog in later stages.
- `uv` was not on the Linux machine used for the Stage 8 fixture check, so the uv path was not run there. Windows uv 0.13.6 was checked in Stage 2.

## Behavior that is intentional

- Database major versions are not migrated. `db migrate` returns an error, and the runtime switch refuses a MySQL or PostgreSQL version change.
- External installations are recorded and left in place. Removing a package does not delete them, and an observed version change does not overwrite the file.
- Removing a runtime does not delete database files. Stopping a project does not stop a shared database. Detach of an external connection does not stop that server.
- Ports below 1024 are refused. A busy high port is reported and the program already listening is left running.
- The system trust store is not modified. Trust install copies only `stackyard-ca.pem` into a directory you name.
- Application updates are not downloaded. Upstream release news is not an installable package.
- `deps recover` reinstalls from the lockfile. It does not roll back package scripts.
- Passwords are mode `0600` files under the data directory. Windows Credential Manager and macOS Keychain are not wired up.
- Mailpit is not included.
- PHP, Composer, extensions, and Memcached are Stage 8A and are not in this build.
