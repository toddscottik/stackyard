# Desktop UI analysis and rework proposal

Date: 3 November 2026. Status: proposal for review; no code changed.
Scope: `apps/desktop` (React/TypeScript), its Tauri command layer, and the engine capabilities that shape what the UI can offer.
Method: read-only review of `apps/desktop/src/App.tsx`, `api.ts`, `styles.css`, `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `crates/core/src/protocol.rs`, `manifest.rs`, `project.rs`, `engine/src/database.rs`, plus the product/development docs and catalog. No desktop window was opened for this paper, so every statement below is about the implementation, not about rendering on a specific machine.

## 1. Short version

The visual language is not the problem. The sheet layout, pills, monospace paths, and restrained palette are worth keeping. The problem is the information architecture and the interaction model:

1. **The app is organized as an inventory** (`Projects`, `Components`, `Databases`, `Settings`), while the work a Laragon-style user does is **project-first** ("create this project, give it Node and PostgreSQL, start it"). Every useful decision is currently made on a different screen from the project it belongs to.
2. **A project cannot be created in the app.** The only entry point is `Import`, which asks the user to type a path to a folder that already contains a hand-written `stackyard.toml`. There is no folder picker, no creation wizard, no validation preview, and no way to edit the manifest.
3. **Components and Databases expose the storage model, not the decisions.** Components is a flat list of every package plus three unrelated blocks (updates, external connections); Databases is three stacked forms that each re-ask for a project. Several useful engine operations are not wired to the UI at all: create database/user on an instance, probe a connection, backup/restore, SQLite backup/restore, remove a package, discover existing installations, set web ports, trust/hosts, dependency recovery.
4. **Long operations and errors are handled with one global string and one top-of-page banner.** A database install blocks the screen for minutes; a failed action further down the page reports at the top, out of view; there is no per-row progress, retry, or cancel.
5. **State is ad hoc.** The whole world is fetched once at mount; polling is keyed to object identity and restarts every second; logs are re-read from offset 0; two different pollers watch the same dependency task. The UI often does not reflect the engine.
6. **The app integration contract is too low-level.** `stackyard.toml` can start a Node/Python process correctly, but the user must already know the framework's exact command and port flags. Frameworks that ignore `PORT`/`HOST` fail on auto-assignment, readiness only accepts HTTP 2xx, workers cannot be modelled, and database credentials are not injected. §7.8 defines the missing launch-profile layer.

The rework should keep the design language, replace the navigation and screen structure, and expose the engine operations that already exist. The concrete plan is in §15.

## 2. What exists today

`App.tsx` is 1,663 lines containing the entire application. There is no router, no shared data layer, and no component directory.

| Screen | What the user can do | Main gaps |
| --- | --- | --- |
| Projects | Type a folder path and Import. Select a project from a list. See Apache status. Start/stop/restart/cancel the selected project. Publish/unpublish a site. Open folder/editor/shell/URL. Pick a runtime version. Run dependency actions with a free-text workspace and package field. Read one service log. | No create, no picker, no preview. Detail is one long scroll. Header and detail duplicate lifecycle actions. Logs are not live. Project databases and site state are not part of the detail. Destructive actions have no confirmation. |
| Components | See recorded packages as rows. Install each catalog package with one long button. Record an existing executable by typing its path. Check updates and remove unused packages. | No grouping by runtime/tool/server, no used-by, no remove, no discover, no validation of the chosen path, no versions side by side. Updates and external connections are unrelated blocks on the same page. |
| Databases | Create a MySQL/PostgreSQL instance (silently downloading the server). Start/stop instances. Attach an external connection with host/port/database/user/password. Select a SQLite file per project. Detach a connection. | No project database creation, no credentials, no probe, no backup/restore, no instance detail, no database list, no remove instance, duplicated project pickers, one shared engine selector for two unrelated forms. |
| Settings | Display name, editor command, leave-running-on-quit, diagnostics export. | No web ports, trust/hosts, data location, component cleanup, app/engine versions, retry engine, or startup behavior. |

### Evidence for the strongest claims

- Import is a text input: `App.tsx:401-425`. There is no dialog plugin in `apps/desktop/package.json` and no picker command in `src-tauri/src/commands.rs`.
- The manifest is only shown as read-only example text when there are zero projects: `App.tsx:485-511`.
- `open_project("shell")` spawns `cmd.exe /D /K` at the project root (`src-tauri/src/open.rs:64-87`, `119-122`). It does not inject the selected Node/Python environment, even though the engine has an `Exec` command that does (`crates/engine/src/control.rs:37-60`, `173`). The product promise "project shell with the same runtime as the server" is not met by the desktop today.
- `Create instance` force-selects the server for the magic project id `"default"` (`App.tsx:1084`). Creating one instance therefore changes the fallback database server for every project without an explicit selection, and the UI does not say so.
- The generated database password is written to a secret file and the `Databases` view redacts its path (`crates/engine/src/database.rs:145-188`, `642-655`; `commands.rs:559-588`). There is no command that returns the password or a connection string. A database created from the UI cannot be configured in the project without finding the secret file by hand.
- `DbAdd` (create database + user for a project), `DbProbe`, `DbBackup`, `DbRestore`, `SqliteBackup`, `SqliteRestore`, `RemovePackage`, `Discover`, `SetFrontPorts`, `TrustInstall/Remove`, `WriteHosts`, `DepRecover`, and `Exec` exist in `crates/core/src/protocol.rs` but are absent from the Tauri handler list (`src-tauri/src/lib.rs:11-50`). `discover_runtimes` is registered but never called from the UI.
- `api.logs(..., 0)` is called on every effect (`App.tsx:320-336`); `nextOffset` is ignored, and the effect re-runs whenever the project object changes, i.e. about once per second while running.
- The polling effect depends on `selected` (`App.tsx:292-318`). Each poll replaces the project object, which tears down and recreates the interval. A second poller inside `dependency()` (`App.tsx:920-946`) watches the same task.
- Errors render once at the top of `main` (`App.tsx:161-168`), while the dependency and database forms are hundreds of lines lower. Focus moves to the banner, so the user is pulled away from the form that failed.
- When a project is stopped, the detail row still offers `Restart`, `Stop`, and `Cancel start` (only the label matching the primary action is filtered, `App.tsx:704-735`). Running projects similarly offer `Start` and `Cancel start`. These are enabled and fail at the engine.
- `index.html:7-12` loads IBM Plex from Google Fonts. An offline-first local tool should not depend on a remote font request to render correctly.

## 3. Why it feels unusable

### 3.1 Object-centric navigation

The sidebar answers "what kinds of records exist?", not "what am I working on?". A user who wants to try a new app must: go to Components to install Node, go to Projects to import a manifest they wrote elsewhere, return to Components to pick the runtime version (which is actually only possible from the project screen), go to Databases to create a server, then attach a database by re-selecting the project, then return to Projects to start. Each hop loses context and the project is re-selected from a dropdown.

### 3.2 No setup model

There is no representation of "this project's stack". `ProjectView` carries services, manager, and runtime selections only. It does not carry the project's database connections, site, manifest path, tools, or readiness. The UI cannot show a checklist like "runtime chosen / dependencies installed / database attached / site published", so the user has no sense of progress or completion.

### 3.3 The engine offers more than the UI exposes

The engine already supports the missing database lifecycle (`DbAdd`, probe, backup, restore) and package removal. The desktop layer is the bottleneck. That is good news: the rework is mostly UI plus a thin command surface, not new domain behavior.

### 3.4 No operation model

Installs are polled inside component handlers, dependency tasks inside two loops, database creation inside a 180-second promise. There is one `progress` string (`App.tsx:39`, `169-173`) and one global error string. Consequences:

- The user cannot tell whether a long operation is still alive or has stalled.
- Navigating away loses the operation's UI.
- Two operations cannot be observed at once.
- Buttons are disabled by a global condition rather than by the row's own state.
- Cancel exists for installs and tasks in the API but is not consistently offered (dependency cancel is never wired).

### 3.5 No progressive disclosure

Every capability is visible at once. The project detail shows site, lifecycle, open actions, services, runtime, a five-button dependency form, and logs in a single column. The Components screen shows install buttons, a use-existing form, updates, cleanup, and external connections. The result is a wall of controls with equal weight, where the primary next step is not obvious.

### 3.6 Unclear vocabulary

The UI uses internal words as user-facing labels: "recorded", "published", "managed instance", "external", "default". They describe the store, not the user's situation. The same concept appears under different names on different screens. A component row can say `managed` or `existing` with no explanation of ownership; a database row says `external` without saying "your own server; Stackyard will not start or stop it".

### 3.7 Missing desktop affordances

- No native folder/file picker, drag-and-drop, or recent folders.
- No copy buttons for paths, URLs, ports, or connection strings.
- No open-folder for logs or data.
- No keyboard shortcuts or command palette.
- No in-app reload after editing `stackyard.toml`; the user must guess that a refresh is needed.
- No confirmation for detach, unpublish, cleanup, or restore.
- No retry when the engine is unreachable.

### 3.8 Layout and accessibility details

- Below 1100 px the sidebar becomes a full-screen overlay with a "Menu" button and a Close button (`styles.css:613-664`). This is a mobile pattern inside a desktop window; a collapsed rail or narrower sidebar is better.
- Lists mix semantic roles: sometimes `<button class="slip">`, sometimes `<li class="slip">` containing a button (`App.tsx:434-447`, `1021-1042`). Screen readers and keyboard order become inconsistent.
- `aria-hidden="false"` on a list item (`App.tsx:1347`) does nothing useful.
- Disabled controls rarely say why. "Open URL" is disabled until a port exists; there is no hint that the service must be started.
- Initial render shows empty states and "Engine unreachable" before the first fetch resolves; there is no loading state.

## 4. Principles for the rework

1. **Project-first.** The project is the primary object. Components and databases are places to get and manage resources, but every project decision is made inside the project.
2. **One home per decision.** Start/stop lives in the project header. Runtime selection lives in Stack. Dependency actions live in Stack → Dependencies. Database creation lives in the project's Databases tab, with a fleet view for administration. No duplicates.
3. **Show ownership and consequences.** Every resource says where it came from (Stackyard-managed, your existing installation, shared instance, your own server) and what an action will and will not touch.
4. **Honest states.** Available, downloading, installed, in use, external, unreachable. Never present an assumption as a running service. Never offer a version switch for a database major version.
5. **Non-blocking operations.** Per-row progress, cancel, retry, and an activity drawer. The rest of the app stays usable.
6. **Actionable errors.** Name the object, the cause, and the next step. Keep raw engine detail one click away.
7. **Progressive disclosure.** The overview shows health and the next step; details live one level down.
8. **Desktop-native.** Folder picker, safe copy actions, keyboard navigation, window-appropriate layout, offline chrome.
9. **The engine stays the authority.** The UI validates for speed, but domain rules remain in Rust. No UI-spawned arbitrary commands.
10. **Keep the visual language.** Sheet, type scale, pill states, monospace for machine text, restrained color.

## 5. Vocabulary to standardize

Use these words consistently in UI copy and code:

| Term | Meaning | Do not call it |
| --- | --- | --- |
| **Component** | A binary Stackyard can install or reference: runtime, tool, or server. | "package" in user-facing text |
| **Runtime** | Component a project executes: Node, Python (later PHP, Go). | "language tool" |
| **Tool** | Component used to manage a project: pnpm, uv, Composer. | "runtime" |
| **Server** | Long-running component: Apache, MySQL, PostgreSQL, later Memcached. | "component" without qualification |
| **Service** | A process a project runs, declared in `stackyard.toml`. | "app", "site" |
| **Instance** | A Stackyard-managed database server with its own data directory. | "database" |
| **Connection** | A project's binding to a database (managed or your own). | "database" |
| **Site** | The Apache host mapping for a project. | "publish" as a noun |
| **Stack** | The project's services, runtimes, tools, connections, and extension profile. | "modules" |
| **Module** | A native extension profile: PHP extensions, Apache modules, the PHP Memcached client. Confirmed by the user on 3 November 2026. Stage 8A; not in this build. | runtime/service/database |

"Modules" is now a confirmed term: it means **native extension profiles**, not the project's services or components. The project's services, runtimes, tools, and connections are its **Stack**. The two must not be merged in labels or navigation; a module changes what a runtime can load, while a component is a binary the app installs. See §7.7 for the reserved design.

## 6. Proposed information architecture

```
Sidebar
├─ Home                      (running work, recent projects, quick create/import)
├─ Projects                  (all projects, searchable)
│   └─ Project workspace
│      ├─ Overview
│      ├─ Services
│      ├─ Stack              (runtimes, tools, dependencies)
│      ├─ Databases
│      ├─ Site
│      ├─ Logs
│      └─ Settings
├─ Components                (runtimes, tools, servers; installed + catalog + existing)
├─ Databases                 (instances + all connections + SQLite)
└─ Settings
   ├─ General
   ├─ Web                     (ports, trust, hosts)
   ├─ Data                    (data directory, cleanup, diagnostics)
   └─ About
