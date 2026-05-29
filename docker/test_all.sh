#!/usr/bin/env bash
# Run upscan installation smoke tests across multiple environments.
# Usage: ./docker/test_all.sh [environment...]
# If no environments are specified, all are tested.
# Examples:
#   ./docker/test_all.sh
#   ./docker/test_all.sh ubuntu alpine
#   ./docker/test_all.sh cargo

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Override with VERSION=x.y.z ./docker/test_all.sh to test a specific release
VERSION="${VERSION:-0.1.0}"

ENVIRONMENTS=(ubuntu debian alpine fedora cargo)

# Filter to requested environments if args supplied
if [ $# -gt 0 ]; then
    ENVIRONMENTS=("$@")
fi

PASS=()
FAIL=()

run_env() {
    local env="$1"
    local tag="upscan-test-${env}"
    local dockerfile="${SCRIPT_DIR}/${env}/Dockerfile"

    if [ ! -f "$dockerfile" ]; then
        echo "[SKIP] $env — no Dockerfile found at $dockerfile"
        return
    fi

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  Testing: $env"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # cargo builds from the project root (needs Cargo.toml/src); others use their own directory
    local build_context="${SCRIPT_DIR}/${env}"
    if [ "$env" = "cargo" ]; then
        build_context="$(dirname "$SCRIPT_DIR")"
        cp "${SCRIPT_DIR}/smoke_test.sh" "${build_context}/docker/cargo/smoke_test.sh"
    else
        cp "${SCRIPT_DIR}/smoke_test.sh" "${SCRIPT_DIR}/${env}/smoke_test.sh"
    fi

    if docker build \
        --file "$dockerfile" \
        --tag "$tag" \
        --build-arg "VERSION=${VERSION}" \
        "$build_context" \
        2>&1 | sed 's/^/  [build] /'; then
        echo "  Build OK — running smoke test…"
        if docker run --rm "$tag" 2>&1 | sed 's/^/  /'; then
            PASS+=("$env")
        else
            FAIL+=("$env")
        fi
    else
        echo "  Build FAILED"
        FAIL+=("$env")
    fi

    rm -f "${SCRIPT_DIR}/${env}/smoke_test.sh"
}

for env in "${ENVIRONMENTS[@]}"; do
    run_env "$env"
done

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
for e in "${PASS[@]:-}"; do [ -n "$e" ] && echo "  PASS  $e"; done
for e in "${FAIL[@]:-}"; do [ -n "$e" ] && echo "  FAIL  $e"; done
echo ""

if [ "${#FAIL[@]}" -gt 0 ] && [ -n "${FAIL[0]}" ]; then
    echo "  ${#FAIL[@]} environment(s) failed."
    exit 1
else
    echo "  All environments passed."
fi
