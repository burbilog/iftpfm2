#!/usr/bin/env bash
#
# iftpfm2 SFTP test script using Docker atmoz/sftp
# Tests SFTP connections with password authentication against a real SFTP server

set -e

# Add cargo to PATH if not already there
if ! command -v cargo &>/dev/null && [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

# Cleanup trap - ensure containers are removed on exit
cleanup() {
    docker rm -f $SRC_CID 2>/dev/null || true
    docker rm -f $DST_CID 2>/dev/null || true
    rm -rf "$SRC_DIR" "$DST_DIR" "$UPLOAD_DIR" 2>/dev/null || true
    rm -f /tmp/sftp_config.jsonl 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Build the project first
echo "Building iftpfm2..."
cargo build

# Create directories for SFTP servers
echo "Creating test directories..."
SRC_BASE=$(mktemp -d)
DST_BASE=$(mktemp -d)
SRC_UPLOAD="$SRC_BASE/upload"
DST_UPLOAD="$DST_BASE/upload"
mkdir -p "$SRC_UPLOAD" "$DST_UPLOAD"

# Make directories writable by the container user (UID 1001)
chmod 777 "$SRC_BASE" "$DST_BASE"
chmod 777 "$SRC_UPLOAD" "$DST_UPLOAD"

# Create test files in the source upload directory BEFORE starting containers
echo "Creating test files in the source directory..."
echo "test1" > "$SRC_UPLOAD/file1.txt"
echo "test2" > "$SRC_UPLOAD/file2.txt"
echo "test3" > "$SRC_UPLOAD/file3.txt"

# Set file modification times to 10 seconds ago so they're immediately "old enough"
touch -d '10 seconds ago' "$SRC_UPLOAD"/*.txt

# Start first SFTP server (source) on port 3222
echo "Starting source SFTP server on port 3222..."
SRC_CID="sftp-test-src-$$"
docker run -d --name $SRC_CID \
    -p 3222:22 \
    -v "$SRC_BASE:/home/test" \
    atmoz/sftp \
    test:pass:1001 > /dev/null

# Start second SFTP server (destination) on port 3223
echo "Starting destination SFTP server on port 3223..."
DST_CID="sftp-test-dst-$$"
docker run -d --name $DST_CID \
    -p 3223:22 \
    -v "$DST_BASE:/home/test" \
    atmoz/sftp \
    test:pass:1001 > /dev/null

# Wait for SFTP servers to be ready
echo "Waiting for SFTP servers to be ready..."
for i in {1..60}; do
    if nc -z localhost 3222 2>/dev/null && nc -z localhost 3223 2>/dev/null; then
        # Additional wait for SSH to be fully ready
        sleep 2
        echo "SFTP servers are ready!"
        break
    fi
    if [ $i -eq 60 ]; then
        echo "ERROR: SFTP servers did not start in time"
        exit 1
    fi
    sleep 0.5
done

# Verify files are visible inside container
echo "Checking file visibility in source container..."
docker exec $SRC_CID ls -la /home/test/upload/ || {
    echo "ERROR: Files not visible in container"
    exit 1
}

# Test 1: Basic SFTP transfer without delete
# Use /upload as the path since we created an upload subdirectory
echo ""
echo "=== Test 1: SFTP transfer without delete ==="
echo '{"proto_from":"sftp","host_from":"localhost","port_from":3222,"login_from":"test","password_from":"pass","path_from":"/upload","proto_to":"sftp","host_to":"localhost","port_to":3223,"login_to":"test","password_to":"pass","path_to":"/upload","age":1,"filename_regexp":".*"}' > /tmp/sftp_config.jsonl

./target/debug/iftpfm2 /tmp/sftp_config.jsonl

# Verify files were transferred to destination
echo ""
echo "=== Verifying file transfer ==="
if [ -f "$DST_UPLOAD/file1.txt" ] && [ -f "$DST_UPLOAD/file2.txt" ] && [ -f "$DST_UPLOAD/file3.txt" ]; then
    echo "SUCCESS: All files transferred to destination"
    if [ "$(cat "$DST_UPLOAD/file1.txt")" = "test1" ]; then
        echo "SUCCESS: File content is correct"
    else
        echo "ERROR: File content mismatch"
        exit 1
    fi
else
    echo "ERROR: Files were not transferred"
    ls -la "$DST_UPLOAD" || true
    exit 1
fi

# Verify source files still exist (no -d flag used)
if [ -f "$SRC_UPLOAD/file1.txt" ]; then
    echo "SUCCESS: Source files still exist (no -d flag)"
else
    echo "ERROR: Source files were deleted without -d flag"
    exit 1
fi

# Test 2: SFTP transfer with delete (-d flag)
echo ""
echo "=== Test 2: SFTP transfer with delete (-d flag) ==="

# Create new test files
echo "test4" > "$SRC_UPLOAD/file4.txt"
echo "test5" > "$SRC_UPLOAD/file5.txt"
touch -d '10 seconds ago' "$SRC_UPLOAD"/*.txt

./target/debug/iftpfm2 -d /tmp/sftp_config.jsonl

# Verify files were transferred
if [ -f "$DST_UPLOAD/file4.txt" ] && [ -f "$DST_UPLOAD/file5.txt" ]; then
    echo "SUCCESS: New files transferred"
else
    echo "ERROR: New files were not transferred"
    exit 1
fi

# Verify source files were deleted
if [ ! -f "$SRC_UPLOAD/file4.txt" ] && [ ! -f "$SRC_UPLOAD/file5.txt" ]; then
    echo "SUCCESS: Source files deleted (with -d flag)"
else
    echo "ERROR: Source files still exist with -d flag"
    exit 1
fi

# Test 3: Regex filter
echo ""
echo "=== Test 3: Regex filter (only .log files) ==="

echo "log1" > "$SRC_UPLOAD/test1.log"
echo "log2" > "$SRC_UPLOAD/test2.log"
echo "data" > "$SRC_UPLOAD/test3.dat"
touch -d '10 seconds ago' "$SRC_UPLOAD"/*

echo '{"proto_from":"sftp","host_from":"localhost","port_from":3222,"login_from":"test","password_from":"pass","path_from":"/upload","proto_to":"sftp","host_to":"localhost","port_to":3223,"login_to":"test","password_to":"pass","path_to":"/upload","age":1,"filename_regexp":".*\\.log"}' > /tmp/sftp_config.jsonl

./target/debug/iftpfm2 /tmp/sftp_config.jsonl

if [ -f "$DST_UPLOAD/test1.log" ] && [ -f "$DST_UPLOAD/test2.log" ] && [ ! -f "$DST_UPLOAD/test3.dat" ]; then
    echo "SUCCESS: Regex filter worked correctly"
else
    echo "ERROR: Regex filter failed"
    exit 1
fi

echo ""
echo "=========================================="
echo "=== ALL SFTP TESTS PASSED! ==="
echo "=========================================="
echo "Tested:"
echo "  - Password authentication"
echo "  - Delete flag (-d)"
echo "  - Regex filtering"
echo ""
echo "NOTE: SSH key authentication tests are in test_sftp_keys_docker.sh"
echo "Run 'make test-sftp-keys' to test SSH key auth with passphrase"
echo "=========================================="
