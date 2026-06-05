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
cargo test && ./test.sh && ./test_age.sh && ./test_conn_timeout.sh && ./test_sftp_timeout.sh && ./test_ftps.sh && ./test_temp_dir.sh && ./test_pid.sh && ./test_pid_no_xdg.sh && ./test_ram_threshold.sh && ./test_bind.sh

# Run only unit tests
cargo test --lib

# Run a specific test
cargo test test_name

# Run tests for specific binary
cargo test --bin migrate_csv_to_jsonl

# Build web UI
make web              # Release
make web-debug        # Debug

# Run web tests
make test-web         # API tests (42 curl-based tests)
make test-web-ui      # UI tests (agent-browser)

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
- `test_sftp_timeout.sh` - SFTP connection timeout test
  - Tests `-t` flag for SFTP connections
  - Verifies timeout works at TCP and SSH session levels
  - Uses non-routable IP to trigger timeout
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
- `test_bind.sh` - Bind address test
  - Tests `bind_from`/`bind_to` fields in JSONL config for outgoing connection bind address
  - Tests invalid bind_from IP error handling
  - Tests successful transfer with `bind_from: "127.0.0.1"` and `bind_to: "127.0.0.1"`
  - Tests transfer without bind fields (OS default)
  - Tests unreachable bind address failure
- `test_sftp_docker.sh` - SFTP test (separate `make test-sftp` target)
  - Prerequisites: Docker with `atmoz/sftp` container
  - Starts two SFTP servers on ports 3222/3223
  - Tests password authentication, delete flag, and regex filtering
- `test_pid_no_xdg.sh` - PID handling test without XDG_RUNTIME_DIR (in `make test`)
  - Unsets XDG_RUNTIME_DIR, verifies fallback to `/tmp/iftpfm2_<uid>.sock`
- `test_sftp_keys_docker.sh` - SFTP SSH key authentication test (in `make test` when Docker available)
  - Prerequisites: Docker with `atmoz/sftp` container
  - Tests key auth with and without passphrase
- `test_web.sh` - Web API tests (34 curl-based tests, in `make test`)
  - Tests Basic Auth (wrong credentials, valid credentials)
  - Tests CRUD operations (Create/Read/Update/Delete)
  - Tests `/api/validate` endpoint
  - Tests read-only mode (blocks write operations)
  - Tests config persistence across server restarts
  - Tests SFTP keyfile configuration
  - Tests JSONL format preservation
- `test_web_agent.sh` - Web UI tests (in `make test` when `agent-browser` available)
  - Prerequisites: `agent-browser` CLI tool
  - Tests page loading, edit/create/delete operations, search/filter, protocol handling, regex testing, password toggle, keyboard shortcuts, duplicate entry, swap source/target

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

**Web UI (separate crate — `iftpfm2-web/`):**
- `iftpfm2-web/src/main.rs` - Axum-based web server for JSONL config editing
- `iftpfm2-web/static/index.html` - SPA frontend (vanilla JS, dark theme, embedded via `include_str!`)
- `iftpfm2-web/Cargo.toml` - Dependencies: axum, tokio, tower-http (cors), base64, constant_time_eq
- Shares `Config`, `ConfigEntry`, `parse_config_entries()`, `write_config_entries()`, `validate_for_edit()` from parent crate

### Key Architectural Patterns

**Single Instance Enforcement:**
1. Paths are user-isolated via `get_lock_paths()` in `instance.rs`:
   - With `$XDG_RUNTIME_DIR`: `$XDG_RUNTIME_DIR/iftpfm2.sock` / `iftpfm2.pid`
   - Without `$XDG_RUNTIME_DIR`: `/tmp/iftpfm2_<uid>.sock` / `iftpfm2_<uid>.pid`
