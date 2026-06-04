#!/usr/bin/env bash
#
# iftpfm2-web integration test script
# Tests REST API CRUD operations, Basic Auth, readonly mode, and config persistence
#
# Prerequisites:
#   - cargo build --bin iftpfm2-web --package iftpfm2-web
#   - curl
#   - jq (optional, for nicer output)

set -e

# Add cargo to PATH if not already there
if ! command -v cargo &>/dev/null && [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

BASE_URL="http://127.0.0.1:13579"
AUTH_USER="testadmin"
AUTH_PASS="testsecret123"
CONFIG_FILE="/tmp/test_web_config.jsonl"
WEB_PID=""
RO_PID=""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

pass() { echo -e "${GREEN}PASS${NC}: $1"; }
fail() { echo -e "${RED}FAIL${NC}: $1"; FAIL_COUNT=$((FAIL_COUNT + 1)); }
info() { echo -e "${YELLOW}---${NC} $1"; }

FAIL_COUNT=0

cleanup() {
    if [ -n "$WEB_PID" ]; then kill $WEB_PID 2>/dev/null || true; fi
    if [ -n "$RO_PID" ]; then kill $RO_PID 2>/dev/null || true; fi
    rm -f "$CONFIG_FILE" "${CONFIG_FILE}.tmp" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# ── Helper: HTTP request with auth ────────────────────────────────────
api_get() {
    curl -s -u "$AUTH_USER:$AUTH_PASS" -w "\n%{http_code}" "$BASE_URL$1"
}
api_get_noauth() {
    curl -s -w "\n%{http_code}" "$BASE_URL$1"
}
api_post() {
    curl -s -u "$AUTH_USER:$AUTH_PASS" -X POST -H "Content-Type: application/json" \
        -d "$2" -w "\n%{http_code}" "$BASE_URL$1"
}
api_put() {
    curl -s -u "$AUTH_USER:$AUTH_PASS" -X PUT -H "Content-Type: application/json" \
        -d "$2" -w "\n%{http_code}" "$BASE_URL$1"
}
api_delete() {
    curl -s -u "$AUTH_USER:$AUTH_PASS" -X DELETE -w "\n%{http_code}" "$BASE_URL$1"
}

# Extract HTTP status code (last line)
status_code() {
    echo "$1" | tail -1
}
# Extract JSON body (everything except last line)
body() {
    echo "$1" | sed '$d'
}

# ── Build ─────────────────────────────────────────────────────────────
info "Building iftpfm2-web..."
cargo build --bin iftpfm2-web --package iftpfm2-web 2>&1 | tail -1

# ── Create test config ────────────────────────────────────────────────
info "Creating test config..."
cat > "$CONFIG_FILE" << 'TESTEOF'
# Test entry 1
{"host_from":"10.0.0.1","port_from":21,"login_from":"user1","password_from":"pass1","path_from":"/src1/","host_to":"10.0.0.2","port_to":21,"login_to":"user2","password_to":"pass2","path_to":"/dst1/","age":3600,"filename_regexp":".*\\.xml$"}
# Test entry 2
{"host_from":"10.0.0.3","port_from":22,"login_from":"user3","password_from":"pass3","path_from":"/src2/","host_to":"10.0.0.4","port_to":22,"login_to":"user4","password_to":"pass4","path_to":"/dst2/","age":7200,"filename_regexp":".*"}
TESTEOF

# ── Kill stale servers on test ports ──────────────────────────────────
if command -v fuser &>/dev/null; then
    fuser -k 13579/tcp 13580/tcp 2>/dev/null || true
elif command -v lsof &>/dev/null; then
    lsof -ti:13579,13580 | xargs -r kill 2>/dev/null || true
fi
sleep 0.3

# ── Start server with auth ────────────────────────────────────────────
info "Starting iftpfm2-web with Basic Auth on $BASE_URL..."
./target/debug/iftpfm2-web --config "$CONFIG_FILE" --listen "127.0.0.1:13579" \
    --user "$AUTH_USER" --password "$AUTH_PASS" > /tmp/test_web_server.log 2>&1 &
WEB_PID=$!

# Wait for server
for i in $(seq 1 30); do
    if curl -s -o /dev/null "$BASE_URL/" 2>/dev/null; then
        break
    fi
    if [ $i -eq 30 ]; then
        fail "Server did not start"
        cat /tmp/test_web_server.log
        exit 1
    fi
    sleep 0.2
done
info "Server started (PID: $WEB_PID)"

# Also start a readonly instance on a different port for readonly tests
info "Starting readonly instance on port 13580..."
./target/debug/iftpfm2-web --config "$CONFIG_FILE" --listen "127.0.0.1:13580" \
    --user "$AUTH_USER" --password "$AUTH_PASS" --readonly > /tmp/test_web_ro.log 2>&1 &
RO_PID=$!
for i in $(seq 1 30); do
    if curl -s -o /dev/null "http://127.0.0.1:13580/" 2>/dev/null; then
        break
    fi
    if [ $i -eq 30 ]; then
        fail "Readonly server did not start"
        cat /tmp/test_web_ro.log
        exit 1
    fi
    sleep 0.2
done

# ====================================================================
echo ""
info "=== TEST SUITE: Authentication ==="
# ====================================================================

echo ""
info "Test 1: Unauthenticated GET /api/configs → 401"
RESP=$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/api/configs")
if [ "$RESP" = "401" ]; then pass "401 Unauthorized"; else fail "Expected 401, got $RESP"; fi

echo ""
info "Test 2: Wrong password → 401"
RESP=$(curl -s -o /dev/null -w "%{http_code}" -u "$AUTH_USER:wrongpass" "$BASE_URL/api/configs")
if [ "$RESP" = "401" ]; then pass "401 for wrong password"; else fail "Expected 401, got $RESP"; fi

echo ""
info "Test 3: Wrong user → 401"
RESP=$(curl -s -o /dev/null -w "%{http_code}" -u "wronguser:$AUTH_PASS" "$BASE_URL/api/configs")
if [ "$RESP" = "401" ]; then pass "401 for wrong user"; else fail "Expected 401, got $RESP"; fi

echo ""
info "Test 4: Correct credentials → 200"
RESP=$(curl -s -o /dev/null -w "%{http_code}" -u "$AUTH_USER:$AUTH_PASS" "$BASE_URL/api/configs")
if [ "$RESP" = "200" ]; then pass "200 OK with correct credentials"; else fail "Expected 200, got $RESP"; fi

echo ""
info "Test 5: SPA page without auth → 401"
RESP=$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/")
if [ "$RESP" = "401" ]; then pass "401 for SPA without auth"; else fail "Expected 401, got $RESP"; fi

echo ""
info "Test 6: SPA page with auth → 200"
RESP=$(curl -s -o /dev/null -w "%{http_code}" -u "$AUTH_USER:$AUTH_PASS" "$BASE_URL/")
if [ "$RESP" = "200" ]; then pass "200 for SPA with auth"; else fail "Expected 200, got $RESP"; fi

# ====================================================================
echo ""
info "=== TEST SUITE: GET endpoints ==="
# ====================================================================

echo ""
info "Test 7: GET /api/configs → 2 entries"
RESP=$(api_get "/api/configs")
CODE=$(status_code "$RESP")
BODY=$(body "$RESP")
if [ "$CODE" = "200" ]; then
    COUNT=$(echo "$BODY" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "0")
    if [ "$COUNT" = "2" ]; then pass "2 entries returned"; else fail "Expected 2 entries, got $COUNT"; fi
else
    fail "Expected 200, got $CODE"
fi

echo ""
info "Test 8: GET /api/configs/0 → first entry"
RESP=$(api_get "/api/configs/0")
CODE=$(status_code "$RESP")
BODY=$(body "$RESP")
if [ "$CODE" = "200" ]; then
    HOST=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['config']['host_from'])" 2>/dev/null || echo "")
    if [ "$HOST" = "10.0.0.1" ]; then pass "host_from=10.0.0.1"; else fail "Expected 10.0.0.1, got '$HOST'"; fi
else
    fail "Expected 200, got $CODE"
fi

echo ""
info "Test 9: GET /api/configs/1 → second entry"
RESP=$(api_get "/api/configs/1")
CODE=$(status_code "$RESP")
BODY=$(body "$RESP")
if [ "$CODE" = "200" ]; then
    HOST=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['config']['host_from'])" 2>/dev/null || echo "")
    if [ "$HOST" = "10.0.0.3" ]; then pass "host_from=10.0.0.3"; else fail "Expected 10.0.0.3, got '$HOST'"; fi
