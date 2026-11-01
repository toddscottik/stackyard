Title: Stackyard, with pnpm as the default Node package manager
Status: accepted
Context and concrete problem:
The Stage 1 placeholder name was localdev. Branding is now chosen, and Node package installs need one default manager without rewriting projects that already chose npm.
Decision:
The product name is Stackyard. Binaries, the control pipe, and the per-user data directory use that name. A previous localdev data directory is renamed to stackyard when the new directory does not already exist. New Node selections use the pinned pnpm from the catalog. Installing Node also installs and selects that pnpm. An existing npm or pnpm lockfile stays on that manager.
Alternatives considered:
Keep the placeholder name until public release. Offer npm and pnpm as equal install buttons with no default.
Consequences and compatibility impact:
The desktop identifier is app.stackyard.desktop. The CLI command is stackyard. State moves from %LOCALAPPDATA%\localdev to %LOCALAPPDATA%\stackyard on Windows, and to ~/Library/Application Support/stackyard on macOS. npm remains available for projects that already use it.
Evidence / tests:
Rename is compile-time. The data-directory rename runs on the next launch. pnpm selection is covered by ensure_default_node_manager after a Node install.
Supersedes (if any):
The temporary localdev package name in the Stage 1 plan.
