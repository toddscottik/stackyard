# Product plan review

Reviewed: 31 October 2026. Based on PRODUCT_PLAN.md as it stands after the existing-installation and Apache revisions. This is a design review, not implementation or compatibility-test evidence.

Follow-up: [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md) incorporates the review into actionable stages; [PACKAGE_VERIFICATION.md](PACKAGE_VERIFICATION.md) records subsequent source and local-runtime verification. Use the development plan as the implementation entry point.

## Verdict and confirmed direction

Proceed with the concept, but treat the current plan as a product vision rather than an implementation-ready release specification. The strongest requirements are native operation, Windows 10 and macOS, a general multi-language model, existing-installation selection, managed downloads, version control, and real Apache support.

The user has now confirmed that the first usable release is for their own development, with a public release later. This should change sequencing: optimize for replacing their real workflow, while keeping architecture extensible. Do not spend the first release building a public package ecosystem or implementing every language recipe.

The user has confirmed the initial workload: Next.js, Flask, FastAPI, a Node.js server, Apache, MySQL, PostgreSQL, and SQLite. The macOS test machine is a MacBook M1 Pro with 16 GB RAM running macOS 14.5, and it is available for testing. Open source and the exact application license were recommendations in the original plan, not yet confirmed user decisions.

This makes Node.js and Python the first managed runtimes to prioritize. PHP and Go remain roadmap capabilities; defer their implementation in the personal-use milestone unless another real project requires them. Include both MySQL and PostgreSQL in the personal-use target, implementing them sequentially. SQLite is file-based and needs project database-path/backup handling rather than a separate daemon. Apache serves as the shared reverse proxy and static server for these initial applications; PHP integration is not a prerequisite for that use.

## Findings, ordered by implementation impact

### 1. Package availability is an unresolved dependency of the central promise

The plan promises one-click installation but leaves Apache and macOS PHP distribution to later investigation. These packages are prerequisites for the promised experience. A successful process supervisor cannot compensate for missing or non-relocatable packages.

Before committing to supported versions, produce an artifact matrix: component, exact version, OS, CPU architecture, source, runtime prerequisites, required libraries/extensions, verification method, install layout, and smoke-test result. Include both an existing-installation example and a managed-download example.

For the private alpha, use a small bundled catalog and vetted download locations. A catalog update can ship with an app update initially. Public hosting, automated release ingestion, broader mirrors, and signing-key rotation belong to the later distribution workstream. Verification and interrupted-install recovery remain necessary from the first real downloader.

Do not describe every upstream release as automatically installable. A notification about an upstream release and a verified catalog package are different states.

### 2. Global tools and project environments need separate workflows

The plan is heavily project-oriented, while the user's latest requirement begins with choosing software to install. A user should be able to install Python, start a database, or inspect Apache before creating a project manifest.

Recommended basic model:

| Entity | Meaning | Example |
| --- | --- | --- |
| Package | A managed installation or external installation reference | Python 3.12 at a verified local path |
| Service instance | A configured runnable process with ownership and persistent storage | A local MySQL instance with a named data directory |
| Project | A folder and its selected tools, processes, routes, and service references | A Python API with a Node frontend |
| Connection | An endpoint owned outside the app | An already running PostgreSQL server |

One package can support multiple service instances. A runtime installation does not have a running/stopped state; a service instance does. The original combined state machine mixes installation and service lifecycle and should be split accordingly.

The minimal desktop experience needs an installed-components list, service controls, a project list, and details/logs. A user should not need to write TOML to get the first site running. Manifest export can follow successful setup, with explicit handling of existing project files.

### 3. Shared ports require a concrete routing topology

The plan promises concurrent projects, selectable web servers, and convenient domains but leaves ownership of ports 80/443 unspecified. Independent server processes cannot all claim the same listening address and port.

Recommended private-alpha topology: one app-managed Apache instance owns the chosen HTTP/HTTPS ports and serves multiple virtual hosts. Each PHP project selects a separate backend/configuration; HTTP applications listen on allocated loopback ports and are reverse-proxied. This needs no Caddy dependency.

The resulting limitation must be explicit: a shared Apache upgrade or restart can affect every project using it. Per-project PHP versions are supported independently; per-project Apache versions are a separate feature. Retain configuration transactionally: build a candidate, validate it, reload, verify health, and restore the last working configuration on failure.

