# Changelog

All notable changes to iftpfm2 will be documented in this file.

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

- **2.1.1** - suppaftp migration, atomic operations, race condition fixes
- **2.1.0** - JSONL config format, better extensibility
- **2.0.6** - Code modularization for better maintainability
- **2.0.5** - Per-server regexp filtering, configurable grace period
- **2.0.3** - Single instance, parallel processing, graceful shutdown
- **2.0.2** - Initial stable release with core FTP transfer functionality
