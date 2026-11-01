# S2-PACKAGES

Date: 1 November 2026.

Target: Windows 10 Pro, build 19045, x64. Disposable data directory: `.artifacts/S2-PACKAGES`. The catalog is `catalog/packages.json`. macOS packages are not in that file.

## Commands

```text
cargo test --workspace --exclude localdev-desktop
cargo run -q -p localdev-cli -- --data-dir .artifacts/S2-PACKAGES discover
cargo run -q -p localdev-cli -- --data-dir .artifacts/S2-PACKAGES use-existing python "C:\Python\Python312\python.exe"
cargo run -q -p localdev-cli -- --data-dir .artifacts/S2-PACKAGES install node-24.22.2-windows-x64
cargo run -q -p localdev-cli -- --data-dir .artifacts/S2-PACKAGES install cpython-3.12.16-windows-x64
cargo run -q -p localdev-cli -- --data-dir .artifacts/S2-PACKAGES install uv-0.13.6-windows-x64
cargo run -q -p localdev-cli -- --data-dir .artifacts/S2-PACKAGES install pnpm-12.16.1-windows-x64
cargo run -q -p localdev-cli -- --data-dir .artifacts/S2-PACKAGES remove existing-python
```

`cargo test` passed, including a bad digest, a cancelled download, an unsafe zip entry, and refusal to delete a selected package or an existing-install reference.

## Recorded packages

| Id | Source | Version | Launch |
| --- | --- | --- | --- |
| node-24.22.2-windows-x64 | managed | 24.22.2 | `.artifacts/S2-PACKAGES/packages/node/24.22.2/node-v24.22.2-win-x64/node.exe` |
| cpython-3.12.16-windows-x64 | managed | 3.12.16+20261026 | `.artifacts/S2-PACKAGES/packages/python/3.12.16+20261026/python/python.exe` |
| existing-python | existing | 3.12.3 | `C:\Python\Python312\python.exe` |
| uv-0.13.6-windows-x64 | managed | 0.13.6 | `.artifacts/S2-PACKAGES/tools/uv/0.13.6/uv.exe` |
| pnpm-12.16.1-windows-x64 | managed | 12.16.1 | `.artifacts/S2-PACKAGES/tools/pnpm/12.16.1/pnpm.exe` |

Downloads reported matching sizes: Node 37754834 bytes, CPython 22014416, uv 15239500, pnpm 23779267. Each install state was `succeeded`.

## Launch checks

- Managed Node printed `v24.22.2`.
- Managed Python printed `3.12.16`. Its `python -m pip --version` printed `pip 26.2.2` from that package's `Lib\site-packages`, not from `C:\Python\Python312`.
- Existing Python still printed `3.12.3`.
- `uv --version` printed `uv 0.13.6` with `UV_PYTHON_DOWNLOADS=never` and `UV_CACHE_DIR` under the artifact directory.
- `pnpm --version` printed `12.16.1` with only that command's `PATH` prepended by the managed Node directory.

`remove existing-python` was refused: `package existing-python cannot be removed: existing installations are referenced, not owned`.

## Not done

- macOS catalog entries and installs
- Apache, MySQL, and PostgreSQL providers
- project manifests beyond the `default` selection row
- a live second Windows user opening the pipe
