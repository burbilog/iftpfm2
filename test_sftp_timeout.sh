#!/usr/bin/env bash
#
# test_sftp_timeout.sh - Tests SFTP connection timeout functionality
#
# This test verifies that the -t (connect-timeout) parameter works correctly
# for SFTP connections by attempting to connect to a non-routable IP address.
#
# Usage: ./test_sftp_timeout.sh

set -e

# Add cargo to PATH if not already there
if ! command -v cargo &>/dev/null && [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

echo "=== SFTP Connection Timeout Test ==="
echo

# Build project
echo "Building project..."
cargo build --quiet

# Timeout to use for testing (in seconds)
TEST_TIMEOUT=5

echo "Testing SFTP with ${TEST_TIMEOUT}s timeout to non-routable IP 10.255.255.1..."
echo

# Create test config with non-routable IP (will cause TCP timeout)
cat > /tmp/test_sftp_timeout.jsonl <<'EOF'
{"host_from":"10.255.255.1","port_from":22,"login_from":"test","password_from":"test","path_from":"/","proto_from":"sftp","host_to":"10.255.255.2","port_to":22,"login_to":"test","password_to":"test","path_to":"/","proto_to":"sftp","age":1,"filename_regexp":".*"}
EOF

# Measure how long the connection attempt takes
START_TIME=$(date +%s)

./target/debug/iftpfm2 -t $TEST_TIMEOUT /tmp/test_sftp_timeout.jsonl 2>&1 | tee /tmp/sftp_timeout_test.log || true

END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))

echo
echo "SFTP connection attempt took ${ELAPSED} seconds (expected ~${TEST_TIMEOUT}s)"

# Verify timeout occurred
if grep -q "timeout\|Timeout\|TIMEDOUT" /tmp/sftp_timeout_test.log; then
    echo "✓ Timeout message found in logs"
else
    echo "✗ ERROR: No timeout message found in logs"
    cat /tmp/sftp_timeout_test.log
    rm -f /tmp/test_sftp_timeout.jsonl /tmp/sftp_timeout_test.log
    exit 1
fi

# Verify the elapsed time is close to the expected timeout
# Allow 3 seconds tolerance for SSH handshake overhead
if [ $ELAPSED -ge $TEST_TIMEOUT ] && [ $ELAPSED -le $((TEST_TIMEOUT + 3)) ]; then
    echo "✓ Timeout duration is correct (within tolerance)"
else
    echo "✗ ERROR: Timeout duration incorrect (got ${ELAPSED}s, expected ~${TEST_TIMEOUT}s)"
    rm -f /tmp/test_sftp_timeout.jsonl /tmp/sftp_timeout_test.log
    exit 1
fi

# Verify error message contains the timeout value
if grep -q "${TEST_TIMEOUT}s timeout" /tmp/sftp_timeout_test.log; then
    echo "✓ Error message shows correct timeout value"
else
    echo "✗ ERROR: Error message doesn't show timeout value"
    rm -f /tmp/test_sftp_timeout.jsonl /tmp/sftp_timeout_test.log
    exit 1
fi

echo
echo "=== Testing SFTP with very short timeout (2s) ==="

# Also test with a 2 second timeout to ensure it works with different values
START_TIME2=$(date +%s)

./target/debug/iftpfm2 -t 2 /tmp/test_sftp_timeout.jsonl 2>&1 | tee /tmp/sftp_timeout_test2.log || true

END_TIME2=$(date +%s)
ELAPSED2=$((END_TIME2 - START_TIME2))

echo
echo "SFTP connection attempt took ${ELAPSED2} seconds (expected ~2s)"

if [ $ELAPSED2 -ge 2 ] && [ $ELAPSED2 -le 5 ]; then
    echo "✓ Short timeout (2s) works correctly for SFTP"
else
    echo "✗ ERROR: Short timeout incorrect (got ${ELAPSED2}s, expected ~2s)"
    rm -f /tmp/test_sftp_timeout.jsonl /tmp/sftp_timeout_test2.log
    exit 1
fi

echo
echo "=== SUCCESS: All SFTP timeout tests passed ==="

# Cleanup
rm -f /tmp/test_sftp_timeout.jsonl /tmp/sftp_timeout_test.log /tmp/sftp_timeout_test2.log

exit 0
