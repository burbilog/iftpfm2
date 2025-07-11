# AGENTS.md for iftpfm2

This document provides guidance for AI agents working with the `iftpfm2` codebase.

## Project Overview

`iftpfm2` is a command-line utility written in Rust. Its primary function is to transfer files between FTP servers based on a configuration file. It is designed to handle scenarios where atomic file transfers are necessary, preventing issues with incomplete files. The '2' in its name signifies it's the second version, replacing an older bash script.

## Project Structure

The project is organized as a Rust binary crate with a supporting library crate.

*   **`src/main.rs`**: This is the main entry point for the executable. It parses command-line arguments, sets up logging, enforces a single instance of the application, reads the configuration, and then orchestrates the file transfer operations using the library components.
*   **`src/lib.rs`**: This file defines the library crate, also named `iftpfm2`. It re-exports functionalities from various modules. The core logic of the application resides in the modules it exposes:
    *   **`cli.rs`**: Handles command-line argument parsing.
    *   **`config.rs`**: Manages parsing and validation of the configuration file.
    *   **`ftp_ops.rs`**: Contains the logic for FTP operations, including connecting, listing files, downloading, uploading, and deleting files.
    *   **`instance.rs`**: Implements the single-instance check using a PID file and a Unix domain socket. It also handles graceful shutdown requests to existing instances.
    *   **`logging.rs`**: Provides logging capabilities, allowing messages to be written to stdout or a specified log file, with timestamps and thread IDs.
    *   **`shutdown.rs`**: Manages graceful shutdown signals (SIGINT, SIGTERM) using `ctrlc`, ensuring current operations can complete before exiting.
*   **`Cargo.toml`**: The manifest file for the Rust project. It defines project metadata, dependencies, and build configurations.
*   **`README.md`**: Contains user-facing documentation, including installation instructions, usage examples, and configuration file format.
*   **`LICENSE`**: Contains the MIT license for the project.
*   **`Makefile`**: Provides utility targets, likely for building, cleaning, and potentially testing. (Agent: You should inspect this file if you need to automate build/test steps not covered by `cargo` commands).
*   **`test.sh`**: A shell script for running tests.
*   **`doc/`**: Contains generated documentation from `cargo doc`.

## Building the Project

To build the project and produce a release executable:

```bash
cargo build --release
```

The executable will be located at `target/release/iftpfm2`.
A debug build can be created with `cargo build`, and the executable will be at `target/debug/iftpfm2`.

## Running the Project

To run the application, you need a configuration file. See the "Configuration File" section below or `README.md` for its format.

```bash
./target/release/iftpfm2 path/to/your/config_file.txt
```

Common command-line options:
*   `-d`: Delete source files after successful transfer.
*   `-l <logfile>`: Specify a file for logging.
*   `-p <number>`: Set the number of parallel threads for transfers.
*   `-r`: Randomize the order of processing configuration entries.
*   `-g <seconds>`: Set the grace period for shutting down an existing instance.
*   `-h`: Display help.
*   `-v`: Display version.

## Testing

The project includes an integration test script: `test.sh`.

**Prerequisites for `test.sh`:**
*   `python3` must be installed.
*   The Python library `pyftpdlib` must be installed (`pip install pyftpdlib`). *Agent Note: This should already be handled by the environment setup script that was run when the VM was loaded.*

**How `test.sh` works:**
1.  Builds the project using `cargo build` (creates a debug build at `target/debug/iftpfm2`).
2.  Creates temporary directories `/tmp/ftp1` and `/tmp/ftp2` to serve as FTP roots.
3.  Starts two instances of a Python-based FTP server (`pyftpdlib`):
    *   Server 1: Port 2121, user `u1`, password `p1`, directory `/tmp/ftp1`.
    *   Server 2: Port 2122, user `u2`, password `p2`, directory `/tmp/ftp2`.
4.  Creates three test files (`test1.txt`, `test2.txt`, `test3.txt`) in `/tmp/ftp1`.
5.  Creates a temporary configuration file (`/tmp/config.txt`) for `iftpfm2` to transfer all `.txt` files older than 1 second from server 1 to server 2.
6.  Waits for 2 seconds to ensure the files meet the age criteria.
7.  Runs `iftpfm2` using the debug build (`./target/debug/iftpfm2`), the generated config, and the `-d` option (delete source files).
8.  Verifies that the test files:
    *   Exist in `/tmp/ftp2`.
    *   No longer exist in `/tmp/ftp1`.