else
    fail "Expected 200, got $CODE"
fi

echo ""
info "Test 10: GET /api/configs/999 → 404"
RESP=$(api_get "/api/configs/999")
CODE=$(status_code "$RESP")
if [ "$CODE" = "404" ]; then pass "404 for out of range"; else fail "Expected 404, got $CODE"; fi

echo ""
info "Test 11: Comment preserved in first entry"
RESP=$(api_get "/api/configs/0")
BODY=$(body "$RESP")
COMMENT=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['comment'])" 2>/dev/null || echo "")
if [ "$COMMENT" = "Test entry 1" ]; then pass "Comment='Test entry 1'"; else fail "Expected 'Test entry 1', got '$COMMENT'"; fi

# ====================================================================
echo ""
info "=== TEST SUITE: POST (create) ==="
# ====================================================================

echo ""
info "Test 12: POST /api/configs → create new entry"
NEW_CONFIG='{"comment":"Created by test","config":{"host_from":"10.0.0.5","port_from":21,"login_from":"newuser","password_from":"newpass","path_from":"/newsrc/","host_to":"10.0.0.6","port_to":21,"login_to":"newuser2","password_to":"newpass2","path_to":"/newdst/","age":1800,"filename_regexp":".*\\.csv$"}}'
RESP=$(api_post "/api/configs" "$NEW_CONFIG")
CODE=$(status_code "$RESP")
BODY=$(body "$RESP")
if [ "$CODE" = "201" ]; then
    IDX=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['index'])" 2>/dev/null || echo "")
    if [ "$IDX" = "2" ]; then pass "Created at index 2"; else fail "Expected index 2, got $IDX"; fi
