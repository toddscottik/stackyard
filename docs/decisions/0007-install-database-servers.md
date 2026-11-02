Title: Creating a database installs its server
Status: accepted
Context and concrete problem:
Create instance asked for an existing `mysqld.exe` or `postgres.exe`. PostgreSQL 18.6 was already a checked Windows catalog package. MySQL 8.4.13 was accepted in Stage 0 with the publisher MD5 sidecar `7b94489d5524dea9b27097068bea25a8` because no SHA256 sidecar exists. Leaving MySQL out of the catalog made the desktop unable to install the server it initializes.
Decision:
Create instance downloads the catalog server for that engine when none is recorded, selects it for the default project, then initializes a new data directory. The Windows catalog includes MySQL 8.4.13 checked against the publisher MD5 sidecar. No SHA256 is invented. macOS database archives stay out. An existing installation can still be recorded and is not overwritten.
Alternatives considered:
Keep asking for a path to a server the user installed separately. Guess a SHA256 for the MySQL zip.
Consequences and compatibility impact:
A managed MySQL install verifies MD5, not SHA256. Database files stay outside the package directory. Supersedes the catalog exclusion in decision 0004; the rest of that decision stands.
Evidence / tests:
Publisher MD5 sidecar fetched again on 1 November 2026 and matched the Stage 0 value. Catalog digest test covers the new entry.
Supersedes (if any):
The sentence in decision 0004 that keeps MySQL out of the catalog until a SHA256 exists.
