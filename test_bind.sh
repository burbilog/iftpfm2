#!/usr/bin/env bash
#
# iftpfm2 bind address test script
# Tests bind_from/bind_to in JSONL config for binding outgoing connections

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
    rm -f /tmp/config_bind*.jsonl 2>/dev/null || true
}
trap cleanup EXIT INT TERM

cargo build

echo "=== Test 1: Config with invalid bind_from IP ==="
echo '{"host_from":"localhost","port_from":2121,"login_from":"u1","password_from":"p1","path_from":"/","host_to":"localhost","port_to":2122,"login_to":"u2","password_to":"p2","path_to":"/","age":1,"filename_regexp":".*","bind_from":"not-an-ip"}' > /tmp/config_bind_invalid.jsonl

if ./target/debug/iftpfm2 /tmp/config_bind_invalid.jsonl 2>/dev/null; then
    echo "FAIL: should have exited with error for invalid bind_from"
    exit 1
fi
echo "PASS: invalid bind_from rejected"

echo ""
echo "=== Test 2: Transfer with bind_from/bind_to in config ==="
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

echo "Creating config file with bind_from and bind_to"
echo '{"host_from":"localhost","port_from":2121,"login_from":"u1","password_from":"p1","path_from":"/","host_to":"localhost","port_to":2122,"login_to":"u2","password_to":"p2","path_to":"/","age":1,"filename_regexp":".*","bind_from":"127.0.0.1","bind_to":"127.0.0.1"}' > /tmp/config_bind.jsonl

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

echo "Running iftpfm2 with bind_from/bind_to 127.0.0.1 in config"
./target/debug/iftpfm2 -d /tmp/config_bind.jsonl

echo "Verifying transfer..."
if [ -f "/tmp/ftp_bind2/bind_test_1.txt" ] && [ -f "/tmp/ftp_bind2/bind_test_2.txt" ] && [ ! -f "/tmp/ftp_bind1/bind_test_1.txt" ] && [ ! -f "/tmp/ftp_bind1/bind_test_2.txt" ]; then
    echo "PASS: files transferred with bind_from/bind_to 127.0.0.1"
else
    echo "FAIL: file transfer with bind_from/bind_to did not work"
    exit 1
fi

echo ""
echo "=== Test 3: Transfer without bind fields (OS default) ==="
# Kill servers and recreate
kill $ftp1_pid 2>/dev/null || true
kill $ftp2_pid 2>/dev/null || true
sleep 0.5
rm -rf /tmp/ftp_bind1 /tmp/ftp_bind2
mkdir -p /tmp/ftp_bind1
mkdir -p /tmp/ftp_bind2

echo "Starting FTP servers again"
python3 -m pyftpdlib -p 2121 -u u1 -P p1 -d /tmp/ftp_bind1 -w 2>/dev/null &
ftp1_pid=$!
python3 -m pyftpdlib -p 2122 -u u2 -P p2 -d /tmp/ftp_bind2 -w 2>/dev/null &
ftp2_pid=$!

echo "no_bind_test" > /tmp/ftp_bind1/no_bind_test.txt
touch -d '10 seconds ago' /tmp/ftp_bind1/no_bind_test.txt

echo "Creating config file without bind fields"
echo '{"host_from":"localhost","port_from":2121,"login_from":"u1","password_from":"p1","path_from":"/","host_to":"localhost","port_to":2122,"login_to":"u2","password_to":"p2","path_to":"/","age":1,"filename_regexp":".*"}' > /tmp/config_bind_nobind.jsonl

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

echo "Running iftpfm2 without bind fields (OS default)"
./target/debug/iftpfm2 /tmp/config_bind_nobind.jsonl

if [ -f "/tmp/ftp_bind2/no_bind_test.txt" ]; then
    echo "PASS: transfer works without bind fields (OS default)"
else
    echo "FAIL: transfer without bind fields did not work"
    exit 1
fi

echo ""
echo "=== Test 4: Config with unreachable bind_from fails gracefully ==="
kill $ftp1_pid 2>/dev/null || true
kill $ftp2_pid 2>/dev/null || true
sleep 0.5
rm -rf /tmp/ftp_bind1 /tmp/ftp_bind2
mkdir -p /tmp/ftp_bind1
mkdir -p /tmp/ftp_bind2
echo "unreachable_test" > /tmp/ftp_bind1/unreachable.txt
touch -d '10 seconds ago' /tmp/ftp_bind1/unreachable.txt

python3 -m pyftpdlib -p 2121 -u u1 -P p1 -d /tmp/ftp_bind1 -w 2>/dev/null &
ftp1_pid=$!
python3 -m pyftpdlib -p 2122 -u u2 -P p2 -d /tmp/ftp_bind2 -w 2>/dev/null &
ftp2_pid=$!

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

# Use 99.99.99.99 which should not be routable
echo "Creating config with unreachable bind_from"
echo '{"host_from":"localhost","port_from":2121,"login_from":"u1","password_from":"p1","path_from":"/","host_to":"localhost","port_to":2122,"login_to":"u2","password_to":"p2","path_to":"/","age":1,"filename_regexp":".*","bind_from":"99.99.99.99"}' > /tmp/config_bind_unreachable.jsonl

echo "Running iftpfm2 with bind_from 99.99.99.99 (unreachable)"
OUTPUT=$(./target/debug/iftpfm2 /tmp/config_bind_unreachable.jsonl 2>&1 || true)
# Check that connection failed (file not transferred)
if [ -f "/tmp/ftp_bind2/unreachable.txt" ]; then
    echo "FAIL: file should NOT have been transferred with unreachable bind"
    exit 1
fi
# Check that error was logged (bind/connect failure message)
if ! echo "$OUTPUT" | grep -qi "error\|failed\|bind\|99.99.99.99"; then
    echo "FAIL: no error message in output for unreachable bind_from"
    echo "Output was: $OUTPUT"
    exit 1
fi
echo "PASS: unreachable bind_from fails gracefully with error message"

echo ""
echo "All bind tests passed!"
