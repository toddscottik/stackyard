# Stage 9 readiness implementation evidence

Worker C implementation notes, 4 November 2026. Target available in this session: Windows x64; macOS 14.5 arm64 remains pending.

The engine now dispatches the frozen `ReadinessSpec` variants through `readiness::wait_ready`. HTTP probes read one raw status line and do not follow redirects; TCP probes only connect; process readiness waits for the configured grace period while checking process survival. Wait outcomes carry the most recent HTTP status. Supervisor writes `pending`, `ready`, `failed`, or `not_checked` observations independently from process lifecycle rows. Process workers use port 0 internally and do not reserve a listener port. Launch removes inherited `PORT` and `FLASK_RUN_PORT` per its scoped launch contract while retaining explicit service environment values.

The fixture adds `worker`, `tcp`, `status CODE`, and `delayed-crash MS` modes. Existing `serve`, `serve-child`, crash, dependency, and PHP modes are retained. Apache proxy targets must be HTTP endpoints; FastCGI's dedicated handler configuration remains unchanged.

Windows x64 checks run on 4 November 2026:

- `cargo test -p stackyard-engine --test readiness` — passed: worker is portless with no published port, TCP readiness publishes no HTTP status, raw 302 is accepted when explicitly listed, and strict 503 health failure retains its observed status.
- `cargo test -p stackyard-engine --lib readiness::` — passed: raw redirect status, malformed and silent response bounds, and TCP-only connection.
- `cargo test -p stackyard-core apache::tests` — passed all 10 Apache tests, including occupied listener and retained FastCGI handler rendering.
- `cargo test -p stackyard-engine --test php_reconfigure` — passed the existing PHP profile rollback/restart scenario.
- `cargo test -p stackyard-engine --test projects occupied_port_restart_limit_and_exec` — passed legacy occupied-port, restart-limit and crash output coverage.

macOS 14.5 arm64 execution remains pending. The dedicated test covers worker port removal with the current inherited environment; an explicit service-provided `PORT` is preserved by the launcher ordering but is not separately asserted here.

Additional native PHP check, 5 November 2026, Windows x64, using the verified PHP 8.5.12 executable at `.artifacts/S8A-ARTIFACTS/php runtime/php.exe`:

- `STACKYARD_TEST_PHP=<absolute verified php.exe> cargo test -p stackyard-engine --test readiness native_schema2_php_worker_profile_rollout_is_scoped_and_portless -- --nocapture` — passed. Two PHP scopes ran endpointless process workers; applying an empty extension profile to `worker-a` wrote its scope profile, restarted only that worker, preserved `worker-b`'s PID, and kept both endpoint/port fields absent.
- With the corresponding local gates set, `cargo test -p stackyard-engine --test php` and `cargo test -p stackyard-engine --test php_reconfigure -- --nocapture` — passed. The existing HTTP PHP rollout retained its original observed port after restart.
