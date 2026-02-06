iftpfm2
=======

"iftpfm2" is a command-line utility that transfers files between FTP/FTPS/SFTP servers based on a configuration file. The name "iftpfm" stands for "Idiotic FTP File Mover" - it was created to solve the problem of transferring large numbers of files between multiple FTP/SFTP servers and directories when using 1C software. Since 1C lacks the ability to write to temporary files and rename them atomically, simple tools like ncftpget/ncftpput can result in transferring incomplete files. The '2' suffix indicates this is the second version, replacing an original messy bash script.

As of January 2023, I had no prior Rust experience before creating this tool. The program was developed primarily with ChatGPT's assistance, which implemented the necessary features by following my plain English instructions. Despite being my first Rust project, the process went remarkably smoothly thanks to ChatGPT handling the heavy lifting and answering numerous basic questions.

Later improvements and refinements in May 2025 were made with help from Sonnet and DeepSeek, which helped polish the documentation and fix subtle code issues.

Installation
============

To install the ifptfm2 program, follow these steps:

1. First, make sure you have Git installed on your system. You can check if Git is already installed by running the following command in your terminal:

~~~
git --version
~~~

If Git is not installed, you can install it by following the instructions on the Git website.

2. Next, install Rust as described here https://www.rust-lang.org/learn/get-started or from your distro repository.

3. Next, clone the ifptfm2 repository by running the following command:

~~~
git clone https://github.com/burbilog/ifptfm2.git
~~~

This will create a new directory called ifptfm2 in your current location, containing all the source code for the program.

4. Change into the ifptfm2 directory by running:

~~~
cd ifptfm2
~~~

5. Finally, build the program by running:

~~~
cargo build --release
~~~

This will compile the program and create an executable file called ifptfm2 in the target/release directory.

You can then run the program by typing `./target/release/ifptfm2` followed by the appropriate command line arguments (for example: `./target/release/ifptfm2 config_file.jsonl`).

Breaking Changes & Migration
=============================

Version 2.1.0 introduces a **breaking change** in the configuration file format.

**CSV format (v2.0.6 and earlier) is no longer supported.**

### What Changed?

The configuration file format has changed from CSV to JSONL (JSON Lines):

**Old CSV format:**
```
host_from,port_from,login_from,password_from,path_from,host_to,port_to,login_to,password_to,path_to,age,filename_regexp
192.168.0.1,21,user1,pass1,/source/,192.168.0.2,21,user2,pass2,/target,86400,.*\.txt$
```

**New JSONL format:**
```
{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"pass1","path_from":"/source/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"pass2","path_to":"/target","age":86400,"filename_regexp":".*\\.txt$"}
```

### Why JSONL?

JSONL format provides:
- **Better extensibility**: New fields can be added without breaking existing configurations
- **Self-documenting**: Field names are explicit in each line
- **Better handling of complex values**: No issues with commas or special characters in values
- **Standard format**: JSONL is a well-known format with good tooling support

### How to Migrate

If you have existing CSV configuration files, use the migration script:

1. **Build the migration tool** (already included in the repository):
   ~~~
   cargo build --release --bin migrate_csv_to_jsonl
   ~~~

2. **Run the migration**:
   ~~~
   ./target/release/migrate_csv_to_jsonl your_old_config.csv your_new_config.jsonl
   ~~~

3. **Verify the output** and test with the new configuration:
   ~~~
   ./target/release/iftpfm2 your_new_config.jsonl
   ~~~

4. **Update your production** once verified

### Field Name Changes

The internal field names have been shortened for cleaner JSON:

| Old CSV Field Name | New JSONL Field Name |
|--------------------|---------------------|
| ip_address_from    | host_from           |
| ip_address_to      | host_to             |

All other field names remain the same (login_from, password_from, path_from, port_from, etc.).

### Need Help?

See `sample.jsonl` for an example of the new format.



Usage
=====

To use iftpfm2, you need to create a configuration file that specifies the connection details for the FTP servers, and the files to be transferred. The configuration file uses JSONL format (JSON Lines - one JSON object per line).

