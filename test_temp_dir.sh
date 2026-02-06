#!/usr/bin/env bash
#
# iftpfm2 temp directory test script
# Tests that -T flag and --debug work correctly

set -e

# Add cargo to PATH if not already there
if ! command -v cargo &>/dev/null && [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

# Cleanup trap
cleanup() {
    if [ -n "$ftp1_pid" ]; then kill $ftp1_pid 2>/dev/null || true; fi
    if [ -n "$ftp2_pid" ]; then kill $ftp2_pid 2>/dev/null || true; fi
    rm -rf /tmp/ftp1 /tmp/ftp2 /tmp/iftpfm2_test_temp 2>/dev/null || true
    rm -f /tmp/config_temp_test.jsonl 2>/dev/null || true
}
trap cleanup EXIT INT TERM

cargo build

rm -rf /tmp/ftp1 /tmp/ftp2 /tmp/iftpfm2_test_temp
mkdir -p /tmp/ftp1
mkdir -p /tmp/ftp2
mkdir -p /tmp/iftpfm2_test_temp

echo "Starting FTP servers for temp dir test..."
python3 -m pyftpdlib -p 2121 -u u1 -P p1 -d /tmp/ftp1 -w 2>/dev/null &
ftp1_pid=$!
python3 -m pyftpdlib -p 2122 -u u2 -P p2 -d /tmp/ftp2 -w 2>/dev/null &
ftp2_pid=$!

echo "Generating test file"
echo "temp_test_content" > /tmp/ftp1/test_temp.txt
touch -d '10 seconds ago' /tmp/ftp1/test_temp.txt

echo "Creating config file"
cat > /tmp/config_temp_test.jsonl << 'EOF'
{"host_from":"localhost","port_from":2121,"login_from":"u1","password_from":"p1","path_from":"/","host_to":"localhost","port_to":2122,"login_to":"u2","password_to":"p2","path_to":"/","age":1,"filename_regexp":"test_temp\\.txt"}
EOF

echo "Waiting for FTP servers..."
for i in {1..30}; do
    if nc -z localhost 2121 2>/dev/null && nc -z localhost 2122 2>/dev/null; then
        break
    fi
    if [ $i -eq 30 ]; then
        echo "ERROR: FTP servers did not start in time"
        exit 1
    fi
    sleep 0.2
done

echo "Running iftpfm2 with -T flag and --debug"
./target/debug/iftpfm2 -T /tmp/iftpfm2_test_temp --debug /tmp/config_temp_test.jsonl 2>&1 | tee /tmp/test_output.log

echo "Checking for debug log with temp file path..."
if grep -q "Using temp file: /tmp/iftpfm2_test_temp/" /tmp/test_output.log; then
    echo "SUCCESS: temp file path found in debug log"
else
    echo "ERROR: temp file path not found in debug log"
    echo "Log contents:"
    cat /tmp/test_output.log
    exit 1
fi

echo "Verifying file transfer..."
if [ -f "/tmp/ftp2/test_temp.txt" ]; then
    echo "SUCCESS: file transferred to target"
else
    echo "ERROR: file was not transferred"
    exit 1
fi

echo "Temp dir test completed successfully!"
