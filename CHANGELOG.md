# Changelog

All notable changes to iftpfm2 will be documented in this file.

## [2.4.6] - 2026-02-06

### Fixed
- **test_pid.sh readonly variable error** - use `CURRENT_UID` instead of `UID`
  - `UID` is a built-in readonly variable in some bash versions
  - Caused test failure on production systems with error "UID: readonly variable"

---

## [2.4.5] - 2026-02-06

### Fixed
- **Multi-user isolation for lock files** - different users can now run the program simultaneously
  - Lock files now use `$XDG_RUNTIME_DIR/iftpfm2.{sock,pid}` when available
  - Fallback to `/tmp/iftpfm2_<uid>.{sock,pid}` when `XDG_RUNTIME_DIR` is not set
  - Resolves codereview.md issue #16 (Medium priority)

- **Rename fallback data loss risk** - documented non-atomic rename behavior
  - Added detailed comment explaining the data loss window between `rm()` and `rename()`
  - Documents FTP protocol limitation (RFC 3659) regarding atomic replace operations
  - Resolves codereview.md issue #15 (Medium priority)

### Changed
- **Code cleanup** - resolved 7 minor code quality issues from codereview.md
  - #17: Removed redundant `.as_str()` calls on config fields
  - #18: Removed redundant `filename.as_str()` in file transfer loop
  - #19: Extracted `full_path()` helper to eliminate SFTP path duplication
  - #21: Removed `process::exit()` from library code, created `CliError` enum
  - #22: Implemented `-s` flag functionality (stdout logging confirmation)
  - #23: Centralized newline sanitization in `log_with_thread()`
  - #24: Replaced `expect()` with proper error handling for regex compilation
  - #25: Fixed typo in `TransferMode::Binary` documentation ("untransferred" → "untranslated")

- **All Medium priority issues completed** (7/7) - 100% of medium priority codereview items resolved
- Overall codereview progress: 21/22 items completed (95%)

### Added
- `test_pid_no_xdg.sh` integration test for lock file behavior without `XDG_RUNTIME_DIR`
- `libc` and `temp-env` dependencies for UID retrieval and testing

### Tested
- All 48 tests pass (39 unit tests + 9 integration tests)
- Multi-user isolation verified with both `XDG_RUNTIME_DIR` set and unset

---

## [2.4.4] - 2026-02-06

### Changed
- **Code quality refactoring** - improved maintainability and reduced code duplication
  - Resolves codereview.md issues #5, #6, #7, #8 (all Serious issues completed!)
  - `Client` enum methods now use `delegate!` macro eliminating ~100 lines of boilerplate
  - Extracted `connect_and_login()` helper function unifies source/target connection logic
  - Extracted `check_file_should_transfer()` encapsulates file validation (regex, age, size)
  - Extracted `handle_successful_rename()` eliminates post-rename code duplication
  - `transfer_files()` reduced from 534 to 385 lines (-28%, -149 lines)
  - Explicit password validation for FTP/FTPS with descriptive error messages

### Fixed
- **SFTP connection timeout** - SSH session now properly respects timeout setting
  - Added `session.set_timeout()` after SSH handshake
  - `test_sftp_timeout.sh` verifies timeout behavior with 5s and 2s tests
  - Resolves codereview.md issue #4 (Critical)

### Tested
- `test_sftp_timeout.sh` added to verify SFTP connection timeout behavior
- All existing integration tests pass

---

## [2.4.3] - 2026-02-06

### Added
- **`--ram-threshold <bytes>` flag** - configurable threshold for RAM vs disk temporary storage
  - Files smaller than threshold (default: 10MB) are transferred via RAM buffer for faster I/O
  - Files larger than threshold use disk temporary files to avoid OOM
  - `--ram-threshold 0` forces all files to use RAM buffer (use with caution)
  - Optimal default (10MB) balances speed and memory safety for most workloads
  - Resolves codereview.md issue #2 (Enhancement)

### Changed
- `transfer_files()` signature now includes `ram_threshold: Option<u64>` parameter
- File size is retrieved via FTP/SFTP SIZE command before download to determine storage strategy
- New `TransferBuffer` enum encapsulates RAM (`Vec<u8>`) or disk (`NamedTempFile`) storage
- Debug logging shows storage decision: "Using RAM buffer" or "Using disk buffer"

### Fixed
- **I/O overhead for many small files** - small files no longer create unnecessary disk temp files
- `test_temp_dir.sh` now uses 11.5MB test file to properly verify disk temp file behavior

### Tested
- `test_ram_threshold.sh` integration test verifies RAM/disk storage decisions
- All existing integration tests pass (test.sh, test_age.sh, test_conn_timeout.sh, test_ftps.sh, test_temp_dir.sh, test_pid.sh)

---

## [2.4.2] - 2026-02-06

