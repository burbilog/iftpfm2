iftpfm2
=======

"iftpfm2" is a command-line utility that transfers files between FTP servers based on a configuration file. The name "iftpfm" stands for "Idiotic FTP File Mover" - it was created to solve the problem of transferring large numbers of files between multiple FTP servers and directories when using 1C software. Since 1C lacks the ability to write to temporary files and rename them atomically, simple tools like ncftpget/ncftpput can result in transferring incomplete files. The '2' suffix indicates this is the second version, replacing an original messy bash script.

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

You can then run the program by typing `./target/release/ifptfm2` followed by the appropriate command line arguments (for example: `./target/release/ifptfm2 config_file.txt`).



Usage
=====

To use iftpfm2, you need to create a configuration file that specifies the connection details for the FTP servers, and the files to be transferred. The configuration file should have the following format:

~~~
# This is a comment
hostfrom,portfrom,userfrom,passfrom,pathfrom,hostto,portto,userto,passto,pathto,age_seconds,filename_regexp
~~~

Where:
- `hostfrom`: Source FTP server hostname/IP (string)
- `portfrom`: Source FTP port (number, typically 21)
- `userfrom`: Source FTP username (string)
- `passfrom`: Source FTP password (string)
- `pathfrom`: Source directory path (must be literal path, no wildcards)
- `hostto`: Destination FTP server hostname/IP (string)
- `portto`: Destination FTP port (number, typically 21)
- `userto`: Destination FTP username (string)
- `passto`: Destination FTP password (string)
- `pathto`: Destination directory path (string)
- `age_seconds`: Minimum file age to transfer (seconds, number)
- `filename_regexp`: Regular expression pattern to match files (string)

Example:
```
192.168.1.100,21,user1,pass1,/outgoing,192.168.1.200,21,user2,pass2,/incoming,3600,.*\.xml$
```

File filtering behavior:
- All files in the literal source path are retrieved via FTP NLST command
- Files are then filtered by:
  - Minimum age (specified in config file)
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

Single Instance Behavior:
- Only one instance can run at a time
- When a new instance starts, it will:
  1. Attempt to gracefully terminate any running instance (SIGTERM)
  2. Wait up to 30 seconds for graceful shutdown
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

Examples
========

Here is an example configuration file that transfers CSV files from the /outgoing directory on the FTP server at 192.168.0.1 to the /incoming directory on the FTP server at 192.168.0.2, if they are at least one day old, filtered by age (86400 seconds) and regexp `.*\.csv$`:

```
192.168.0.1,21,user1,password1,/outgoing,192.168.0.2,21,user2,password2,/incoming,86400,.*\.csv$
```

Add this text to config.txt and run iftpfm2 to copy files using this config file and delete source files after transfer:

```
iftpfm2 -d config.csv
```

Author
======

    ChatGPT/Sonnet/DeepSeek/etc

License
=======

iftpfm2 is distributed under the terms of the MIT license. See LICENSE for details.