Move optional Caddy implementation out of the private-alpha scope. Leave a provider interface, but solve simultaneous Apache/Caddy routing only when that feature is actually added. Existing external Apache can either be used as a validated executable with app-owned configuration or remain externally controlled; automatic takeover of a running installation is outside the initial workflow.

### 4. Apache solves .htaccess parsing, but PHP compatibility is more specific

The user's correction is correct: Apache should interpret .htaccess directly. However, PHP-specific directives such as php_value/php_flag are documented for PHP running as an Apache module. They are not an unconditional promise of the proposed FastCGI configuration. See [PHP configuration documentation](https://www.php.net/manual/en/configuration.changes.php).

Keep Apache plus per-project FastCGI as the proposed default because it supports independent PHP processes/configuration. During import, identify handler-dependent settings and show an actionable result; do not silently rewrite the project's .htaccess. Decide whether mod_php compatibility is needed based on the user's real projects.

Also, Apache's mod_proxy_fcgi forwards requests but does not launch the application process. The plan needs an explicit Windows PHP process-management choice and concurrency tests; the word "supervised" is not enough. See [Apache mod_proxy_fcgi](https://httpd.apache.org/docs/2.4/mod/mod_proxy_fcgi.html).

Prototype concurrent requests, a slow request, a worker exit, POST/upload requests, and rewrite/PATH_INFO behavior. Verify server and CLI PHP version, extension set, and intended settings. Selecting the same installation does not by itself ensure that different PHP interfaces load the same configuration.

### 5. MySQL is absent from the proposed initial database support

The scope selects MariaDB and PostgreSQL without establishing what the user's projects require. If existing work uses MySQL, replacing it with MariaDB may turn importing the environment into a database migration. MariaDB documents compatibility differences and migration choices explicitly. See [MariaDB migration guidance](https://mariadb.com/docs/server/server-management/install-and-upgrade-mariadb/migrating-to-mariadb/moving-from-mysql).

The user's actual workload now confirms MySQL and PostgreSQL, so prioritize those engines and defer MariaDB. Represent MySQL and MariaDB as separate engine identities if MariaDB is added later. Preserve the source engine on import where possible. Support connecting to existing servers early; defer automated major-version migration until a tested engine-specific workflow exists. SQLite needs engine-appropriate backup handling as well; copying a live database file without considering active transactions/journals is not a general backup strategy.

Backups need a defined scope: selected databases, users/grants if included, routines/events as applicable, and consistency conditions. A successful dump exit is insufficient evidence of recoverability. Restore into a disposable instance and run application checks.

### 6. Existing-installation support needs a capability contract

The current ownership distinctions are good but too broad to promise universal reuse. An installation can be executable yet unsuitable: a PHP CLI-only build, Python without the required environment tools, an incompatible architecture, missing libraries, or an external service whose configuration cannot be controlled independently.

Define capabilities per provider: discover, inspect, run, create isolated configuration, attach, install, and update. Show availability based on those capabilities. Explicit executable paths are local machine bindings; shared manifests contain requirements and portable intent. Revalidate bindings before launch.

For the first release, updating software owned by Homebrew, pyenv, another installer, or the OS should remain an owner-managed operation. Offer a separate managed replacement if desired. This preserves the user's request to reuse installed tools without expanding the app into a universal package-manager frontend.

### 7. Configuration ownership and crash recovery are underspecified

The plan proposes SQLite, a project manifest, a lockfile, generated server files, and user overrides without fully defining which wins.

Recommended authority:

- Manifest: shareable desired project configuration.
- Lockfile: resolved managed artifacts and recipe versions.
- Local state: machine-specific external paths, secret references, allocated ports, install records.
- Generated files: derived output, with a documented supported override mechanism.
- Observed state: actual service processes and health, refreshed rather than treated as durable truth.

Avoid storing conflicting copies of project settings in SQLite and TOML. UI edits should use the same validation and persistence path as CLI edits. Manual manifest changes should be detected; invalid changes should produce a visible error while retaining the last applied configuration.

Record ongoing mutations so restart can reconcile an interrupted install, project switch, or service start. Serialize conflicting operations and lock package installation. Garbage collection must consider every project and live instance before deleting a package. Disallow uninstalling an in-use package by default, with explicit reassignment available.

### 8. Windows 10 and macOS support need actual target evidence