### Fixed
- **External `lsof` and `kill` dependency removed** - PID handling now uses native Rust code
  - PID is read directly from lock file instead of using `lsof` command
  - Signals are sent via `nix` crate instead of `kill` command
  - Resolves codereview.md issue #3 (Critical)
  - Works on any Unix system without external utilities

### Added
- `nix` crate dependency for signal handling (features: signal, process)
- `test_pid.sh` integration test for PID file creation and signaling
- `make test-pid` target for running PID test independently
- `test_pid.sh` included in `make test` suite

### Changed
- `signal_process_to_terminate()` now reads PID from file instead of calling `lsof`
- Uses `nix::sys::signal::kill()` for sending SIGTERM/SIGKILL
- Uses `nix::sys::signal::kill(pid, None)` for checking if process exists

### Tested
- `test_pid.sh` verifies PID file creation and correct PID value
- `test_pid.sh` confirms no `lsof` string in binary
- All integration tests pass

---

### Fixed
- **OOM on large file transfers** - files are now streamed to disk using `tempfile::NamedTempFile` instead of loading entirely into RAM
  - Previously: `Vec::new()` + `read_to_end()` loaded entire file into memory
  - Now: `std::io::copy()` streams data to temporary file, then `put_file()` reads from disk
  - Resolves codereview.md issue #1 (Critical)

### Added
- **`-T <dir>` flag** - specify custom directory for temporary files
  - Useful for directing temp files to faster storage (SSD) or larger filesystems
  - Default: system temp directory (`/tmp` on Unix, `%TEMP%` on Windows)
- **`--debug` flag** - enable debug logging for diagnostic information
  - Shows temporary file paths during transfer
  - Zero overhead when disabled (compile-time check via `AtomicBool`)
- `log_debug()` function in `logging.rs` - debug-only logging with early return when disabled
- `test_temp_dir.sh` - integration test for `-T` and `--debug` flags
- `make test-temp` target - run temp directory test independently

### Changed
- `transfer_files()` signature now includes `temp_dir: Option<&str>` parameter
- Debug mode can be enabled/disabled at runtime via `set_debug_mode()`
- All integration tests now run as part of `make test` (including `test_temp_dir.sh`)

### Documentation
- Updated README.md with new CLI flags (`-T`, `--debug`) and Testing section
- Updated CLAUDE.md with CLI flags reference table and tempfile documentation
- TODO comment added for future `--verify-redownload` feature with hash computation

---

### Added
- **SFTP protocol support** - new `sftp` option for `proto_from` and `proto_to` config fields
- Password and SSH key authentication for SFTP connections
- `test_sftp_docker.sh` integration test for SFTP with Docker (atmoz/sftp container)
- `make test-sftp` target for running SFTP tests (separate from main test suite)

### Fixed
- **SFTP working directory tracking** - SFTP client now properly tracks current directory for file operations
  - Added `current_dir` field to `SftpClient` to handle SFTP's lack of true CWD concept
  - Updated `nlst()`, `mdtm()`, `size()`, `retr()`, `put_file()`, `rename()`, and `rm()` to use full paths

### Changed
- SFTP implementation uses `ssh2` crate for SSH file transfer operations
- SFTP paths are now properly resolved relative to the current working directory

---

## [2.3.0] - 2026-02-03

### Added
- **Separate protocol modules** - FTP and FTPS implementations split into distinct modules (`protocols::ftp` and `protocols::ftps`)
- `verify_final_file()` helper function for final file size verification

### Fixed
- **Incorrect protocol logging** - log messages now correctly display "ftp" or "ftps" instead of hardcoded "FTPS"
  - "TARGET FTPS login successful" -> "TARGET {proto} login successful"
  - "TARGET FTPS binary mode set successfully" -> "TARGET {proto} binary mode set successfully"
- **Code duplication** - final file verification logic extracted into reusable function

---

## [2.2.1] - 2026-02-02

### Fixed
- **CWD error messages** now include login username and target path for easier debugging
- All clippy warnings resolved (code quality improvements)
- `never_loop` error in signal handler - replaced for-break with `if let Some()`
- Added explicit `truncate(true)` when opening PID file
- Replaced manual `impl Default` with `#[default]` derive for `Protocol` enum
- Removed redundant closures in `ftp_ops.rs` (using function pointers)
- Fixed documentation indentation in `logging.rs`

### Changed
- Refactored `parse_args()` to return `CliArgs` struct instead of 8-element tuple
- Replaced `io::Error::new(Other, ...)` with `io::Error::other(...)`

---

## [2.2.0] - 2026-01-30

### Added
- **FTPS protocol support** - new `proto_from` and `proto_to` config fields for selecting FTP or FTPS per connection
- **Connection timeout parameter** - `-t seconds` flag for configurable FTP connection timeout (default: 30s)
- **Self-signed certificate support** - `--insecure-skip-verify` flag to bypass TLS certificate verification for FTPS
- `test_ftps.sh` integration test for FTPS with self-signed certificates
- `test_conn_timeout.sh` integration test for connection timeout

