# Language tools, project dependencies, and native extensions

Requirement added 31 October 2026. The application must manage language tools and project dependencies as well as runtimes/services. PHP extension installation is an explicit product requirement, with memcached as a requested example. No additional package payload has been tested in this planning update.

## User-facing model

| Kind | Examples | Scope and behavior |
| --- | --- | --- |
| Runtime | Node.js, Python, PHP | Existing installation or managed version |
| Language tool | pnpm, npm, pip, uv, Composer | A selected compatible version; run against the intended runtime/environment |
| Project dependency | Next.js, Flask, FastAPI, Laravel libraries | Install with the project's chosen ecosystem tool and preserve its manifests/lockfiles |
| Native runtime extension | PHP memcached, redis, Xdebug | Match the runtime's binary compatibility; manage activation and restart affected processes |
| Server/service | Memcached, MySQL, PostgreSQL | A separate process or external endpoint with its own lifecycle |

Memcached is a cache server. PHP's memcached extension is a client interface; it does not install/start the server. A project using this cache backend needs a reachable server plus the appropriate client. A remote server is valid. PHP memcache and memcached are different extensions and must not be substituted. Whether a framework needs them depends on the application's selected cache/session backend and dependencies.

## Package-manager integration

The app provides Install dependencies, Add package, Remove package, Update selected, List installed, and command output. The existing ecosystem manager owns resolution and lockfile semantics. A manager version upgrade is separate from an application dependency upgrade.

- **Node:** support npm and pnpm initially. Detect packageManager metadata, existing lockfiles, and workspace root. If they conflict, ask for a project-level choice rather than generating a second lockfile. Reuse an existing compatible pnpm or install a pinned version into app-owned tool storage. Validate Node/tool/platform compatibility. Do not assume Corepack is bundled or overwrite global npm/pnpm. Honor workspace boundaries and package-manager script/build policies. pnpm publishes multiple platform distributions; pin and verify an appropriate version instead of running an unbounded latest bootstrap. See [pnpm installation](https://pnpm.io/installation).
- **Python:** support pip explicitly, in addition to uv-managed workflows. Invoke the selected virtual environment's Python with `-m pip`, rather than an arbitrary pip on PATH. Provision pip into an app-created environment when necessary; never install project dependencies into a system interpreter by default. Preserve requirements files, pyproject metadata, constraints, and uv.lock according to the chosen workflow. Do not silently rewrite a uv-managed environment with pip or promise that every pyproject has an equivalent pip-only workflow. pip does not universally record installed changes into a project requirements file; show runtime-environment changes versus persisted dependency declarations honestly. See [pip user guide](https://pip.pypa.io/en/stable/user_guide/).
- **PHP:** use Composer for application libraries and platform-requirement checks. Run it with the project's selected PHP and intended extension configuration. Composer dependencies and PHP extension binaries have separate installation paths. Preserve composer.json/composer.lock and distinguish install from update. See [Composer basic usage](https://getcomposer.org/doc/01-basic-usage.md).

Registry credentials belong to existing supported tool configuration or OS-protected storage and must be redacted in output. Package installation can execute project/package build scripts: display the command and target environment, respect the tool's policy, and never run it elevated. Failed dependency installation may leave a partial node_modules/virtual environment; report this state and offer reinstall/recreation from recorded inputs. Do not promise a generic atomic rollback of package-manager side effects.

## PHP extension provider

Represent an extension artifact separately from its activation profile. Bundled extensions may only need enabling; downloaded extensions require verification and dependency checks. Store managed artifacts in versioned directories and generate per-project ini fragments rather than modifying a shared runtime installation for every project.

Compatibility metadata includes PHP version/API/build identity, OS, CPU architecture, thread-safety mode, debug setting, build-tool/runtime compatibility, dependent native libraries, and extension version. Windows DLLs must match the PHP executable; never install an arbitrary DLL based only on its extension name. See [PHP Windows extension guide](https://www.php.net/manual/en/install.pecl.windows.php).

Use approved prebuilt artifacts where possible. Evaluate PIE as an extension-installation provider where its target/version behavior is verified; the current PHP guide recommends it. It is not a guarantee that every extension has a matching binary. Missing builds must show unavailable or use an explicit tested build path. No silent source compilation requiring a toolchain the user does not have. See [PHP Installer for Extensions](https://github.com/php/pie).

For memcached, validate the extension's native dependencies as well as PHP compatibility. Its build documentation references libmemcached and optional compression/authentication dependencies. Availability of matching Windows x64 and macOS 14.5 arm64 artifacts, and of a suitable native Memcached server on each target, remains unverified. See [PHP memcached installation](https://www.php.net/manual/en/memcached.installation.php).

Activation workflow: resolve compatible artifact/dependencies, download/verify, generate candidate ini profile, probe extension load in a separate PHP process, apply the profile, restart only affected PHP workers, and verify the server plus CLI. Retain the previous profile on failure. PHP updates must resolve a compatible extension set before switching the project; never copy old binaries into a different ABI blindly.

## Acceptance scenarios

1. A Next.js fixture pinned to pnpm installs and runs with that manager; its lockfile is preserved. An npm fixture stays on npm. Workspace operations affect the correct package/root.
2. A Python fixture installs a dependency through its selected environment's pip; another project and the system Python remain unaffected. A uv-managed fixture retains its own workflow.
3. Add/remove/update actions show the exact project scope, tool version, output, and resulting manifest/environment changes; interrupted installs have actionable recovery.
4. Two PHP projects share a runtime artifact but use different supported extension profiles. CLI and HTTP probes observe the intended extension availability.
5. A mismatched or unloadable extension is rejected before active workers are changed. Switching PHP version revalidates extensions and native libraries.
6. A PHP memcached fixture sets/gets a value against a real local managed server and against an existing endpoint. Missing extension and unreachable server produce distinct diagnostics. A test double cannot establish native target compatibility.

## Delivery placement

- Stage 0: validate candidate pnpm/pip workflows and record separate PHP-extension/Memcached feasibility work.
- Stages 1–2: model tool selection and extension-profile requirements; provide Node/Python tool installation/reference.
- Stages 3–4: implement dependency commands and UI for npm/pnpm/pip/uv workflows.
- Stage 7: keep tool/runtime/dependency updates separate and recover failed operations honestly.
- Stage 8: require actual pnpm and pip workflow acceptance in the personal Node/Python release.
- Stage 8A: implement PHP, Composer, extensions, and Memcached integration as a dedicated capability milestone, independent of public-distribution infrastructure.
