#!/usr/bin/env bash
#
# test_conn_timeout.sh - Tests FTP connection timeout functionality
#
# This test verifies that the -t (connect-timeout) parameter works correctly
# by attempting to connect to a non-routable IP address which will cause
# a TCP connection timeout.
#
# Usage: ./test_conn_timeout.sh

set -e

echo "=== FTP Connection Timeout Test ==="
echo

# Build project
echo "Building project..."
cargo build --quiet

# Timeout to use for testing (in seconds)
TEST_TIMEOUT=5

echo "Testing with ${TEST_TIMEOUT}s timeout to non-routable IP 10.255.255.1..."
echo

# Create test config with non-routable IP (will cause TCP timeout)
cat > /tmp/test_timeout.jsonl <<'EOF'
{"host_from":"10.255.255.1","port_from":21,"login_from":"test","password_from":"test","path_from":"/","host_to":"10.255.255.2","port_to":21,"login_to":"test","password_to":"test","path_to":"/","age":1,"filename_regexp":".*"}
EOF

# Measure how long the connection attempt takes
START_TIME=$(date +%s)

./target/debug/iftpfm2 -t $TEST_TIMEOUT /tmp/test_timeout.jsonl 2>&1 | tee /tmp/timeout_test.log || true

END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))

echo
echo "Connection attempt took ${ELAPSED} seconds (expected ~${TEST_TIMEOUT}s)"

# Verify timeout occurred
if grep -q "timeout" /tmp/timeout_test.log; then
    echo "✓ Timeout message found in logs"
else
    echo "✗ ERROR: No timeout message found in logs"
    rm -f /tmp/test_timeout.jsonl /tmp/timeout_test.log
    exit 1
fi

# Verify the elapsed time is close to the expected timeout
# Allow 2 seconds tolerance for system load
if [ $ELAPSED -ge $TEST_TIMEOUT ] && [ $ELAPSED -le $((TEST_TIMEOUT + 2)) ]; then
    echo "✓ Timeout duration is correct (within tolerance)"
else
    echo "✗ ERROR: Timeout duration incorrect (got ${ELAPSED}s, expected ~${TEST_TIMEOUT}s)"
    rm -f /tmp/test_timeout.jsonl /tmp/timeout_test.log
    exit 1
fi

# Verify error message contains the timeout value
if grep -q "${TEST_TIMEOUT}s timeout" /tmp/timeout_test.log; then
    echo "✓ Error message shows correct timeout value"
else
    echo "✗ ERROR: Error message doesn't show timeout value"
    rm -f /tmp/test_timeout.jsonl /tmp/timeout_test.log
    exit 1
fi

echo
echo "=== Testing with very short timeout (2s) ==="

# Also test with a 2 second timeout to ensure it works with different values
START_TIME2=$(date +%s)

./target/debug/iftpfm2 -t 2 /tmp/test_timeout.jsonl 2>&1 | tee /tmp/timeout_test2.log || true

END_TIME2=$(date +%s)
ELAPSED2=$((END_TIME2 - START_TIME2))

echo
echo "Connection attempt took ${ELAPSED2} seconds (expected ~2s)"

if [ $ELAPSED2 -ge 2 ] && [ $ELAPSED2 -le 4 ]; then
    echo "✓ Short timeout (2s) works correctly"
else
    echo "✗ ERROR: Short timeout incorrect (got ${ELAPSED2}s, expected ~2s)"
    rm -f /tmp/test_timeout.jsonl /tmp/timeout_test2.log
    exit 1
fi

echo
echo "=== SUCCESS: All timeout tests passed ==="

# Cleanup
rm -f /tmp/test_timeout.jsonl /tmp/timeout_test.log /tmp/timeout_test2.log

exit 0