2. New instance checks for socket file
3. If exists: sends SIGTERM to old PID, waits grace period, forces SIGKILL if needed
4. Removes stale socket and creates new one
5. Spawns listener thread to watch for "SHUTDOWN" commands from new instances
6. Creates PID file with current PID
7. Uses `scopeguard` to ensure cleanup on exit

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
- **Session hash** (`logging.rs`): 4-character hex hash (`generate_session_hash()`) stored in thread-local `SESSION_CONTEXT`
  - Set via `set_session_context(thread_id, hash)` at start of `transfer_files()`
  - Cleared via `clear_session_context()` on exit (uses scopeguard)
  - Log format with context: `[timestamp] [Tn] [hash] message`

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
   - Convert MDTM to UTC using `tz_from` offset (SFTP skipped — mtime is always UTC)
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

**Error Protection (ftp_ops.rs):**
- `MAX_CONSECUTIVE_ERRORS: u32 = 5` — session aborts after 5 consecutive transfer errors
- `MAX_RECONNECT_ATTEMPTS: u32 = 3` — reconnect attempts per file on `DataConnectionAlreadyOpen` error

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
- `tz_from` and `tz_to` default to `TzOffset::Utc` if not specified (backward compatible)
- `bind_from` and `bind_to` default to `None` if not specified (OS chooses source address)
- `bind_from`/`bind_to` validated as `IpAddr` during serde deserialization (invalid IP → parse error with line number)
- `keyfile_pass_from`/`keyfile_pass_to`: passphrase for SSH private keys (optional, only used with `keyfile_from`/`keyfile_to`)
- For SFTP: either password OR keyfile must be specified (validated in config parsing)
- Regex pattern validated once during parsing (not re-validated during transfer)

**Timezone Offset (`tz_from` / `tz_to`):**
- FTP servers may return local time in MDTM responses instead of UTC
- `TzOffset` enum: `Utc` (default) or `Fixed(i32)` (seconds from UTC)
- Supported formats: `"utc"`, `"+03:00"`, `"-05:30"`, `"+0300"`, `"+3"`
- Parsed by `parse_tz_offset()` in `config.rs`, errors include line number via `parse_config()`
- Conversion applied in `naive_datetime_to_utc()` (`ftp_ops.rs`) using `FixedOffset::from_local_datetime()`
- SFTP is excluded from tz conversion (mtime is always a Unix timestamp / UTC)
- `check_file_should_transfer()` takes `tz_from` and `proto_from` parameters to decide whether to apply offset

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
  - This runs `cargo test`, `./test.sh`, `./test_age.sh`, `./test_conn_timeout.sh`, `./test_sftp_timeout.sh`, `./test_ftps.sh`, `./test_temp_dir.sh`, `./test_pid.sh`, `./test_pid_no_xdg.sh`, `./test_ram_threshold.sh`, and `./test_bind.sh`, plus Docker tests (`test_sftp_docker.sh`, `test_sftp_keys_docker.sh`) if Docker is available, plus web API tests (`test_web.sh`) and web UI tests (`test_web_agent.sh`) if `agent-browser` is available
  - Rule: NEVER run make test directly. Only through the Task tool with a sub-agent.

**Connection Timeout:**
- Configurable via `-t seconds` CLI flag (default: 30 seconds)
- Passed to `connect_server()` as `Duration`
- Applied via `connect_timeout()` methods for FTP/FTPS, and stream timeouts for SFTP
- Error messages include the timeout value for debugging
- Control connection read/write timeout: 60 seconds (`DEFAULT_RW_TIMEOUT` in `ftp.rs`/`ftps.rs`). Applies to all read/write ops on the FTP/FTPS control connection. Distinct from connect timeout (`-t`).

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

**Web UI (`iftpfm2-web`):**
- Separate crate in `iftpfm2-web/` directory, depends on parent `iftpfm2` library
- Axum + Tokio async web server
- SPA frontend embedded via `include_str!("../static/index.html")` — single binary, no external files
- API endpoints: `GET /api/configs`, `GET/PUT/DELETE /api/configs/{index}`, `POST /api/configs`, `POST /api/validate`
- Basic Auth (optional): `--user`/`--password` CLI flags or `IFTPFM2_WEB_USER`/`IFTPFM2_WEB_PASSWORD` env vars
  - Also accepts `IFTPM2_WEB_USER`/`IFTPM2_WEB_PASSWORD` (legacy typo fallback)
  - Uses `constant_time_eq` for password comparison (timing attack prevention)
