#!/usr/bin/env bash
# S0-MAC-PACKAGES
#
# Execution has not been run. This script is pending verification on the
# MacBook M1 Pro, 16 GB, macOS 14.5 arm64. Pins below were fetched on
# 31 October 2026 from Windows. A syntax check or a refused run on Windows
# is not a macOS pass, and a probe result is not runnable-catalog approval.
#
# The script does not register OS services, does not edit /etc/hosts, and
# does not change the global PATH. It does not use sudo. Server components
# with no approved relocatable archive stay refused, so they never bind a
# port. Resolved runtime probes bind 127.0.0.1 only, on a high port, and
# close the socket before exit.
#
# Compatible with macOS system bash 3.2 (no associative arrays, no ${var,,}).

set -eu
set -o pipefail
export LC_ALL=C

NODE_VERSION="24.22.2"
NODE_FILE="node-v${NODE_VERSION}-darwin-arm64.tar.gz"
NODE_URL="https://nodejs.org/dist/v${NODE_VERSION}/${NODE_FILE}"
NODE_SUMS_URL="https://nodejs.org/dist/v${NODE_VERSION}/SHASUMS256.txt"
NODE_SHA256="aa9549349e47dffaa42dcf95d6e06ade8c4bdafc980979883b88767670df385a"

UV_VERSION="0.13.6"
UV_FILE="uv-aarch64-apple-darwin.tar.gz"
UV_URL="https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/${UV_FILE}"
UV_SHA_URL="https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/${UV_FILE}.sha256"
UV_SHA256="60fbd5c8a1ed681845eea28d9d622fcde03c51e91a43465b64a1e17db1ed3472"

PY_FILE="cpython-3.12.16+20261026-aarch64-apple-darwin-install_only_stripped.tar.gz"
PY_URL="https://github.com/astral-sh/python-build-standalone/releases/download/20261026/cpython-3.12.16%2B20261026-aarch64-apple-darwin-install_only_stripped.tar.gz"
PY_SUMS_URL="https://github.com/astral-sh/python-build-standalone/releases/download/20261026/SHA256SUMS"
PY_SHA256="353d5f1b4fd2715e7053f12f068435d5bbad16b91e057006b5d01e0925709a7e"

PNPM_VERSION="12.16.1"
PNPM_FILE="pnpm-darwin-arm64.tar.gz"
PNPM_URL="https://github.com/pnpm/pnpm/releases/download/v${PNPM_VERSION}/${PNPM_FILE}"
PNPM_API_URL="https://api.github.com/repos/pnpm/pnpm/releases/tags/v${PNPM_VERSION}"
PNPM_SHA256="6661212a6ec725634b0d3268e5bc9408e3152506dab5997891e38fa1ef22bf9c"

# Observed 31 October 2026 and intentionally not downloaded: the EDB page
# redirects the Mac OS X binaries link for installer 18.6 to this zip and
# publishes no digest. PostgreSQL probes refuse before any transfer.
PG_OBSERVED_URL="https://get.enterprisedb.com/postgresql/postgresql-18.6-5-osx-binaries.zip"

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "${SCRIPT_DIR}/../../.." && pwd)
DEFAULT_ARTIFACT_ROOT="${REPO_ROOT}/.artifacts/S0-MAC-PACKAGES"

ARTIFACT_ROOT=""
COMMAND=""
ONLY=""
PROJECT=""
PROBE_PORT="18080"
HELP_EXIT="0"

