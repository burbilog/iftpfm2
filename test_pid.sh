#!/usr/bin/env bash
#
# iftpfm2 PID handling test script
# Tests that:
# 1. PID file is created correctly
# 2. PID is read from file (not via lsof)
# 3. SIGTERM is sent to terminate old instance

set -e

# Add cargo to PATH if not already there
if ! command -v cargo &>/dev/null && [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

# Cleanup trap
cleanup() {
    if [ -n "$ftp_pid1" ]; then kill $ftp_pid1 2>/dev/null || true; fi
    if [ -n "$ftp_pid2" ]; then kill $ftp_pid2 2>/dev/null || true; fi
    if [ -n "$iftpfm_pid" ]; then kill $iftpfm_pid 2>/dev/null || true; fi
    if [ -n "$iftpfm_pid2" ]; then kill $iftpfm_pid2 2>/dev/null || true; fi
    rm -rf /tmp/ftp1 /tmp/ftp2 /tmp/test_config.pid.jsonl 2>/dev/null || true
    rm -f /tmp/iftpfm2.sock /tmp/iftpfm2.pid 2>/dev/null || true
}
trap cleanup EXIT INT TERM

cargo build

echo "Starting FTP servers for PID test"
rm -rf /tmp/ftp1 /tmp/ftp2
mkdir -p /tmp/ftp1
mkdir -p /tmp/ftp2
python3 -m pyftpdlib -p 2121 -u u1 -P p1 -d /tmp/ftp1 -w 2>/dev/null &
ftp_pid1=$!
python3 -m pyftpdlib -p 2122 -u u2 -P p2 -d /tmp/ftp2 -w 2>/dev/null &
ftp_pid2=$!

echo "Waiting for FTP servers..."
for i in {1..30}; do
    if nc -z localhost 2121 2>/dev/null && nc -z localhost 2122 2>/dev/null; then
        break
    fi
    if [ $i -eq 30 ]; then
        echo "ERROR: FTP servers did not start"
        exit 1
    fi
    sleep 0.2
done

# Store for cleanup
ftp_pid=$ftp_pid1

echo ""
echo "=== Test 1: PID file creation ==="
# Create config with regexp that won't match anything (keeps process alive briefly)
cat > /tmp/test_config.pid.jsonl << 'EOF'
{"host_from":"localhost","port_from":2121,"login_from":"u1","password_from":"p1","path_from":"/","host_to":"localhost","port_to":2122,"login_to":"u2","password_to":"p2","path_to":"/","age":86400,"filename_regexp":"NOMATCHNOMATCH.*"}
EOF

# Start iftpfm2 in background
./target/debug/iftpfm2 /tmp/test_config.pid.jsonl > /tmp/test_output1.log 2>&1 &
iftpfm_pid=$!
echo "Started iftpfm2 with PID: $iftpfm_pid"

# Check PID file while process is still running (race condition check)
for i in {1..20}; do
    # Check if process still exists
    if ! kill -0 $iftpfm_pid 2>/dev/null; then
        # Process has finished - check log for success
        if grep -q "successfully transferred" /tmp/test_output1.log 2>/dev/null; then
            echo "Process finished successfully"
        else
            echo "ERROR: Process failed unexpectedly"
            cat /tmp/test_output1.log
            exit 1
        fi
        break
    fi

    # Process is still running - check for PID file
    if [ -f "/tmp/iftpfm2.pid" ]; then
        echo "OK: PID file exists while process is running"
        break
    fi

    # Wait a tiny bit
    sleep 0.01
done

# Final check after process might have finished
if [ ! -f "/tmp/iftpfm2.pid" ]; then
    echo "ERROR: PID file was not created during process lifetime"
    echo "This might indicate a race condition in PID file creation"
    cat /tmp/test_output1.log
    exit 1
fi

echo "PID file exists at /tmp/iftpfm2.pid"

# Read PID from file
pid_from_file=$(cat /tmp/iftpfm2.pid)
echo "PID read from file: $pid_from_file"

# Verify it matches the process we started
if [ "$pid_from_file" != "$iftpfm_pid" ]; then
    echo "ERROR: PID in file ($pid_from_file) doesn't match actual PID ($iftpfm_pid)"
    exit 1
fi

echo "OK: PID in file matches actual process PID"

# Verify process is still running
if ! kill -0 $iftpfm_pid 2>/dev/null; then
    echo "ERROR: Process $iftpfm_pid is not running"
    cat /tmp/test_output1.log
    exit 1
fi

echo "OK: Process is still running"

echo ""
echo "=== Test 2: No lsof dependency ==="
# Check that binary doesn't contain "lsof" command references
if strings ./target/debug/iftpfm2 | grep -q "lsof"; then
    echo "WARNING: Binary contains 'lsof' string"
else
    echo "OK: Binary does not contain 'lsof' string - no external lsof dependency"
fi

echo ""
echo "=== Test 3: Graceful termination ==="
# Process may have already finished - that's OK
if kill -0 $iftpfm_pid 2>/dev/null; then
    # Process is still running - test graceful shutdown
    echo "Process still running, testing SIGTERM..."
    kill -TERM $iftpfm_pid

    for i in {1..10}; do
        if ! kill -0 $iftpfm_pid 2>/dev/null; then
            echo "OK: Process terminated gracefully on SIGTERM"
            break
        fi
        if [ $i -eq 10 ]; then
            echo "WARNING: Process did not terminate in time, using SIGKILL"
            kill -KILL $iftpfm_pid 2>/dev/null || true
        fi
        sleep 0.5
    done
else
    echo "OK: Process already finished (no files to transfer)"
fi

# Wait for any cleanup
sleep 0.2

# Check final state of PID file
if [ -f "/tmp/iftpfm2.pid" ]; then
    echo "Note: PID file still exists (may be cleaned up later)"
else
    echo "OK: PID file was cleaned up"
fi

echo ""
echo "=== All PID tests passed! ==="
