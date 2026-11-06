# Project import and configuration ownership

6 November 2026. Stage 12 extends the existing schema 2 manifest and engine
preview. The desktop and CLI use the same inspection, review and save commands.

## Detection and manual setup

Detection inspects the selected folder and a bounded list of conventional
children, including common frontend/backend directories and immediate
`apps/*` or `packages/*` workspace directories. Generated directories are
excluded. The scanner reads ordinary files only, caps each text read at
64 KiB and rejects symlinks and junctions. It does not execute project code.

| Evidence | Suggestion |
| --- | --- |
| Node package metadata, a supported manager declaration or lockfile, and a simple Next.js or Vite script | Reuse the script with manager-specific host and port forwarding |
| Express dependency and a usable script | Process-only launch. Enter the actual fixed port if the app supports one |
| Conventional Flask or FastAPI application object in a Python file | Loopback launch using the detected entry point and the pip or uv scope |
| Composer metadata or an ordinary PHP front controller | Managed PHP FastCGI and an editable document root |

Each suggestion carries its source and confidence. Complex scripts, unknown
entry points, unsupported managers and conflicting lockfiles require manual
review. The form can add a scope and a service explicitly. The installer
continues to honor existing manager and lockfile constraints.

## Review and save

Inspection returns a revision for the root and current manifest. Preview
validates the edited argument arrays and scope/site paths, then fingerprints
the exact proposed file. A save rechecks both fingerprints. New manifests
publish without replacing any destination that appears after inspection.
Edits stage a sibling file, recheck the original revision and replace it in
one rename. The form preserves comments on supported fields and retains
advanced fields when those entries stay intact. Existing legacy manifests
can be registered as written. Use the editor for unsupported forms or
advanced changes, then explicitly reload the saved file.

Inspection and preview can run while services are active. Saving and
reloading require a stopped project with no active tasks. Unpublish a
published site before either action so Apache configuration remains
consistent. Reload validates the file and updates the project view. It
leaves service startup to an explicit Start action.

In the CLI, save the `configure inspect` output as a draft, edit its
`form` if needed, and run `configure preview`. Pass the resulting
fingerprint and the same draft to `configure apply`.

## Forget

The engine refuses forget while any project operation, process or
publication is active. One SQLite transaction detaches the registration,
project and scope selections, environment records, process observations and
database bindings. Source, manifests, logs, virtual environment files, PHP
profiles, database instances, data and secret files stay on disk.

Schema 9 reserves the forgotten project ID for its original canonical
folder because some retained paths use that ID. Same-folder reimport with
the same ID can reuse retained paths. Selections and database bindings must
be configured again. Explicit removal of a runtime can still affect a
retained virtual environment that references it.
