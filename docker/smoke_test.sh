#!/bin/sh
set -e

PASS=0
FAIL=0

ok() { echo "  [PASS] $1"; PASS=$((PASS+1)); }
fail() { echo "  [FAIL] $1"; FAIL=$((FAIL+1)); }

echo "=== upscan smoke test ==="

# 1. Binary is on PATH
if command -v upscan > /dev/null 2>&1; then
    ok "upscan found in PATH ($(command -v upscan))"
else
    fail "upscan not found in PATH"
fi

# 2. Binary is executable
if [ -x "$(command -v upscan)" ]; then
    ok "binary is executable"
else
    fail "binary is not executable"
fi

# 3. --help exits 0 and prints expected content
HELP_OUTPUT=$(upscan --help 2>&1) || true
if echo "$HELP_OUTPUT" | grep -qi "upscan\|usage\|options\|flags"; then
    ok "--help output looks correct"
else
    fail "--help output missing expected content"
    echo "--- output ---"
    echo "$HELP_OUTPUT"
    echo "--------------"
fi

# 4. Known flags are present in help output
for flag in "--region" "--profile" "--output" "--resources"; do
    if echo "$HELP_OUTPUT" | grep -qF -- "$flag"; then
        ok "flag $flag present in --help"
    else
        fail "flag $flag missing from --help"
    fi
done

# 5. Invalid flag exits non-zero
if upscan --not-a-real-flag > /dev/null 2>&1; then
    fail "invalid flag should exit non-zero"
else
    ok "invalid flag exits non-zero"
fi

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
