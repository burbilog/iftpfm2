#!/usr/bin/env bash
#
# iftpfm2 FTPS test script
# Tests FTPS connections with self-signed certificates using --insecure-skip-verify
# and upload verification using --size-check flag
# requires python3, pyftpdlib with TLS support, and openssl

set -e

# Add cargo to PATH if not already there
if ! command -v cargo &>/dev/null && [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

# Build the project first
echo "Building iftpfm2..."
cargo build

# Create directories for FTP servers
echo "Creating test directories..."
mkdir -p /tmp/ftps1
mkdir -p /tmp/ftps2

# Generate self-signed certificate for FTPS
echo "Generating self-signed certificate..."
CERT_DIR=$(mktemp -d)
openssl req -x509 -newkey rsa:2048 -keyout "$CERT_DIR/key.pem" -out "$CERT_DIR/cert.pem" \
    -days 1 -nodes -subj "/CN=localhost" 2>/dev/null

# Create Python script for FTPS server
echo "Creating FTPS server script..."
cat > "$CERT_DIR/ftps_server.py" << 'PYTHON_SCRIPT'
import sys
import os
import time
from pyftpdlib.authorizers import DummyAuthorizer
from pyftpdlib.handlers import TLS_FTPHandler
from pyftpdlib.servers import FTPServer

def main():
    port = int(sys.argv[1])
    directory = sys.argv[2]
    cert_file = sys.argv[3]
    key_file = sys.argv[4]

    authorizer = DummyAuthorizer()
    authorizer.add_user(directory, directory, directory, perm='elradfmw')

    handler = TLS_FTPHandler
    handler.certfile = cert_file
    handler.keyfile = key_file
    handler.tls_control_required = True
    handler.tls_data_required = True
    handler.authorizer = authorizer

    server = FTPServer(("127.0.0.1", port), handler)
    print(f"FTPS server started on port {port}", flush=True)
    server.serve_forever()

if __name__ == "__main__":
    main()
PYTHON_SCRIPT

# Cleanup trap - ensure servers are killed on exit
cleanup() {
    if [ -n "$ftps1_pid" ]; then
        kill $ftps1_pid 2>/dev/null || true
    fi
    if [ -n "$ftps2_pid" ]; then
        kill $ftps2_pid 2>/dev/null || true
    fi
    rm -rf /tmp/ftps1 /tmp/ftps2 "$CERT_DIR" 2>/dev/null || true
    rm -f /tmp/ftps_config.jsonl 2>/dev/null || true
}

trap cleanup EXIT INT TERM

# Start first FTPS server on port 2123
echo "Starting first FTPS server on port 2123..."
python3 "$CERT_DIR/ftps_server.py" 2123 /tmp/ftps1 "$CERT_DIR/cert.pem" "$CERT_DIR/key.pem" 2>/dev/null &
ftps1_pid=$!

# Start second FTPS server on port 2124
echo "Starting second FTPS server on port 2124..."
python3 "$CERT_DIR/ftps_server.py" 2124 /tmp/ftps2 "$CERT_DIR/cert.pem" "$CERT_DIR/key.pem" 2>/dev/null &
ftps2_pid=$!

# Wait for servers to start
echo "Waiting for FTPS servers to be ready..."
for i in {1..60}; do
    if nc -z localhost 2123 2>/dev/null && nc -z localhost 2124 2>/dev/null; then
        echo "FTPS servers are ready!"
        break
    fi
    if [ $i -eq 60 ]; then
        echo "ERROR: FTPS servers did not start in time"
        exit 1
    fi
    sleep 0.5
done

# Create test files in the first server's directory
echo "Creating test files in the source directory..."
echo "test1" > /tmp/ftps1/test1.txt
echo "test2" > /tmp/ftps1/test2.txt
echo "test3" > /tmp/ftps1/test3.txt

# Create config file for iftpfm2 with FTPS protocol
echo "Creating config file with FTPS protocol..."
echo '{"host_from":"localhost","port_from":2123,"login_from":"/tmp/ftps1","password_from":"/tmp/ftps1","path_from":"/","proto_from":"ftps","host_to":"localhost","port_to":2124,"login_to":"/tmp/ftps2","password_to":"/tmp/ftps2","path_to":"/","proto_to":"ftps","age":1,"filename_regexp":".*\\.txt"}' > /tmp/ftps_config.jsonl

# Set file modification times to 10 seconds ago so they're immediately "old enough"
touch -d '10 seconds ago' /tmp/ftps1/*.txt

# Test without --insecure-skip-verify (should fail)
echo ""
echo "=== Test 1: Without --insecure-skip-verify (should fail) ==="
if ./target/debug/iftpfm2 /tmp/ftps_config.jsonl 2>&1 | grep -q "certificate verify failed\|SSL routines\|error"; then
    echo "EXPECTED: Connection failed due to certificate verification"
else
    echo "UNEXPECTED: Connection succeeded when it should have failed"
fi

# Test with --insecure-skip-verify (should succeed)
echo ""
echo "=== Test 2: With --insecure-skip-verify (should succeed) ==="
if ./target/debug/iftpfm2 --insecure-skip-verify /tmp/ftps_config.jsonl; then
    echo "SUCCESS: Transfer completed with --insecure-skip-verify"
else
    echo "ERROR: Transfer failed even with --insecure-skip-verify"
    exit 1
fi

# Clear destination directory for next test
rm -f /tmp/ftps2/*.txt

# Test with --size-check (should verify file sizes)
echo ""
echo "=== Test 3: With --size-check (should verify file sizes) ==="
OUTPUT=$(./target/debug/iftpfm2 --insecure-skip-verify --size-check /tmp/ftps_config.jsonl 2>&1)
if echo "$OUTPUT" | grep -q "Verifying upload of"; then
    echo "SUCCESS: Upload verification messages found"
    if echo "$OUTPUT" | grep -q "Upload verification passed"; then
        echo "SUCCESS: Upload verification passed for files"
    else
        echo "WARNING: No 'Upload verification passed' messages found"
    fi
else
    echo "WARNING: No upload verification messages found (SIZE command may not be supported)"
fi

# Check for verification warnings
if echo "$OUTPUT" | grep -q "WARNING: Upload verification FAILED"; then
    echo "ERROR: Upload verification failed"
    exit 1
fi

# Verify files were transferred
echo ""
echo "=== Verifying file transfer ==="
if [ -f "/tmp/ftps2/test1.txt" ] && [ -f "/tmp/ftps2/test2.txt" ] && [ -f "/tmp/ftps2/test3.txt" ]; then
    echo "SUCCESS: All files transferred to destination"
    if [ -f "/tmp/ftps2/test1.txt" ] && [ "$(cat /tmp/ftps2/test1.txt)" = "test1" ]; then
        echo "SUCCESS: File content is correct"
    else
        echo "ERROR: File content mismatch"
    fi
else
    echo "ERROR: Not all files were transferred"
    ls -la /tmp/ftps2/
fi

# Cleanup (handled by trap, but keep explicit kill here for clarity)
echo ""
echo "Cleanup..."
kill $ftps1_pid $ftps2_pid 2>/dev/null || true
# Trap will handle full cleanup

echo "FTPS test completed!"
