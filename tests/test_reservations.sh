#!/usr/bin/env bash
# End-to-end reservation lifecycle tests.
# Requires a running Sovereign Engine instance with BREAK_GLASS=true.
#
# Usage:
#   BASE_URL=https://ai.example.com BOOTSTRAP_USER=admin BOOTSTRAP_PASSWORD=secret ./tests/test_reservations.sh
#
# Or with defaults (localhost:443, admin/admin):
#   BASE_URL=https://localhost ./tests/test_reservations.sh

set -euo pipefail

BASE_URL="${BASE_URL:-https://localhost}"
USER="${BOOTSTRAP_USER:-admin}"
PASS="${BOOTSTRAP_PASSWORD:-admin}"

PASS_COUNT=0
FAIL_COUNT=0
CLEANUP_IDS=()

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

api() {
    local method="$1" path="$2" data="${3:-}"
    local args=(-s -k -u "${USER}:${PASS}" -H "Content-Type: application/json")
    if [[ -n "$data" ]]; then
        args+=(-d "$data")
    fi
    curl -X "$method" "${args[@]}" "${BASE_URL}/api${path}"
}

assert_status() {
    local expected="$1" method="$2" path="$3" data="${4:-}"
    local args=(-s -k -o /dev/null -w '%{http_code}' -u "${USER}:${PASS}" -H "Content-Type: application/json")
    if [[ -n "$data" ]]; then
        args+=(-d "$data")
    fi
    local got
    got=$(curl -X "$method" "${args[@]}" "${BASE_URL}/api${path}")
    if [[ "$got" == "$expected" ]]; then
        return 0
    else
        echo "  Expected HTTP $expected, got $got" >&2
        return 1
    fi
}

future_time() {
    local hours_offset="$1"
    python3 -c "
from datetime import datetime, timedelta, timezone
dt = datetime.now(timezone.utc) + timedelta(hours=${hours_offset})
# Align to next 30-min boundary
if dt.minute < 30:
    dt = dt.replace(minute=30, second=0, microsecond=0)
else:
    dt = dt.replace(minute=0, second=0, microsecond=0) + timedelta(hours=1)
print(dt.strftime('%Y-%m-%dT%H:%M:%S'))
"
}

pass() {
    echo "  PASS: $1"
    ((PASS_COUNT++))
}

fail() {
    echo "  FAIL: $1"
    ((FAIL_COUNT++))
}

cleanup() {
    echo ""
    echo "Cleaning up test reservations..."
    for id in "${CLEANUP_IDS[@]}"; do
        # Try deactivate first (in case it's active), then delete
        api POST "/admin/reservations/${id}/deactivate" '{}' > /dev/null 2>&1 || true
        api DELETE "/admin/reservations/${id}" > /dev/null 2>&1 || true
    done
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

echo "=== Reservation E2E Tests ==="
echo "Target: ${BASE_URL}"
echo ""

# Generate time slots
START1=$(future_time 2)
END1=$(future_time 4)
START2=$(future_time 6)
END2=$(future_time 8)

# 1. Create reservation
echo "1. Create reservation"
RESP=$(api POST "/user/reservations" "{\"start_time\":\"${START1}\",\"end_time\":\"${END1}\",\"reason\":\"E2E test\"}")
RES_ID=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null || echo "")
STATUS=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status',''))" 2>/dev/null || echo "")
if [[ -n "$RES_ID" && "$STATUS" == "pending" ]]; then
    pass "Created reservation $RES_ID"
    CLEANUP_IDS+=("$RES_ID")
else
    fail "Create reservation (got: $RESP)"
fi

# 2. List own reservations
echo "2. List own reservations"
RESP=$(api GET "/user/reservations")
if echo "$RESP" | python3 -c "import sys,json; d=json.load(sys.stdin); assert any(r['id']=='${RES_ID}' for r in d['reservations'])" 2>/dev/null; then
    pass "Reservation appears in user list"