usage() {
  cat <<EOF
S0-MAC-PACKAGES smoke script (pending MacBook M1 Pro, macOS 14.5 arm64)

This file has not been executed on macOS. Help is the default and does not
download, extract, or launch anything. Checksum and probe commands succeed
only when uname is Darwin arm64 and sw_vers ProductVersion is 14.5 or 14.5.x.
They re-fetch the publisher checksum (or the pnpm GitHub asset digest) and
compare it to the pin from 31 October 2026 before download, extract, or exec.

Artifact root
  --artifact-root PATH   disposable directory (created only after the OS check)
                         default: ${DEFAULT_ARTIFACT_ROOT}
                         that path is gitignored; do not commit downloads

Commands
  help                   show this text (default)
  checksum-compare       verify pins and payload digests; do not launch
  probe-node             Node ${NODE_VERSION} version plus one loopback bind
  probe-python           CPython 3.12.16 venv and ssl/sqlite3 imports
  probe-uv               uv ${UV_VERSION} version, cache kept in the artifact root
  probe-pnpm             pnpm ${PNPM_VERSION} version, home kept in the artifact root
  pnpm-install           run the pinned pnpm against --project (no global install)
  probe-apache           refused: no relocatable macOS arm64 archive
  probe-mysql            refused: macOS arm64 archive URL unresolved
  probe-postgres         refused: zip URL has no published digest

Options
  --only NAME            checksum-compare one of: node, uv, python, pnpm
  --project PATH         project directory for pnpm-install
  --port N               first loopback port for the Node bind probe (default ${PROBE_PORT})

The Node probe listens on 127.0.0.1 only, at --port or the next free high
port, then closes the socket. It does not edit /etc/hosts and does not
register a launchd service. Apache, MySQL, and PostgreSQL commands exit
before opening a socket.

Exit codes
  0  help, or a checksum/probe that matched on Darwin arm64 macOS 14.5
  2  usage, or uname is not Darwin arm64 (no success claimed)
  3  digest mismatch, or a digest the publisher did not publish
  4  archive URL unresolved or packaging gate
  5  Darwin arm64 whose ProductVersion is not 14.5

Resolved pins (metadata only until this script is run on the Mac)
  node  ${NODE_SHA256}
        ${NODE_URL}
        sums ${NODE_SUMS_URL}
  uv    ${UV_SHA256}
        ${UV_URL}
        sums ${UV_SHA_URL}
  python ${PY_SHA256}
        ${PY_URL}
        sums ${PY_SUMS_URL}
  pnpm  ${PNPM_SHA256}
        ${PNPM_URL}
        digest field of ${PNPM_API_URL}
        (v${PNPM_VERSION} assets include no maintainer SHA256SUMS file)
EOF
}

die() {
  printf '%s\n' "$1" >&2
  exit "$2"
}

report_host() {
  printf 'task=S0-MAC-PACKAGES\n'
  printf 'expected_target=MacBook M1 Pro, 16 GB, macOS 14.5 arm64\n'
  printf 'uname=%s %s\n' "$(uname -s)" "$(uname -m)"
  printf 'catalog=not-approved\n'
}

require_named_target() {
  local sys arch ver
  sys=$(uname -s)
  arch=$(uname -m)
  printf 'task=S0-MAC-PACKAGES\n'
  printf 'expected_target=MacBook M1 Pro, 16 GB, macOS 14.5 arm64\n'
  printf 'uname=%s %s\n' "$sys" "$arch"
  if [ "$sys" != "Darwin" ] || [ "$arch" != "arm64" ]; then
    printf 'result=refused\n'
    printf 'catalog=not-approved\n'
    printf 'reason=uname is not Darwin arm64\n'
    die "refusing to claim success: uname is ${sys} ${arch}" 2
  fi
  ver=$(sw_vers -productVersion)
  printf 'product_version=%s\n' "$ver"
  case "$ver" in
    14.5|14.5.*)
      printf 'named_target_match=yes\n'
      ;;
    *)
      printf 'result=refused\n'
      printf 'named_target_match=no\n'
      printf 'catalog=not-approved\n'
      die "refusing to claim success: ProductVersion is ${ver}, expected 14.5" 5
      ;;
  esac
}

prepare_root() {
  if [ -z "$ARTIFACT_ROOT" ]; then
    ARTIFACT_ROOT="$DEFAULT_ARTIFACT_ROOT"
  fi
  case "$ARTIFACT_ROOT" in
    ""|/|.)
      die "refusing artifact root: ${ARTIFACT_ROOT}" 2
      ;;
    *$'\n'*)
      die "refusing artifact root with a newline" 2
      ;;
  esac
  mkdir -p "$ARTIFACT_ROOT/downloads" "$ARTIFACT_ROOT/extract" "$ARTIFACT_ROOT/home" \
    "$ARTIFACT_ROOT/cache" "$ARTIFACT_ROOT/config" "$ARTIFACT_ROOT/data" \
    "$ARTIFACT_ROOT/pnpm-home" "$ARTIFACT_ROOT/uv-cache" "$ARTIFACT_ROOT/results"
  ARTIFACT_ROOT=$(CDPATH= cd -- "$ARTIFACT_ROOT" && pwd)
  export HOME="${ARTIFACT_ROOT}/home"
  export XDG_CACHE_HOME="${ARTIFACT_ROOT}/cache"
  export XDG_CONFIG_HOME="${ARTIFACT_ROOT}/config"
  export XDG_DATA_HOME="${ARTIFACT_ROOT}/data"
  export PNPM_HOME="${ARTIFACT_ROOT}/pnpm-home"
  export UV_CACHE_DIR="${ARTIFACT_ROOT}/uv-cache"
  export UV_PYTHON_INSTALL_DIR="${ARTIFACT_ROOT}/extract/uv-python"
  export npm_config_cache="${ARTIFACT_ROOT}/cache/npm"
  mkdir -p "$UV_PYTHON_INSTALL_DIR" "$npm_config_cache"
  printf 'artifact_root=%s\n' "$ARTIFACT_ROOT"
}

