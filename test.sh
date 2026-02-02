#!/usr/bin/env bash
#
# iftpfm2 test script
# requires python3 and pyftpdlib installed

set -e

# Add cargo to PATH if not already there
if ! command -v cargo &>/dev/null && [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

# Cleanup trap - ensure servers are killed on exit
cleanup() {
    if [ -n "$ftp1_pid" ]; then kill $ftp1_pid 2>/dev/null || true; fi
    if [ -n "$ftp2_pid" ]; then kill $ftp2_pid 2>/dev/null || true; fi
    rm -rf /tmp/ftp1 /tmp/ftp2 2>/dev/null || true
    rm -f /tmp/config.jsonl 2>/dev/null || true
}
trap cleanup EXIT INT TERM

cargo build

rm -rf /tmp/ftp1 /tmp/ftp2
mkdir -p /tmp/ftp1
mkdir -p /tmp/ftp2

echo "Starting first FTP server on port 2121"
python3 -m pyftpdlib -p 2121 -u u1 -P p1 -d /tmp/ftp1 -w 2>/dev/null &
ftp1_pid=$!

echo "Starting second FTP server on port 2122"
python3 -m pyftpdlib -p 2122 -u u2 -P p2 -d /tmp/ftp2 -w 2>/dev/null &
ftp2_pid=$!

echo "Generating some files in the first servers directory"
echo "test1" > /tmp/ftp1/test1.txt
echo "test2" > /tmp/ftp1/test2.txt
echo "test3" > /tmp/ftp1/test3.txt

# Set file modification times to 10 seconds ago so they're immediately "old enough"
touch -d '10 seconds ago' /tmp/ftp1/*.txt

echo "Creating config file for iftpfm2, age is 1 second"
echo '{"host_from":"localhost","port_from":2121,"login_from":"u1","password_from":"p1","path_from":"/","host_to":"localhost","port_to":2122,"login_to":"u2","password_to":"p2","path_to":"/","age":1,"filename_regexp":".*\\.txt"}' > /tmp/config.jsonl

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

echo "Running iftpfm2 using the config file, the -d option to delete source files"
./target/debug/iftpfm2 -d /tmp/config.jsonl

echo "Ensure that the files were moved to the second servers directory and deleted from the source server"
echo
if [ -f "/tmp/ftp2/test1.txt" ] && [ -f "/tmp/ftp2/test2.txt" ] && [ -f "/tmp/ftp2/test3.txt" ] && [ ! -f "/tmp/ftp1/test1.txt" ] && [ ! -f "/tmp/ftp1/test2.txt" ] && [ ! -f "/tmp/ftp1/test3.txt" ]; then
    echo "SUCCESS: files transferred and deleted as expected"
else
    echo "ERROR: unexpected file transfer or deletion"
fi

echo "Test completed!"
