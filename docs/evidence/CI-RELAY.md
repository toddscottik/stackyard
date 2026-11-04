# CI relay test portability

5 November 2026. Windows 10 build 19045 x64, Rust 1.99.1
(`x86_64-pc-windows-msvc`). No Mac was available. Only loopback TCP and
disposable private-socket fixtures were used; no native Memcached artifact or
user service was launched.

## Failures and correction

GitHub Windows timed out while waiting for all 32 backend connections. The test
opened TCP clients in a burst and then started one three-second deadline. TCP
connect completion does not establish that the relay worker and private backend
connection are ready. The same assertion failed locally under concurrent runs.

Each fixture backend now sends a readiness message through the relay. The test
waits for that message before opening the next client, retaining every client so
all 32 slots are occupied when it checks rejection of connection 33. The
two-second shutdown assertion is unchanged. Client closure checks now require
EOF/reset/abort rather than accepting every result other than a read timeout.
Disconnecting the fixture stop channel also ends its accept loop during panic
unwinding.

GitHub macOS reached shutdown but failed immediate rebinding of the old TCP
port. Closed active connections can retain TCP state after the listener closes;
immediate port reuse is not a portable shutdown assertion. The active-connection
test now checks that a new TCP connection fails. Windows locally returned a
timeout for this probe, so both refusal and timeout are accepted. A separate
`idle_relay_shutdown_releases_listener` test requires immediate exclusive
rebinding without prior client connections, where connection TIME_WAIT cannot
interfere. Production relay and exclusive-listener behavior are unchanged.

## Commands and results

- `cargo test -p stackyard-engine --lib cache::tests::relay_bounds_connections_and_closes_inflight_on_shutdown --offline -- --exact --nocapture`: baseline single run passed; repaired test passed.
- Diagnostic baseline: 20 runs of that test with four concurrent test processes
  produced 13 failures, including the reported backend-count deadline failure.
  Temporary error logging in the relay worker was removed after investigation.
- Repaired test: 20 runs with the same four-process concurrency all passed.
  A 120-second subprocess watchdog allowed the sequential handshakes to complete
  under contention. Setup now uses a three-second read timeout per connection
  instead of one three-second deadline for all backend accepts; the original
  two-second relay-shutdown assertion is unchanged. Earlier exploratory runs used a
  20-second whole-process watchdog, which was too short for some synchronized
  runs; these are not included in the final 20/20 result.
- `cargo test --workspace --offline`: passed, including 74 core and 14 engine
  unit tests, the new idle-listener test, and the existing transport/half-close
  checks. Optional native artifact tests were not enabled by this run.
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: passed.
- `rustfmt --check --edition 2021 crates/engine/src/cache.rs`: passed.
- `git diff --check`: passed.

Local stress logs are in gitignored `.artifacts/CI-RELAY/`. To reproduce the
concurrency check after building the engine unit tests, run this Python from the
repository root with the emitted test executable as its argument:

```python
import concurrent.futures
import subprocess
import sys

def run(index):
    result = subprocess.run([
        sys.argv[1], "--exact",
        "cache::tests::relay_bounds_connections_and_closes_inflight_on_shutdown",
        "--nocapture",
    ], capture_output=True, text=True, timeout=120)
    return index, result.returncode, result.stdout + result.stderr

with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
    results = list(pool.map(run, range(20)))
failed = [item for item in results if item[1]]
print(f"20 runs, 4 concurrent processes, {len(failed)} failures")
for index, code, output in failed:
    print(index, code, output)
sys.exit(bool(failed))
```

The executed local harness selected
`target/debug/deps/stackyard_engine-4164f116c3c58935.exe` and saved each process's
output. Compiler-generated executable names can differ on another checkout.

Next: push this correction and verify new Windows/macOS CI runs. Local stress
success is evidence against the reproduced timing failure, not Mac acceptance
or a guarantee against every possible CI failure.
