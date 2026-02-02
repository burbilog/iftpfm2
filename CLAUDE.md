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
cargo test
./test.sh
# or
make test

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

## Project Architecture

`iftpfm2` is a Rust library crate with a binary entry point. The binary delegates to the library for all core functionality.

### Module Structure

**Entry point:**
- `src/main.rs` - Binary crate entry point. Parses CLI args, orchestrates the flow via library calls.

**Library modules (src/lib.rs re-exports these):**
- `cli.rs` - Command-line argument parsing (`parse_args()`)
- `config.rs` - JSONL config parsing + validation (`parse_config()`, `Config::validate()`)
- `ftp_ops.rs` - Core FTP transfer logic (`transfer_files()`)
- `instance.rs` - Single-instance enforcement via PID file + Unix socket
- `logging.rs` - Thread-safe logging to file/stdout (`log()`, `log_with_thread()`, `set_log_file()`)
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
- Handles mutex poisoning gracefully
- In non-test code, logging failures never panic (uses `let _ =`)

**FTP Transfer Flow (per config entry):**
1. Connect to source FTP/FTPS (using `connect_server()` with protocol from `proto_from`)
2. Connect to target FTP/FTPS (using `connect_server()` with protocol from `proto_to`)
3. Login to both servers
4. CWD to path on both servers
5. Set binary mode once on both connections (outside the file loop)
6. Get file list via NLST from source
7. For each file:
   - Check regex match
   - Get MDTM (modified time)
   - Check file age
   - Delete from target if exists
   - Transfer via `retr()` + `put_file()` to temporary file
   - Verify upload size (if `--upload-verify` flag is set)
   - Rename temporary file to final name
   - Delete from source if `-d` flag
8. Call `quit()` on both connections
9. Log summary

**FTPS Support:**
- Protocol selected via `proto_from`/`proto_to` config fields (`ftp` or `ftps`)
- For FTPS, `connect_server()` creates a `TlsConnector` and calls `into_secure()`
- Use `--insecure-skip-verify` CLI flag to bypass certificate verification (for self-signed certs)
- Default: `TlsConnector::new()` - verifies certificates
- With flag: `TlsConnector::builder().danger_accept_invalid_certs(true).build()`

**Config Validation:**
- All fields validated during parsing (non-empty hosts/logins/passwords/paths, ports > 0, age > 0, valid regex)
- `proto_from` and `proto_to` default to `Protocol::Ftp` if not specified
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

**Connection Timeout:**
- Configurable via `-t seconds` CLI flag (default: 30 seconds)
- Passed to `connect_server()` as `Duration`
- Applied via `connect_timeout()` methods for both FTP and FTPS
- Error messages include the timeout value for debugging

**Upload Verification:**
- Configurable via `--upload-verify` CLI flag (default: disabled)
- After upload, uses FTP `SIZE` command to verify file size on target server
- Logs warning if size mismatch detected (non-breaking, continues with rename)
- Requires server support for SIZE command (RFC 3659)
- Log messages:
  - `Verifying upload of '{tmp_filename}' (expected {size} bytes)...`
  - `Upload verification passed: '{tmp_filename}' is {size} bytes`
  - `WARNING: Upload verification FAILED: '{tmp_filename}' expected {X} bytes, got {Y} bytes`
  - `WARNING: Upload verification error for '{tmp_filename}': {error}`

## Common Issues to Avoid

1. **Leaking FTP connections** - Always call `quit()` on error paths
2. **Panicking on log failure** - Use `let _ = log(...)` pattern
3. **I/O in signal handlers** - Only set atomic flags, defer logging
4. **Blocking on listener thread join** - Listener thread is blocked on `incoming()`, spawn a thread to join it instead
5. **Hardcoding version** - Always use `crate::PROGRAM_VERSION`
6. **Using `unwrap()` in production code** - Use proper error handling; only use in tests
