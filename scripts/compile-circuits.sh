#!/usr/bin/env bash
# Compile Noir circuits with a co-noir-compatible Nargo toolchain.
#
# Usage:
#   ./scripts/compile-circuits.sh [--force] [--help]
#
# Flags:
#   -f, --force   Recompile every circuit, ignoring the freshness cache.
#   -h, --help    Show this help text and exit.
#
# Caching:
#   By default a circuit is recompiled only when its compiled artifact
#   (circuits/<name>/target/<name>.json) is missing or older than any of its
#   source inputs — the circuit's own src/ and Nargo.toml, plus the shared
#   circuits/lib/ that every circuit depends on. Up-to-date circuits are
#   skipped (their artifact version is still verified). Use --force to bypass
#   this and always recompile. CI complements this with actions/cache keyed on
#   the circuit source hash + nargo version (see .github/workflows/ci.yml).
#
# Optional env vars:
#   EXPECTED_NOIR_VERSION (default: 1.0.0-beta.17)
#   NARGO_BIN            (override path to nargo binary)
#
# This script downloads a pinned Nargo release if the system nargo version
# does not match EXPECTED_NOIR_VERSION.

set -euo pipefail

usage() {
    sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'
}

FORCE=0
for arg in "$@"; do
    case "${arg}" in
        -f|--force) FORCE=1 ;;
        -h|--help) usage; exit 0 ;;
        *)
            echo "Unknown argument: ${arg}" >&2
            usage >&2
            exit 1
            ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

EXPECTED_NOIR_VERSION="${EXPECTED_NOIR_VERSION:-1.0.0-beta.17}"

# Issue #92: Download CRS with optimization
if [ ! -f "${PROJECT_DIR}/.crs/bn254_g1.dat" ]; then
    echo "Downloading CRS (cached for future compilations)..."
    "${SCRIPT_DIR}/download-crs.sh"
fi
EXPECTED_NOIR_TAG="v${EXPECTED_NOIR_VERSION}"
TOOLS_DIR="${PROJECT_DIR}/.tmp_tools"
CIRCUITS=(deal_valid reveal_board_valid showdown_valid)

detect_platform_asset() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "${os}:${arch}" in
        Darwin:arm64|Darwin:aarch64)
            echo "nargo-aarch64-apple-darwin.tar.gz"
            ;;
        Darwin:x86_64)
            echo "nargo-x86_64-apple-darwin.tar.gz"
            ;;
        Linux:x86_64)
            echo "nargo-x86_64-unknown-linux-gnu.tar.gz"
            ;;
        Linux:arm64|Linux:aarch64)
            echo "nargo-aarch64-unknown-linux-gnu.tar.gz"
            ;;
        *)
            echo "unsupported platform ${os}/${arch}" >&2
            exit 1
            ;;
    esac
}

nargo_matches_expected() {
    local bin="$1"
    local version_line
    version_line="$("${bin}" --version 2>/dev/null | head -n 1 || true)"
    [[ "${version_line}" == *"${EXPECTED_NOIR_VERSION}"* ]]
}

download_pinned_nargo() {
    local asset url version_dir tarball_path
    asset="$(detect_platform_asset)"
    version_dir="${TOOLS_DIR}/noir-${EXPECTED_NOIR_VERSION}"
    tarball_path="${version_dir}/${asset}"
    mkdir -p "${version_dir}"

    if [ ! -x "${version_dir}/nargo" ]; then
        url="https://github.com/noir-lang/noir/releases/download/${EXPECTED_NOIR_TAG}/${asset}"
        echo "Downloading ${url}" >&2
        curl -fsSL -o "${tarball_path}" "${url}"
        tar -xzf "${tarball_path}" -C "${version_dir}"
        chmod +x "${version_dir}/nargo"
    fi

    echo "${version_dir}/nargo"
}

resolve_nargo_bin() {
    if [ -n "${NARGO_BIN:-}" ]; then
        if [ ! -x "${NARGO_BIN}" ]; then
            echo "NARGO_BIN is set but not executable: ${NARGO_BIN}" >&2
            exit 1
        fi
        echo "${NARGO_BIN}"
        return
    fi

    if command -v nargo >/dev/null 2>&1; then
        local system_nargo
        system_nargo="$(command -v nargo)"
        if nargo_matches_expected "${system_nargo}"; then
            echo "${system_nargo}"
            return
        fi
        echo "System nargo is not ${EXPECTED_NOIR_VERSION}; using pinned toolchain." >&2
    fi

    download_pinned_nargo
}