download_file() {
  local url="$1" dest="$2" tmp
  tmp="${dest}.partial"
  rm -f "$tmp"
  curl --fail --silent --show-error --location --retry 3 --retry-delay 2 \
    --output "$tmp" "$url"
  mv "$tmp" "$dest"
}

file_sha256() {
  shasum -a 256 "$1" | awk 'NR==1 { print $1 }' | tr 'A-F' 'a-f'
}

expect_hash() {
  local got="$1" pin="$2" label="$3"
  got=$(printf '%s' "$got" | tr 'A-F' 'a-f')
  if [ "$got" != "$pin" ]; then
    printf 'component=%s\nresult=digest-mismatch\nobserved=%s\npinned=%s\ncatalog=not-approved\n' \
      "$label" "$got" "$pin"
    die "${label} digest mismatch" 3
  fi
}

sums_hash_for() {
  local sums="$1" filename="$2"
  awk -v name="$filename" '
    {
      hash=$1
      file=$2
      sub(/^\*/, "", file)
      if (file == name) { print hash; found=1; exit }
    }
    END { if (!found) exit 1 }
  ' "$sums" | tr 'A-F' 'a-f'
}

verify_payload() {
  local label="$1" filename="$2" url="$3" pin="$4" payload got
  payload="${ARTIFACT_ROOT}/downloads/${filename}"
  if [ -f "$payload" ]; then
    got=$(file_sha256 "$payload")
    if [ "$got" != "$pin" ]; then
      rm -f "$payload"
      printf 'component=%s\nresult=removed-bad-payload\nobserved=%s\n' "$label" "$got" >&2
    else
      printf 'component=%s\nresult=checksum-ok\nsha256=%s\npayload=%s\ncatalog=not-approved\n' \
        "$label" "$pin" "$payload"
      return 0
    fi
  fi
  printf 'component=%s\nresult=downloading\nurl=%s\n' "$label" "$url" >&2
  download_file "$url" "$payload"
  expect_hash "$(file_sha256 "$payload")" "$pin" "$label"
  printf 'component=%s\nresult=checksum-ok\nsha256=%s\npayload=%s\ncatalog=not-approved\n' \
    "$label" "$pin" "$payload"
}

reject_unsafe_tar() {
  local archive="$1" entry
  while IFS= read -r entry; do
    entry=${entry%$'\r'}
    case "$entry" in
      "" ) continue ;;
      /*|../*|*/../*|*/..|..)
        die "unsafe tar member: ${entry}" 3
        ;;
      *\\*)
        die "unsafe tar member: ${entry}" 3
        ;;
    esac
  done <<EOF
$(tar -tzf "$archive")
EOF
}

extract_verified() {
  local label="$1" filename="$2" pin="$3" dest marker payload
  dest="${ARTIFACT_ROOT}/extract/${label}"
  marker="${dest}/.verified-sha256"
  payload="${ARTIFACT_ROOT}/downloads/${filename}"
  if [ -f "$marker" ] && [ "$(cat "$marker")" = "$pin" ]; then
    printf 'component=%s\nresult=extract-reused\n' "$label"
    return 0
  fi
  [ -f "$payload" ] || die "${label} payload missing after checksum" 3
  expect_hash "$(file_sha256 "$payload")" "$pin" "$label"
  reject_unsafe_tar "$payload"
  rm -rf "$dest"
  mkdir -p "$dest"
  tar -xzf "$payload" -C "$dest"
  printf '%s\n' "$pin" > "$marker"
  printf 'component=%s\nresult=extracted\n' "$label"
}

