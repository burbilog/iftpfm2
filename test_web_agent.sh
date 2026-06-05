#!/usr/bin/env bash
#
# iftpfm2-web UI integration test via agent-browser
# Tests CRUD operations through the actual web interface
#
# Prerequisites:
#   - agent-browser CLI (https://github.com/nicepkg/agent-browser)
#   - cargo build --bin iftpfm2-web --package iftpfm2-web
#
# This test runs the server WITHOUT auth (auth is covered by test_web.sh)
# and tests all UI operations through a headless browser.

set -e

# ── Check prerequisites ──────────────────────────────────────────────
if ! command -v agent-browser &>/dev/null; then
    echo "SKIP: agent-browser is not installed"
    echo "  Install: npm install -g agent-browser"
    echo "  See: https://github.com/nicepkg/agent-browser"
    exit 0
fi

# Add cargo to PATH if not already there
if ! command -v cargo &>/dev/null && [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

BASE_URL="http://127.0.0.1:13581"
CONFIG_FILE="/tmp/test_web_agent_config.jsonl"
RESULT_FILE="/tmp/_test_web_agent_result.tmp"
WEB_PID=""
FAIL_COUNT=0

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

pass() { echo -e "  ${GREEN}PASS${NC}: $1"; }
fail() { echo -e "  ${RED}FAIL${NC}: $1"; FAIL_COUNT=$((FAIL_COUNT + 1)); }
info() { echo -e "${YELLOW}---${NC} $1"; }

cleanup() {
    agent-browser close 2>/dev/null || true
    if [ -n "$WEB_PID" ] && kill -0 "$WEB_PID" 2>/dev/null; then
        kill "$WEB_PID" 2>/dev/null || true
    fi
    rm -f "$CONFIG_FILE" "${CONFIG_FILE}.tmp" "$RESULT_FILE" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# ── Build ─────────────────────────────────────────────────────────────
info "Building iftpfm2-web..."
cargo build --bin iftpfm2-web --package iftpfm2-web 2>&1 | tail -1

# ── Create test config ────────────────────────────────────────────────
cat > "$CONFIG_FILE" << 'EOF'
# Production transfer
{"host_from":"10.0.0.1","port_from":21,"login_from":"prod1","password_from":"pass1","path_from":"/out/","host_to":"10.0.0.2","port_to":21,"login_to":"prod2","password_to":"pass2","path_to":"/in/","age":86400,"filename_regexp":".*\\.xml$"}
# Backup via FTPS
{"host_from":"ftps.backup.local","port_from":21,"login_from":"bak1","password_from":"bakpass","path_from":"/data/","proto_from":"ftps","host_to":"10.0.0.4","port_to":21,"login_to":"stor1","password_to":"storpass","path_to":"/archive/","age":3600,"filename_regexp":".*"}
EOF

# ── Kill stale server on our port ────────────────────────────────────
if command -v fuser &>/dev/null; then
    fuser -k 13581/tcp 2>/dev/null || true
elif command -v lsof &>/dev/null; then
    lsof -ti:13581 | xargs -r kill 2>/dev/null || true
fi
sleep 0.3

# ── Start server (no auth) ───────────────────────────────────────────
info "Starting iftpfm2-web on $BASE_URL..."
./target/debug/iftpfm2-web --config "$CONFIG_FILE" --listen "127.0.0.1:13581" \
    --no-auth-i-know-what-im-doing > /tmp/test_web_agent.log 2>&1 &
WEB_PID=$!

for i in $(seq 1 30); do
    if curl -s -o /dev/null "$BASE_URL/" 2>/dev/null; then break; fi
    if [ $i -eq 30 ]; then fail "Server did not start"; exit 1; fi
    sleep 0.2
done
info "Server started (PID: $WEB_PID)"

# ── Helpers ───────────────────────────────────────────────────────────
# All agent-browser calls use file redirect instead of $(...) to avoid
# subshell issues that can close the browser daemon in bash scripts.

# Run JS, discard output. No subshell.
runjs() {
    local encoded
    encoded=$(printf '%s' "$1" | base64 -w0)
    agent-browser eval -b "$encoded" >/dev/null 2>&1 || true
}

# Run JS, capture output to RESULT_FILE. No subshell for agent-browser.
capture_js() {
    local encoded
    encoded=$(printf '%s' "$1" | base64 -w0)
    agent-browser eval -b "$encoded" > "$RESULT_FILE" 2>/dev/null || true
}

# Read captured result from RESULT_FILE, strip surrounding quotes.
read_result() {
    local r
    r=$(<"$RESULT_FILE")
    r="${r#\"}"
    r="${r%\"}"
    printf '%s' "$r"
}

# Assert JS expression equals expected value.
# Usage: assert_js <js_expr> <expected_value>
assert_js() {
    local js_expr="$1"
    local expected="$2"
    capture_js "$js_expr"
    local result
    result=$(read_result)
    if [ "$result" = "$expected" ]; then
        pass "$js_expr"
    else
        fail "$js_expr — expected '$expected', got '$result'"
    fi
}

# ══════════════════════════════════════════════════════════════════════
info "=== Test 1: Page loads correctly ==="
# ══════════════════════════════════════════════════════════════════════

agent-browser open "$BASE_URL/" >/dev/null 2>&1 || true
agent-browser wait 1500 >/dev/null 2>&1 || true

info "Test 1a: Title is correct"
agent-browser get title > "$RESULT_FILE" 2>/dev/null || true
TITLE=$(read_result)
if echo "$TITLE" | grep -q "iftpfm2"; then
    pass "Title: '$TITLE'"
else
    fail "Title wrong: '$TITLE'"
fi

info "Test 1b: 2 config entries shown"
assert_js "document.querySelector('#headerInfo').textContent" "2 config entries loaded"

info "Test 1c: Table has 2 data rows"
assert_js "document.querySelectorAll('#tbody tr').length" "2"

info "Test 1d: First row shows correct host"
assert_js "document.querySelector('#tbody tr:first-child td:nth-child(3)').childNodes[0].textContent" "10.0.0.1:21"

info "Test 1e: Second row shows FTPS protocol"
assert_js "document.querySelector('#tbody tr:nth-child(2) td:nth-child(2)').textContent" "FTPS → FTP"

# ══════════════════════════════════════════════════════════════════════
info "=== Test 2: Edit entry (update host_from) ==="
# ══════════════════════════════════════════════════════════════════════

info "Test 2a: Open edit modal for entry 0"
runjs "document.querySelectorAll('#tbody tr:first-child button')[0].click()"
agent-browser wait 500 >/dev/null 2>&1 || true

assert_js "document.querySelector('.modal-overlay').classList.contains('active')" "true"
assert_js "document.querySelector('#modalTitle').textContent" "Edit Config #0"
assert_js "document.getElementById('f_host_from').value" "10.0.0.1"

info "Test 2b: Change host_from and save"
runjs "document.getElementById('f_host_from').value='99.99.99.99'; document.getElementById('f_host_from').dispatchEvent(new Event('input',{bubbles:true})); window.saveConfig();"
agent-browser wait 1500 >/dev/null 2>&1 || true

info "Test 2c: Verify notification"
capture_js "document.querySelector('.notification')?.textContent || ''"
NOTIF=$(read_result)
if echo "$NOTIF" | grep -q "updated"; then
    pass "Notification: '$NOTIF'"
else
    fail "Expected 'updated' notification, got: '$NOTIF'"
fi

assert_js "document.querySelector('.modal-overlay').classList.contains('active')" "false"

info "Test 2d: Verify table updated"
assert_js "document.querySelector('#tbody tr:first-child td:nth-child(3)').childNodes[0].textContent" "99.99.99.99:21"

info "Test 2e: Verify change persisted to disk"
sleep 0.3
if grep -q "99.99.99.99" "$CONFIG_FILE"; then
    pass "host_from=99.99.99.99 written to disk"
else
    fail "host_from=99.99.99.99 not found in config file"
fi

# ══════════════════════════════════════════════════════════════════════
info "=== Test 3: Add new entry ==="
# ══════════════════════════════════════════════════════════════════════

info "Test 3a: Open create modal"
runjs "document.querySelector('button.btn-primary').click()"
agent-browser wait 500 >/dev/null 2>&1 || true

assert_js "document.querySelector('#modalTitle').textContent" "Add Config"
assert_js "document.querySelector('.modal-overlay').classList.contains('active')" "true"

info "Test 3b: Fill form and save"
runjs "
document.getElementById('f_host_from').value='newhost.local';
document.getElementById('f_port_from').value='2222';
document.getElementById('f_login_from').value='newuser';
document.getElementById('f_password_from').value='newpass';
document.getElementById('f_path_from').value='/src/';
document.getElementById('f_host_to').value='newhost2.local';
document.getElementById('f_port_to').value='2222';
document.getElementById('f_login_to').value='newuser2';
document.getElementById('f_password_to').value='newpass2';
document.getElementById('f_path_to').value='/dst/';
document.getElementById('f_age').value='500';
document.getElementById('f_filename_regexp').value='.*\\\\.csv$';
window.saveConfig();
"
agent-browser wait 1500 >/dev/null 2>&1 || true

info "Test 3c: Verify notification"
capture_js "document.querySelector('.notification')?.textContent || ''"
NOTIF=$(read_result)
if echo "$NOTIF" | grep -q "created"; then
    pass "Notification: '$NOTIF'"
else
    fail "Expected 'created' notification, got: '$NOTIF'"
fi

info "Test 3d: Verify 3 entries"
assert_js "document.querySelectorAll('#tbody tr').length" "3"
assert_js "document.querySelector('#headerInfo').textContent" "3 config entries loaded"

info "Test 3e: Verify new entry data"
assert_js "document.querySelector('#tbody tr:nth-child(3) td:nth-child(3)').childNodes[0].textContent" "newhost.local:2222"

info "Test 3f: Verify on disk"
sleep 0.3
DISK_COUNT=$(grep -c "^{" "$CONFIG_FILE" 2>/dev/null || echo "0")
if [ "$DISK_COUNT" = "3" ]; then pass "3 JSONL lines on disk"; else fail "Expected 3 lines, got $DISK_COUNT"; fi

# ══════════════════════════════════════════════════════════════════════
info "=== Test 4: Delete entry ==="
# ══════════════════════════════════════════════════════════════════════

info "Test 4a: Delete entry 2 (click + confirm)"
runjs "document.getElementById('delbtn2').click()"
agent-browser wait 200 >/dev/null 2>&1 || true
runjs "document.getElementById('delbtn2').click()"
agent-browser wait 1500 >/dev/null 2>&1 || true

info "Test 4b: Verify notification"
capture_js "document.querySelector('.notification')?.textContent || ''"
NOTIF=$(read_result)
if echo "$NOTIF" | grep -q "deleted"; then
    pass "Notification: '$NOTIF'"
else
    fail "Expected 'deleted' notification, got: '$NOTIF'"
fi

info "Test 4c: Verify 2 entries remain"
assert_js "document.querySelectorAll('#tbody tr').length" "2"
assert_js "document.querySelector('#headerInfo').textContent" "2 config entries loaded"

info "Test 4d: Verify on disk"
sleep 0.3
DISK_COUNT=$(grep -c "^{" "$CONFIG_FILE" 2>/dev/null || echo "0")
if [ "$DISK_COUNT" = "2" ]; then pass "2 JSONL lines on disk"; else fail "Expected 2 lines, got $DISK_COUNT"; fi

# ══════════════════════════════════════════════════════════════════════
info "=== Test 5: Search/filter ==="
# ══════════════════════════════════════════════════════════════════════

info "Test 5a: Search 'ftps' filters to 1 entry"
runjs "document.getElementById('search').value='ftps'; document.getElementById('search').dispatchEvent(new Event('input',{bubbles:true}));"
agent-browser wait 500 >/dev/null 2>&1 || true

assert_js "document.querySelectorAll('#tbody tr').length" "1"

info "Test 5b: Visible row is the FTPS entry"
assert_js "document.querySelector('#tbody tr:first-child td:nth-child(3)').childNodes[0].textContent" "ftps.backup.local:21"

info "Test 5c: Clear search"
runjs "document.getElementById('search').value=''; document.getElementById('search').dispatchEvent(new Event('input',{bubbles:true}));"
agent-browser wait 300 >/dev/null 2>&1 || true
assert_js "document.querySelectorAll('#tbody tr').length" "2"

# ══════════════════════════════════════════════════════════════════════
info "=== Test 6: Edit with protocol/tz/bind fields ==="
# ══════════════════════════════════════════════════════════════════════

info "Test 6a: Open edit for entry 1"
runjs "document.querySelectorAll('#tbody tr:nth-child(2) button')[0].click()"
agent-browser wait 500 >/dev/null 2>&1 || true
assert_js "document.querySelector('.modal-overlay').classList.contains('active')" "true"

info "Test 6b: Change proto, keyfile, tz, bind and save"
runjs "document.getElementById('f_proto_from').value='sftp'; document.getElementById('f_password_from').value=''; document.getElementById('f_keyfile_from').value='/tmp/dummy_key'; document.getElementById('f_tz_from').value='+05:30'; document.getElementById('f_bind_from').value='192.168.1.100'; window.saveConfig();"
agent-browser wait 1500 >/dev/null 2>&1 || true

info "Test 6c: Verify notification"
capture_js "document.querySelector('.notification')?.textContent || ''"
NOTIF=$(read_result)
if echo "$NOTIF" | grep -q "updated"; then pass "Notification: '$NOTIF'"; else fail "Expected 'updated', got: '$NOTIF'"; fi

info "Test 6d: Protocol badge in table"
capture_js "document.querySelector('#tbody tr:nth-child(2) td:nth-child(2)').textContent"
PROTO=$(read_result)
if echo "$PROTO" | grep -q "SFTP"; then pass "Protocol: '$PROTO'"; else fail "Expected SFTP, got: '$PROTO'"; fi

info "Test 6e: Re-open and verify fields persisted"
runjs "document.querySelectorAll('#tbody tr:nth-child(2) button')[0].click()"
agent-browser wait 500 >/dev/null 2>&1 || true
assert_js "document.getElementById('f_proto_from').value" "sftp"
assert_js "document.getElementById('f_keyfile_from').value" "/tmp/dummy_key"
assert_js "document.getElementById('f_tz_from').value" "+05:30"
assert_js "document.getElementById('f_bind_from').value" "192.168.1.100"
runjs "window.closeModal()"
agent-browser wait 300 >/dev/null 2>&1 || true

# ══════════════════════════════════════════════════════════════════════
info "=== Test 7: Regex test in modal ==="
# ══════════════════════════════════════════════════════════════════════

info "Test 7a: Open add modal and test regex preview"
runjs "document.querySelector('button.btn-primary').click()"
agent-browser wait 500 >/dev/null 2>&1 || true

runjs "document.getElementById('f_filename_regexp').value='.*\\\\.csv$'; document.getElementById('f_filename_regexp').dispatchEvent(new Event('input',{bubbles:true})); document.getElementById('f_regex_test').value='report.csv'; document.getElementById('f_regex_test').dispatchEvent(new Event('input',{bubbles:true}));"
agent-browser wait 200 >/dev/null 2>&1 || true
assert_js "document.getElementById('regexResult').textContent" "MATCH"

info "Test 7b: Non-matching filename"
runjs "document.getElementById('f_regex_test').value='report.xlsx'; document.getElementById('f_regex_test').dispatchEvent(new Event('input',{bubbles:true}));"
agent-browser wait 200 >/dev/null 2>&1 || true
assert_js "document.getElementById('regexResult').textContent" "no match"

info "Test 7c: Invalid regex"
runjs "document.getElementById('f_filename_regexp').value='(invalid['; document.getElementById('f_filename_regexp').dispatchEvent(new Event('input',{bubbles:true}));"
agent-browser wait 200 >/dev/null 2>&1 || true
assert_js "document.getElementById('regexResult').textContent" "invalid"

runjs "window.closeModal()"
agent-browser wait 300 >/dev/null 2>&1 || true

# ══════════════════════════════════════════════════════════════════════
info "=== Test 8: Password visibility toggle ==="
# ══════════════════════════════════════════════════════════════════════

info "Test 8a: Toggle password field"
runjs "document.querySelectorAll('#tbody tr:first-child button')[0].click()"
agent-browser wait 500 >/dev/null 2>&1 || true
assert_js "document.getElementById('f_password_from').type" "password"
runjs "togglePw('f_password_from')"
assert_js "document.getElementById('f_password_from').type" "text"
runjs "togglePw('f_password_from')"
assert_js "document.getElementById('f_password_from').type" "password"
runjs "window.closeModal()"
agent-browser wait 300 >/dev/null 2>&1 || true

# ══════════════════════════════════════════════════════════════════════
info "=== Test 9: Refresh button ==="
# ══════════════════════════════════════════════════════════════════════

info "Test 9a: Refresh reloads data"
runjs "document.querySelector('button.btn-secondary').click()"
agent-browser wait 1000 >/dev/null 2>&1 || true
assert_js "document.querySelector('#headerInfo').textContent" "2 config entries loaded"

# ══════════════════════════════════════════════════════════════════════
info "=== Test 10: Escape closes modal ==="
# ══════════════════════════════════════════════════════════════════════

info "Test 10a: Escape key closes modal"
runjs "document.querySelectorAll('#tbody tr:first-child button')[0].click()"
agent-browser wait 500 >/dev/null 2>&1 || true
assert_js "document.querySelector('.modal-overlay').classList.contains('active')" "true"
agent-browser press Escape >/dev/null 2>&1 || true
agent-browser wait 300 >/dev/null 2>&1 || true
assert_js "document.querySelector('.modal-overlay').classList.contains('active')" "false"

# ══════════════════════════════════════════════════════════════════════
info "=== Test 11: Screenshot ==="
# ══════════════════════════════════════════════════════════════════════

info "Test 11: Take final screenshot"
agent-browser screenshot /tmp/test_web_agent_final.png >/dev/null 2>&1 || true
if [ -f /tmp/test_web_agent_final.png ]; then pass "Screenshot saved"; else fail "Screenshot not created"; fi

# ══════════════════════════════════════════════════════════════════════
info "=== Test 12: Duplicate entry ==="
# ══════════════════════════════════════════════════════════════════════

info "Test 12a: Click Duplicate on entry 0"
runjs "document.querySelectorAll('#tbody tr:first-child button')[1].click()"
agent-browser wait 500 >/dev/null 2>&1 || true

info "Test 12b: Modal title shows 'New Config (based on #0)'"
assert_js "document.querySelector('#modalTitle').textContent" "New Config (based on #0)"
assert_js "document.querySelector('.modal-overlay').classList.contains('active')" "true"

info "Test 12c: Form pre-filled with entry 0 data"
capture_js "document.getElementById('f_host_from').value"
HOST_VAL=$(read_result)
if [ "$HOST_VAL" != "" ]; then pass "host_from pre-filled: '$HOST_VAL'"; else fail "host_from empty"; fi

info "Test 12d: Comment has (copy) suffix"
capture_js "document.getElementById('f_comment').value"
COMMENT=$(read_result)
if echo "$COMMENT" | grep -q "(copy)"; then pass "Comment: '$COMMENT'"; else fail "Expected (copy) in comment, got: '$COMMENT'"; fi

info "Test 12e: Change host and save"
runjs "document.getElementById('f_host_from').value='duphost.local'; document.getElementById('f_host_from').dispatchEvent(new Event('input',{bubbles:true})); window.saveConfig();"
agent-browser wait 1500 >/dev/null 2>&1 || true

info "Test 12f: Verify created notification"
capture_js "document.querySelector('.notification')?.textContent || ''"
NOTIF=$(read_result)
if echo "$NOTIF" | grep -q "created"; then pass "Notification: '$NOTIF'"; else fail "Expected 'created', got: '$NOTIF'"; fi

info "Test 12g: Verify 3 entries"
assert_js "document.querySelectorAll('#tbody tr').length" "3"
assert_js "document.querySelector('#headerInfo').textContent" "3 config entries loaded"

info "Test 12h: Verify original entry 0 unchanged"
capture_js "document.querySelector('#tbody tr:first-child td:nth-child(3)').childNodes[0].textContent"
ORIG_HOST=$(read_result)
if echo "$ORIG_HOST" | grep -q "99.99.99.99"; then pass "Original entry intact: '$ORIG_HOST'"; else fail "Original entry changed: '$ORIG_HOST'"; fi

info "Test 12i: Verify on disk"
sleep 0.3
DISK_COUNT=$(grep -c "^{" "$CONFIG_FILE" 2>/dev/null || echo "0")
if [ "$DISK_COUNT" = "3" ]; then pass "3 JSONL lines on disk"; else fail "Expected 3 lines, got $DISK_COUNT"; fi

# ══════════════════════════════════════════════════════════════════════
info "=== Test 13: Swap source/target ==="
# ══════════════════════════════════════════════════════════════════════

info "Test 13a: Open Edit on entry 0"
runjs "document.querySelectorAll('#tbody tr:first-child button')[0].click()"
agent-browser wait 500 >/dev/null 2>&1 || true
assert_js "document.querySelector('.modal-overlay').classList.contains('active')" "true"

info "Test 13b: Capture pre-swap values"
capture_js "document.getElementById('f_host_from').value"
HOST_FROM=$(read_result)
capture_js "document.getElementById('f_host_to').value"
HOST_TO=$(read_result)
capture_js "document.getElementById('f_port_from').value"
PORT_FROM=$(read_result)
capture_js "document.getElementById('f_port_to').value"
PORT_TO=$(read_result)

info "Test 13c: Click Swap button"
runjs "document.querySelector('.swap-row button').click()"
agent-browser wait 300 >/dev/null 2>&1 || true

info "Test 13d: Verify host swapped"
assert_js "document.getElementById('f_host_from').value" "$HOST_TO"
assert_js "document.getElementById('f_host_to').value" "$HOST_FROM"

info "Test 13e: Verify port swapped"
assert_js "document.getElementById('f_port_from').value" "$PORT_TO"
assert_js "document.getElementById('f_port_to').value" "$PORT_FROM"

info "Test 13f: Close modal without saving"
runjs "document.querySelector('.modal-overlay').click()"
agent-browser wait 300 >/dev/null 2>&1 || true

# ══════════════════════════════════════════════════════════════════════
echo ""
echo "=================================================="
if [ "$FAIL_COUNT" -eq 0 ]; then
    echo -e "${GREEN}All UI tests passed!${NC}"
else
    echo -e "${RED}$FAIL_COUNT test(s) failed${NC}"
fi
echo "=================================================="

[ "$FAIL_COUNT" -eq 0 ]