```

Rules:

- The sidebar always shows the project list with a state dot. Selecting a project opens its workspace; the last selected tab is remembered.
- Global screens are inventories and administration, never the only place to do a project task.
- The top bar is contextual: project name and state strip on a project; screen title elsewhere. Primary action is on the right, once.
- No full-screen menu overlay. At narrow widths the sidebar becomes a rail (icons + status) or is hidden behind a proper sheet, not a blank full-screen list of four words.

### Where each action lives

| Action | Home |
| --- | --- |
| Create / import project | Home, sidebar "+", empty states |
| Start / stop / restart project | Project header |
| Start / stop individual service | Services |
| Choose runtime version | Stack |
| Install another runtime version | Stack inline, or Components |
| Use an existing installation | Components and the Stack picker |
| Install/add/remove/update dependencies | Stack → Dependencies |
| Publish/unpublish site | Site |
| Create database / attach / SQLite | Project → Databases |
| Create instance / backup / restore | Databases (fleet) and project → Databases |
| Remove unused component | Components |
| HTTP/HTTPS ports, trust, hosts | Settings → Web |
| Diagnostics, data location | Settings → Data |

## 7. Screen designs

### 7.1 Home

Purpose: answer "what is running, and what do I do next?" on launch.

```
Home
┌ Running now ──────────────────────────────────────────────┐
│ my-app        web :3000 · api :8000        [Open] [Stop]  │
│ shop          Apache :8080                  [Open]        │
│ MySQL 8.4.13  :3307 · 2 connections          [Manage]     │
└───────────────────────────────────────────────────────────┘
┌ Recent projects ──────────────────────────────────────────┐
│ my-app   running    D:\work\my-app       [Open project]   │
│ flask-api stopped   D:\work\flask-api    [Open project]   │
└───────────────────────────────────────────────────────────┘
[Create project] [Import folder]     or drop a folder here
```

- One summary line, then two short lists. No explanatory paragraphs.
- "Create project" is the primary action when there are no projects.
- Drop a folder on the window to start the import flow (Tauri drag-drop event) once the flow exists.

### 7.2 Create project wizard

Five steps, full-page, no modal. The wizard writes only files the user explicitly approves.

```
New project                                    Step 1 of 5
───────────────────────────────────────────────────────────
How do you want to start?
( ) Open an existing folder
(•) Create a project in a new or empty folder
( ) Start from a recipe            (later; shown disabled with reason)

Folder   [ D:\work\my-app               ] [Browse…]
Name     [ my-app                       ]  (lowercase, digits, hyphens)
Preview  stackyard.toml                             [Show]

                                  [Cancel]  [Continue]
```

Step 2 — Stack:

```
Detected application                                    high confidence
  Next.js 16.0.0  ·  pnpm-lock.yaml  ·  dev script
  Will run  pnpm run dev   ·   port auto   ·   readiness any HTTP at /
  [Edit]  [Why this?]
```

If detection finds nothing, the same box says "We could not detect a start command" and shows the manual form instead. Detection also runs when an existing folder is imported; if that folder already has a manifest, the box offers a diff rather than replacing it.

```
Services
[x] web     runtime [Node ▾]    command [pnpm run dev                     ]
            port [auto ▾]       ready [any HTTP ▾]   path [/]
[x] api     runtime [Python ▾]  command [python -m uvicorn app.main:app --host 127.0.0.1 --port {port}]
            port [auto ▾]       ready [any HTTP ▾]   path [/health]
[ ] worker  runtime [Node ▾]    command [node worker.js                  ]
            ready [process alive ▾]  (no port; no HTTP check)

Add service
```

Keep this at the level a user can understand. Advanced fields (depends_on, restart policy, env, timeout) go under a disclosure per service. The readiness control uses the plain-language labels from §7.8 ("any HTTP response", "2xx only", "TCP only", "process alive") and explains the current choice in one sentence.

Step 3 — Runtimes and tools:

For each role used by the stack, one row:

```
Node.js   ( ) Install 24.22.2 (managed)   ( ) Use existing…   (•) Already installed
Python    (•) Install 3.12.16 (managed)   ( ) Use existing…
Manager   [pnpm ▾] detected from pnpm-lock.yaml
```

The install runs inline with progress and cancel. The wizard can continue while it runs; it must not freeze.

Step 4 — Databases:

```
( ) No database
(•) Create a database on a Stackyard-managed server
      Server   [ PostgreSQL 18.6 ▾ ]   (Download and start if needed)
      Database [my_app]    User [my_app]   Password [Generate] [Copy]
( ) Use an existing database connection
      Host [127.0.0.1] Port [5432] Database [ ] User [ ] Password [ ]
( ) Use a SQLite file inside the project
      Path [data/app.sqlite]
```

Step 5 — Review:

```
Will create
  stackyard.toml
  data/                     (only if the database step needs it)

Will download
  Node.js 24.22.2   ~ size shown by installer
  PostgreSQL 18.6   ~ size shown by installer

Will start
  PostgreSQL on 127.0.0.1:5437   (not a Windows service)

[Back] [Create project]
```

Rules:

- Never overwrite an existing `stackyard.toml` without an explicit "Replace" choice and a visible diff.
- Never write starter source files in the first slice. Recipes that create app files are a separate, explicit decision; the personal release only needs project creation with a manifest.
- Slugify the name and show the validation inline (1–63 chars, lowercase ASCII, digits, hyphen).
- If the folder is non-empty, say so and what Stackyard will add; do not adopt other files.
- After creation, land on the project Overview with a readiness checklist and a single next action.

#### What "starter app files" means

The phrase covers three different levels of file creation. They have very different trust and maintenance costs:

| Level | What Stackyard writes | Example | Cost / risk |
| --- | --- | --- | --- |
| **A. Manifest only** (recommended first) | `stackyard.toml` (and a folder if needed). No source files. | A folder with only the manifest; the user clones or generates the app themselves. | Small, reviewable, no network, no unknown templates. |
| **B. Minimal local template** | Manifest plus a few fixed, audited files with no downloads. | `server.js` + `package.json` for Node HTTP; `app.py` + `requirements.txt` for Flask; `public/index.php` for PHP. | Larger, but deterministic and offline. Still creates user-owned files. |
| **C. Framework generator** | Runs the ecosystem's own generator, which downloads dependencies and writes many files. | `npm create vite@…`, `npx create-next-app@…`, `composer create-project laravel/laravel`, `flask --app … init`. | Largest: network, package scripts, template drift, version pinning, failure cleanup. Needs an explicit recipe/command allowlist and preview. |

The wizard in this proposal implements **A** first: it removes the "write a TOML by hand" barrier, which is the actual usability problem. **B** is a reasonable later addition if "create an empty project that already starts" is wanted; it should still show every file and require confirmation. **C** belongs to the recipes milestone and must not be smuggled into the manifest wizard: it executes third-party generators, respects each ecosystem's lockfile rules, and needs a pinned, reviewed recipe per stack. Laragon's "quick app creation" is closest to C, but the Stage 4 acceptance for this project only needs create/import, runtime selection, dependencies, start, logs, and stop.

Recommendation: **A was approved by the user on 3 November 2026.** The first slice creates a folder/`stackyard.toml` as the home for the actual application, and detects the app's start command to prefill the manifest. **B** stays an optional follow-up, and **C** is deferred until recipes are designed with the same care as catalog packages (pinned generator versions, preview, no elevation, honest partial-failure reporting).

### 7.3 Project workspace

Header:

```
my-app      ● Running        [Open site]  [Stop]  [⋯]
D:\work\my-app · Node 24.22.2 · PostgreSQL 18.6

Overview  Services  Stack  Databases  Site  Logs  Settings
```

The `⋯` menu holds Restart, Cancel start, Open folder, Open editor, Open terminal, Reload from disk, Forget project. Only valid items are enabled, and each disabled item carries a reason on hover/focus.

Overview:

```
┌ Readiness ───────────────────────┐  ┌ At a glance ──────────────┐
│ ✓ Manifest valid                 │  │ web   running  :3000      │
│ ✓ Node 24.22.2 selected          │  │ api   stopped  :8000      │
│ ✓ Dependencies installed         │  │ PostgreSQL 18.6  :5437    │
│ ! Database password not saved    │  │ Site  https://my-app…     │
│ – Site not published             │  └───────────────────────────┘
└──────────────────────────────────┘
  [Install dependencies] [Create database] [Publish site]