else
    fail "Reservation not found in user list"
fi

# 3. Get active (should be none)
echo "3. Get active (expect none)"
RESP=$(api GET "/user/reservations/active")
ACTIVE=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['active'])" 2>/dev/null || echo "")
if [[ "$ACTIVE" == "False" ]]; then
    pass "No active reservation"
else
    fail "Expected active=false (got: $RESP)"
fi

# 4. Admin list contains reservation
echo "4. Admin list contains reservation"
RESP=$(api GET "/admin/reservations")
if echo "$RESP" | python3 -c "import sys,json; d=json.load(sys.stdin); assert any(r['id']=='${RES_ID}' for r in d['reservations'])" 2>/dev/null; then
    pass "Reservation appears in admin list"
else
    fail "Reservation not found in admin list"
fi

# 5. Admin approve
echo "5. Admin approve"
if assert_status 200 POST "/admin/reservations/${RES_ID}/approve" '{"note":"E2E approved"}'; then
    pass "Approved reservation"
else
    fail "Approve reservation"
fi

# 6. Admin force-activate
echo "6. Admin force-activate"
if assert_status 200 POST "/admin/reservations/${RES_ID}/activate" '{}'; then
    pass "Force-activated reservation"
else
    fail "Force-activate reservation"
fi

# 7. Get active (should exist)
echo "7. Get active (expect match)"
RESP=$(api GET "/user/reservations/active")
ACTIVE_ID=$(echo "$RESP" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('reservation_id',''))" 2>/dev/null || echo "")
if [[ "$ACTIVE_ID" == "$RES_ID" ]]; then
    pass "Active reservation matches"
else
    fail "Active reservation mismatch (expected $RES_ID, got $ACTIVE_ID)"
fi

# 8. Create overlapping (should fail 409)
echo "8. Create overlapping reservation"
if assert_status 409 POST "/user/reservations" "{\"start_time\":\"${START1}\",\"end_time\":\"${END1}\"}"; then
    pass "Overlap correctly rejected"
else
    fail "Overlap not rejected"
fi

# 9. Admin force-deactivate
echo "9. Admin force-deactivate"
if assert_status 200 POST "/admin/reservations/${RES_ID}/deactivate" '{}'; then
    pass "Force-deactivated reservation"
else
    fail "Force-deactivate reservation"
fi

# 10. Get active (should be none again)
echo "10. Get active (expect none)"
RESP=$(api GET "/user/reservations/active")
ACTIVE=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['active'])" 2>/dev/null || echo "")
if [[ "$ACTIVE" == "False" ]]; then
    pass "No active reservation after deactivate"
else
    fail "Expected active=false after deactivate"
fi

# 11. Admin delete
echo "11. Admin delete"
if assert_status 200 DELETE "/admin/reservations/${RES_ID}"; then
    pass "Deleted reservation"
    # Remove from cleanup since it's already deleted
    CLEANUP_IDS=("${CLEANUP_IDS[@]/$RES_ID/}")
else
    fail "Delete reservation"
fi

# 12. Create + cancel own
echo "12. Create and cancel own reservation"
RESP=$(api POST "/user/reservations" "{\"start_time\":\"${START2}\",\"end_time\":\"${END2}\",\"reason\":\"cancel test\"}")
RES_ID2=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null || echo "")
if [[ -n "$RES_ID2" ]]; then
    CLEANUP_IDS+=("$RES_ID2")
    if assert_status 200 POST "/user/reservations/${RES_ID2}/cancel" '{}'; then
        pass "Created and cancelled reservation"
    else
        fail "Cancel own reservation"
    fi
else
    fail "Create reservation for cancel test"
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

echo ""
echo "=== Results ==="
echo "Passed: ${PASS_COUNT}"
echo "Failed: ${FAIL_COUNT}"

if [[ "$FAIL_COUNT" -gt 0 ]]; then
    exit 1
fi