The earlier macOS 13+, Intel, and Apple Silicon matrix was an assumption. The confirmed initial Mac is an M1 Pro with 16 GB RAM on macOS 14.5. Prioritize native arm64 artifacts and test on this actual OS version; public support can expand later. No claim of compatibility follows from identifying the machine alone.

Measure the combined resource use of the desktop/engine, Apache, a Next.js development server, Python services, MySQL, and PostgreSQL on this 16 GB machine. Include idle behavior, active requests, file watching/hot reload, sleep/wake, and clean shutdown. Start only requested services and avoid installing every available component by default. This validates normal development alongside the user's editor and browser without inventing a performance budget before measurement.

Likewise, distinguish app-shell compatibility from runtime compatibility. A component can drop an OS version independently of the desktop framework. The catalog must filter by the actual target and keep an intentional policy for older supported packages.

For the private alpha, a reproducible result on the user's Windows 10 machine and target Mac is a better gate than a generic promise covering all combinations. Keep package IDs architecture-specific from the beginning so adding targets does not require redesign.

### 9. Multi-language does not require rewriting each ecosystem's workflow

Retain a language-neutral process runner and explicit per-process environment. Reuse Composer, npm/pnpm, uv, Go tooling, and their existing project/dependency metadata.

Do not unconditionally put every project runtime on a single global PATH. Services can require different versions or virtual environments. The command runner needs an explicit tool selection, working directory, environment, readiness policy, and stop behavior. For ambiguous multi-service projects, the shell command should select a service context.

Keep raw shell escape hatches visible. Recipe detection can suggest setup, but imported hooks and dependency installation execute project code and need an intentional first-use action. A general extension marketplace is unnecessary for this stage.

### 10. Schedule and release gates need revision

The 4–8 month estimate is not grounded in a package matrix, verified target hardware, staffing, or a defined acceptance project. Treat it as an earlier rough illustration rather than a delivery commitment; do not replace it with another precise estimate before the feasibility results.

For personal use, add a thin desktop interface earlier so the user can test the main workflow. Implement the requested MySQL and PostgreSQL support sequentially, and prioritize recipes for Next.js, Flask, FastAPI, and a generic Node server. Defer unrelated framework recipes, PHP/Go implementation, automated cross-machine exports, advanced debug integrations, and the second web-server implementation. These are proposed sequencing changes, not removal of the multi-language product goal.

Public distribution then adds broader compatibility, installer/updater trust, signing/notarization, support policy, provenance/license notices, and ongoing catalog maintenance. A private alpha should still verify downloads, protect database data, and report its limits honestly.

## Revised delivery gates

1. **Package feasibility:** choose the user's first actual stack; obtain one validated managed artifact per required component and prove reuse of an external runtime on both target machines.
2. **Real project running:** download/select components in a small desktop UI, create a project, start its processes, and open its local URL. Apache handles required .htaccess rules.
3. **Daily-use replacement:** logs, mail if needed, project shell, concurrent projects, clean shutdown, connection-only database support, persistent data, and useful diagnostics.
4. **Update rehearsal:** install a newer package alongside the old one, switch one project, preserve other project bindings, and recover from a failed switch. Prove database restore separately.
5. **Public-release preparation:** broaden artifacts and CI, publish a precise support matrix, complete release packaging, and establish catalog/update maintenance.

A useful first acceptance story is: "On Windows 10 and my Mac, I can select an existing runtime, download a missing component, run my real application with its existing server rules and database, and stop it without losing data or affecting unrelated software."

For the confirmed workload, exercise Next.js hot reload/WebSockets through Apache, a Flask service, a FastAPI service, and a generic Node service. Give each service an explicit command and environment, and verify URL generation and forwarded host/scheme handling. Start with development commands already used by the projects. Configure MySQL and PostgreSQL connections independently, and test SQLite persistence/backup using a small actual application. Next.js SSR/server routes must run through the Node process rather than being treated as a static export by default.

## Decisions requested from the user

- Confirmed: own development first; public release later.
- Confirmed: MacBook M1 Pro, 16 GB RAM, macOS 14.5, available for testing; native arm64 is the initial macOS target.
- Confirmed workload: Next.js, Flask, FastAPI, Node.js server, Apache, MySQL, PostgreSQL, SQLite. Exact versions can be determined from the selected real project before package selection.
- Later, before public distribution: application license and willingness to maintain package-build infrastructure.

The Tauri/Rust recommendation remains plausible, but technology preference is secondary to proving the above behavior. No change to the implementation stack is required by this review.
