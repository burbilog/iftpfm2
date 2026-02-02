#!/usr/bin/env bash
#
# test_age.sh - Checks age filtering logic
#

set -e

# Add cargo to PATH if not already there
if ! command -v cargo &>/dev/null && [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

# Function to cleanup processes on exit (successful or not)
cleanup() {
    echo "Stopping FTP servers..."
    if [ -n "$ftp1_pid" ]; then kill $ftp1_pid 2>/dev/null; fi
    if [ -n "$ftp2_pid" ]; then kill $ftp2_pid 2>/dev/null; fi
    rm -f /tmp/config_age.jsonl
    rm -rf /tmp/ftp1 /tmp/ftp2
}
trap cleanup EXIT

# Build project to ensure latest binary
cargo build

# Prepare directories
rm -rf /tmp/ftp1 /tmp/ftp2
mkdir -p /tmp/ftp1
mkdir -p /tmp/ftp2

# Start FTP servers
echo "Starting Source FTP (port 2121)..."
python3 -m pyftpdlib -p 2121 -u u1 -P p1 -d /tmp/ftp1 -w > /dev/null 2>&1 &
ftp1_pid=$!

echo "Starting Target FTP (port 2122)..."
python3 -m pyftpdlib -p 2122 -u u2 -P p2 -d /tmp/ftp2 -w > /dev/null 2>&1 &
ftp2_pid=$!

# Give servers time to start
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

# AGE Scenario
AGE_LIMIT=3

echo "Creating 'old.txt'..."
echo "I am old" > /tmp/ftp1/old.txt

echo "Aging 'old.txt' via touch..."
# Make it 10 seconds old (older than AGE_LIMIT=3)
touch -d "10 seconds ago" /tmp/ftp1/old.txt

echo "Creating 'young.txt'..."
echo "I am young" > /tmp/ftp1/young.txt
# Ensure young.txt is fresh
touch /tmp/ftp1/young.txt

# Create config with age = 3 seconds
# We use a simple regex that matches both files
echo "Creating config with age=${AGE_LIMIT}..."
cat > /tmp/config_age.jsonl <<'EOF'
{"host_from":"localhost","port_from":2121,"login_from":"u1","password_from":"p1","path_from":"/","host_to":"localhost","port_to":2122,"login_to":"u2","password_to":"p2","path_to":"/","age":3,"filename_regexp":".*\\.txt"}
EOF

echo "Running iftpfm2..."
# Run without -d flag (delete=false) so we can verify source state if needed, 
# but mostly we care about what landed in target.
./target/debug/iftpfm2 /tmp/config_age.jsonl

# Verify results
echo "Verifying results..."

ERRORS=0

# 1. Old file SHOULD be on target server
if [ -f "/tmp/ftp2/old.txt" ]; then
    echo "[OK] old.txt transferred successfully."
else
    echo "[FAIL] old.txt was NOT transferred!"
    ERRORS=1
fi

# 2. Young file should NOT be on target server
if [ ! -f "/tmp/ftp2/young.txt" ]; then
    echo "[OK] young.txt was correctly skipped."
else
    echo "[FAIL] young.txt WAS transferred (but shouldn't have been)!"
    ERRORS=1
fi

if [ $ERRORS -eq 0 ]; then
    echo "SUCCESS: Age logic works correctly."
    exit 0
else
    echo "FAILURE: Age logic failed."
    exit 1
fi