first_file() {
  local candidate
  for candidate in "$@"; do
    if [ -f "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

ensure_exec() {
  local path="$1"
  [ -f "$path" ] || return 1
  chmod u+x "$path"
  [ -x "$path" ]
}

verify_node() {
  local sums got
  sums="${ARTIFACT_ROOT}/downloads/node-SHASUMS256.txt"
  download_file "$NODE_SUMS_URL" "$sums"
  got=$(sums_hash_for "$sums" "$NODE_FILE") || die "node checksum file has no ${NODE_FILE}" 3
  expect_hash "$got" "$NODE_SHA256" "node-sums"
  verify_payload "node" "$NODE_FILE" "$NODE_URL" "$NODE_SHA256"
}

verify_uv() {
  local side got
  side="${ARTIFACT_ROOT}/downloads/${UV_FILE}.sha256"
  download_file "$UV_SHA_URL" "$side"
  got=$(sums_hash_for "$side" "$UV_FILE") || die "uv checksum file has no ${UV_FILE}" 3
  expect_hash "$got" "$UV_SHA256" "uv-sums"
  verify_payload "uv" "$UV_FILE" "$UV_URL" "$UV_SHA256"
}

verify_python() {
  local sums got
  sums="${ARTIFACT_ROOT}/downloads/python-SHA256SUMS"
  download_file "$PY_SUMS_URL" "$sums"
  got=$(sums_hash_for "$sums" "$PY_FILE") || die "python checksum file has no ${PY_FILE}" 3
  expect_hash "$got" "$PY_SHA256" "python-sums"
  verify_payload "python" "$PY_FILE" "$PY_URL" "$PY_SHA256"
}

pnpm_api_digest() {
  local body="${ARTIFACT_ROOT}/downloads/pnpm-release.json"
  download_file "$PNPM_API_URL" "$body"
  awk '
    {
      s = $0
      key = "\"name\"[[:space:]]*:[[:space:]]*\"pnpm-darwin-arm64.tar.gz\""
      if (match(s, key)) {
        rest = substr(s, RSTART, 4000)
        if (match(rest, "\"digest\"[[:space:]]*:[[:space:]]*\"sha256:[0-9a-fA-F]+\"")) {
          d = substr(rest, RSTART, RLENGTH)
          sub(/^.*sha256:/, "", d)
          sub(/"$/, "", d)
          print d
          found = 1
        }
      }
    }
    END { if (!found) exit 1 }
  ' "$body" | tr 'A-F' 'a-f'
}

verify_pnpm() {
  local got
  got=$(pnpm_api_digest) || die "pnpm release API has no darwin-arm64 digest" 3
  expect_hash "$got" "$PNPM_SHA256" "pnpm-api"
  verify_payload "pnpm" "$PNPM_FILE" "$PNPM_URL" "$PNPM_SHA256"
}

node_bin() {
  first_file \
    "${ARTIFACT_ROOT}/extract/node/node-v${NODE_VERSION}-darwin-arm64/bin/node" \
    "${ARTIFACT_ROOT}/extract/node/bin/node"
}

uv_bin() {
  local candidate dir
  if candidate=$(first_file "${ARTIFACT_ROOT}/extract/uv/uv"); then
    printf '%s\n' "$candidate"
    return 0
  fi
  for dir in "${ARTIFACT_ROOT}/extract/uv"/*; do
    if [ -f "${dir}/uv" ]; then
      printf '%s\n' "${dir}/uv"
      return 0
    fi
  done
  return 1
}

python_bin() {
  local candidate dir
  if candidate=$(first_file \
    "${ARTIFACT_ROOT}/extract/python/python/bin/python3" \
    "${ARTIFACT_ROOT}/extract/python/bin/python3"); then
    printf '%s\n' "$candidate"
    return 0
  fi
  for dir in "${ARTIFACT_ROOT}/extract/python"/*; do
    if [ -f "${dir}/bin/python3" ]; then
      printf '%s\n' "${dir}/bin/python3"
      return 0
    fi
  done
  return 1
}

pnpm_bin() {
  local candidate dir
  if candidate=$(first_file "${ARTIFACT_ROOT}/extract/pnpm/pnpm"); then
    printf '%s\n' "$candidate"
    return 0
  fi
  for dir in "${ARTIFACT_ROOT}/extract/pnpm"/*; do
    if [ -f "${dir}/pnpm" ]; then
      printf '%s\n' "${dir}/pnpm"
      return 0
    fi
  done
  return 1
}

use_bin() {
  local label="$1" path="$2"
  if [ -z "$path" ] || [ ! -f "$path" ] || ! ensure_exec "$path"; then
    printf 'component=%s\nresult=binary-missing\nextract=%s\n' \
      "$label" "${ARTIFACT_ROOT}/extract/${label}" >&2
    ls -1 "${ARTIFACT_ROOT}/extract/${label}" >&2 || true
    die "${label} executable not found after extract" 3
  fi
}

probe_node() {
  local bin listen_log
  verify_node
  extract_verified "node" "$NODE_FILE" "$NODE_SHA256"
  bin=$(node_bin || true)
  use_bin "node" "$bin"
  listen_log="${ARTIFACT_ROOT}/results/node-listen.txt"
  "$bin" -p 'process.version + " " + process.platform + " " + process.arch'
  PROBE_PORT="$PROBE_PORT" "$bin" -e '
    const net = require("net");
    const start = Number(process.env.PROBE_PORT || "18080");
    function tryListen(port, left) {
      const server = net.createServer();
      server.once("error", (err) => {
        if (err && err.code === "EADDRINUSE" && left > 0) {
          tryListen(port + 1, left - 1);
          return;
        }
        console.error(err && err.code ? err.code : String(err));
        process.exit(1);
      });
      server.listen({ host: "127.0.0.1", port: port, exclusive: true }, () => {
        const addr = server.address();
        if (!addr || addr.address !== "127.0.0.1" || addr.port < 1024) {
          console.error("refusing non-loopback or privileged port");
          process.exit(1);
        }
        console.log("listen=127.0.0.1:" + addr.port);
        server.close();
      });
    }
    if (!Number.isFinite(start) || start < 1024 || start > 65535) {
      console.error("port out of range");
      process.exit(1);
    }
    tryListen(start, 20);
  ' | tee "$listen_log"
  printf 'component=node\nresult=probe-ok\ncatalog=not-approved\n'
}

probe_python() {
  local bin venv
  verify_python
  extract_verified "python" "$PY_FILE" "$PY_SHA256"
  bin=$(python_bin || true)
  use_bin "python" "$bin"
  "$bin" -c 'import platform,sys; print(sys.version); print(platform.platform()); print(platform.machine())'
  venv="${ARTIFACT_ROOT}/venvs/cpython-probe"
  rm -rf "$venv"
  "$bin" -m venv "$venv"
  "${venv}/bin/python" -c 'import sqlite3,ssl,sys; print(sys.version); print(ssl.OPENSSL_VERSION); print(sqlite3.sqlite_version)'
  "${venv}/bin/python" -m pip --version
  printf 'component=python\nresult=probe-ok\ncatalog=not-approved\n'
}

probe_uv() {
  local bin
  verify_uv
  extract_verified "uv" "$UV_FILE" "$UV_SHA256"
  bin=$(uv_bin || true)
  use_bin "uv" "$bin"
  "$bin" --version
  printf 'component=uv\nresult=probe-ok\ncatalog=not-approved\n'
}

probe_pnpm() {
  local bin
  verify_pnpm
  extract_verified "pnpm" "$PNPM_FILE" "$PNPM_SHA256"
  bin=$(pnpm_bin || true)
  use_bin "pnpm" "$bin"
  "$bin" --version
  printf 'component=pnpm\nresult=probe-ok\ncatalog=not-approved\n'
}

cmd_pnpm_install() {
  local bin
  [ -n "$PROJECT" ] || die "pnpm-install requires --project" 2
  [ -f "${PROJECT}/package.json" ] || die "no package.json in ${PROJECT}" 2
  PROJECT=$(CDPATH= cd -- "$PROJECT" && pwd)
  verify_pnpm
  extract_verified "pnpm" "$PNPM_FILE" "$PNPM_SHA256"
  bin=$(pnpm_bin || true)
  use_bin "pnpm" "$bin"
  mkdir -p "${ARTIFACT_ROOT}/pnpm-store"
  "$bin" install --dir "$PROJECT" --store-dir "${ARTIFACT_ROOT}/pnpm-store"
  printf 'component=pnpm\nresult=install-finished\nproject=%s\ncatalog=not-approved\n' "$PROJECT"
}

refuse_apache() {
  printf 'component=apache\nresult=unresolved\nurl=unresolved\ndigest=unresolved\ncatalog=not-approved\n'
  printf 'reason=no official relocatable macOS 14.5 arm64 httpd archive\n'
  cat <<'EOF' >&2
Apache packaging gate: build a relocatable arm64 httpd plus apr, apr-util,
pcre2, OpenSSL, nghttp2, and brotli, with relative Mach-O install names.
ASF publishes source only. Apache Lounge publishes Windows binaries only.
Homebrew bottles use /opt/homebrew/Cellar and the 31 October 2026 httpd
bottle list has no arm64_sonoma entry. An existing local Apache may be
referenced later for prototype work and does not satisfy managed-download
acceptance. This command does not build or listen.
EOF
  exit 4
}

refuse_mysql() {
  printf 'component=mysql\nresult=unresolved\nurl=unresolved\ndigest=unresolved\ncatalog=not-approved\n'
  printf 'reason=macOS arm64 archive URL was not on the pages this session could read\n'
  cat <<'EOF' >&2
MySQL 8.4 is the candidate family. The supported-platforms page marks 8.4 LTS
for macOS arm64 under a latest-two-major-releases policy and does not name
14.5. The downloads and archives pages require an operating-system selector
that was not completed here. No archive URL is invented. MariaDB is not a
substitute. This command does not listen and does not register a service.
EOF
  exit 4
}

refuse_postgres() {
  printf 'component=postgresql\nresult=blocked\nurl=%s\ndigest=unresolved\ncatalog=not-approved\n' \
    "$PG_OBSERVED_URL"
  printf 'reason=publisher archive has no published digest\n'
  cat <<EOF >&2
EDB exposes one Mac OS X zip for installer binaries 18.6. A HEAD request on
31 October 2026 redirected fileid 1260613 to ${PG_OBSERVED_URL} and returned
no sha256. The filename does not say arm64. Download and launch stay disabled.
This command does not listen and does not register a service.
EOF
  exit 3
}

checksum_compare() {
  case "$ONLY" in
    "")
      verify_node
      verify_uv
      verify_python
      verify_pnpm
      ;;
    node) verify_node ;;
    uv) verify_uv ;;
    python) verify_python ;;
    pnpm) verify_pnpm ;;
    apache) refuse_apache ;;
    mysql) refuse_mysql ;;
    postgres|postgresql) refuse_postgres ;;
    *)
      die "unknown --only ${ONLY}" 2
      ;;
  esac
  printf 'result=checksum-compare-ok\ncatalog=not-approved\n'
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      -h|--help|help)
        COMMAND="help"
        shift
        ;;
      --artifact-root)
        [ $# -ge 2 ] || die "--artifact-root requires a path" 2
        ARTIFACT_ROOT="$2"
        shift 2
        ;;
      --only)
        [ $# -ge 2 ] || die "--only requires a component" 2
        ONLY="$2"
        shift 2
        ;;
      --project)
        [ $# -ge 2 ] || die "--project requires a path" 2
        PROJECT="$2"
        shift 2
        ;;
      --port)
        [ $# -ge 2 ] || die "--port requires a number" 2
        PROBE_PORT="$2"
        shift 2
        ;;
      checksum-compare|probe-node|probe-python|probe-uv|probe-pnpm|pnpm-install|probe-apache|probe-mysql|probe-postgres)
        COMMAND="$1"
        shift
        ;;
      *)
        printf 'unknown argument: %s\n' "$1" >&2
        HELP_EXIT=2
        COMMAND="help"
        shift
        ;;
    esac
  done
  if [ -z "$COMMAND" ]; then
    COMMAND="help"
  fi
}

main() {
  parse_args "$@"
  case "$COMMAND" in
    help)
      usage
      exit "$HELP_EXIT"
      ;;
    checksum-compare)
      require_named_target
      prepare_root
      checksum_compare
      ;;
    probe-node)
      require_named_target
      prepare_root
      probe_node
      ;;
    probe-python)
      require_named_target
      prepare_root
      probe_python
      ;;
    probe-uv)
      require_named_target
      prepare_root
      probe_uv
      ;;
    probe-pnpm)
      require_named_target
      prepare_root
      probe_pnpm
      ;;
    pnpm-install)
      require_named_target
      prepare_root
      cmd_pnpm_install
      ;;
    probe-apache)
      report_host
      refuse_apache
      ;;
    probe-mysql)
      report_host
      refuse_mysql
      ;;
    probe-postgres)
      report_host
      refuse_postgres
      ;;
    *)
      usage
      exit 2
      ;;
  esac
}

main "$@"