~~~
# This is a comment
{"host_from":"192.168.1.100","port_from":21,"login_from":"user1","password_from":"pass1","path_from":"/outgoing","host_to":"192.168.1.200","port_to":21,"login_to":"user2","password_to":"pass2","path_to":"/incoming","age":3600,"filename_regexp":".*\\.xml$"}
~~~

Where:
- `host_from`: Source server hostname/IP (string)
- `port_from`: Source port (number, typically 21 for FTP/FTPS, 22 for SFTP)
- `login_from`: Source username (string)
- `password_from`: Source password (string, for FTP/FTPS/SFTP password auth)
- `keyfile_from`: Source SSH private key path (string, optional for SFTP key auth)
- `keyfile_pass_from`: Source SSH private key passphrase (string, optional, only used with keyfile_from)
- `path_from`: Source directory path (must be literal path, no wildcards)
- `proto_from`: Source protocol - "ftp", "ftps", or "sftp" (optional, default: "ftp")
- `host_to`: Destination server hostname/IP (string)
- `port_to`: Destination port (number, typically 21 for FTP/FTPS, 22 for SFTP)
- `login_to`: Destination username (string)
- `password_to`: Destination password (string, for FTP/FTPS/SFTP password auth)
- `keyfile_to`: Destination SSH private key path (string, optional for SFTP key auth)
- `keyfile_pass_to`: Destination SSH private key passphrase (string, optional, only used with keyfile_to)
- `path_to`: Destination directory path (string)
- `proto_to`: Destination protocol - "ftp", "ftps", or "sftp" (optional, default: "ftp")
- `age`: Minimum file age to transfer (seconds, number) - use `0` to disable age checking and transfer all files immediately
- `filename_regexp`: Regular expression pattern to match files (string)

File filtering behavior:
- All files in the literal source path are retrieved via FTP NLST command
- Files are then filtered by:
  - Minimum age (specified in config file) - when `age` is `0`, all files pass age check
  - Regular expression pattern (specified per server in config file)

Once you have created the configuration file, you can run iftpfm2 with the following command:

~~~
iftpfm2 config_file
~~~

You can also use the following options:

    -h: Print usage information and exit.
    -v: Print version information and exit.
    -d: Delete the source files after transferring them.
    -l logfile: Write log information to the specified log file.
    -p number: Set number of parallel threads to use (default: 1)
    -r: Randomize processing order of configuration entries
    -g seconds: Grace period for shutdown in seconds (default: 30)
    -t seconds: Connection timeout in seconds (default: 30)
    -T dir: Directory for temporary files (default: system temp directory)
    --debug: Enable debug logging (shows temp file paths, etc.)
    --ram-threshold bytes: RAM threshold for temp files (default: 10485760)
    --insecure-skip-verify: Skip TLS certificate verification for FTPS connections (use with caution)

Single Instance Behavior:
- Only one instance can run at a time
- When a new instance starts, it will:
  1. Attempt to gracefully terminate any running instance (SIGTERM)
  2. Wait up to configured grace period (default: 30 seconds) for graceful shutdown
  3. Forcefully terminate if needed (SIGKILL)
- Uses a Unix domain socket (/tmp/iftpfm2.sock) and PID file (/tmp/iftpfm2.pid)
- Automatically removes lock files on exit

Graceful Shutdown:
- Handles SIGTERM/SIGINT signals
- Completes current file transfer before exiting
- Logs shutdown status
- Thread-safe shutdown flag

Logging Features:
- Timestamps all messages
- Includes thread IDs when using parallel mode
- Can log to file or stdout
- Debug mode (--debug) shows additional diagnostic information like temp file paths

Temporary Files:
- Files are downloaded to temporary storage during transfer
- Use -T flag to specify custom temp directory (useful for SSD/fast storage)
- Use --ram-threshold to control RAM vs disk storage (default: 10MB)
  - Files smaller than threshold use RAM buffer (faster, no disk I/O)
  - Files larger than threshold use disk temp files (avoids OOM)
  - --ram-threshold 0 forces all files to RAM (use with caution)
- Default: system temp directory (/tmp on Unix, %TEMP% on Windows)
- Temp files are automatically cleaned up after transfer
- Debug mode shows exact temp file paths and storage strategy for troubleshooting