else
    fail "Expected 201, got $CODE. Body: $BODY"
fi

echo ""
info "Test 13: After create, GET /api/configs → 3 entries"
RESP=$(api_get "/api/configs")
BODY=$(body "$RESP")
COUNT=$(echo "$BODY" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "0")
if [ "$COUNT" = "3" ]; then pass "3 entries after create"; else fail "Expected 3, got $COUNT"; fi

echo ""
info "Test 14: New entry persisted to disk"
sleep 0.2
DISK_COUNT=$(grep -c "^{" "$CONFIG_FILE" 2>/dev/null || echo "0")
if [ "$DISK_COUNT" = "3" ]; then pass "3 JSONL lines on disk"; else fail "Expected 3 lines on disk, got $DISK_COUNT"; fi

echo ""
info "Test 15: POST with invalid config → 400"
BAD_CONFIG='{"comment":"bad","config":{"host_from":"","port_from":0,"login_from":"","password_from":"","path_from":"","host_to":"","port_to":0,"login_to":"","password_to":"","path_to":"","age":0,"filename_regexp":"(invalid["}}'
RESP=$(api_post "/api/configs" "$BAD_CONFIG")
CODE=$(status_code "$RESP")
if [ "$CODE" = "400" ]; then pass "400 for invalid config"; else fail "Expected 400, got $CODE"; fi

# ====================================================================
echo ""
info "=== TEST SUITE: PUT (update) ==="
# ====================================================================

