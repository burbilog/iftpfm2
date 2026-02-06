#!/usr/bin/env bash
#
# iftpfm2 SFTP SSH key test script
# Tests SFTP SSH key authentication (with and without passphrase)

set -e

# Add cargo to PATH if not already there
if ! command -v cargo &>/dev/null && [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

# Cleanup trap
cleanup() {
    docker rm -f sftp-test-src-key 2>/dev/null || true
    docker rm -f sftp-test-dst-key 2>/dev/null || true
    rm -rf "$TEST_KEYS_DIR" 2>/dev/null || true
    rm -rf "$TEST_UPLOAD_SRC" 2>/dev/null || true
    rm -rf "$TEST_UPLOAD_DST" 2>/dev/null || true
    rm -f /tmp/sftp_key_config.jsonl 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Build the project first
echo "Building iftpfm2..."
cargo build

# Generate SSH keys
echo "Generating test SSH keys..."
TEST_KEYS_DIR=$(mktemp -d)
chmod 700 "$TEST_KEYS_DIR"

# Key without passphrase
ssh-keygen -t rsa -f "$TEST_KEYS_DIR/id_rsa_nopass" -N "" -q

# Key with passphrase
ssh-keygen -t rsa -f "$TEST_KEYS_DIR/id_rsa_pass" -N "test_passphrase" -q

echo "Keys generated in $TEST_KEYS_DIR"

# Create upload directories
TEST_UPLOAD_SRC=$(mktemp -d)
TEST_UPLOAD_DST=$(mktemp -d)
mkdir -p "$TEST_UPLOAD_SRC"
mkdir -p "$TEST_UPLOAD_DST"
# Make directories writable by container user (UID 1001)
chmod 777 "$TEST_UPLOAD_SRC"
chmod 777 "$TEST_UPLOAD_DST"

# Create test files
echo "test1" > "$TEST_UPLOAD_SRC/file1.txt"
echo "test2" > "$TEST_UPLOAD_SRC/file2.txt"
touch -d '10 seconds ago' "$TEST_UPLOAD_SRC"/*.txt

# ==============================================================================
# Test 1: SSH key without passphrase
# ==============================================================================
echo ""
echo "=========================================="
echo "Test 1: SSH key without passphrase"
echo "=========================================="

# Build SFTP test image
echo "Building SFTP test image..."
docker build -f Dockerfile.sftp_test -t iftpfm2-sftp-test . > /dev/null 2>&1

echo "Starting source SFTP server on port 3224..."
docker run -d --name sftp-test-src-key -p 3224:22 \
    -v "$TEST_UPLOAD_SRC:/home/sftpuser/upload" \
    -v "$TEST_KEYS_DIR/id_rsa_nopass.pub:/etc/sftp/keys/sftpuser/.ssh/authorized_keys:ro" \
    iftpfm2-sftp-test

echo "Starting destination SFTP server on port 3225..."
docker run -d --name sftp-test-dst-key -p 3225:22 \
    -v "$TEST_UPLOAD_DST:/home/sftpuser/upload" \
    -v "$TEST_KEYS_DIR/id_rsa_nopass.pub:/etc/sftp/keys/sftpuser/.ssh/authorized_keys:ro" \
    iftpfm2-sftp-test

# Wait for SFTP servers
echo "Waiting for SFTP servers..."
for i in {1..30}; do
    if nc -z localhost 3224 2>/dev/null && nc -z localhost 3225 2>/dev/null; then
        sleep 2
        echo "SFTP servers ready!"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "ERROR: SFTP servers not ready"
        docker logs sftp-test-src-key 2>&1 | tail -10
        exit 1
    fi
    sleep 0.5
done

# Run transfer test
echo '{"proto_from":"sftp","host_from":"localhost","port_from":3224,"login_from":"sftpuser","keyfile_from":"'"$TEST_KEYS_DIR"'/id_rsa_nopass","path_from":"/upload","proto_to":"sftp","host_to":"localhost","port_to":3225,"login_to":"sftpuser","keyfile_to":"'"$TEST_KEYS_DIR"'/id_rsa_nopass","path_to":"/upload","age":1,"filename_regexp":".*"}' > /tmp/sftp_key_config.jsonl

./target/debug/iftpfm2 /tmp/sftp_key_config.jsonl

# Verify
if [ -f "$TEST_UPLOAD_DST/file1.txt" ] && [ -f "$TEST_UPLOAD_DST/file2.txt" ]; then
    echo "SUCCESS: SSH key authentication (no passphrase) works!"
else
    echo "ERROR: SSH key transfer failed"
    ls -la "$TEST_UPLOAD_DST" || true
    docker logs sftp-test-src-key 2>&1 | tail -20
    exit 1
fi

# ==============================================================================
# Test 2: SSH key WITH passphrase
# ==============================================================================
echo ""
echo "=========================================="
echo "Test 2: SSH key WITH passphrase"
echo "=========================================="

# Restart containers with passphrase key
docker rm -f sftp-test-src-key sftp-test-dst-key 2>/dev/null || true
sleep 1

# Clean destination for new test
rm -rf "$TEST_UPLOAD_DST"/*
mkdir -p "$TEST_UPLOAD_DST"
chmod 777 "$TEST_UPLOAD_DST"

# Create new test files
echo "pass1" > "$TEST_UPLOAD_SRC/file3.txt"
echo "pass2" > "$TEST_UPLOAD_SRC/file4.txt"
touch -d '10 seconds ago' "$TEST_UPLOAD_SRC"/*.txt

echo "Starting source SFTP server (key with passphrase) on port 3224..."
docker run -d --name sftp-test-src-key -p 3224:22 \
    -v "$TEST_UPLOAD_SRC:/home/sftpuser/upload" \
    -v "$TEST_KEYS_DIR/id_rsa_pass.pub:/etc/sftp/keys/sftpuser/.ssh/authorized_keys:ro" \
    iftpfm2-sftp-test

echo "Starting destination SFTP server (key with passphrase) on port 3225..."
docker run -d --name sftp-test-dst-key -p 3225:22 \
    -v "$TEST_UPLOAD_DST:/home/sftpuser/upload" \
    -v "$TEST_KEYS_DIR/id_rsa_pass.pub:/etc/sftp/keys/sftpuser/.ssh/authorized_keys:ro" \
    iftpfm2-sftp-test

# Wait for SFTP servers
for i in {1..30}; do
    if nc -z localhost 3224 2>/dev/null && nc -z localhost 3225 2>/dev/null; then
        sleep 2
        break
    fi
    sleep 0.5
done

# Run transfer test with passphrase
echo '{"proto_from":"sftp","host_from":"localhost","port_from":3224,"login_from":"sftpuser","keyfile_from":"'"$TEST_KEYS_DIR"'/id_rsa_pass","keyfile_pass_from":"test_passphrase","path_from":"/upload","proto_to":"sftp","host_to":"localhost","port_to":3225,"login_to":"sftpuser","keyfile_to":"'"$TEST_KEYS_DIR"'/id_rsa_pass","keyfile_pass_to":"test_passphrase","path_to":"/upload","age":1,"filename_regexp":".*"}' > /tmp/sftp_key_config.jsonl

./target/debug/iftpfm2 /tmp/sftp_key_config.jsonl

# Verify
if [ -f "$TEST_UPLOAD_DST/file3.txt" ] && [ -f "$TEST_UPLOAD_DST/file4.txt" ]; then
    echo "SUCCESS: SSH key authentication with passphrase works!"
    if [ "$(cat "$TEST_UPLOAD_DST/file3.txt")" = "pass1" ]; then
        echo "SUCCESS: File content is correct!"
    fi
else
    echo "ERROR: SSH key with passphrase transfer failed"
    ls -la "$TEST_UPLOAD_DST" || true
    docker logs sftp-test-src-key 2>&1 | tail -20
    exit 1
fi

echo ""
echo "=========================================="
echo "=== ALL SSH KEY TESTS PASSED! ==="
echo "=========================================="