```

Readiness checks are computed in the UI from existing endpoints:

1. Manifest loads without error (`services` not `invalid`).
2. Every runtime used by a service has a selection.
3. Manager exists and dependencies are installed (use `DepList`/lockfile presence; do not claim more than observed).
4. Every database binding probes successfully, or the project has no binding.
5. A site is published if the manifest declares one.

Each unmet check has one action and a one-sentence explanation. This is the core of "setting up a project".

Services:

```
Launch: Express · node server.js · auto port · any HTTP at /api/health
web      running              127.0.0.1:53124  pid 1234  up 4m  ready 1.2s   [Open] [Logs] [Stop] [Edit]
api      running with errors  127.0.0.1:53125  pid 1240  up 3m  / returned 404  [Logs] [Edit]
worker   running              no port          pid 1244  up 3m                [Logs] [Stop] [Edit]
```

A service row carries three independent facts: **process** (running/stopped/exited), **readiness** (ready / never answered / not probed), and **HTTP health** (the last status code). "Running with errors" is a real state, not a failure; "failed" means the process exited or never answered. Per-row overflow for start/stop/restart when the engine supports it per service (today it is project-wide; show that honestly). A failed service shows the last log line and a link to Logs, not a generic error banner.

The "Launch" line is a one-sentence summary of the detected or edited profile, with **Edit** opening the same form as the wizard. This is where the integration contract is visible without reading TOML.

Stack:

- **Runtimes**: Node and Python rows with current selection, source, path, and change control.
- **Tools**: pnpm/uv/Composer, detected from lockfiles, with an explanation of when each is used.
- **Dependencies**: manager, lockfile, last operation, and the five actions. Replace the two free-text fields with a package picker plus scope selector; keep a manual package field only under "Advanced". Show task output in a drawer with Cancel and Copy.
- **Modules** (Stage 8A): a reserved summary row, not a fake manager. Until the 8A artifacts exist, the Stack tab says "PHP extension profiles are not in this build" and points to the limitation note. The design for the real panel is in §7.7.

Databases (project-scoped):

```
Connections
  my_app @ 127.0.0.1:5437   PostgreSQL 18.6 (managed)   probe ✓
  [Copy .env] [Backup] [Test] [Detach]

[Create database on a managed server] [Connect an existing database] [Use SQLite]
```

- Creating a database here calls `DbAdd`: the server must be running; the UI offers to start it.
- After creation, show host, port, database, user, password source, and a `Copy .env` action. The password must be obtainable deliberately; see §10.
- Detach asks for confirmation and states that the server keeps running and data is kept.
- SQLite uses a project-relative path with a picker constrained to the project root.

Site:

- Host (from manifest), mode, upstream or root, HTTP/HTTPS URLs, copy/open.
- Publish/unpublish; Apache state; restart impact ("restarts every published site").
- Trust and hosts status with links to Settings → Web. Do not duplicate those actions here.

Logs:

- Tabs per service, follow/pause, clear view, copy, open log folder.
- Read incrementally with `nextOffset`, cap the buffer, keep scroll position unless following.
- Show the log path and rotation note quietly; no permanent paragraph.

Settings (project):

- Reload from disk, open manifest, remove/forget project (files untouched), and project-level overrides if they are added later.

### 7.4 Components

```
Components                                  [Search…] [Kind ▾] [Source ▾]
─────────────────────────────────────────────────────────────────────────
Runtimes
  Node.js 24.22.2   managed   win-x64   used by 2   [⋯]
  Python 3.12.3     existing  C:\Python\Python312  used by 1  [⋯]
Tools
  pnpm 12.16.1      managed   win-x64   used by 1   [⋯]
Servers
  Apache 2.4.69     managed   win-x64   shared      [⋯]
  MySQL 8.4.13      managed   win-x64   1 instance  [⋯]
  PostgreSQL 18.6   existing  127.0.0.1:5432        [⋯]
─────────────────────────────────────────────────────────────────────────
[Install another version] [Find installations…] [Use existing…]
```

Row behavior:

- Source badge with ownership: **Stackyard** / **Your installation** / **Your server**. External rows explain that Stackyard will not update or remove them.
- "Used by" opens the list of projects; for servers, the instances/connections using it.
- Overflow: Install another version, Switch projects…, Reveal in folder, Copy path, Remove (disabled with reason when selected by a project).
- Catalog is a second tab, filterable by kind and target. Show "Not available for this platform" explicitly for entries that exist upstream but have no approved artifact, instead of hiding the category.
- Update notices move to a compact banner with a single "Review" action. Four kinds are labeled distinctly: app update, runtime update, external installation changed, project dependency update. Cleanup stays under Settings → Data.
- "Use existing" becomes a flow: role → find candidates (`Discover`) or browse → show path, version, target → validate → record. No raw path field without feedback.

### 7.5 Databases (fleet)

```
Instances                                            [New instance]
┌ MySQL 8.4.13 ──────────────────────────────────────────────────────┐
│ 127.0.0.1:3307 · running · D:\...\databases\mysql-…   [Stop] [⋯]   │
│ 2 databases · 2 project connections                                 │
│   my_app   my-app   [Open] [Backup] [⋯]                             │
│   shop     shop     [Open] [Backup] [⋯]                             │
└─────────────────────────────────────────────────────────────────────┘

Project connections
  my-app → my_app @ 127.0.0.1:3307   managed   probe ✓   [Details]
  blog   → blog   @ 127.0.0.1:5432   your own  —         [Details]

SQLite
  notes  data/notes.sqlite   [Backup] [Restore]
```

- Instance creation shows a review before downloading: engine version, artifact source and digest type, target, private data directory, loopback bind, "not a system service", and the default-selection side effect if it is kept.
- Instance detail lists databases and actions; backup/restore use native save/open dialogs and report the path.
- No instance removal or database drop until an engine command exists; say so where the user would look for it rather than leaving a dead end.
- If stopping an instance would affect connections, warn with the count and leave the choice to the user.
- The shared `engine` selector bug disappears: create and attach are separate forms with their own state.

### 7.6 Settings

- **General**: display name, start-at-login (if supported), tray behavior, theme (light/dark/system).
- **Web**: HTTP/HTTPS ports (from engine `SetFrontPorts`), occupied-port alternatives, CA path, trust install/remove with explicit directory, hosts write with target path and a preview of the marked lines.
- **Data**: data directory (open/copy), log folder, disk usage by category, "remove unused packages" with a preview of what will be removed and what is protected.
- **Diagnostics**: report preview, copy/save; keep the redaction explanation.
- **About**: app version, engine version, API/schema version, catalog revision, licenses.

### 7.7 Modules: native extension profiles (Stage 8A, reserved)

Confirmed on 3 November 2026: "modules" means **native extension profiles**, not services, components, or databases. Examples: the PHP `memcached` and `redis` extensions, Xdebug, and Apache modules. The PHP `memcached` extension is a client interface; the Memcached server is a separate Server/instance. They belong in different places and must never be presented as one install.

Where the design reserves space:

| Surface | Module behavior |
| --- | --- |
| Components → **Modules** tab (later) | Artifacts compatible with the selected PHP runtime: name, version, build identity, OS/arch, thread safety, debug mode, native dependencies, source, digest. Incompatible combinations state why; no silent source compilation. |
| Project → Stack → **Modules** | The project's active profile: enabled extensions, artifact versions, CLI probe, HTTP probe, and which workers need a restart to apply them. Two projects may share a PHP runtime but enable different sets. |
| Updates | Switching PHP revalidates the whole profile before activation; a failed candidate keeps the previous profile. Old binaries are never copied across ABIs. |
| Databases / Components → Servers | Memcached remains a server (managed instance or existing endpoint). It is not a module and does not disappear from the server inventory because a client extension is enabled. |

Activation flow the UI will need to express once Stage 8A is implemented:

1. Choose or install a PHP runtime for the project.
2. Pick a compatible extension artifact; show compatibility before download.
3. Download, verify, and store the artifact in versioned app-owned storage.
4. Generate a candidate per-project ini fragment and probe it in a separate PHP process.
5. Apply the profile, restart only affected PHP workers, then verify both CLI and HTTP.
6. On failure, keep the previous profile and show the exact mismatch (ABI, dependency, or load error).

The engine will need module catalog/install/profile/probe commands; none of that is part of the UI-1…UI-6 slices below. Until Stage 8A artifacts are verified on the target machines, the UI shows no Modules navigation entry and no "install extension" affordance. Advertising unavailable PHP features would contradict `docs/DEPENDENCY_MANAGEMENT.md` and the known-limitations policy.

### 7.8 How an actual application plugs in (launch profiles)

#### Two process models

| | Apache + PHP (Stage 8A) | Node/Python application |
| --- | --- | --- |
| What Stackyard starts | One shared Apache; vhosts are added per site | One process per declared service, per project |
| How requests are handled | Apache (and PHP) handles each request | The app listens continuously on a loopback port |
| What the app must provide | Nothing beyond files and a site root | A start command, a working directory, and a port/readiness contract |
| How the UI describes it | Site: host, root or proxy target | Service: runtime, command, port, ready path, environment |

PHP is **not** wired in this build. There is no PHP runtime or FastCGI/CGI bridge yet; Apache currently serves static files and proxies Node/Python. The user's description — "start Apache and it interprets PHP on request" — is the Stage 8A target, not current behavior. The model still generalizes: PHP-FPM/CGI is also a supervised process behind Apache, just as a Node process is.

#### The manifest is already the integration contract

The engine does more than run an arbitrary command. For every service it:

- resolves the project's selected runtime and rewrites `node`, `python`, `npm`, and `pnpm` to the selected binaries (`crates/core/src/project.rs:417-477`);
- creates/uses a project virtual environment for Python and puts it on `PATH` (`project.rs:207-252`);
- puts the runtime directory and `node_modules/.bin` at the front of `PATH`;
- injects `HOST=127.0.0.1`, `PORT`, `FLASK_RUN_HOST`, `FLASK_RUN_PORT`, `STACKYARD_PROJECT`, and `STACKYARD_SERVICE`;
- runs in the declared working directory with logs captured, waits for readiness according to the service's `ready` mode (HTTP by default), and kills the process tree on stop;
- proxies `[site] service = "web"` to the assigned port through Apache. A service with `ready = "none"` has no port and cannot be a site target.

What it does **not** know is how a particular framework wants to be started, or whether that framework honours `PORT`/`HOST`. That is the missing layer.

Today's manifest is already sufficient for a hand-written app:

```toml
schema = 1
name = "my-app"
manager = "pnpm"

