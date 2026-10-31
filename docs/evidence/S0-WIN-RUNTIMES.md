# S0-WIN-RUNTIMES

Date: 2026-10-31 23:20:46 +03:00

Host: Windows 10 Pro, build 19045, version 22H2, 64-bit. PowerShell 5.1.19041.6456. Architecture x64.

This run downloaded publisher payloads, compared them with publisher digests, and executed them under `D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES`. macOS was not tested. The product was not built. `C:\Program Files\nodejs` and `C:\Python\Python312` were executed in place and left unchanged.

Structured result: `.artifacts/S0-WIN-RUNTIMES/result.json`. Command log: `.artifacts/S0-WIN-RUNTIMES/logs/commands.txt`.

## Command

```text
powershell.exe -NoProfile -ExecutionPolicy Bypass -File D:\_work\laragon_alternative\scripts\verification\windows\runtimes.ps1 -Action Verify
```

Outcome: exit 0. 16 checks passed, 0 failed.

The default action `Help` prints usage and does not download or create the artifact directory. That was confirmed before this run.

## Publisher digests

Downloaded bytes matched the publisher digest in every case. `docs/verification-evidence.json` agreed with the publisher for Node, uv, and Python. That file has no pnpm observation. Expected digests came from the publisher at run time, not from the JSON.

| Component | Version | URL | SHA-256 | Publisher digest source |
| --- | --- | --- | --- | --- |
| Node.js | 24.22.2 | https://nodejs.org/dist/v24.22.2/node-v24.22.2-win-x64.zip | `ca1bff5d8ead13c9f2ea404a176db46bd430704d5b5113734049fa365e7799e2` | https://nodejs.org/dist/v24.22.2/SHASUMS256.txt |
| CPython | 3.12.16 build 20261026 install_only_stripped | https://github.com/astral-sh/python-build-standalone/releases/download/20261026/cpython-3.12.16%2B20261026-x86_64-pc-windows-msvc-install_only_stripped.tar.gz | `1f2ab53880fc67ea323398c6388e05af28addc99a8e389209f08a51fdff4b149` | https://github.com/astral-sh/python-build-standalone/releases/download/20261026/SHA256SUMS |
| uv | 0.13.6 | https://github.com/astral-sh/uv/releases/download/0.13.6/uv-x86_64-pc-windows-msvc.zip | `a2a8820d5eea32f08ef467ef3b7f66883a9b076d5737a2fc2fbb5f458d1bb50f` | https://github.com/astral-sh/uv/releases/download/0.13.6/sha256.sum |
| pnpm | 12.16.1 | https://github.com/pnpm/pnpm/releases/download/v12.16.1/pnpm-win32-x64.zip | `253cd5caa8cdf3859a92995bdf1819d3b433690157616cc372af00d22eb0c6a4` | https://api.github.com/repos/pnpm/pnpm/releases/tags/v12.16.1 asset digest for `pnpm-win32-x64.zip` |

Node's immutable `v24.22.2/SHASUMS256.txt` matches the previously recorded floating `latest-v24.x` hash. Python's `SHA256SUMS` matches the GitHub release asset digest `sha256:1f2ab53880fc67ea323398c6388e05af28addc99a8e389209f08a51fdff4b149`. uv's `sha256.sum` matches the GitHub asset digest `sha256:a2a8820d5eea32f08ef467ef3b7f66883a9b076d5737a2fc2fbb5f458d1bb50f`. The uv release also publishes `uv-x86_64-pc-windows-msvc.zip.sha256`.

pnpm v12.16.1 is the official `pnpm/pnpm` GitHub release read for this probe (published 2026-10-27). The script pins tag `v12.16.1` and does not call `/releases/latest`. The release asset list has no separate checksum file. The GitHub asset digest field was present, and the downloaded zip matched it before `pnpm.exe` ran.

Authenticode and PGP signatures were not checked.

## Managed Node

Digest check passed before extract. The zip was extracted under a directory whose path contains a space, then the tree was moved and run again.

Space-path launch, exit 0, stdout `v24.22.2`:

```text
"D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES\path with spaces\node\node-v24.22.2-win-x64\node.exe" -v
```

A second command, `node -e "process.stdout.write('node-ok')"`, exited 0 and printed `node-ok`.

After moving the tree to `.artifacts/S0-WIN-RUNTIMES/relocated/node`, the same `node -v` exited 0 and printed `v24.22.2`. The original space-path `node.exe` was no longer present.

## Managed Python, venv, and pip

Archive: CPython 3.12.16+20261026 `install_only_stripped` for `x86_64-pc-windows-msvc`. Digest check passed before `tar.exe` ran. Entry names were listed and rejected if they contained `..` or an absolute path. Extraction used a relative archive path so the Windows `tar.exe` arguments did not include a drive-letter colon.

Space-path launch, exit 0:

```text
"D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES\path with spaces\python\python\python.exe" -c "import ssl, sqlite3, venv, sys; print(sys.version); print(\"imports-ok\")"
```

Stdout began `3.12.16 (main, Oct 26 2026, 14:10:56) [MSC v.1944 64 bit (AMD64)]` and included `imports-ok`.

The tree was moved to `.artifacts/S0-WIN-RUNTIMES/relocated/python`. The same import command there exited 0 with the same version line.

Virtual environment, exit 0, created only under the artifact root from the relocated interpreter:

```text
D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES\relocated\python\python\python.exe -m venv D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES\venvs\pip-smoke
```