- Read-only mode: `--readonly` flag blocks POST/PUT/DELETE
- Default listen: `127.0.0.1:3000` (configurable via `--listen`)
- Config modifications are atomic: validation → memory update → disk write; rollback on write failure
- Build: `make web` (release), `make web-debug` (debug), or `cargo build --bin iftpfm2-web --package iftpfm2-web`
- Tests: `make test-web` (API), `make test-web-ui` (UI with agent-browser)
- Log Viewer: `--logfile <path>` enables a second tab for viewing iftpfm2 log files
  - Reads from end of file (O(chunk), not O(file_size)) — handles 200MB+ logs
  - `GET /api/logs/stats` — file metadata (exists, size, path)
  - `GET /api/logs?tail=N` — last N lines (default 1000)
  - `GET /api/logs?search=Q&limit=N` — search with sliding window (VecDeque), returns last N matches + total_matches count
  - Frontend: monospace `<pre>` display, ERROR/WARNING highlighting, search, load more, refresh
- Install: `make install` installs both `iftpfm2` and `iftpfm2-web` to `~/.cargo/bin`

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

**Bind Address Behavior (`bind_from`/`bind_to` in JSONL config):**
- `bind_from`: binds outgoing connections to source server to specified local IP address
- `bind_to`: binds outgoing connections to target server to specified local IP address
- On multi-homed servers with iproute2 policy routing, source and target can use different interfaces
- FTP/FTPS: binds both control connection and passive data connections (via `passive_stream_builder`)
- SFTP: binds control connection only (data goes through SSH channel, no separate TCP connection)
- When not specified (default): OS chooses source address automatically (standard behavior)
- Uses `socket2` crate for bind-then-connect pattern
- Address validated as `IpAddr` during JSONL config parsing (serde deserialization)

## Web UI CLI Flags (`iftpfm2-web`)

| Flag | Argument | Description |
|------|----------|-------------|
| `--config` | `<path>` | Path to JSONL config file (required) |
| `--listen` | `<addr:port>` | Listen address:port (default: `127.0.0.1:3000`) |
| `--readonly` | — | Read-only mode (blocks write operations) |
| `--user` | `<login>` | Basic Auth username (env: `IFTPFM2_WEB_USER` or `IFTPM2_WEB_USER`) |
| `--password` | `<pass>` | Basic Auth password (env: `IFTPFM2_WEB_PASSWORD` or `IFTPM2_WEB_PASSWORD`) |
| `--logfile` | `<path>` | Path to iftpfm2 log file (enables Log Viewer tab) |

**Web API Endpoints:**

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/` | Serve SPA frontend (HTML) |
| GET | `/api/configs` | List all config entries |
| GET | `/api/configs/{index}` | Get config by 0-based index |
| POST | `/api/configs` | Create new config entry |
| PUT | `/api/configs/{index}` | Update config entry |
| DELETE | `/api/configs/{index}` | Delete config entry |
| POST | `/api/validate` | Validate config without saving |
| GET | `/api/logs/stats` | Log file metadata (size, exists, path) |
| GET | `/api/logs?tail=N` | Last N log lines (default 1000) |
| GET | `/api/logs?search=Q&limit=N` | Search log for substring Q, return last N matches |

**Web UI Implementation Notes:**
- SPA is vanilla JavaScript, no framework — embedded in binary via `include_str!("../static/index.html")`
- State held in `AppState`: `config_path`, `readonly`, `auth_user`, `auth_password`, `log_file: Option<String>`, `entries: Mutex<Vec<ConfigEntry>>`
- Config modifications are atomic: validate → update memory → write disk; rollback in-memory on disk write failure
- CORS enabled permissive (`CorsLayer::permissive()`) for development
- Password comparison uses `constant_time_eq` to prevent timing attacks
- All write endpoints check `readonly` flag and return 403 when enabled