echo ""
info "Test 16: PUT /api/configs/0 → update first entry"
UPDATE_CONFIG='{"comment":"Updated comment","config":{"host_from":"10.0.0.100","port_from":2121,"login_from":"updated_user","password_from":"updated_pass","path_from":"/updated_src/","host_to":"10.0.0.200","port_to":2121,"login_to":"updated_to","password_to":"updated_pass2","path_to":"/updated_dst/","age":9999,"filename_regexp":".*\\.zip$","proto_from":"ftps","tz_from":"+05:30"}}'
RESP=$(api_put "/api/configs/0" "$UPDATE_CONFIG")
CODE=$(status_code "$RESP")
if [ "$CODE" = "200" ]; then pass "200 OK"; else fail "Expected 200, got $CODE"; fi

echo ""
info "Test 17: Verify updated values"
RESP=$(api_get "/api/configs/0")
BODY=$(body "$RESP")
HOST=$(echo "$BODY" | python3 -c "import sys,json; d=json.load(sys.stdin)['config']; print(d['host_from'])" 2>/dev/null || echo "")
PROTO=$(echo "$BODY" | python3 -c "import sys,json; d=json.load(sys.stdin)['config']; print(d['proto_from'])" 2>/dev/null || echo "")
TZ=$(echo "$BODY" | python3 -c "import sys,json; d=json.load(sys.stdin)['config']; print(d['tz_from'])" 2>/dev/null || echo "")
COMMENT=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['comment'])" 2>/dev/null || echo "")
OK=true
[ "$HOST" = "10.0.0.100" ] || { fail "host_from expected 10.0.0.100 got $HOST"; OK=false; }
[ "$PROTO" = "ftps" ] || { fail "proto_from expected ftps got $PROTO"; OK=false; }
[ "$TZ" = "+05:30" ] || { fail "tz_from expected +05:30 got $TZ"; OK=false; }
[ "$COMMENT" = "Updated comment" ] || { fail "comment expected 'Updated comment' got '$COMMENT'"; OK=false; }
if [ "$OK" = "true" ]; then pass "All fields updated correctly"; fi

echo ""
info "Test 18: PUT /api/configs/999 → 404"
RESP=$(api_put "/api/configs/999" "$UPDATE_CONFIG")
CODE=$(status_code "$RESP")
if [ "$CODE" = "404" ]; then pass "404 for non-existent index"; else fail "Expected 404, got $CODE"; fi

# ====================================================================
echo ""
info "=== TEST SUITE: DELETE ==="
# ====================================================================

echo ""
info "Test 19: DELETE /api/configs/2 → remove last entry"
RESP=$(api_delete "/api/configs/2")
CODE=$(status_code "$RESP")
if [ "$CODE" = "200" ]; then pass "200 OK"; else fail "Expected 200, got $CODE"; fi

echo ""
info "Test 20: After delete, count is 2"
RESP=$(api_get "/api/configs")
BODY=$(body "$RESP")
COUNT=$(echo "$BODY" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "0")
if [ "$COUNT" = "2" ]; then pass "2 entries after delete"; else fail "Expected 2, got $COUNT"; fi

echo ""
info "Test 21: Disk file updated after delete"
sleep 0.2
DISK_COUNT=$(grep -c "^{" "$CONFIG_FILE" 2>/dev/null || echo "0")
if [ "$DISK_COUNT" = "2" ]; then pass "2 JSONL lines on disk"; else fail "Expected 2 lines on disk, got $DISK_COUNT"; fi

echo ""
info "Test 22: DELETE /api/configs/999 → 404"
RESP=$(api_delete "/api/configs/999")
CODE=$(status_code "$RESP")
if [ "$CODE" = "404" ]; then pass "404 for non-existent"; else fail "Expected 404, got $CODE"; fi

# ====================================================================
echo ""
info "=== TEST SUITE: Validate endpoint ==="
# ====================================================================