Examples
========

Here is an example configuration file that transfers CSV files from the /outgoing directory on the FTP server at 192.168.0.1 to the /incoming directory on the FTP server at 192.168.0.2, if they are at least one day old, filtered by age (86400 seconds) and regexp `.*\.csv$`:

```
{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/outgoing","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/incoming","age":86400,"filename_regexp":".*\\.csv$"}
```

Add this text to config.jsonl and run iftpfm2 to copy files using this config file and delete source files after transfer:

```
iftpfm2 -d config.jsonl
```

FTPS with self-signed certificates:

```
{"host_from":"ftps.example.com","port_from":21,"login_from":"user1","password_from":"pass1","path_from":"/outgoing","proto_from":"ftps","host_to":"ftps.example.com","port_to":21,"login_to":"user2","password_from":"pass2","path_to":"/incoming","proto_to":"ftps","age":86400,"filename_regexp":".*\\.csv$"}
```

Run with `--insecure-skip-verify` to skip certificate verification (use only for testing/trusted environments):

```
iftpfm2 --insecure-skip-verify config.jsonl
```

SFTP with password authentication:

```
{"host_from":"sftp.example.com","port_from":22,"login_from":"user1","password_from":"pass1","path_from":"/outgoing","proto_from":"sftp","host_to":"sftp.example.com","port_to":22,"login_to":"user2","password_to":"pass2","path_to":"/incoming","proto_to":"sftp","age":86400,"filename_regexp":".*\\.csv$"}
```

SFTP with SSH key authentication (requires `keyfile_from`/`keyfile_to` fields):

```
{"host_from":"sftp.example.com","port_from":22,"login_from":"user1","keyfile_from":"/home/user/.ssh/id_rsa","path_from":"/outgoing","proto_from":"sftp","host_to":"sftp.example.com","port_to":22,"login_to":"user2","keyfile_to":"/home/user/.ssh/id_rsa","path_to":"/incoming","proto_to":"sftp","age":86400,"filename_regexp":".*\\.csv$"}
```

SFTP with SSH key authentication with passphrase (use `keyfile_pass_from`/`keyfile_pass_to` fields):

```
{"host_from":"sftp.example.com","port_from":22,"login_from":"user1","keyfile_from":"/home/user/.ssh/id_rsa","keyfile_pass_from":"my_secret_passphrase","path_from":"/outgoing","proto_from":"sftp","host_to":"sftp.example.com","port_to":22,"login_to":"user2","keyfile_to":"/home/user/.ssh/id_rsa","keyfile_pass_to":"my_secret_passphrase","path_to":"/incoming","proto_to":"sftp","age":86400,"filename_regexp":".*\\.csv$"}
```

Using a fast SSD for temporary files with debug logging:

```
iftpfm2 -T /mnt/ssd/tmp --debug config.jsonl
```

Testing
======

To run the full test suite (including integration tests):

~~~
make test
~~~

This runs:
- Unit tests (`cargo test`)
- Basic FTP transfer test (`test.sh`)
- File age filtering test (`test_age.sh`)
- Connection timeout test (`test_conn_timeout.sh`)
- SFTP connection timeout test (`test_sftp_timeout.sh`)
- FTPS with self-signed certificates test (`test_ftps.sh`)
- Temp directory test (`test_temp_dir.sh`)
- PID handling test (`test_pid.sh`)
- RAM threshold test (`test_ram_threshold.sh`)

To run SFTP tests (requires Docker):

~~~
make test-sftp
~~~

This runs:
- Password authentication test
- SSH key authentication (no passphrase)
- SSH key authentication with passphrase
- Delete flag test
- Regex filtering test

Individual tests can be run directly:

~~~
./test.sh           # Basic FTP transfer
./test_temp_dir.sh  # Temp directory with -T and --debug flags
./test_pid.sh       # PID file creation and signaling (no lsof dependency)
~~~

Author
======

    ChatGPT/Sonnet/DeepSeek/etc

License
=======

iftpfm2 is distributed under the terms of the MIT license. See LICENSE for details.