9.  Prints "SUCCESS" or "ERROR" based on the verification.
10. Cleans up by killing the FTP server processes and removing temporary files and directories.

**Running the tests:**
Simply execute the script from the repository root:
```bash
bash test.sh
```
Ensure you have the prerequisites installed. The script is self-contained in terms of test data generation and cleanup.

*Agent Note: Always run `test.sh` after making any code changes to ensure core functionality remains intact. The script uses the debug build; for final testing, you might also consider manually testing with a release build.*

## Coding Conventions and Best Practices

*   **Rust Edition**: The project uses Rust 2021 edition.
*   **Error Handling**: Adhere to Rust's standard error handling mechanisms. Use `Result<T, E>` for operations that can fail. Provide meaningful error messages.
*   **Option Type**: Use `Option<T>` for values that might be absent.
*   **Clarity and Readability**: Write clear, concise, and well-commented code.
*   **Documentation**:
    *   Write `rustdoc` comments for public functions, structs, enums, and modules.
    *   Generate documentation using `cargo doc --open`.
*   **Dependencies**:
    *   Understand the purpose of key dependencies:
        *   `csv`: For parsing the configuration file (which is in CSV format).
        *   `chrono`: For handling timestamps, especially file age.
        *   `ftp`: For FTP client operations.
        *   `regex`: For matching filenames against regular expressions.
        *   `rayon`: For parallel processing of configuration entries.
        *   `ctrlc`: For handling Ctrl+C (SIGINT) and SIGTERM for graceful shutdown.
        *   `scopeguard`: Used for ensuring resources (like lock files) are cleaned up, even in case of panics.
        *   `once_cell`: For safe lazy static initialization.
        *   `tempfile`: For creating temporary files during transfers.
*   **Logging**:
    *   Use the `log()` function from `src/logging.rs` for all informational and error messages.
    *   Ensure log messages are descriptive.
*   **Single Instance Mechanism**:
    *   The application ensures only one instance runs at a time using a lock file (`/tmp/iftpfm2.pid`) and a Unix domain socket (`/tmp/iftpfm2.sock`).
    *   Be mindful of this if you are modifying startup or shutdown logic.
*   **Resource Management**: Pay attention to how resources like FTP connections and file handles are managed and closed, especially in `ftp_ops.rs` and through `scopeguard`.
*   **Parallelism**: The use of `rayon` for parallel transfers means that shared data must be handled safely (e.g., using `Arc`, `Mutex` where appropriate, though the current design passes `Config` items by reference to parallel tasks which are read-only within that scope).

## Configuration File Format

The configuration file is a CSV file where each line defines a transfer task. Comments start with `#`. The fields are:

`hostfrom,portfrom,userfrom,passfrom,pathfrom,hostto,portto,userto,passto,pathto,age_seconds,filename_regexp`

Example:
`192.168.1.100,21,user1,pass1,/outgoing,192.168.1.200,21,user2,pass2,/incoming,3600,.*\.xml$`

## Workflow for Making Changes

1.  **Understand the Task**: Ensure you have a clear understanding of the requested change or bug fix.
2.  **Explore the Code**: Identify the relevant modules and functions.
3.  **Write/Modify Code**: Implement the changes, adhering to the coding conventions.
4.  **Write/Modify Tests**:
    *   Add unit tests for new logic if applicable.
    *   Update existing tests.
    *   Ensure `test.sh` passes.
5.  **Documentation**: Update `rustdoc` comments and any relevant sections in `README.md` or this `AGENTS.md` if the changes are significant.
6.  **Build and Test**:
    *   `cargo build` (for debug) or `cargo build --release` (for release).
    *   Run `bash test.sh`.
7.  **Commit**: Follow standard commit message conventions.

## Important Files to Check

*   `README.md`: For user-level understanding.
*   `src/main.rs`: For program flow.
*   `src/lib.rs`: For library structure.
*   Relevant module files in `src/` for specific logic.
*   `Cargo.toml`: For dependencies and project settings.
*   `test.sh`: For testing procedures.

By following these guidelines, you can contribute effectively to the `iftpfm2` project.