echo ""
info "Test 23: POST /api/validate with valid config → valid:true"
VALID='{"config":{"host_from":"1.2.3.4","port_from":21,"login_from":"u","password_from":"p","path_from":"/a/","host_to":"5.6.7.8","port_to":21,"login_to":"u2","password_to":"p2","path_to":"/b/","age":100,"filename_regexp":".*"}}'
RESP=$(api_post "/api/validate" "$VALID")
CODE=$(status_code "$RESP")
BODY=$(body "$RESP")
if [ "$CODE" = "200" ]; then
    IS_VALID=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['valid'])" 2>/dev/null || echo "")
    if [ "$IS_VALID" = "True" ]; then pass "valid=True"; else fail "Expected valid=True, got $IS_VALID"; fi
else
    fail "Expected 200, got $CODE"
fi

echo ""
info "Test 24: POST /api/validate with invalid regex → valid:false"
INVALID_RE='{"config":{"host_from":"1.2.3.4","port_from":21,"login_from":"u","password_from":"p","path_from":"/a/","host_to":"5.6.7.8","port_to":21,"login_to":"u2","password_to":"p2","path_to":"/b/","age":100,"filename_regexp":"(bad["}}'
RESP=$(api_post "/api/validate" "$INVALID_RE")
CODE=$(status_code "$RESP")
BODY=$(body "$RESP")
IS_VALID=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['valid'])" 2>/dev/null || echo "")
if [ "$IS_VALID" = "False" ]; then pass "valid=False for bad regex"; else fail "Expected valid=False, got $IS_VALID"; fi

echo ""
info "Test 25: POST /api/validate with empty host → valid:false"
EMPTY_HOST='{"config":{"host_from":"","port_from":21,"login_from":"u","password_from":"p","path_from":"/a/","host_to":"5.6.7.8","port_to":21,"login_to":"u2","password_to":"p2","path_to":"/b/","age":100,"filename_regexp":".*"}}'
RESP=$(api_post "/api/validate" "$EMPTY_HOST")
BODY=$(body "$RESP")
IS_VALID=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['valid'])" 2>/dev/null || echo "")
if [ "$IS_VALID" = "False" ]; then pass "valid=False for empty host"; else fail "Expected valid=False"; fi

# ====================================================================
echo ""
info "=== TEST SUITE: Readonly mode ==="
# ====================================================================

echo ""
info "Test 26: POST /api/configs on readonly → 403"
RESP=$(curl -s -u "$AUTH_USER:$AUTH_PASS" -X POST -H "Content-Type: application/json" \
    -d "$VALID" -w "\n%{http_code}" "http://127.0.0.1:13580/api/configs")
CODE=$(echo "$RESP" | tail -1)
if [ "$CODE" = "403" ]; then pass "403 Forbidden in readonly"; else fail "Expected 403, got $CODE"; fi

echo ""
info "Test 27: PUT /api/configs/0 on readonly → 403"
RESP=$(curl -s -u "$AUTH_USER:$AUTH_PASS" -X PUT -H "Content-Type: application/json" \
    -d "$UPDATE_CONFIG" -w "\n%{http_code}" "http://127.0.0.1:13580/api/configs/0")
CODE=$(echo "$RESP" | tail -1)
if [ "$CODE" = "403" ]; then pass "403 Forbidden for PUT in readonly"; else fail "Expected 403, got $CODE"; fi

echo ""
info "Test 28: DELETE /api/configs/0 on readonly → 403"
RESP=$(curl -s -u "$AUTH_USER:$AUTH_PASS" -X DELETE -w "\n%{http_code}" "http://127.0.0.1:13580/api/configs/0")
CODE=$(echo "$RESP" | tail -1)
if [ "$CODE" = "403" ]; then pass "403 Forbidden for DELETE in readonly"; else fail "Expected 403, got $CODE"; fi

echo ""
info "Test 29: GET works in readonly"
RESP=$(curl -s -u "$AUTH_USER:$AUTH_PASS" -w "\n%{http_code}" "http://127.0.0.1:13580/api/configs")
CODE=$(echo "$RESP" | tail -1)
if [ "$CODE" = "200" ]; then pass "200 GET in readonly"; else fail "Expected 200, got $CODE"; fi

