Title: Runtime updates stay beside the current install
Status: accepted
Context and concrete problem:
Stage 7 needs a newer Node or Python without replacing the one a project already uses, and without treating a database major version, an external tool, a project dependency, or an application release as the same operation. The runnable catalog is still the bundled file. No second Node or Python pin has a checked digest beyond the Stage 0 versions, so a new catalog row would be a guessed package.
Decision:
Refresh reads the bundled catalog and its news list. It does not download a release feed. A news item has no URL and is not installed. A compatible newer catalog package for the same host is reported as installable beside the current version: Node, pnpm, and uv stay on the same major; Python and Apache stay on the same major and minor. The install lands in a versioned directory, which already existed. Switching writes one project's selection and records the previous package id. Other projects are unchanged, and the previous directory stays. MySQL and PostgreSQL version changes return the existing migration error and are not part of this switch. An external installation can be noted when its observed version differs; the recorded file is not overwritten. A missing launch file is a health notice and does not clear the selection. An install left in the journal as running is marked interrupted on the next engine start, and the incoming directory is removed. Cleanup deletes a managed package only when no project selects it. A running project's selection cannot be removed. `deps recover` reinstalls from the project's lockfile and says that package scripts are not rolled back. Application updates are a notice, not a download.
Alternatives considered:
Fetch the newest Node and Python releases during refresh and add them to the catalog without a local digest check. Overwrite the external executable when its version changes. Put database majors on the same switch as Node. Treat `deps update` as a runtime update.
Consequences and compatibility impact:
Schema version is 6. API version stays 1. Windows and macOS still have no second checked runtime pin, so this machine did not download a newer Node or Python payload. The switch, recovery, and notice behavior is covered with fixture packages and the bundled catalog. Credential Manager and Keychain remain as recorded in Stage 6.
Evidence / tests:
`cargo test --workspace --exclude stackyard-desktop` on Linux x86_64, including `crates/core/src/updates.rs` and `crates/engine/tests/updates.rs`. Details: `docs/evidence/S7-UPDATES.md`.
Supersedes (if any):
None.
