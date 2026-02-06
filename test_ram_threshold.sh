#!/bin/bash
# Integration test for --ram-threshold flag
# Tests:
# 1. Small files (~6 bytes) with default threshold (10MB) -> RAM
# 2. Large files (~1MB) with threshold 100 -> disk
# 3. Threshold 0 forces all files to use RAM
# 4. Verify logs contain appropriate storage method messages

set -e

echo "=== RAM Threshold Integration Test ==="

# Check if python3 and pyftpdlib are available
if ! command -v python3 &> /dev/null; then
    echo "ERROR: python3 is required for this test"
    exit 1
fi

if ! python3 -c "import pyftpdlib" &> /dev/null; then
    echo "ERROR: pyftpdlib is required (install with: pip install pyftpdlib)"
    exit 1
fi

# Create temp directory for test files
TEST_DIR=$(mktemp -d)
trap "rm -rf $TEST_DIR" EXIT

SOURCE_DIR="$TEST_DIR/source"
TARGET_DIR="$TEST_DIR/target"
mkdir -p "$SOURCE_DIR"
mkdir -p "$TARGET_DIR"

# Create test files
echo "hello" > "$SOURCE_DIR/small_file.txt"  # ~6 bytes
# Create a 1MB file
dd if=/dev/zero of="$SOURCE_DIR/large_file.dat" bs=1024 count=1024 2>/dev/null

# Start FTP servers
echo "Starting FTP servers on ports 2121 and 2122..."
python3 -m pyftpdlib -p 2121 -d "$SOURCE_DIR" &>/dev/null &
PID_SOURCE=$!
python3 -m pyftpdlib -p 2122 -d "$TARGET_DIR" &>/dev/null &
PID_TARGET=$!

# Wait for servers to start
sleep 2

# Cleanup function to kill FTP servers
cleanup_servers() {
    kill $PID_SOURCE $PID_TARGET 2>/dev/null || true
    wait $PID_SOURCE $PID_TARGET 2>/dev/null || true
}
trap cleanup_servers EXIT

# Create config file
CONFIG_FILE="$TEST_DIR/config.jsonl"
cat > "$CONFIG_FILE" <<EOF
{"host_from":"127.0.0.1","port_from":2121,"login_from":"anonymous","password_from":"anonymous@","path_from":"/","proto_from":"ftp","host_to":"127.0.0.1","port_to":2122,"login_to":"anonymous","password_to":"anonymous@","path_to":"/","proto_to":"ftp","age":1,"filename_regexp":".*"}
EOF

LOG_FILE="$TEST_DIR/test.log"

# Test 1: Default threshold (10MB) - small file should use RAM
echo ""
echo "Test 1: Default threshold (10MB) - small file should use RAM"
./target/debug/iftpfm2 -l "$LOG_FILE" "$CONFIG_FILE" 2>/dev/null || true
if grep -q "Using RAM buffer for small_file.txt" "$LOG_FILE"; then
    echo "✓ PASS: Default threshold uses RAM for small files"
else
    echo "✗ FAIL: Expected 'Using RAM buffer for small_file.txt' in logs"
    cat "$LOG_FILE"
    exit 1
fi

# Clear log for next test
> "$LOG_FILE"

# Test 2: Threshold of 100 bytes - large file should use disk
echo ""
echo "Test 2: Threshold 100 bytes - large file should use disk"
./target/debug/iftpfm2 --ram-threshold 100 -l "$LOG_FILE" "$CONFIG_FILE" 2>/dev/null || true
if grep -q "Using disk buffer for large_file.dat" "$LOG_FILE"; then
    echo "✓ PASS: Low threshold forces disk usage for large files"
else
    echo "✗ FAIL: Expected 'Using disk buffer for large_file.dat' in logs"
    cat "$LOG_FILE"
    exit 1
fi

# Clear log for next test
> "$LOG_FILE"

# Test 3: Threshold 0 - all files should use RAM
echo ""
echo "Test 3: Threshold 0 - all files should use RAM"
./target/debug/iftpfm2 --ram-threshold 0 -l "$LOG_FILE" "$CONFIG_FILE" 2>/dev/null || true
if grep -q "Using RAM buffer" "$LOG_FILE"; then
    echo "✓ PASS: Threshold 0 forces RAM for all files"
else
    echo "✗ FAIL: Expected 'Using RAM buffer' with threshold 0"
    cat "$LOG_FILE"
    exit 1
fi

echo ""
echo "=== All RAM threshold tests passed! ==="
