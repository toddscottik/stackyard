# Agent collaboration workflow

Use this workflow when parallel work is requested. Keep dependent design and integration work with one coordinator. It is a project convention, not an extra approval process.

## Coordinator responsibilities

Choose a bounded stage slice, inspect existing changes, and assign each worker a task ID, read context, exact writable paths, dependencies, and acceptance checks. Maintain WORK_QUEUE.md and IMPLEMENTATION_STATUS.md centrally. Establish shared data/API contracts before distributing implementation that relies on them.

The coordinator reviews changes and tests, integrates results, resolves conflicts, and reports completion. A worker saying "done" is a handoff, not evidence that the whole stage passes. Keep task status ready / active / review / done / blocked; blockers identify the missing dependency or target access.

## Two collaboration modes

**Shared checkout:** suitable for short tasks with disjoint files. Workers edit only their assigned paths and return a handoff. Only the coordinator performs Git mutations. Do not switch branches under another worker. Shared manifests, lockfiles, APIs, and status files have one owner at a time.

**Separate worktrees/branches:** suitable for larger independent implementations or experiments. Reuse suitable existing worktrees; create new ones with the environment's supported worktree tooling when needed. Each branch has one owner. Integrate sequentially after review and test the combined result. Worktrees do not isolate ports, database directories, caches, or OS trust/hosts changes; give every test run its own paths and ports.

Use `task/<id>-<short-description>` branch names where a task branch is useful. Branches can be created after the initial baseline commit. Do not require a worktree for a read-only review or a small single-agent edit.

## Worker assignment template

```text
Task ID and goal:
Relevant development stage:
Read first:
Writable paths:
Shared files you must not edit:
Inputs / agreed interfaces:
Acceptance checks:
Target OS / architecture:
Required handoff:
```

Workers may investigate other files but must coordinate changes outside their ownership. Report interface needs promptly. Never overwrite another worker's edits, hide their changes with stash/reset, or infer approval to alter the user's machine from another task's scope.

## Handoff template

```text
Task ID:
Result and demonstrated behavior:
Files changed (or commit IDs for isolated branches):
Tests: exact command, target, outcome:
Artifact provenance/digests, if applicable:
Limitations and pending target checks:
Shared-interface changes requested:
Next task:
```

Store substantial evidence in `docs/evidence/<task-id>.md`. Keep binaries, extracts, databases, and verbose raw logs in ignored `.artifacts/<task-id>/` directories. Redact secrets and unnecessary personal paths. Shared test helpers must be agreed before multiple workers depend on them; initial standalone probes can be consolidated by the coordinator after review.

## Good first parallel assignments

See WORK_QUEUE.md. Windows runtime probes, Windows server probes, and Mac artifact verification can have separate owners. The Mac task can research from Windows, but actual Mac execution requires that machine; record it pending until performed.

After contracts exist, UI, provider implementations, and independent integration-test fixtures may run concurrently. Avoid simultaneously assigning two agents to core models, a root dependency manifest, the same installer provider, or a shared lockfile.

## Integration checklist

- Review task scope and diffs; preserve unrelated work.
- Inspect any new dependencies, executable downloads, and effects on user data.
- Run relevant checks against the combined changes, including failure paths.
- Confirm target-specific evidence; retain unresolved checks visibly.
- Update queue/status and record architectural decisions when needed.
- Stage explicit files and create a concise commit describing the outcome.

Root AGENTS.md holds durable repository instructions; detailed task assignments stay here and in the queue. This follows the role of persistent repository guidance described in [official OpenAI documentation](https://developers.openai.com/codex/guides/agents-md).