[[service]]
id = "web"
runtime = "node"
command = ["pnpm", "run", "dev"]
port = 0                 # 0 = assign a free loopback port
ready = "http-any"       # any HTTP answer proves the server is listening
ready_path = "/"
ready_timeout_secs = 60

[site]
service = "web"
host = "my-app.localhost"
```

For a Vite app the command must carry the assigned port, because Vite ignores `PORT`:

```toml
command = ["pnpm", "run", "dev", "--", "--host", "127.0.0.1", "--port", "{port}"]
```

#### Launch profiles

A launch profile is a small, reviewed adapter that turns "this looks like a Next.js app" into a proposed service. It is data plus detection, not arbitrary code execution.

| Profile | Detection evidence | Proposed command | Port handling | Ready path |
| --- | --- | --- | --- | --- |
| Next.js | `next` in `package.json` dependencies; `dev` script | `pnpm run dev` (or `npm run dev`) | respects `PORT` | `/` |
| Vite (React/Vue/Svelte) | `vite` dependency; `dev` script | `pnpm run dev -- --host 127.0.0.1 --port {port}` | CLI flags | `/` |
| Nuxt | `nuxt` dependency; `dev` script | `pnpm run dev -- --host 127.0.0.1 --port {port}` | CLI flags | `/` |
| Astro | `astro` dependency; `dev` script | `pnpm run dev -- --host 127.0.0.1 --port {port}` | CLI flags | `/` |
| Angular | `@angular/cli`; `start` script | `pnpm run start -- --host 127.0.0.1 --port {port}` | CLI flags | `/` |
| Express/Fastify/Koa | dependency; `start`/`dev` script or `server.js` | `node server.js` or `pnpm run start` | `PORT` if the app reads it; otherwise fixed | `/` |
| Flask | `flask` dependency; `app.py`/`wsgi.py` | `python -m flask --app app run --host 127.0.0.1 --port {port}` | CLI flags; `FLASK_RUN_*` also injected | `/` or a health route |
| FastAPI | `fastapi` + `uvicorn` deps; `app/main.py` | `python -m uvicorn app.main:app --host 127.0.0.1 --port {port}` | CLI flags | `/docs` or `/health` |
| Django | `manage.py` + `django` dependency | `python manage.py runserver 127.0.0.1:{port}` | positional argument | `/`; accept a redirect |
| Generic Node | `package.json` only | `pnpm run dev` / `start` | ask | `/` |
| Generic Python | `main.py`/`app.py` only | `python main.py` | ask | `/` |
| Static | `index.html`, no app server | no service; `[site] root = "public"` | Apache serves it | n/a |

**First slice (approved 3 November 2026):** Next.js, Vite, Express, Flask, and FastAPI. These five cover the user's actual work (Node apps including Express APIs, Vite front ends, Python APIs) and exercise both port strategies (`PORT`-aware and CLI-flag frameworks). Nuxt, Astro, Angular, and Django join once their detection tests exist; the generic Node/Python and Static rows are the fallback when detection fails, not profiles that guess.

Rules:

- **Evidence is shown before anything is saved.** "Detected Next.js from `package.json` (`next` 16.0.0) and the `dev` script." Confidence is explicit: high (dependency + script), medium (script only), low (file-name heuristic).
- **Nothing guessed is started silently.** The detection result is a suggestion the user reviews and can edit. The saved manifest contains the final command, not a hidden framework rule.
- **No detection side effects.** Detection parses `package.json`, `pyproject.toml`, `requirements.txt`, lockfiles, `manage.py`, and conventional entry files. It does not run npm/Python, install anything, or touch the network.
- **Script precedence** is `dev`, then `start`, then `serve`. If none exists, offer the framework binary via the selected manager (`pnpm exec vite`, `npm exec -- vite`) or a manual command.
- **Monorepos:** detect package roots (`apps/*`, `packages/*`, workspace metadata) and let the user add one service per package with its own `working_directory`.
- **Unknown apps:** do not invent a command. Show "We could not detect a start command" with a manual form and examples.

#### Detection rules and limits

Detection is a bounded file read, not a build. The contract:

1. **Scope.** Read only the project root plus conventional one-level folders (`app/`, `src/`, `server/`, `apps/*`, `packages/*`). Skip `.git`, `node_modules`, `.venv`, `venv`, `dist`, `build`, `.next`, and anything over 2 MB. Do not follow directory symlinks or junctions outside the root. Cap the number of entries scanned and report "scan truncated" if the cap is hit.
2. **Node evidence.** Parse `package.json`; read `dependencies`, `devDependencies`, and `scripts`. A framework hit without a matching script proposes the framework binary through the project manager (`pnpm exec next dev`, `npm exec -- vite`), with lower confidence. A parse error is shown as a warning and falls back to manual, not a failed import.
3. **Manager evidence.** `pnpm-lock.yaml` → `pnpm`; `package-lock.json` → `npm`. With **no lockfile**, propose `npm` (bundled with the managed Node) rather than the engine's implicit pnpm default, which fails when pnpm is not installed. `yarn.lock`, `bun.lockb`, and `packageManager` fields naming yarn/bun are reported as **unsupported managers** rather than silently mapped, because the manifest only knows npm, pnpm, pip, and uv. The UI offers "use npm anyway" with a warning that the lockfile will not be respected, or manual setup.
4. **Python evidence.** `pyproject.toml` (PEP 621 `[project]`, `[tool.poetry]`, `[tool.uv]`), `requirements.txt`, `manage.py`, `app.py`, `main.py`, `wsgi.py`, `asgi.py`. `uv.lock` or `[tool.uv]` → `uv`; otherwise `pip`. Poetry and conda are reported as unsupported managers for the same reason.
5. **Framework precedence.** When several dependencies match, the most specific app framework wins (Next over React, Nuxt over Vue, FastAPI over Starlette, Django over generic WSGI). A library-only dependency (for example `react` without Vite/Next) is **not** a startable app.
6. **Multiple services.** In a workspace, emit one candidate per package that has a start script or entry file, labelled by package name; the wizard makes the user pick which ones to add. It never adds services automatically.
7. **Determinism.** Same folder and file contents produce the same result, with no timestamps or absolute user paths in the evidence text. Evidence quotes the file and key: "`package.json` → `scripts.dev` = `vite`".
8. **Safety.** No process is spawned, no package manager runs, no network call is made, and nothing is written. `DetectProject` is safe to call on every wizard step and on import.

False positives are acceptable only when labelled: the "Detected application" card always names the evidence and the confidence, and the user can switch to Manual in one click. A wrong guess must cost one edit, never a broken start.

#### Dev server versus production start

Profiles propose the **dev** command, because that is what a local development manager is for (HMR, readable errors, fast iteration). Production starts are a different shape:

- `pnpm build && pnpm start` is two commands, and the current manifest runs **one** command per service. Do not hide a second command inside the engine.
- Acceptable options, in order of preference:
  1. A project script that does both (`"start": "npm run build && node dist/server.js"`). It is the user's own script, visible in `package.json`, and runs with the project environment. It is not a sandbox escape; project scripts are already trusted local code.
  2. Build as an explicit task in the Stack tab ("Run build"), then start the service with the production command. This needs a small task runner later; it is not in UI-2.
  3. A future `before_start = [...]` list in the manifest, only if the first two prove insufficient. Each entry would be an argv array, run in order, with the same timeouts and log handling.
- `NODE_ENV` and similar mode variables are **not** injected implicitly. Add them under `env` when the app needs them; the form shows a "Production mode" preset that sets `NODE_ENV=production` and leaves the command alone.
- For a static build, the production shape is `[site] root = "dist"` with **no service**: build, then publish the folder. The wizard's Static row already models this.
- The UI must say which mode the current service is in ("Development server" vs "Production"), because "it works with `pnpm dev` but not when published" is otherwise a mystery.

#### Port, host, and readiness contract

This is where most "the app won't start" failures come from.

| Issue | Rule |
| --- | --- |
| App ignores `PORT` | The profile passes `--port {port}` / positional port. If the app hardcodes a port, declare that fixed port instead of `0`. |
| App binds `localhost` (possibly `::1`) | Always pass `--host 127.0.0.1` when the framework supports it. Apache proxies to `127.0.0.1`; an IPv6-only listener is not reachable. |
| Readiness is too strict | Today readiness requires an HTTP **2xx** (`crates/engine/src/supervisor.rs:698-750`, `770-790`). Django may redirect (`302`) and FastAPI's `/` may be `404`. Readiness becomes an explicit mode; `http-any` is the default for web services (approved 3 November 2026). |
| Slow first start | Next/Vite can spend 30–60 s compiling. The profile sets `ready_timeout_secs` (60–120), and the failure message includes the last log lines. |
| App has no HTTP endpoint | Every service currently gets a port and an HTTP readiness check, so a worker/queue process cannot be modelled honestly. `ready = "none"` (approved) gives such a service no port and no HTTP check. |
| Port occupied | The engine reports `PortInUse` and leaves the occupant running. The UI offers another port or a fixed-port change; it never kills the other process. |

#### What "readiness" means, in plain terms

Starting a process is not the same as the app being usable. A Node server may take ten seconds to compile before it listens; a crashed process may also leave the terminal quiet for ten seconds. So after launching, the engine asks the app a simple question over and over: **"are you answering on the port I gave you?"** It stops asking when the app answers, when the process exits, or when the timeout runs out.

The only argument is what counts as an answer:

| Mode | What the engine accepts | Use it for | Example |
| --- | --- | --- | --- |
| `http-2xx` | An HTTP response with status 200–299. | Strict health endpoints. | `/health` returning `200 ok`. |
| `http-any` **(default for web)** | **Any** valid HTTP response — 200, 302, 404, even 500. | Normal dev servers. | Django `/` redirects to `/login` (302); FastAPI `/` has no route (404); the app is still alive and serving. |
| `tcp` | A TCP connection is accepted; no HTTP is spoken. | Databases, mail catchers, raw sockets. | PostgreSQL on 5432. |
| `none` | Nothing is probed. The process is "started" while it stays alive. | Workers, queues, cron-like loops. | `python worker.py` with no listening port. |

Two distinctions matter:

- **"Answering" is not "healthy".** Under `http-any`, a 404 or 500 proves the server is listening and routing; it does not prove the app is correct. The UI shows the status code and the last log lines, and labels it **"running with errors"** rather than pretending the start failed. That is deliberate: if Stackyard refused to start because a route was missing, every half-built app would look broken.
- **"No answer at all" is still a failure.** If the port never responds before the timeout and the process is alive, the message says "the app started but never answered on port N" and shows the last log lines. If the process exited, the message says so with its exit code. `none` removes only the probe, not the crash detection.

So the recommendation the user accepted is: **`http-any` for web services, `none` for workers, and the stricter modes available when a project wants them.** The app is never given a fake HTTP check for a process that has no HTTP server, and a redirect is never reported as a crash.

Readiness must still prove the intended process is serving, not merely that some socket accepts. Keeping the port bound to the selected service and showing the last log lines on failure is enough for now; `tcp` is a fallback for non-HTTP services, not a default.

#### Database and environment wiring

Manifest `env` is static and shareable. A project's database credentials are machine-local, so they must not go into `stackyard.toml`.

Proposed local environment layer:

- Each connection gets an optional environment-variable name, defaulting to `DATABASE_URL`.
- The engine resolves the secret at launch and injects it into the selected services only (for example `web` and `api`, not a build worker).
- The UI shows the key and target services, never the value; value lives in the secret file / OS store.
- Example injected value: `postgres://my_app:••••@127.0.0.1:5437/my_app`; SQLite: `sqlite:///D:/work/my-app/data/app.sqlite`.
- For frameworks that require a file (`dotenv`, Laravel), offer an explicit "Write `.env.local`" action with a preview; do not silently create or overwrite it.
- PHP/Apache later: the same mapping becomes a vhost/FastCGI environment entry; it is not a Node-specific feature.

This is what closes the loop between "I created a database in Stackyard" and "my app can connect without hand-copying a password".

Machine-local storage (no schema change to the manifest, one store migration):

```
project_db_env(project_id, connection_id, variable, services, created_at)
  primary key (project_id, connection_id, variable)
```

`services` is a JSON list of service ids, or `["*"]`. The password stays in the existing secret file; this table stores only the mapping. Migration 7 follows the current migration 6 in `crates/core/src/store.rs`.

#### A worked example: Express end to end

Folder `D:\work\shop-api`, created or imported as an existing folder:

```
package.json
server.js
```

```json
{
  "name": "shop-api",
  "private": true,
  "scripts": { "start": "node server.js", "dev": "node --watch server.js" },
  "dependencies": { "express": "^5.1.0" }
}
```

```js
const express = require("express");
const app = express();
const port = Number(process.env.PORT ?? 3000);   // honours the injected PORT
app.get("/api/health", (req, res) => res.json({ ok: true }));
app.listen(port, "127.0.0.1", () => console.log(`listening on ${port}`));
```

Detection reports: Express (dependency), `start` script, no lockfile yet → manager `npm`. It proposes `node server.js` rather than `npm start` because the entry file exists and skips npm's process layer; both are offered. `npm` is the safe default without a lockfile because it ships with the managed Node; `pnpm` is proposed only when `pnpm-lock.yaml` exists or the pnpm tool is selected, since the engine refuses a `pnpm` command when pnpm is not installed.

The wizard writes:

```toml
schema = 1
name = "shop-api"
manager = "npm"

[[service]]
id = "web"
runtime = "node"
command = ["node", "server.js"]
port = 0
ready = "http-any"
ready_path = "/api/health"
ready_timeout_secs = 30

[site]
service = "web"
host = "shop-api.localhost"
```

`ResolveService` (Test configuration) shows, before anything runs:

```
program     C:\Users\…\stackyard\packages\node\24.22.2\node.exe
args        server.js
cwd         D:\work\shop-api
port        auto (assigned at start)
env         PATH+= <node dir>;D:\work\shop-api\node_modules\.bin
            HOST=127.0.0.1  PORT=<assigned>  DATABASE_URL=postgres://shop_api:••••@127.0.0.1:5437/shop_api
readiness   any HTTP response at /api/health within 30 s
```

At start the engine assigns, say, 53124, injects `PORT=53124`, and waits. The app logs `listening on 53124`, `GET /api/health` returns 200, the service becomes `running`, and Apache proxies `http://shop-api.localhost/` to `127.0.0.1:53124`. The UI shows the URL, the assigned port, and "ready in 1.2 s".

Failure cases and what the UI says:

| What happened | Message | Fix offered |
| --- | --- | --- |
| `server.js` listens on a hardcoded 3000 | "The app started but never answered on port 53124. It may ignore `PORT`." | Set `port = 3000` (fixed) or change the app to read `process.env.PORT`. |
| `node_modules` missing | Process exits with `MODULE_NOT_FOUND`; last 20 log lines shown. | "Install dependencies" on the Stack tab. |
| App binds `localhost` → `::1` only | No answer from `127.0.0.1`. | Add `"127.0.0.1"` as the listen host, or set the profile's host flag. |
| `/api/health` returns 500 | `http-any` marks it running with errors; status and log tail shown. | Open Logs; the start is not rolled back. |

#### The same app in FastAPI

`app/main.py` with `FastAPI()` and a `/health` route, `requirements.txt` containing `fastapi` and `uvicorn`:

```toml
[[service]]
id = "api"
runtime = "python"
command = ["python", "-m", "uvicorn", "app.main:app", "--host", "127.0.0.1", "--port", "{port}"]
port = 0
ready = "http-any"
ready_path = "/health"
ready_timeout_secs = 60
```

The engine rewrites `python` to the project virtualenv interpreter, creates that venv on first start if needed, and injects `FLASK_RUN_*` as well as `PORT`/`HOST` (today those are injected for every service, not only Flask commands; harmless here). `uvicorn` prints `Uvicorn running on http://127.0.0.1:53125`, `/health` returns 200, done. If the user's manager is `uv`, `uv` must also be rewritten to the managed binary — that is the `rewrite_command` gap listed below.

#### UI flow

1. **Import/create** → `DetectProject` → "Detected application" card in the wizard with the proposed service(s), evidence, and an Edit action.
2. **Services tab → Add service** → choose Detected or Manual; edit command, working directory, port (Auto / fixed), ready path, timeout, environment; **Test configuration** resolves the executable/args/cwd/env keys without launching; then Save writes the manifest through the engine.
3. **Overview** shows "Launch: Next.js · `pnpm run dev` · auto port · ready `/`" and the readiness checklist.
4. **Start failure** shows the last log lines plus the likely cause: wrong port flag, bound to `::1`, readiness path returns 404/302, missing dependencies, or a command that does not exist. Each cause links to the field that fixes it.
5. **Existing manifest:** the same detection runs when a folder is imported without one; if a manifest already exists, detection offers a diff rather than replacing it.

#### Engine work this implies

| Need | Command / change |
| --- | --- |
| Read-only detection | `DetectProject { path }` → candidate services, evidence, confidence, manager, lockfile, suggested command/port/ready path. No writes, no execution, no network. |
| Preview a launch | `ResolveService { project_id, service }` → resolved program, args, working directory, environment keys (values redacted), port. Reuses `service_launch`. |
| Save edits | `UpdateProjectManifest { project_id, manifest }` with a diff preview. |
| Readiness modes | Manifest `ready = "http-2xx" \| "http-any" \| "tcp" \| "none"`; schedule port/readiness accordingly. Required for workers and redirecting frameworks. |
| Tool rewriting | Add `uv` (and consider `pip`) to `rewrite_command`; today a service command starting with `uv` is treated as a project-relative file and fails. |
| Local environment | Store connection→variable mappings in the local store, resolve secrets at launch, inject per service, redact diagnostics. |
| Port check | Optional `ProbePort { port }` for inline validation before saving; start still owns the authoritative check. |

Adapters belong in the core crate with unit tests for detection and command resolution. Do not introduce a plugin system for this; a fixed, reviewed table is enough for the personal release.

Manifest compatibility and validation rules for `ready`:

- `RawManifest` and `RawService` use `#[serde(deny_unknown_fields)]`, so adding `ready` means an **older build cannot read a newer manifest** ("unknown field `ready`"). That is acceptable for a single-user local app, but the error must say so and the UI must offer "Open in editor" rather than a generic parse failure.
- Omitted `ready` means `http-any` (the friendlier default approved above). `ready = "none"` requires `port = 0` and ignores `ready_path`; `http-2xx`, `http-any`, and `tcp` require `ready_path` and a real port. Invalid combinations are rejected with the field name, not a TOML line number alone.
- `ready = "none"` also changes scheduling: no port is reserved, the Apache site cannot proxy to that service (validation rejects `[site] service` naming a `none` service), and the supervisor reports `running` when the process stays alive past a short settle time (for example 1 s) instead of waiting for HTTP.
- Lua/`path`-less readiness is deliberately out of scope; no user-supplied scripts run as health checks.

## 8. Interaction patterns and components

Build a small component set instead of repeating markup in `App.tsx`:

| Component | Responsibility |
| --- | --- |
| `AppShell` | Sidebar, project list, contextual header, main slot, activity tray. |
| `StatusPill` | One mapping from engine state to label/tone; no ad hoc ternaries. |
| `ResourceRow` | Name, badges, metadata, one primary action, overflow menu; used for projects, services, components, instances, connections. |
| `OperationButton` | Wraps an async action: idle → running (spinner + Cancel) → failed (Retry) → done; prevents double submit. |
| `OperationTray` | Non-blocking list of active operations, cancellable, survives navigation. |
| `InlineError` | Field- or row-level message with "Details" and "Retry". |
| `EmptyState` | One sentence, one primary action, optional secondary link. No dashed box with three paragraphs. |
| `ConfirmDialog` | Title states the action, body states the consequence, destructive button names the action. |
| `Wizard` | Step header, validation per step, review, back/next, keyboard focus. |
| `CopyField` | Read-only value + Copy, with a transient "Copied" confirmation. |
| `PathField` | Native picker, drag-drop, recent paths, validation. |
| `PortField` | Auto/manual, 1024–65535, live availability hint when cheap, inline error from `PortInUse`. |
| `LogView` | Incremental read, follow/pause, copy, open folder, empty state. |
| `ReadinessList` | Check status + fix action per item. |
| `UpdateBanner` | Four notice kinds, one action each. |

Rules of thumb:

- One primary button per view region. Everything else is secondary or in overflow.
- A disabled control must be accompanied by a reason in the same view (not only a tooltip).
- No global `progress` string. Operations are objects with identity.
- Every destructive action uses `ConfirmDialog` with a consequence sentence; the confirm label repeats the verb ("Detach connection", not "OK").

## 9. State and data layer

The current approach (one `refresh` at mount, ad hoc slice updates, identity-keyed polling) should be replaced with a small, explicit data layer. A third-party cache library is optional; the important part is the contract:

1. **Resources**: `status`, `projects`, `project(id)`, `packages`, `catalog`, `front`, `databases`, `updates`. Each has `data`, `loading`, `error`, `refresh()`, and `invalidate()`.
2. **Mutations** declare which resources they invalidate. `startProject` → `project(id)`, `projects`; `createDatabase` → `databases`; `install` completion → `packages`, `catalog` (availability), `project(id)` if a selection changed.
3. **Polling** is owned by a single scheduler: 1 s while a visible project has an active lifecycle state or an operation is running; 5 s otherwise; paused when the window is hidden (tray). No effect depends on a freshly parsed object.
4. **Logs** use offsets and append. Polling stops when the service is stopped; follow mode only when the user is at the bottom.
5. **Operations**: installs and dependency tasks are tracked by id in one store, independent of the screen that started them. The tray shows name, percent (if known), elapsed time, Cancel. Completion triggers invalidation and a toast with the result.
6. **Errors** are typed by source: engine error (map common variants), validation error (inline), unexpected (banner with Details). Keep `errorText` for the raw layer but stop using it directly in screens.
7. **Loading**: first paint uses skeletons or a quiet "Connecting to the engine…" state; never show "No projects" before the first response.
8. **Engine unavailable**: one panel with Retry and the engine/data-directory path. Everything else disabled with an explanation.
9. **Optimistic updates** only for selection highlight; never for process state.

Suggested file layout (for the implementation slice, not this paper):

```
src/
  app/            shell, navigation, route state
  api/            typed client, resource hooks, error mapping
  features/
    projects/     list, create wizard, workspace, tabs
    components/   library, catalog, existing, updates
    databases/    fleet, instance, project panel
    settings/     general, web, data, about
    logs/
    operations/   tray, hooks, store
  ui/             shared primitives from §8
  styles/         tokens, base, layout
```

`App.tsx` should not remain a 1,600-line file. The Rust view types are currently hand-mirrored in `api.ts`; either generate them (`ts-rs`) or keep one `bindings.ts` and a contract test to prevent drift.

## 10. Engine and Tauri gaps to close

### 10.1 Existing engine operations that are not wired

| Capability | Engine command | Tauri command | UI use |
| --- | --- | --- | --- |
| Create database + user for a project | `DbAdd` | missing | Project → Databases "Create database" |
| Probe a connection | `DbProbe` | missing | "Test" on a connection |
| Backup / restore a database | `DbBackup`, `DbRestore` | missing | Instance detail, project Databases |
| Backup / restore SQLite | `SqliteBackup`, `SqliteRestore` | missing | Project Databases |
| Remove a package | `RemovePackage` | missing | Components row action |
| Discover installations | `Discover` | exposed, unused | Components "Find installations" |
| HTTP/HTTPS ports | `SetFrontPorts` | missing | Settings → Web |
| Trust install/remove | `TrustInstall`, `TrustRemove` | missing | Settings → Web |
| Hosts write | `WriteHosts` | missing | Settings → Web |
| Dependency recovery | `DepRecover` | missing | Stack → Dependencies |
| Run command in project environment | `Exec` | missing | "Run command" panel; later a real project terminal |

Wiring these is mostly serialization plus UI. No new domain behavior for the first three database actions.

### 10.2 New commands the UI needs

| Need | Proposed command | Contract notes |
| --- | --- | --- |
| Create a project | `CreateProject { path, manifest, create_dir, replace }` | Validate by serializing and re-parsing through `parse_manifest`; refuse non-empty target unless explicit; write `stackyard.toml` atomically; then import. Engine owns file writing. |
| Validate before writing | `ValidateProject { path }` | Returns parsed preview, warnings, and which files would be written. |
| Forget a project | `ForgetProject { project_id }` | Removes the store record only; requires stopped state; never deletes files. |
| Edit the stack later | `UpdateProjectManifest { project_id, manifest }` | Diff preview and explicit replace; optional for the first slice. |
| Detect an application | `DetectProject { path }` | Read-only parse of `package.json`, `pyproject.toml`, `requirements.txt`, lockfiles, `manage.py`, and entry files. Returns candidate services with evidence, confidence, and any **unsupported manager** (`yarn.lock`, `bun.lockb`, Poetry, conda) as a warning rather than a guess. No writes, execution, or network; bounded scan per §7.8. |
| Preview a launch | `ResolveService { project_id, service }` | Returns resolved program, args, working directory, environment keys (values redacted), and assigned or fixed port. Reuses `service_launch`. |
| Check a port | `ProbePort { port }` | Optional inline validation; `start` remains authoritative. |
| Project database environment | Local store mapping, injected at launch | Connection→variable mapping such as `DATABASE_URL`; machine-local, never in the shareable manifest; resolved secrets are not logged. |
| Database credentials | `DbCredentials { project_id, connection_id, reveal }` | Returns host/port/db/user and, on explicit reveal, the password. Never logged, never in diagnostics. Alternative: let the user set the password at creation and show it once. |
| List databases in an instance | `DbListDatabases { instance_id }` | Needed for instance detail and drop. |
| Drop a database / instance | `DbDropDatabase`, `DbRemoveInstance` | Typed confirmation; data deletion is permanent; currently absent. Consider deferring. |
| Rename/reset user | `DbResetUser` | Optional; useful when a password is lost. |
| Log location | `LogPath { project_id, service_id }` or derive from `status.data_dir` | Enables "open log folder" without exposing arbitrary paths. |
| Engine restart/retry | `EnsureEngine` or reuse `Status` with a UI retry | The Tauri layer already calls `ensure_engine`; expose a retry action. |

### 10.3 New Tauri plugins

- **Dialog** (`@tauri-apps/plugin-dialog`) for folder/file save/open. This is the main missing desktop affordance; add it deliberately and scope permissions in `capabilities`.
- **Clipboard** or the webview clipboard with permission. Copy buttons are essential for paths, URLs, and connection strings.
- Optional: **opener** instead of the custom `explorer`/`open` logic, but the current code works and validates URLs; do not churn without need.
- Optional: **single-instance** so clicking the app icon focuses the tray window; not required for this rework.

Each added plugin needs a short decision note and a capability entry. Keep the rule that project commands never run elevated and the UI never builds shell strings.

### 10.4 Engine contract improvements (no behavior change)

- Extend `ServiceView` with `ready_mode`, `ready_state` (`ready` / `never-answered` / `not-probed`), `ready_ms`, and `last_http_status` so the Services tab can show process, readiness, and health separately instead of inventing them from `detail`. Keep `detail` for the log tail on failure.
- Add a `ProjectView` composition endpoint or fields for: manifest path, site summary, bound connections, readiness hints. This avoids three round-trips and prevents the UI from inventing state. If that is too much, compose in the UI from `databases` and `front`; prefer the former for consistency.
- Return `state` plus `desired_state` for services if the supervisor distinguishes them; the UI currently maps observed state only.
- Expose whether a dependency install has run for the current manifest fingerprint (the engine already tracks fingerprints) so the readiness check is honest.
- Keep `databases.message`, `front.restartImpact`, and `project.managerDetail` as user-facing strings only if they are copy-ready; otherwise return structured data and let the UI write the sentence. Today some messages are developer-facing.

## 11. Copy and error handling

Replace descriptions of internals with sentences about the user's situation. Examples:

| Today | Proposed |
| --- | --- |
| "No managed MySQL or PostgreSQL instance." | "No database server yet. Create one to give projects their own MySQL or PostgreSQL on this machine." |
| "Create downloads MySQL 8.4 into Stackyard, then starts a private instance. It does not register a Windows service." | "Stackyard will download MySQL 8.4.13 into its own folder, verify it, and start a private server on 127.0.0.1. It will not install a Windows service or change your existing MySQL." |
| "A component row says `managed`." | "Installed by Stackyard" / "Your existing installation" / "Your own server (Stackyard will not start, stop, or update it)." |
| "An external database host must be loopback in this build." | "Only local databases are supported in this build. Host must be 127.0.0.1 or localhost." |
| "Recorded" | "Installed" or "Added" as appropriate. |
| "Publish" button with no explanation. | "Publish a local site. Apache will proxy this project at my-app.localhost. Restarting Apache interrupts every published site." |

Error mapping (engine → UI):

| Engine error | UI headline | Action |
| --- | --- | --- |
| `PortInUse { port }` | "Port 3000 is already in use." | "Choose another port" + "Stackyard left the other program running." |
| `FrontPortBusy { port, alternatives }` | "Apache cannot use port 8080." | Show alternatives, let the user pick, apply via `SetFrontPorts`. |
| `DigestMismatch` | "The download did not match the published checksum." | "Nothing was installed." `[Retry] [Details]` |
| `NotRemovable { id, reason }` | "Node 24.22.2 is still used by my-app." | "Switch that project's runtime first." `[Open my-app]` |
| `ProjectBusy` | "my-app is still starting." | `[Cancel start]` |
| `DatabaseMigrationUnsupported` | "Switching MySQL 8.0 to 8.4 needs a migration." | "Back up and restore into a new instance. Stackyard will not open existing data with a new major version." |
| `DataDirNotEmpty` | "That folder already has database files." | "Choose an empty folder. Stackyard will not initialize over existing data." |
| `InvalidManifest` | "stackyard.toml could not be loaded." | Show the parsed detail, `[Open manifest] [Reload]`. |
| `Timeout` / unreachable engine | "The engine did not respond." | `[Retry]` + data directory path. |

Rules:

- Show the error where the action happened. A page-level banner is only for cross-cutting failures.
- Always include a next action. If there is none, say what state was left behind.
- Keep the raw error under "Details" for bug reports.
- Never concatenate secrets into an error or a progress message.

## 12. Accessibility, layout, and visual details

- Sidebar: 228 px is fine at ≥1200 px. At narrower widths, collapse to a 64 px rail with project initials/status; keep navigation available. Do not use a full-screen overlay.
- Minimum window: set and test a minimum size (1024×700, per §17) instead of relying on CSS breakpoints to rescue a cramped window. The layout scrolls rather than clipping.
- Keyboard: `Ctrl+K` command palette (projects, actions, settings), `Ctrl+1..6` for project tabs, `Esc` closes drawer/dialog, `Ctrl+Enter` toggles the project's primary lifecycle action. Focus returns to the invoking control after a dialog.
- Every list row reachable by Tab; overflow menus open with Enter/Space and arrow keys; visible focus ring preserved.
- Use `<button>` for actions and `<a>`/link-like buttons only for navigation. Do not style `<li>` as an interactive row unless the whole row is a button.
- `aria-live="polite"` for operation completion, `role="alert"` for blocking errors. Do not focus the error banner from a distant form; focus the field or keep focus stable.
- Status is not conveyed by color alone: pill text plus a dot; failed rows include a short reason.
- Respect `prefers-reduced-motion` (already partly done) and avoid the indeterminate meter as the only progress signal.
- Bundle IBM Plex Sans/Mono with the app. Remove the Google Fonts requests. Keep the two-font system; it suits the product.
- Add a dark theme through the existing CSS variable set (`--ground`, `--surface`, etc.); the tokens are already centralized.
- Density: the current generous spacing is pleasant but wastes vertical space for a tool window. Reduce section gaps, remove permanent hint paragraphs, and use inline help only where a decision needs it.

## 13. Performance and offline behavior

- The engine status call is fast (documented 0.106 s), so a short skeleton is enough; do not add a splash screen.
- Avoid polling everything every second. Poll only what the visible view needs, and only while something is active.
- Cap log buffers (for example 5,000 lines or 1 MB), append incrementally, and virtualize only if real logs exceed that. The engine already caps chunks at 24 KB and rotates at 1 MB.
- Install progress is per operation; no full-page busy state. The window stays interactive.
- No remote fonts, no remote images, no analytics. The UI should render fully offline with cached packages.
- Keep list rendering simple; project and component lists are small. Do not add a virtualization dependency yet.
- Measure startup with an editor and browser open on the acceptance machines, as required by Stage 8; report numbers rather than targets.

## 14. Safety and scope boundaries

- **Project files are user-owned.** Creating a manifest is allowed; overwriting one requires an explicit choice and a diff. Starter files are a separate decision.
- **External installations are not adopted implicitly.** "Use existing" records a path and version; it never copies, upgrades, or removes that installation. The UI must say so on the row, not in a paragraph at the bottom.
- **Database data is preserved.** No instance removal or drop in the first slice unless a command and typed confirmation exist. Restore always targets a new database name.
- **Secrets** are never displayed by default, never logged, and never included in diagnostics. A deliberate "Copy password" action is acceptable if the engine provides it; otherwise the UI must say where the secret lives.
- **No invented catalog entries.** Platform gaps appear as honest "not available yet" states with the use-existing alternative.
- **No arbitrary shell.** Dependency and project commands remain fixed verbs mapped to engine argv arrays. A future "Run command" panel uses `Exec` with explicit args, not a shell string.
- **Loopback only.** Keep the current host validation; surface it as inline validation, not a late engine error.
- **No global changes.** Trust, hosts, and ports stay explicit Settings actions with previews and reversibility.

Non-goals for this rework: a database IDE, an editor, recipe marketplaces, containers, tunnels, and PHP/extension management. The confirmed "Modules" concept (native extension profiles) is Stage 8A and is designed but not implemented here; see §7.7.

## 15. Delivery plan

Each slice is reviewable on its own and keeps `cargo test --workspace`, clippy, and `pnpm --dir apps/desktop run build` green. UI acceptance is manual on Windows 10 first, then macOS 14.5 when the desktop runs there.

### Rollout without breaking the working app

- UI-1 changes presentation, not commands or JSON shapes. The new shell can land beside the current screen behind a development switch (`?ui=v2` or a build constant); the default flips only after the parity checklist passes, and the switch is deleted in UI-2. This is a temporary migration aid, not a plugin system.
- Parity checklist before flipping: import, select, start, stop, restart, log tail, component install/select, database list/create/attach, site publish/open. A flow that is not ported yet keeps its old panel; nothing disappears to satisfy the new layout.
- There is no test harness in `apps/desktop` today — `package.json` has no `test` script and no test files. UI-1 adds Vitest + Testing Library with a fake `invoke` boundary. Those tests cover state transitions, error mapping, and copy; they never pretend to exercise the engine. Process and database behavior still needs a disposable-fixture run on Windows.
- If a slice has to choose between a polished screen and a working flow, the flow wins; polish is UI-6.

### UI-1 — Shell, state, and project workspace (no engine change)

- Split `App.tsx` into features; add resource hooks, operation tray, error mapping, loading states, and the project list in the sidebar.
- Replace the long project detail with tabs (Overview, Services, Stack, Databases, Site, Logs, Settings).
- Fix logs to use offsets; remove duplicate polling; make lifecycle actions valid per state.
- Add project databases and site summary by composing existing `databases`/`front` data.
- Acceptance: import an existing fixture, select it, start, watch logs grow, restart, stop; no global blocking; keyboard traversal; 1024×700 usable; no double submit; errors near the action.

### UI-2 — Create, import, and application integration

- Add the dialog plugin and a native folder picker; drag-and-drop.
- Add `CreateProject`, `ValidateProject`, `ForgetProject`, `DetectProject`, `ResolveService`, and `UpdateProjectManifest` to the engine and Tauri layers.
- Implement the five-step wizard, manifest preview, validation, and review.
- Add the "Detected application" card, the Add/Edit service form, Test configuration (resolve only), and the launch profiles from §7.8: Next.js, Vite, Express, Flask, and FastAPI in the first slice.
- Fix `rewrite_command` so a managed `uv` command is resolved like `pnpm`.
- Add readiness modes (`http-2xx`, `http-any`, `tcp`, `none`) so redirecting frameworks and non-HTTP workers are honest; default web services to `http-any`.
- Acceptance: create a project in an empty folder; import a Next.js repo and accept the detected `pnpm run dev` profile; import a Vite repo and verify the port flags; import an Express repo and verify `PORT` handling; import Flask/FastAPI and verify readiness; refuse overwrite; refuse invalid names; forget a project and verify files remain; a worker starts with `ready = "none"` and no port.

### UI-3 — Components library

- Regroup by runtime/tool/server; add used-by, ownership badges, search/filter, catalog tab, remove, discover, and existing-install validation.
- Move updates and cleanup out of the page.
- Acceptance: install a second Node version, see both, switch one project, verify the other is unchanged; remove an unused version; attempt to remove a used version and get an actionable refusal; record an existing Python path and see version/target.

### UI-4 — Databases

- Expose `DbAdd`, `DbProbe`, `DbBackup`, `DbRestore`, `SqliteBackup`, `SqliteRestore`, and credentials.
- Build the fleet view, instance detail, and project Databases tab. Separate create/attach state per form.
- Add the local connection→environment mapping (`DATABASE_URL` by default), inject it at launch for selected services, and offer the explicit `.env` write for frameworks that need a file.
- Acceptance: create an instance, create a project database, copy `.env`, probe, stop/start the instance, back up, restore into a new name, verify data; attach an existing local server; detach and confirm the server keeps running; SQLite backup/restore; start an app that reads the injected `DATABASE_URL` without a hand-copied password.

### UI-5 — Web and settings

- Expose `SetFrontPorts`, trust, hosts, and diagnostics save; build Settings → Web/Data/About.
- Acceptance: change ports with an occupied-port path, publish/open/unpublish a site, install/remove trust in a named directory, write only marked hosts lines, copy/save diagnostics with secrets redacted.

### UI-6 — Polish

- Command palette, dark theme, local fonts, empty-state copy pass, confirmations, responsive rail, UI smoke tests against the Vite page with a typed invoke stub (test-only; never presented as a running engine).
- Acceptance: full keyboard pass, screen-reader labels, reduced motion, offline launch, 1024–1440 px layouts.

Dependencies and documentation:

- UI-1 can start immediately.
- UI-2 and UI-4 need small engine/protocol additions; write a decision note (proposed `docs/decisions/0008-desktop-information-architecture.md`) and update Stage 4/6 deliverables in `docs/DEVELOPMENT_PLAN.md`.
- The coordinator updates `docs/IMPLEMENTATION_STATUS.md` and `docs/WORK_QUEUE.md`; workers should not edit them in a shared checkout.
- Keep the desktop a client of the engine. Do not move process or database logic into React.
- **Modules are not a slice of this rework.** Stage 8A gets its own milestone after PHP/Composer and extension artifacts are verified on both targets. UI-1…UI-6 must leave the Stack tab honest about their absence and must not invent module state.

### Risks and mitigations

| Risk | Why it matters | Mitigation |
| --- | --- | --- |
| Detection mislabels a framework | A wrong command wastes a debugging session and erodes trust. | Evidence + confidence on the card; one-click Manual; no silent start; profiles covered by unit tests. |
| `http-any` hides a broken app | A 500 could be read as "started fine". | Show the status code and log tail; label "running with errors"; `http-2xx` remains selectable per service. |
| The rewrite loses a working flow | Users cannot import or start while the UI is half-migrated. | Parity checklist and temporary old/new switch; delete the switch only after parity passes. |
| Manifest schema change breaks older builds | `deny_unknown_fields` makes `ready` unreadable to a downgraded build. | Explicit "unknown field `ready`" message; regenerate or remove the field; document in release notes. |
| Secret leaks through the new env injection | A logged `DATABASE_URL` exposes the password. | Only keys shown in the UI; redaction tests; never print resolved env; secret stays in the secret file. |
| Apache proxying breaks HMR websockets | Vite/Next dev overlays stop working through the site host. | The vhost already uses `upgrade=websocket`; add an HMR acceptance check per framework profile. |
| Windows paths and junctions | Canonicalization, case-insensitivity, and junctions can escape the project root. | Canonicalize, reject escapes, test with a junction fixture; keep platform logic in Rust. |
| Fixed-port app collides with another project | Start fails late with a confusing message. | Surface `PortInUse` with the owner if known; offer auto/fixed switch in the form. |
| Mac behavior drifts | The app targets macOS 14.5 but is developed on Windows. | Keep process/path logic in Rust providers; leave Mac checks explicitly pending until run. |
| Dev profiles are mistaken for production | "Works in `pnpm dev`" does not mean the app is deployable. | Label services Development/Production; document the one-command limit; static builds publish from `dist/`. |

## 16. Acceptance scenarios for the reworked UI

1. **Greenfield Node + PostgreSQL:** create `my-app` in an empty folder, install Node 24.22.2, create a managed PostgreSQL 18.6 instance, create database/user, copy `.env`, install dependencies, start, open site, read logs, stop. Every step happens without leaving the project workspace except the initial creation.
2. **Existing Flask + SQLite:** import a folder with a manifest, record Python 3.12.3 as existing, select it, use a project-relative SQLite file, install pip dependencies, start, verify the API responds.
3. **Two runtimes at once:** project A uses Node 22, project B installs Node 24; switching B does not change A; removing 22 is refused while A uses it and the refusal links to A.
4. **Failure paths:** occupy the requested port (clear message, other process untouched); cancel a runtime download (nothing installed, retry works); use an invalid manifest (field-level reason, open/reload actions); point at a non-empty DB directory (refused); stop an instance with two connections (warning, explicit choice).
5. **Destructive actions:** unpublish, detach, cleanup, and restore each show the consequence and require the verb on the confirm button; none deletes project files or database data implicitly.
6. **Desktop behavior:** close the window (tray keeps running per setting), reopen, keyboard-only create/start/stop, copy path/URL/connection string, open log folder, engine-down retry.
7. **Offline:** launch with no network, open an installed project, start it from cache, see a clear notice when a requested component is not downloaded.
8. **Detected Next.js:** import a Next.js repository with a `pnpm-lock.yaml`; the wizard proposes `pnpm run dev`, auto port, ready `/`, timeout 60; start, proxy through Apache, edit a file, see HMR through the proxy.
9. **Vite ignores `PORT`:** the profile passes `--host 127.0.0.1 --port {port}`; remove the flags and the start failure explains why the assigned port was never reached.
10. **FastAPI/Django readiness:** `/docs` returning 200 and `/` returning 302/404 are both treated as "serving" under `http-any`; the app is not reported as crashed for a redirect. A 500 is shown as "running with errors".
11. **Database injection:** create a connection, map it to `DATABASE_URL` for `web` and `api`, start, and verify the app connects without a hand-copied password; the value is absent from logs and diagnostics.
12. **Express with `PORT`:** import an Express app whose `server.js` reads `process.env.PORT`; the profile proposes `node server.js`, auto port, `http-any`; start and confirm the app listens on the assigned loopback port. An Express app that hardcodes its port is offered a fixed-port field instead.
13. **Honest worker state:** a worker process starts with `ready = "none"`, receives no port, and is reported as running while alive; it is never given a fake HTTP readiness check.
14. **Production shape:** a Node app with a `start` script that builds and serves is started in production mode with `NODE_ENV=production` and no dev flags; a static Vite build is published from `dist/` with no service. The UI labels each service "Development" or "Production" so the two are not confused.

## 17. Open questions, with recommendations

These do not block UI-1. Each has a proposed default so work can proceed if the answer is "your call".

1. **Manifest editing.** Recommendation: for UI-2, a **read-only viewer** of the generated `stackyard.toml`, plus "Open in editor" and "Reload after editing". A structured editor is deferred; it duplicates TOML semantics and is the largest UI-2 risk. The service form (§7.3) already covers the fields people change most.
2. **Database credentials.** Recommendation: generate the password at creation and **show it once** in a copy field, then keep it in the secret file. A "Reveal" action behind an explicit click (no logging, cleared on view change) is acceptable; auto-filling it into logs, diagnostics, or exported manifests is not.
3. **Instance and database removal.** Recommendation: first slice supports **detach** only. "Delete database" needs typed confirmation and stays out until the database stage is properly tested; deleting an instance is refused while any connection is attached and requires typing the instance name. Data is never removed implicitly.
4. **Theme and window size.** Recommendation: keep the current light visual language for the rework, add dark mode later as tokens, and support a minimum window of **1024×700** (the layout must scroll, not clip). Record actual measurements on the two target machines during acceptance.

Resolved on 3 November 2026:

- "Modules" means native extension profiles (Stage 8A), not the project stack.
- Project creation uses **A**: a folder and `stackyard.toml` as the home for the actual application, with detected launch profiles prefilling the manifest. Starter source files (**B**) and framework generators (**C**) are later.
- Apache handles PHP only after Stage 8A; Node/Python apps are integrated as supervised services with a command, port, readiness, and optional proxy.
- **First slice profiles:** Next.js, Vite, Express, Flask, FastAPI. Django, Nuxt, Astro, and Angular follow after that.
- **Readiness:** `http-any` is the default for web services, `ready = "none"` covers workers, and `http-2xx` / `tcp` remain available. A non-2xx HTTP answer is reported as "running with errors", not as a failed start; a port that never answers is still a failure. The plain-language explanation is in §7.8.
- **Database environment:** connections map to environment variables (`DATABASE_URL` by default) and are injected into selected services at launch; an explicit, previewed `.env` write is offered for frameworks that need a file. Credentials never go into `stackyard.toml` or logs.

## Appendix A — Defect and friction register

| # | Symptom | Evidence | Direction |
| --- | --- | --- | --- |
| 1 | Cannot create a project; must type a manifest path | `App.tsx:401-425`; no dialog plugin | Creation wizard + picker |
| 2 | Whole world fetched once; stale lists | `App.tsx:47-86` | Resource layer + invalidation |
| 3 | Poll restarts every second; duplicate task pollers | `App.tsx:292-318`, `920-946` | One scheduler; operation store |
| 4 | Logs re-read from zero, no append | `App.tsx:320-336`, `api.ts:154-155` | Offset-based tail |
| 5 | Global progress/error; errors far from the form | `App.tsx:39`, `161-173` | Inline + operation tray |
| 6 | Invalid lifecycle buttons when stopped/running | `App.tsx:704-735` | State-driven action set |
| 7 | Actions duplicated in header and detail | `App.tsx:91-108`, `704-735` | One primary action |
| 8 | Database creation downloads a server, selects `default`, blocks UI | `App.tsx:1044-1122`, `1084` | Review step; explicit default; inline progress |
| 9 | Create/attach share one `engine` selector | `App.tsx:969`, `1097-1101`, `1168` | Separate form state |
| 10 | No project database creation; engine `DbAdd` unexposed | `protocol.rs:141-145`; `lib.rs:11-50` | Wire `DbAdd` |
| 11 | No credentials/connection string; password unrecoverable | `engine/src/database.rs:145-188`, `642-655` | `DbCredentials` or choose-on-create |
| 12 | No probe/backup/restore/SQLite maintenance | `protocol.rs:146-156`, `179-187` | Wire commands |
| 13 | External attach rejects blank password in UI | `App.tsx:1227` | Allow empty for local dev, validate at engine |
| 14 | Components flat; install is one long button per entry | `App.tsx:1358-1393` | Grouped table + filters |
| 15 | No remove/discover/used-by for components | `protocol.rs:30`, `48-50`; `App.tsx:1290-1503` | Row actions + discover |
| 16 | Updates and external connections live on Components | `App.tsx:1435-1500` | Move to banner/Settings/Databases |
| 17 | "Open shell" lacks project runtime env | `open.rs:64-87`; `control.rs:37-60` | Use `Exec` or inject env |
| 18 | Project list has no search/sort/status filter | `App.tsx:429-450` | Sidebar search + filters |
| 19 | No confirmation for detach/unpublish/cleanup/restore | `App.tsx:622-628`, `1140-1149`, `1462-1482` | ConfirmDialog |
| 20 | Full-screen menu below 1100 px | `styles.css:613-664` | Rail/sheet |
| 21 | Remote fonts; offline dependency | `index.html:7-12` | Bundle fonts |
| 22 | No app/engine version or engine retry | `App.tsx:1505-1627` | About + retry |
| 23 | Hand-mirrored Rust/TS types | `api.ts:3-119` | Generated bindings or contract test |
| 24 | 1,663-line single component file | `App.tsx` | Feature folders |
| 25 | No app detection or launch-profile layer; user must know the framework command | `App.tsx:401-425`; no detect command in `protocol.rs` | `DetectProject` + Add service |
| 26 | Frameworks that ignore `PORT`/`HOST` cannot use auto-assignment | `project.rs:239-246`; no framework adapter | Profile flags, e.g. `--host 127.0.0.1 --port {port}` |
| 27 | Readiness only accepts HTTP 2xx; redirects and 404s look like crashes | `supervisor.rs:734-750`, `770-790` | Readiness modes; accept any HTTP response for dev servers |
| 28 | Every service is assumed to be HTTP; workers cannot be modelled | `supervisor.rs:252-297`, `698-750` | `ready = "none"` / optional port |
| 29 | Managed `uv` is not used by service commands | `project.rs:417-477` (only node/python/npm/pnpm) | Rewrite `uv` like `pnpm` |
| 30 | No project database environment injection; generated password must be hand-copied | `project.rs:239-246`; `database.rs:642-655` | Local mapping + launch injection + `.env` option |
| 31 | Manager enum knows only npm/pnpm/pip/uv; yarn, bun, Poetry, and conda projects cannot be expressed | `manifest.rs:10-44`, `199-206`; `deps.rs:44-80` | Report unsupported manager honestly; do not silently map to npm/pip |
| 32 | `deny_unknown_fields` makes any new manifest key unreadable by older builds | `manifest.rs:468-514` | Explicit error + regenerate; document around `ready` |
| 33 | Service view carries only a free-text `detail`; no readiness state, mode, duration, or HTTP status | `project.rs:396-420`; `protocol.rs` `ServiceView` | Structured readiness fields |
| 34 | Site proxy can target a service that has no HTTP endpoint once `ready = "none"` exists | `manifest.rs:244-276`; `apache.rs:485-500` | Validation rejects proxying a non-HTTP service |
| 35 | Implicit Node manager defaults to pnpm even when pnpm is not installed | `deps.rs:44-80`; `project.rs:446-453` | Propose npm without a lockfile; explain the missing tool instead of a late command error |

## Appendix B — Files reviewed

- `apps/desktop/src/App.tsx`, `api.ts`, `styles.css`, `main.tsx`, `index.html`, `package.json`, `vite.config.ts`
- `apps/desktop/src-tauri/src/lib.rs`, `commands.rs`, `open.rs`
- `crates/core/src/protocol.rs`, `manifest.rs`, `project.rs`, `deps.rs`, `store.rs`, `apache.rs`, `logs.rs`, `error.rs`
- `crates/engine/src/database.rs`, `supervisor.rs`, `control.rs`, `lib.rs`
- `crates/cli/src/main.rs`
- `docs/DEVELOPMENT_PLAN.md`, `docs/PRODUCT_PLAN.md`, `docs/IMPLEMENTATION_STATUS.md`, `docs/KNOWN_LIMITATIONS.md`, `docs/decisions/0002-project-runner.md`, `0004-databases.md`, `0007-install-database-servers.md`, `docs/evidence/S4-DESKTOP.md`
- `catalog/packages.json`

No files outside this document were changed; no build, test, or desktop launch was run for this paper. Validation is limited to structure: heading order, balanced code fences, no trailing whitespace, and `git diff --check`.
