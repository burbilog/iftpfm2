use std::env;
use std::fmt;

/// Command line arguments parsed into a struct
#[derive(Debug, PartialEq)]
pub struct CliArgs {
    pub delete: bool,
    pub log_file: Option<String>,
    pub stdout: bool,
    pub config_file: Option<String>,
    pub parallel: usize,
    pub randomize: bool,
    pub grace_seconds: u64,
    pub connect_timeout: Option<u64>,
    pub insecure_skip_verify: bool,
    pub temp_dir: Option<String>,
    pub debug: bool,
    pub ram_threshold: Option<u64>, // None = 10MB default, Some(0) = all RAM
}

/// Error types for command line argument parsing
#[derive(Debug, PartialEq)]
pub enum CliError {
    /// User requested help (exit code 0)
    HelpRequested,
    /// User requested version (exit code 0)
    VersionRequested,
    /// Missing required argument
    MissingArgument(String),
    /// Invalid argument value
    InvalidArgument(String),
    /// Unexpected argument
    UnexpectedArgument(String),
    /// Mutually exclusive flags
    MutuallyExclusiveFlags(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::HelpRequested => write!(f, "Help requested"),
            CliError::VersionRequested => write!(f, "Version requested"),
            CliError::MissingArgument(arg) => write!(f, "Missing argument: {}", arg),
            CliError::InvalidArgument(arg) => write!(f, "Invalid argument: {}", arg),
            CliError::UnexpectedArgument(arg) => write!(f, "Unexpected argument: {}", arg),
            CliError::MutuallyExclusiveFlags(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for CliError {}

/// Prints usage instructions for the program.
///
/// Uses `PROGRAM_NAME` constant from `crate` for the executable name.
pub fn print_usage() {
    println!(
        "Usage: {} [OPTIONS] config_file

Options:
  -h                 Show this help message and exit
  -v                 Show version information
  -d                 Delete source files after successful transfer
  -r                 Randomize file transfer order
  -l <logfile>       Write logs to the specified file (mutually exclusive with -s)
  -s                 Write logs to stdout (mutually exclusive with -l)
  -p <parallel>      Number of parallel transfers (default: 1)
  -g <seconds>       Grace period in seconds before SIGKILL (default: 30)
  -t <seconds>       Connection timeout in seconds (default: 30)
  -T <dir>           Directory for temporary files (default: system temp dir)
  --debug            Enable debug logging (shows temp file paths, etc.)
  --ram-threshold <bytes>
                     RAM threshold for temp files (default: 10485760)
                     Files below this size use RAM, larger use disk
  --insecure-skip-verify
                     Skip TLS certificate verification for FTPS (DANGEROUS)

Arguments:
  config_file        Path to JSONL configuration file",
        crate::PROGRAM_NAME
    );
}

/// Parses command line arguments and returns configuration options
///
/// # Returns
/// A `Result<CliArgs, CliError>` containing all parsed command line arguments.
///
/// # Errors
/// - Returns `CliError::MissingArgument` if required arguments are missing
/// - Returns `CliError::InvalidArgument` if numeric arguments can't be parsed
/// - Returns `CliError::UnexpectedArgument` if unknown arguments are provided
/// - Returns `CliError::MutuallyExclusiveFlags` if conflicting flags are used
/// - Returns `CliError::HelpRequested` if `-h` flag is used
/// - Returns `CliError::VersionRequested` if `-v` flag is used
///
/// # Example
/// ```text
/// // let args = parse_args()?;
/// // let delete = args.delete;
/// ```
pub fn parse_args() -> Result<CliArgs, CliError> {
    let mut log_file = None;
    let mut delete = false;
    let mut stdout = false;
    let mut config_file = None;
    let mut parallel = 1;
    let mut randomize = false;
    let mut grace_seconds = 30; // Default grace period
    let mut connect_timeout: Option<u64> = None; // Default 30 seconds will be applied in ftp_ops
    let mut insecure_skip_verify = false; // Default: verify certificates
    let mut temp_dir = None; // Default: use system temp directory
    let mut debug = false; // Default: no debug logging
    let mut ram_threshold: Option<u64> = None;

    let mut args = env::args();
    args.next(); // Skip program name

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" => {
                print_usage();
                return Err(CliError::HelpRequested);
            }
            "-v" => {
                println!("{} version {}", crate::PROGRAM_NAME, crate::PROGRAM_VERSION); // Using constants from lib.rs
                return Err(CliError::VersionRequested);
            }
            "-d" => delete = true,
            "-s" => stdout = true,
            "-l" => {
                let arg = args.next().ok_or_else(|| {
                    eprintln!("Error: Missing log file argument");
                    print_usage();
                    CliError::MissingArgument("log file".to_string())
                })?;
                log_file = Some(arg);
            }
            "-p" => {
                let arg = args.next().ok_or_else(|| {
                    eprintln!("Error: Missing parallel count argument");
                    print_usage();
                    CliError::MissingArgument("parallel count".to_string())
                })?;
                parallel = arg.parse().map_err(|_| {
                    eprintln!("Error: Parallel count must be a positive number");
                    print_usage();
                    CliError::InvalidArgument("parallel count must be a positive number".to_string())
                })?;
            }
            "-r" => randomize = true,
            "-g" => {
                let arg = args.next().ok_or_else(|| {
                    eprintln!("Error: Missing grace seconds argument");
                    print_usage();
                    CliError::MissingArgument("grace seconds".to_string())
                })?;
                grace_seconds = arg.parse().map_err(|_| {
                    eprintln!("Error: Grace seconds must be a positive number");
                    print_usage();
                    CliError::InvalidArgument("grace seconds must be a positive number".to_string())
                })?;
            }
            "-t" => {
                let arg = args.next().ok_or_else(|| {
                    eprintln!("Error: Missing connect timeout argument");
                    print_usage();
                    CliError::MissingArgument("connect timeout".to_string())
                })?;
                let timeout: u64 = arg.parse().map_err(|_| {
                    eprintln!("Error: Connect timeout must be a positive number");
                    print_usage();
                    CliError::InvalidArgument("connect timeout must be a positive number".to_string())
                })?;
                if timeout > 0 {
                    connect_timeout = Some(timeout);
                } else {
                    eprintln!("Error: Connect timeout must be a positive number");
                    print_usage();
                    return Err(CliError::InvalidArgument("connect timeout must be a positive number".to_string()));
                }
            }
            "--insecure-skip-verify" => {
                insecure_skip_verify = true;
            }
            "--debug" => {
                debug = true;
            }
            "-T" => {
                let arg = args.next().ok_or_else(|| {
                    eprintln!("Error: Missing temp directory argument");
                    print_usage();
                    CliError::MissingArgument("temp directory".to_string())
                })?;
                temp_dir = Some(arg);
            }
            "--ram-threshold" => {
                let arg = args.next().ok_or_else(|| {
                    eprintln!("Error: Missing RAM threshold argument");
                    print_usage();
                    CliError::MissingArgument("RAM threshold".to_string())
                })?;
                ram_threshold = Some(arg.parse().map_err(|_| {
                    eprintln!("Error: RAM threshold must be a non-negative number");
                    print_usage();
                    CliError::InvalidArgument("RAM threshold must be a non-negative number".to_string())
                })?);
            }
            _ => {
                if config_file.is_none() {
                    config_file = Some(arg);
                } else {
                    eprintln!("Unexpected argument: {}", arg);
                    print_usage();
                    return Err(CliError::UnexpectedArgument(arg));
                }
            }
        }
    }

    if config_file.is_none() {
        eprintln!("Missing config file argument");
        print_usage();
        return Err(CliError::MissingArgument("config file".to_string()));
    }

    // Check mutual exclusivity of -s and -l flags
    if stdout && log_file.is_some() {
        eprintln!("Error: -s and -l flags are mutually exclusive");
        print_usage();
        return Err(CliError::MutuallyExclusiveFlags("-s and -l flags are mutually exclusive".to_string()));
    }

    Ok(CliArgs {
        delete,
        log_file,
        stdout,
        config_file,
        parallel,
        randomize,
        grace_seconds,
        connect_timeout,
        insecure_skip_verify,
        temp_dir,
        debug,
        ram_threshold,
    })
}
