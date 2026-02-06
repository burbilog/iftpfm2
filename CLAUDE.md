# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Test Commands

```bash
# Build (debug)
cargo build
# or
make debug

# Build (release)
cargo build --release
# or
make release

# Install to ~/.cargo/bin
make install

# Run all tests (unit + integration)
make test
# or manually:
cargo test && ./test.sh && ./test_age.sh && ./test_conn_timeout.sh && ./test_ftps.sh && ./test_temp_dir.sh && ./test_pid.sh && ./test_ram_threshold.sh

# Run only unit tests
cargo test --lib

# Run a specific test
cargo test test_name

# Run tests for specific binary
cargo test --bin migrate_csv_to_jsonl

# Generate documentation
make doc
# or
cargo doc --open
```

**Integration tests:**
- `test.sh` - Basic FTP transfer test
  - Prerequisites: Python 3 with `pyftpdlib` installed
  - Starts two FTP servers on ports 2121/2122
  - Creates temp files and verifies transfer behavior
- `test_age.sh` - File age filtering test
- `test_conn_timeout.sh` - Connection timeout test
- `test_ftps.sh` - FTPS with self-signed certificates test
  - Generates self-signed certificate using openssl
  - Tests with and without `--insecure-skip-verify` flag
- `test_temp_dir.sh` - Temp directory and debug logging test
  - Tests `-T` flag for custom temp directory
  - Tests `--debug` flag for debug logging
  - Verifies temp file paths appear in debug output