verify_artifact_version() {
    local artifact="$1"
    local version=""

    if command -v jq >/dev/null 2>&1; then
        version="$(jq -r '.noir_version // empty' "${artifact}" 2>/dev/null || true)"
    fi

    if [ -z "${version}" ]; then
        version="$(grep -o '"noir_version":"[^"]*"' "${artifact}" | head -n1 | cut -d'"' -f4 || true)"
    fi

    if [[ "${version}" != "${EXPECTED_NOIR_VERSION}"* ]]; then
        echo "ERROR: ${artifact} noir_version='${version}' does not match expected '${EXPECTED_NOIR_VERSION}'" >&2
        exit 1
    fi
}

# Return 0 (fresh) if the compiled artifact exists and is newer than every
# source input for the circuit; return 1 (stale) otherwise. Sources include
# the circuit's own src/ + Nargo.toml and the shared circuits/lib it depends on.
artifact_is_fresh() {
    local circuit="$1"
    local artifact="$2"

    [ -f "${artifact}" ] || return 1

    local newer
    newer="$(find \
        "${PROJECT_DIR}/circuits/${circuit}/src" \
        "${PROJECT_DIR}/circuits/${circuit}/Nargo.toml" \
        "${PROJECT_DIR}/circuits/lib/src" \
        "${PROJECT_DIR}/circuits/lib/Nargo.toml" \
        -type f -newer "${artifact}" -print -quit 2>/dev/null)"

    [ -z "${newer}" ]
}

main() {
    # Decide up front which circuits actually need recompiling so we can skip
    # the (slow) nargo download entirely on a full cache hit.
    local to_compile=()
    local circuit artifact
    for circuit in "${CIRCUITS[@]}"; do
        artifact="${PROJECT_DIR}/circuits/${circuit}/target/${circuit}.json"
        if [ "${FORCE}" -eq 0 ] && artifact_is_fresh "${circuit}" "${artifact}"; then
            echo "Skipping ${circuit} — artifact up to date (use --force to recompile)."
            verify_artifact_version "${artifact}"
        else
            to_compile+=("${circuit}")
        fi
    done

    if [ "${#to_compile[@]}" -eq 0 ]; then
        echo "All circuit artifacts are up to date; nothing to compile."
        return 0
    fi

    local nargo_bin
    nargo_bin="$(resolve_nargo_bin)"

    echo "Using nargo: ${nargo_bin}"
    "${nargo_bin}" --version | head -n 2

    mkdir -p "${PROJECT_DIR}/.tmp_nargo_home"

    for circuit in "${to_compile[@]}"; do
        echo "Compiling ${circuit}..."
        HOME="${PROJECT_DIR}/.tmp_nargo_home" \
            "${nargo_bin}" compile --program-dir "${PROJECT_DIR}/circuits/${circuit}"
        verify_artifact_version "${PROJECT_DIR}/circuits/${circuit}/target/${circuit}.json"
    done

    echo "Circuit compilation complete."
}

main "$@"

# Issue #92: CRS download optimization
download_crs_if_needed() {
    local crs_dir="${PROJECT_DIR}/.tmp_crs"
    local crs_file="${crs_dir}/bn254_g1.dat"
    local crs_version_file="${crs_dir}/version.txt"
    local expected_version="v1.0.0"
    
    mkdir -p "$crs_dir"
    
    if [ -f "$crs_version_file" ]; then
        local current_version=$(cat "$crs_version_file")
        if [ "$current_version" = "$expected_version" ] && [ -f "$crs_file" ]; then
            echo "CRS already downloaded ($expected_version)"
            return 0
        fi
    fi
    
    echo "Downloading CRS ($expected_version)..."
    local crs_url="https://aztec-ignition.s3.amazonaws.com/MAIN%20IGNITION/sealed/bn254_g1.dat"
    
    curl -L --progress-bar -o "$crs_file" "$crs_url" || {
        echo "CRS download failed" >&2
        return 1
    }
    
    echo "$expected_version" > "$crs_version_file"
    echo "CRS downloaded successfully"
}

download_crs_if_needed
