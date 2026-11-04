# Review of the desktop rework proposal

Date: 4 November 2026. Planning review; no application code or runtime acceptance changes.

## Existing implementation is the baseline

Stages 0–8 already have implementation and recorded evidence. DeepSeek's proposal is a corrective follow-up to that implementation, principally project workflows in Stages 3–4 and missing desktop access to Stage 6 capabilities. It does not restart the roadmap. Pending Windows/macOS acceptance is tracked separately from code being implemented.

The remaining work is Stage 8A, then the bounded Stages 9–17, then Stage 18 public release. See the execution plan for current dependencies and acceptance.

## Requirements and findings

The original product plan mentioned recipes, but execution acceptance covered import rather than creation. The user confirmed on 4 November that Create project must produce runnable starters for **Node HTTP, Next.js, Flask, and FastAPI**. This supersedes the proposal's recorded manifest-only choice.

DeepSeek's project workspace, discovery, per-operation progress, incremental logs, and database workflows are useful directions. Resolve these points when implementing the affected features:

- A folder plus manifest cannot pass a greenfield runnable-app scenario. Recipes must supply source and dependency declarations, with pinned inputs or generator versions.
- The current project-wide manager cannot express independent pnpm frontend and pip/uv API dependency scopes. Define directory, manager, runtime/environment, and lockfile authority for each scope.
- Review downloads/file changes before execution. Define cancellation, retry, and partial failure without overwriting existing files or deleting persistent data.
- The proposal's readiness examples include proposed fields. Preserve old-manifest behavior or version the change. Separate process state, listener readiness, and application health; TCP has no HTTP path, and redirects are not automatically errors.
- Preserve project scripts, packageManager, lockfiles, and the selected manager. Do not bypass script setup using a guessed entry file or silently switch pnpm to npm.
- Preview/validation accepts the unsaved draft and must not install, create environments, or reserve ports. Recheck file fingerprints and ports when applying.
- Database connection injection needs explicit per-service mappings, environment precedence, URL encoding/driver format, secret handling, and safe .env updates.
- A frontend activity tray does not by itself provide backend operation recovery. Add only the missing protocol/domain support; reuse existing lifecycle, package, database, and routing code.
- Keep actual target-machine acceptance open until executed; Linux evidence does not establish Windows 10/macOS 14.5 support.

## Corrected execution stages

Complete Stage 8A first. The former broad workflow stage is split into these independently reviewable stages in DEVELOPMENT_PLAN.md:

| Stage | Scope |
| --- | --- |
| 9 | Define project scopes and launch contracts |
| 10 | Build the shared desktop workspace and activity layer |
| 11 | Create runnable projects from the desktop |
| 12 | Import applications and edit their configuration |
| 13 | Unify components, dependencies and PHP extensions |
| 14 | Complete project database workflows |
| 15 | Complete sites, HTTPS and desktop settings |
| 16 | Finish desktop usability and failure recovery |
| 17 | Accept the integrated app on Windows and macOS |
| 18 | Public release and distribution |

Each stage specifies dependencies, four numbered tasks, acceptance, implementation ownership and handoff boundaries. Existing Stage 0–8 implementations are reused. Missing platform evidence remains pending, and Stage 18 depends on Stage 17 acceptance.

Acceptance for starters: empty folder → generated application and manifest → selected runtime/tool → dependencies → start → responding URL/logs → stop. Also verify cancellation/retry, existing files preserved, mixed-language scope isolation, and import without a handwritten manifest.

Parallel work can follow the existing AGENT_WORKFLOW.md once shared contracts and file ownership are agreed. No workers are assigned by this review.

## Validation

Source findings are from the preceding review; no application execution is claimed. This document clarifies planning and preserves existing stage evidence.