- `test_pid.sh` - PID handling test
  - Tests PID file creation with correct PID
  - Verifies no `lsof` dependency (binary doesn't contain "lsof" string)
  - Tests graceful termination via SIGTERM
- `test_ram_threshold.sh` - RAM threshold test
  - Tests `--ram-threshold` flag behavior
  - Verifies RAM buffer usage for small files
  - Verifies disk temp file usage for large files
  - Tests `--ram-threshold 0` forces all files to RAM
- `test_sftp_docker.sh` - SFTP test (separate `make test-sftp` target)
  - Prerequisites: Docker with `atmoz/sftp` container
  - Starts two SFTP servers on ports 3222/3223
  - Tests password authentication, delete flag, and regex filtering

## Project Architecture

`iftpfm2` is a Rust library crate with a binary entry point. The binary delegates to the library for all core functionality.

### Module Structure

**Entry point:**
- `src/main.rs` - Binary crate entry point. Parses CLI args, orchestrates the flow via library calls.

**Library modules (src/lib.rs re-exports these):**
- `cli.rs` - Command-line argument parsing (`parse_args()`)
- `config.rs` - JSONL config parsing + validation (`parse_config()`, `Config::validate()`)
- `ftp_ops.rs` - Core FTP transfer logic (`transfer_files()`, `verify_final_file()`)
- `instance.rs` - Single-instance enforcement via PID file + Unix socket
- `logging.rs` - Thread-safe logging to file/stdout (`log()`, `log_with_thread()`, `set_log_file()`)
- `protocols/` - Protocol implementations for FTP, FTPS, and SFTP
  - `protocols/mod.rs` - Trait definitions and `Client` enum wrapper
  - `protocols/ftp.rs` - FTP protocol implementation (`FtpClient`)
  - `protocols/ftps.rs` - FTPS protocol implementation (`FtpsClient`)
  - `protocols/sftp.rs` - SFTP protocol implementation (`SftpClient`)
- `shutdown.rs` - Async-signal-safe shutdown flag (`is_shutdown_requested()`, `request_shutdown()`)

**Migration script (separate binary):**
- `migrate_csv_to_jsonl.rs` - Converts legacy CSV configs to JSONL format

### Key Architectural Patterns

**Single Instance Enforcement:**
1. New instance checks for `/tmp/iftpfm2.sock`
2. If exists: sends SIGTERM to old PID, waits grace period, forces SIGKILL if needed
3. Removes stale socket and creates new one
4. Spawns listener thread to watch for "SHUTDOWN" commands from new instances
5. Creates `/tmp/iftpfm2.pid` with current PID
6. Uses `scopeguard` to ensure cleanup on exit

**Graceful Shutdown:**
- Signal handler (SIGINT/SIGTERM) only sets atomic flags (async-signal-safe)
- Main thread spawns a watcher thread that polls for shutdown flag and logs signal receipt
- After transfers complete, `request_shutdown()` is called to signal watcher thread to exit
- `join_listener_thread()` does NOT block - spawns a thread to join the listener (which is often blocked on `incoming()`)

**Logging:**
- Global `LOG_FILE` stores optional log file path
- Global `LOG_FILE_HANDLE` caches `BufWriter<File>` to avoid opening file per message
- Global `DEBUG_MODE` (AtomicBool) for enabling debug logging at runtime
- `log_debug()` function - only logs when debug mode is enabled (zero overhead when disabled)
- `set_debug_mode()` - enable/disable debug logging
- Handles mutex poisoning gracefully
- In non-test code, logging failures never panic (uses `let _ =`)

**FTP Transfer Flow (per config entry):**
1. Connect to source FTP/FTPS/SFTP (using `Client::connect()` with protocol from `proto_from`)
2. Connect to target FTP/FTPS/SFTP (using `Client::connect()` with protocol from `proto_to`)
3. Login to both servers
4. CWD to path on both servers
5. Set binary mode once on both connections (outside the file loop)
6. Get file list via NLST from source
7. For each file:
   - Check regex match
   - Get MDTM (modified time)
   - Check file age
   - Get file size via SIZE command to determine storage strategy
   - Transfer via `retr()` → RAM buffer or disk temp file → `put_file()`
     - Files ≤ `--ram-threshold` (default: 10MB) use RAM buffer (Vec<u8>)
     - Files > `--ram-threshold` use disk temp file (NamedTempFile)
     - `--ram-threshold 0` forces all files to RAM buffer
     - Temp directory: `-T <dir>` flag (default: system temp dir)
     - Debug mode (`--debug`) logs storage decision and temp file paths
   - Verify upload size (MANDATORY - transfer fails if verification fails)
   - Rename temporary file to final name (retry after deleting existing file if rename fails)
   - Verify final file size using `verify_final_file()` (MANDATORY)
   - Delete from source if `-d` flag (only after successful verification)
8. Call `quit()` on both connections
9. Log summary

**FTPS Support:**
- Protocol selected via `proto_from`/`proto_to` config fields (`ftp` or `ftps`)
- `Client::connect()` creates either `FtpClient` or `FtpsClient` based on protocol
- `FtpsClient` creates a `TlsConnector` and calls `into_secure()` on the FTP stream
- Use `--insecure-skip-verify` CLI flag to bypass certificate verification (for self-signed certs)
- Default: `TlsConnector::new()` - verifies certificates
- With flag: `TlsConnector::builder().danger_accept_invalid_certs(true).build()`

**SFTP Support:**
- Protocol selected via `proto_from`/`proto_to` config fields (`sftp`)
- `Client::connect()` creates `SftpClient` for SFTP connections
- Uses `ssh2` crate for SSH file transfer operations
- Authentication methods:
  - Password auth: `password_from`/`password_to` fields
  - Key auth: `keyfile_from`/`keyfile_to` fields (path to SSH private key)
- SFTP doesn't have a true "current working directory" concept like FTP
  - `SftpClient` tracks `current_dir` internally to maintain compatibility with FTP operations
  - All file operations (`mdtm`, `size`, `retr`, `put_file`, `rename`, `rm`) prepend `current_dir` to filenames
- SFTP test: `make test-sftp` (uses Docker atmoz/sftp container, separate from main test suite)

**Config Validation:**
- All fields validated during parsing (non-empty hosts/logins/passwords/paths, ports > 0, age > 0, valid regex)
- `proto_from` and `proto_to` default to `Protocol::Ftp` if not specified
- For SFTP: either password OR keyfile must be specified (validated in config parsing)
- Regex pattern validated once during parsing (not re-validated during transfer)

## Important Implementation Notes

**Version number:**
- Defined in `Cargo.toml` only
- `src/lib.rs` uses `env!("CARGO_PKG_VERSION")` to read it at compile time
- Never hardcode version elsewhere

**Error handling in non-test code:**
- Use `let _ = log(...)` instead of `log(...).unwrap()` for logging calls
- For FTP operations, use `if let Err(e) = ...` to log and continue/return early
- Login failures: log error, call `quit()`, return 0 (don't use `unwrap_or_else`)

**Thread safety:**
- `LOG_FILE` and `LOG_FILE_HANDLE` are `Lazy<Mutex<>>` for thread-safe access
- When locking multiple mutexes, be careful about deadlock (drop lock before acquiring another)
- Shutdown flag is `AtomicBool` for lock-free reads

**Signal handler safety:**
- Signal handler ONLY sets atomic flags (`SIGNAL_TYPE`, `SHUTDOWN_REQUESTED`)
- No I/O in signal handler - logging deferred to watcher thread in main
- Uses `ctrlc` crate which sets up handlers

**Testing:**
- Unit tests use `serial_test` for tests that modify global state
- `reset_shutdown_for_tests()` available to reset shutdown flag between tests
- Integration tests use real FTP/FTPS servers (test.sh, test_ftps.sh, test_conn_timeout.sh, test_age.sh)
- `test_temp_dir.sh` - Tests `-T` flag and `--debug` logging
- `test_pid.sh` - Tests PID file creation and nix-based signaling
- SFTP tests: `make test-sftp` (separate target, uses Docker atmoz/sftp container)
- **Run all tests (unit + integration):** `make test` in the project root directory
  - This runs `cargo test`, `./test.sh`, `./test_age.sh`, `./test_conn_timeout.sh`, `./test_ftps.sh`, `./test_temp_dir.sh`, and `./test_pid.sh`
  - Rule: NEVER run make test directly. Only through the Task tool with a sub-agent.

**Connection Timeout:**
- Configurable via `-t seconds` CLI flag (default: 30 seconds)
- Passed to `connect_server()` as `Duration`
- Applied via `connect_timeout()` methods for FTP/FTPS, and stream timeouts for SFTP
- Error messages include the timeout value for debugging

**Upload Verification (Mandatory):**
- ALWAYS uses FTP `SIZE` command to verify file size on target server after upload
- Transfer FAILS if verification fails (no rename, no source deletion)
- Requires server support for SIZE command (RFC 3659)
- Log messages:
  - `Verifying upload of '{tmp_filename}' (expected {size} bytes)...`
  - `Upload verification passed: '{tmp_filename}' is {size} bytes`
  - `ERROR: Upload verification FAILED: '{tmp_filename}' expected {X} bytes, got {Y} bytes - transfer aborted`
  - `ERROR: Upload verification error for '{tmp_filename}': {error} - transfer aborted`

**Building:**
- To check "if it builds" — use the Task tool: `cargo build`, return only success/errors.
- If you need to fix compilation errors — run directly to see the full output.

## Common Issues to Avoid

1. **Leaking FTP/SFTP connections** - Always call `quit()` on error paths
2. **Panicking on log failure** - Use `let _ = log(...)` pattern
3. **I/O in signal handlers** - Only set atomic flags, defer logging
4. **Blocking on listener thread join** - Listener thread is blocked on `incoming()`, spawn a thread to join it instead
5. **Hardcoding version** - Always use `crate::PROGRAM_VERSION`
6. **Using `unwrap()` in production code** - Use proper error handling; only use in tests

## CLI Flags Reference

| Flag | Argument | Description |
|------|----------|-------------|
| `-h` | - | Show help message and exit |
| `-v` | - | Show version information |
| `-d` | - | Delete source files after successful transfer |
| `-l` | `<logfile>` | Write logs to specified file |
| `-s` | - | Write logs to stdout (mutually exclusive with `-l`) |
| `-p` | `<number>` | Number of parallel transfers (default: 1) |
| `-r` | - | Randomize file processing order |
| `-g` | `<seconds>` | Grace period before SIGKILL (default: 30) |
| `-t` | `<seconds>` | Connection timeout in seconds (default: 30) |
| `-T` | `<dir>` | Directory for temporary files (default: system temp) |
| `--debug` | - | Enable debug logging (shows temp file paths, etc.) |
| `--ram-threshold` | `<bytes>` | RAM threshold for temp files (default: 10485760) |
| `--insecure-skip-verify` | - | Skip TLS certificate verification for FTPS (DANGEROUS) |

**RAM Threshold Behavior (`--ram-threshold`):**
- Default: 10485760 (10MB) - balances speed and memory safety
- Files smaller than threshold use RAM buffer (faster, no disk I/O)
- Files larger than threshold use disk temp files (avoids OOM)
- `--ram-threshold 0` forces ALL files to RAM buffer (use with caution!)
- Debug logging shows decision: "Using RAM buffer" or "Using disk buffer"
