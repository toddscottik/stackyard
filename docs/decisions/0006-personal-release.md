Title: Personal-release checks stay tied to the two target machines
Status: accepted
Context and concrete problem:
Stage 8 asks for a private release gate: setup instructions, a redacted diagnostics export, regression coverage, and an acceptance report. The gate is Windows 10 x64 and macOS 14.5 arm64, including the user's projects, sleep and wake, and measured startup with an editor and browser. This workspace is a Linux agent. Inventing those results would mark the gate passed.
Decision:
`diagnostics` writes a report of projects, packages, database instances, and setting keys. Setting values, secret paths, secret file contents, and private keys are omitted or replaced with `[redacted]`. Setup steps live in `docs/SETUP.md`. Limits live in `docs/KNOWN_LIMITATIONS.md`. Linux fixture runs and the Linux startup sample are evidence for this machine only. Stage 8 stays partial until the same checks run on Windows 10 and the M1 Pro.
Alternatives considered:
Treat the Linux fixture run as the personal-release acceptance. Add a guessed startup budget. Download an unchecked newer Node build so the report can say a runtime update was installed.
Consequences and compatibility impact:
API version stays 1. Schema version stays 6. Mailpit stays out. PHP stays in Stage 8A.
Evidence / tests:
`cargo test --workspace --exclude stackyard-desktop` on Linux x86_64, plus the fixture notes in `docs/evidence/S8-PERSONAL.md`.
Supersedes (if any):
None.
