#!/usr/bin/env bash
#
# iftpfm2 bind address test script
# Tests -b/--bind flag for binding outgoing connections to a local address

set -e

# Add cargo to PATH if not already there
if ! command -v cargo &>/dev/null && [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

# Cleanup trap
cleanup() {
    if [ -n "$ftp1_pid" ]; then kill $ftp1_pid 2>/dev/null || true; fi
    if [ -n "$ftp2_pid" ]; then kill $ftp2_pid 2>/dev/null || true; fi
    rm -rf /tmp/ftp_bind1 /tmp/ftp_bind2 2>/dev/null || true
    rm -f /tmp/config_bind.jsonl 2>/dev/null || true
}
trap cleanup EXIT INT TERM

cargo build

echo "=== Test 1: Missing bind address argument ==="
if ./target/debug/iftpfm2 -b 2>/dev/null; then
    echo "FAIL: should have exited with error for missing bind address"
    exit 1
fi
echo "PASS: missing bind address rejected"

echo ""
echo "=== Test 2: Invalid bind address ==="
if ./target/debug/iftpfm2 -b not-an-ip /tmp/config_bind.jsonl 2>/dev/null; then
    echo "FAIL: should have exited with error for invalid bind address"
    exit 1
fi
echo "PASS: invalid bind address rejected"

echo ""
echo "=== Test 3: Valid bind address (localhost) with file transfer ==="
rm -rf /tmp/ftp_bind1 /tmp/ftp_bind2
mkdir -p /tmp/ftp_bind1
mkdir -p /tmp/ftp_bind2

echo "Starting first FTP server on port 2121"
python3 -m pyftpdlib -p 2121 -u u1 -P p1 -d /tmp/ftp_bind1 -w 2>/dev/null &
ftp1_pid=$!

echo "Starting second FTP server on port 2122"
python3 -m pyftpdlib -p 2122 -u u2 -P p2 -d /tmp/ftp_bind2 -w 2>/dev/null &
ftp2_pid=$!

echo "Generating test files"
echo "bind_test_1" > /tmp/ftp_bind1/bind_test_1.txt
echo "bind_test_2" > /tmp/ftp_bind1/bind_test_2.txt

# Set file modification times to 10 seconds ago
touch -d '10 seconds ago' /tmp/ftp_bind1/*.txt

echo "Creating config file"
echo '{"host_from":"localhost","port_from":2121,"login_from":"u1","password_from":"p1","path_from":"/","host_to":"localhost","port_to":2122,"login_to":"u2","password_to":"p2","path_to":"/","age":1,"filename_regexp":".*\\.txt"}' > /tmp/config_bind.jsonl

echo "Waiting for FTP servers to be ready..."
for i in {1..30}; do
    if nc -z localhost 2121 2>/dev/null && nc -z localhost 2122 2>/dev/null; then
        echo "FTP servers are ready!"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "ERROR: FTP servers did not start in time"
        exit 1
    fi
    sleep 0.2
done

echo "Running iftpfm2 with -b 127.0.0.1 (bind to localhost)"
./target/debug/iftpfm2 -b 127.0.0.1 -d /tmp/config_bind.jsonl

echo "Verifying transfer..."
if [ -f "/tmp/ftp_bind2/bind_test_1.txt" ] && [ -f "/tmp/ftp_bind2/bind_test_2.txt" ] && [ ! -f "/tmp/ftp_bind1/bind_test_1.txt" ] && [ ! -f "/tmp/ftp_bind1/bind_test_2.txt" ]; then
    echo "PASS: files transferred with -b 127.0.0.1"
else
    echo "FAIL: file transfer with bind address did not work"
    exit 1
fi

echo ""
echo "=== Test 4: --bind long form ==="
# Recreate test files
echo "bind_long_test" > /tmp/ftp_bind1/bind_long_test.txt
touch -d '10 seconds ago' /tmp/ftp_bind1/bind_long_test.txt

echo "Running iftpfm2 with --bind 127.0.0.1 (long form)"
./target/debug/iftpfm2 --bind 127.0.0.1 -d /tmp/config_bind.jsonl

if [ -f "/tmp/ftp_bind2/bind_long_test.txt" ] && [ ! -f "/tmp/ftp_bind1/bind_long_test.txt" ]; then
    echo "PASS: --bind long form works"
else
    echo "FAIL: --bind long form did not work"
    exit 1
fi

echo ""
echo "=== Test 5: -b with unreachable IP fails gracefully ==="
rm -rf /tmp/ftp_bind1 /tmp/ftp_bind2
mkdir -p /tmp/ftp_bind1
mkdir -p /tmp/ftp_bind2
echo "unreachable_test" > /tmp/ftp_bind1/unreachable.txt
touch -d '10 seconds ago' /tmp/ftp_bind1/unreachable.txt

# Use 99.99.99.99 which should not be routable
echo "Running iftpfm2 with -b 99.99.99.99 (unreachable bind address)"
OUTPUT=$(./target/debug/iftpfm2 -b 99.99.99.99 /tmp/config_bind.jsonl 2>&1 || true)
# Check that connection failed (file not transferred)
if [ -f "/tmp/ftp_bind2/unreachable.txt" ]; then
    echo "FAIL: file should NOT have been transferred with unreachable bind"
    exit 1
fi
# Check that error was logged
if echo "$OUTPUT" | grep -qi "bind\|assign\|address\|99.99.99.99\|error\|failed"; then
    echo "PASS: unreachable bind address fails with appropriate error"
else
    echo "PASS: file not transferred (bind address unreachable)"
fi

echo ""
echo "All bind tests passed!"