# ====================================================================
echo ""
info "=== TEST SUITE: Config persistence (round-trip) ==="
# ====================================================================

echo ""
info "Test 30: Config survives server restart"
# Kill current server
kill $WEB_PID 2>/dev/null || true
sleep 0.5

# Restart with same config
./target/debug/iftpfm2-web --config "$CONFIG_FILE" --listen "127.0.0.1:13579" \
    --user "$AUTH_USER" --password "$AUTH_PASS" > /tmp/test_web_server2.log 2>&1 &
WEB_PID=$!

for i in $(seq 1 30); do
    if curl -s -o /dev/null "$BASE_URL/" 2>/dev/null; then break; fi
    sleep 0.2
done

RESP=$(api_get "/api/configs")
BODY=$(body "$RESP")
COUNT=$(echo "$BODY" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "0")
if [ "$COUNT" = "2" ]; then pass "2 entries after restart"; else fail "Expected 2, got $COUNT"; fi

# ====================================================================
echo ""
info "=== TEST SUITE: SFTP config with keyfile ==="
# ====================================================================

echo ""
info "Test 31: Create SFTP entry with keyfile"
SFTP_CONFIG='{"comment":"SFTP test","config":{"host_from":"sftp.example.com","port_from":22,"login_from":"sftpuser","keyfile_from":"/tmp/test_key","path_from":"/sftp/src/","proto_from":"sftp","host_to":"10.0.0.2","port_to":21,"login_to":"u2","password_to":"p2","path_to":"/dst/","age":100,"filename_regexp":".*"}}'
RESP=$(api_post "/api/configs" "$SFTP_CONFIG")
CODE=$(status_code "$RESP")
if [ "$CODE" = "201" ]; then pass "SFTP entry created"; else fail "Expected 201, got $CODE. Body: $(body "$RESP")"; fi

echo ""
info "Test 32: Verify SFTP proto_from field"
RESP=$(api_get "/api/configs/2")
BODY=$(body "$RESP")
PROTO=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['config']['proto_from'])" 2>/dev/null || echo "")
KEYFILE=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['config'].get('keyfile_from',''))" 2>/dev/null || echo "")
OK=true
[ "$PROTO" = "sftp" ] || { fail "Expected proto_from=sftp, got $PROTO"; OK=false; }
[ "$KEYFILE" = "/tmp/test_key" ] || { fail "Expected keyfile_from=/tmp/test_key, got $KEYFILE"; OK=false; }
if [ "$OK" = "true" ]; then pass "SFTP fields correct"; fi

# Cleanup the SFTP entry
api_delete "/api/configs/2" > /dev/null 2>&1

# ====================================================================
echo ""
info "=== TEST SUITE: JSONL format on disk ==="
# ====================================================================

echo ""
info "Test 33: Disk file has comment lines"
if grep -q "^# " "$CONFIG_FILE"; then pass "Comment lines present"; else fail "No comment lines in $CONFIG_FILE"; fi

echo ""
info "Test 34: Disk file is valid JSONL (parseable)"
if python3 -c "
import json, sys
with open('$CONFIG_FILE') as f:
    for i, line in enumerate(f, 1):
        line = line.strip()
        if not line or line.startswith('#'):
            continue
        try:
            json.loads(line)
        except json.JSONDecodeError as e:
            print(f'Invalid JSON on line {i}: {e}')
            sys.exit(1)
print('OK')
" 2>/dev/null; then
    pass "All JSONL lines valid"
else
    fail "Invalid JSONL on disk"
fi

# ====================================================================
# Summary
# ====================================================================
echo ""
echo "=================================================="
if [ "$FAIL_COUNT" -eq 0 ]; then
    echo -e "${GREEN}All 34 tests passed!${NC}"
else
    echo -e "${RED}$FAIL_COUNT test(s) failed${NC}"
fi
echo "=================================================="

[ "$FAIL_COUNT" -eq 0 ]
