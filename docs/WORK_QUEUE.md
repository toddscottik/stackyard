# Work queue

The coordinator owns this file. Status is separate from stage completion. Claim a task by setting an owner and status before assigning writes. This initial queue is prepared for future work; no workers are active.

| ID | Status / owner | Scope and writable paths | Acceptance / dependency |
| --- | --- | --- | --- |
| S0-WIN-RUNTIMES | Ready / unassigned | `scripts/verification/windows/runtimes.ps1`, `docs/evidence/S0-WIN-RUNTIMES.md` | Verify managed Node/Python payloads, reuse local runtimes, test pnpm/pip in disposable environments; record exact artifacts and commands |
| S0-WIN-SERVERS | Ready / unassigned | `scripts/verification/windows/servers.ps1`, `docs/evidence/S0-WIN-SERVERS.md` | Apache HTTP/proxy modules; MySQL/PostgreSQL initialize/query/restart/stop against disposable data; no system-service takeover |
| S0-MAC-PACKAGES | Ready for research; execution needs Mac / unassigned | `scripts/verification/macos/`, `docs/evidence/S0-MAC-PACKAGES.md` | Native arm64 artifacts on macOS 14.5; resolve Apache/MySQL delivery; validate actual execution on M1 Pro |
| S0-INTEGRATE | Waiting for probe results / coordinator | Verification report, validated catalog inputs, status and queue | Review evidence, approve or reject target combinations, retain incomplete Mac checks, agree shared fixture format |
| S1-BOOTSTRAP | Waiting for Stage 0 toolchain findings / unassigned | Stage 1 source/toolchain files; explicit paths assigned before start | Rust/Cargo and platform prerequisites; Tauri shell, core contracts, engine/CLI skeleton; may proceed independently of unresolved DB packaging |

All workers may read existing documentation. Runtime caches and test data belong under `.artifacts/<task-id>/` and stay out of Git. Root instructions, root dependency manifests/lockfiles, shared APIs, PACKAGE_VERIFICATION.md, IMPLEMENTATION_STATUS.md, and this queue remain coordinator-owned unless explicitly reassigned.

The scripts listed above are planned deliverables, not existing commands. Each must provide help, explicit paths/ports, non-destructive defaults, and structured results. Do not treat missing software or a compiler as a passed check.