### Changed
- Config format: `proto_from` and `proto_to` fields added (default: `ftp`)
- Error messages now show timeout value when connection fails

---

## [2.1.1] - 2026-01-30

### Changed
- **Replaced `ftp` crate with `suppaftp`** - actively maintained fork with better async/FTPS support
- Binary mode setup moved outside file transfer loop for better performance (reduces FTP commands)

### Added
- `fs2` dependency for atomic file locking

### Fixed
- **Race condition in single-instance enforcement** - now uses atomic `flock()` on PID file instead of socket check/bind
- **Data loss risk during file transfer** - files now uploaded to temporary name (`.filename.tmp~`) and renamed after successful transfer
- Deprecated `timestamp()` calls updated to `and_utc().timestamp()` for compatibility with newer chrono

---

## [2.1.0] - 2025-01-30

### Breaking Changes
- **Config format migrated from CSV to JSONL** - CSV format (v2.0.6 and earlier) is no longer supported
- JSON field names shortened for cleaner config (`host_from`, `host_to`, etc.)

### Added
- `serde` and `serde_json` dependencies for JSONL parsing
- Migration script `migrate_csv_to_jsonl` for converting existing CSV configs
- New sample config file `sample.jsonl` in JSONL format
- Support for comments (lines starting with `#`) and empty lines in config files

### Changed
- `Config` struct now uses `#[serde(rename = "...")]` attributes for JSON field mapping
- `parse_config` now uses `serde_json::from_str` instead of manual CSV parsing
- README updated with "Breaking Changes & Migration" section

### Removed
- `csv` dependency (no longer needed)

### Fixed
- Added `serial_test` dependency to prevent race conditions in logging tests
- Updated `test.sh` to use new JSONL format

---

## [2.0.6] - 2024-05-09

### Added
- `AGENTS.md` for contributor guidance

### Changed
- Refactored monolithic `main.rs` into multiple modules (`src/cli.rs`, `src/config.rs`, `src/ftp_ops.rs`, `src/logging.rs`, `src/instance.rs`, `src/transfer.rs`)
- Improved code organization and maintainability

### Fixed
- Test script adjustments for proper execution after cargo tests

---

## [2.0.5] - 2024-05-06

### Added
- Per-server filename regexp pattern in config (`filename_regexp` field)
- Grace period parameter (`-g seconds`) for configurable shutdown timeout (default: 30)

### Changed
- Config now requires `filename_regexp` field for each transfer entry

### Fixed
- Grace period now properly passed to single instance check and signal handlers

---

## [2.0.3] - 2023-05-17

### Added
- Graceful shutdown mechanism with SIGTERM/SIGINT signal handling
- Single instance enforcement using Unix domain socket (`/tmp/iftpfm2.sock`) and PID file (`/tmp/iftpfm2.pid`)
- Thread ID logging in parallel mode (`[T1]`, `[T2]`, etc.)
- Parallel processing support (`-p number` flag)
- Config randomization (`-r` flag)
- Automatic instance termination with graceful then forceful (SIGKILL) shutdown
- Comprehensive Rust documentation for all modules, structs, and functions
- `doc` make target for generating Rust documentation

### Changed
- Increased default shutdown timeout from 5 to 30 seconds
- Improved error messages and logging

### Fixed
- stderr output removed to prevent cron emails
- Log setup moved before instance check
- Various import and type annotation fixes
- File permission handling with `PermissionsExt`

---

## [2.0.2] - 2023-02-07

### Fixed
- Rust compiler warnings
- Forced binary mode for all FTP transfers

---

## [2.0.0] - 2023-01-30

### Added
- Initial release of iftpfm2
- FTP file transfer between servers based on configuration file
- File filtering by age (seconds) and filename regexp patterns
- Delete source files after transfer (`-d` flag)
- Logging to file (`-l logfile`) or stdout with timestamps
- Configuration file support (CSV format)
- `test.sh` integration test with FTP servers
- Makefile with `debug`, `release`, `install`, `test`, and `doc` targets

### Fixed
- MDTM (modified time) parsing and handling for directories
- Error handling for CWD, LOGIN, and RM FTP commands
- Date parsing issues

---

## Version Reference

- **2.4.2** - Removed lsof/kill dependency, native PID handling via nix crate
- **2.4.1** - OOM fix with tempfile streaming, custom temp directory, debug logging
- **2.4.0** - SFTP protocol support, working directory tracking
- **2.3.0** - Separate protocol modules, logging fixes, code deduplication
- **2.2.1** - Improved error messages, code quality improvements
- **2.2.0** - FTPS protocol support, connection timeout, self-signed certificates
- **2.1.1** - suppaftp migration, atomic operations, race condition fixes
- **2.1.0** - JSONL config format, better extensibility
- **2.0.6** - Code modularization for better maintainability
- **2.0.5** - Per-server regexp filtering, configurable grace period
- **2.0.3** - Single instance, parallel processing, graceful shutdown
- **2.0.2** - Initial stable release with core FTP transfer functionality
