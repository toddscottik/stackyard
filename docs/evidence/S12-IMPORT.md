# Stage 12 import and configuration evidence

6 November 2026. Windows 10 Pro build 19045 x64, Rust 1.99.1, and WebView2
156.0.4314.58. A compiled Tauri app used disposable project folders and an
isolated engine and WebView profile. The project parent contained spaces
and Cyrillic characters. Native macOS 14.5 arm64 acceptance remains pending.

## Windows desktop

The native folder picker selected an existing Next.js folder. The other
folders were entered by path. Inspection and preview showed evidence,
dependency scopes and launch arguments before each save. None of the
five application fixtures began with a Stackyard manifest.

| Application | Runtime and dependencies | Observed result |
| --- | --- | --- |
| Next.js | Node 24.22.2, pnpm 12.16.1, Next 16.4.3 and React 19.3.1 | Reused the pnpm lockfile and development script, served the page and stopped |
| Flask | Python 3.12.3, pip 24.0, Flask 3.1.3 | Installed into the project scope, served the detected application and stopped |
| FastAPI | Python 3.12.3, pip 24.0, FastAPI 0.162.0 and Uvicorn 0.54.0 | Installed into the project scope, served JSON from the detected application and stopped |
| PHP | PHP 8.5.12 NTS x64, Apache 2.4.69 x64 | Detected `public/index.php`, started FastCGI, served through Apache on a temporary loopback port, unpublished and stopped |
| Next.js + FastAPI | The same Node and Python pins in separate scopes | Both services returned their expected responses in the same run and stopped |

Dependency installs used the populated pnpm store and PyPI wheelhouse from
Stage 11. The PHP fixture had no third-party Composer dependencies. Apache
used an explicit test hostname resolution and temporary loopback ports.

- A manifest created by an editor after review made Save fail. Its exact
  contents survived. Inspecting again allowed a fresh review and import.
- The mixed workspace reloaded an externally edited manifest, saved a
  display name change from the Configuration form and kept its project ID
  and the editor's comment. Forget removed the registration and preserved
  the manifest, source and the value `42` in a real SQLite database.
- A schema 1 manifest registered with its original bytes intact. The
  import review rendered at 390, 768 and 1440 px without horizontal page
  overflow. The run reported no uncaught WebView exceptions.
- The CLI completed `configure inspect`, `preview`, `apply`, `reload`
  and `forget` in a separate disposable folder. Inspect and Preview left
  the manifest absent. Forget left the saved file present.
- A manually configured Node HTTP application kept its explicit fixed
  port. Start refused while a disposable listener occupied that port and
  left the listener running. After releasing the listener, the same saved
  configuration started and returned the expected HTTP response.

## Regression coverage

The detector tests cover malformed metadata, conflicting lockfiles,
unsupported managers, unknown entry points, Express port review, separate
npm/pnpm argument forwarding, bounded output and a real NTFS junction.
Configuration tests verify a read-only preview, preservation of comments
and advanced fields, and refusal of stale revisions. The native forget
test covers active exec and service refusal, publication refusal, SQLite
data retention and reimport. The store tests cover schema 8-to-9 migration
and transactional rollback.

The first native suite exposed a shutdown fault after the PHP rollout
fixture stopped its processes. Its client timed out before receiving the
Stop reply and the engine kept listening. A second Stop against that
disposable engine let the test finish. The engine now exits after a
successful shutdown even if delivering the reply fails. A regression test
closes the client after sending Stop and verifies that the server exits.

The final `cargo test --workspace --offline` run passed all 158 tests
with the Windows native fixtures enabled.
`cargo clippy --workspace --all-targets --offline -- -D warnings`,
`pnpm --dir apps/desktop test` and `pnpm --dir apps/desktop run build`
passed. See [detection and ownership rules](../decisions/0010-import-configuration.md)
for the supported evidence, manual fallback and retained runtime caveat.