pip used that venv's `Scripts\python.exe`, with `PIP_CACHE_DIR` and `PIP_CONFIG_FILE` under the artifact root and index `https://pypi.org/simple`. Exit 0:

```text
D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES\venvs\pip-smoke\Scripts\python.exe -m pip install six==1.17.0 --disable-pip-version-check --no-warn-script-location --index-url https://pypi.org/simple
```

`python -c "import six; print(six.__version__)"` in that venv printed `1.17.0`. The system interpreter was not used for pip.

## uv

`uv.exe` was extracted under `.artifacts/S0-WIN-RUNTIMES/path with spaces/uv` only after the digest matched. The process environment set `UV_CACHE_DIR`, `UV_PYTHON_INSTALL_DIR`, and `UV_TOOL_DIR` under `.artifacts/S0-WIN-RUNTIMES/uv-home`, plus `UV_PYTHON_DOWNLOADS=never` and `UV_NO_CACHE=1`. No Python download was requested.

```text
"D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES\path with spaces\uv\uv.exe" --version
```

Exit 0. Stdout: `uv 0.13.6 (329bf9448 2026-10-26 x86_64-pc-windows-msvc)`.

## pnpm

Pinned release v12.16.1, Windows x64 zip. `pnpm.exe` ran from `.artifacts/S0-WIN-RUNTIMES/path with spaces/pnpm`. The managed Node directory was prepended to `Path` only in that process and in the follow-up `node` process. Corepack was not enabled. `npm install -g` was not used. The store, `PNPM_HOME`, and npm cache were under the artifact root. An isolated user npmrc was passed so the user npmrc was not required.

`pnpm --version` printed `12.16.1`.

Install, exit 0:

```text
"D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES\path with spaces\pnpm\pnpm.exe" install --store-dir D:\_work\laragon_alternative\.artifacts\S0-WIN-RUNTIMES\pnpm-store --registry https://registry.npmjs.org/ --ignore-scripts
```

Working directory: `.artifacts/S0-WIN-RUNTIMES/path with spaces/pnpm-project`, with `is-number` pinned at `7.0.0`. `pnpm-lock.yaml` is present there (417 bytes) and names `is-number` `7.0.0`. The managed Node then ran `require('is-number')('1')` in that project and printed `true`. The project stays under `.artifacts` and is not in Git.

## Local reuse

Existing installs were executed by full path. They were not copied, upgraded, or replaced. `PYTHONDONTWRITEBYTECODE=1` was set for the Python processes.

Before the managed installs, and again after them:

```text
"C:\Program Files\nodejs\node.exe" -v
```

Exit 0, stdout `v24.13.0`. `node -e "process.stdout.write('reuse-ok')"` exited 0 and printed `reuse-ok`.

```text
C:\Python\Python312\python.exe -c "import sys; print(sys.version)"
```

Exit 0, stdout `3.12.3 (tags/v3.12.3:f6650f9, Apr  9 2024, 14:05:25) [MSC v.1938 64 bit (AMD64)]`. A second command imported `ssl`, `sqlite3`, and `venv` and printed `reuse-ok` (exit 0). The after-check still reported `v24.13.0` and `3.12.3`.

## Path with a space and relocation

Node, Python, uv, and pnpm were extracted and launched from `.artifacts/S0-WIN-RUNTIMES/path with spaces`. Node and Python were then moved to `.artifacts/S0-WIN-RUNTIMES/relocated` and launched again. Both relocated launches exited 0.

## Failure paths

Both guards ran and rejected their inputs before creating an install directory.

Unsafe zip entry `../escape.txt`: `Expand-ZipSafe` returned `unsafe zip entry: ../escape.txt`. The escape file was not written, and the destination directory was not created.

Unsafe absolute zip entry `D:/_work/laragon_alternative/.artifacts/S0-WIN-RUNTIMES/synthetic/abs-marker/pwned.txt`: rejected the same way. The marker file was not written. The absolute path stays inside the artifact root so a faulty extractor cannot write outside the disposable directory.

A safe synthetic zip with `nested/hello.txt` extracted and the file contents were `hello`. That fixture is not a runtime archive.

Bad digest: local file `synthetic/digest-mismatch/payload.bin` has SHA-256 `69f67b87257682f7aa7ca805297ac372c1b7f16e604fa2a4c2f8b0c9bb527ca1`. The gate expected 64 zero hex digits, returned a mismatch, and did not create `synthetic/digest-mismatch/install`.

## Cleanup

Every child process was waited on until exit. None of these checks timed out. The artifact directory `.artifacts/S0-WIN-RUNTIMES` was retained for review, including downloads, checksum files, extracts, the relocated trees, the pip venv, and the pnpm project with its lockfile. This evidence file was not deleted.

## Limitations

- Digest comparison only. Authenticode and PGP signatures were not checked.
- macOS 14.5 arm64 was not tested. These results do not accept the macOS artifacts.
- uv was limited to `uv --version` with Python downloads disabled. It did not install a Python or run a uv-managed project.
- pnpm installed one pinned package, `is-number@7.0.0`, with `--ignore-scripts`. This is not a Next.js fixture.
- pip installed `six==1.17.0` into the disposable venv. pip does not record that install in a project requirements file.
- Apache, MySQL, and PostgreSQL were not started.
- The floating Node index was not used as the download URL. The immutable `v24.22.2` URL was used, and its checksum matched the previously recorded floating-index hash.
