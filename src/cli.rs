use std::env;
use std::process;

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
}

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
/// A `CliArgs` struct containing all parsed command line arguments.
///
/// # Panics
/// - If required arguments are missing
/// - If numeric arguments can't be parsed
///
/// # Example
/// ```text
/// // let args = parse_args();
/// // let delete = args.delete;
/// ```
pub fn parse_args() -> CliArgs {
    let mut log_file = None;
    let mut delete = false;
    let mut stdout = false;
    let mut config_file = None;
    let mut parallel = 1;
    let mut randomize = false;
    let mut grace_seconds = 30; // Default grace period
    let mut connect_timeout: Option<u64> = None; // Default 30 seconds will be applied in ftp_ops
    let mut insecure_skip_verify = false; // Default: verify certificates

    let mut args = env::args();
    args.next(); // Skip program name

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" => {
                print_usage();
                process::exit(0);
            }
            "-v" => {
                println!("{} version {}", crate::PROGRAM_NAME, crate::PROGRAM_VERSION); // Using constants from lib.rs
                process::exit(0);
            }
            "-d" => delete = true,
            "-s" => stdout = true,
            "-l" => log_file = Some(args.next().unwrap_or_else(|| {
                eprintln!("Error: Missing log file argument");
                print_usage();
                process::exit(1);
            })),
            "-p" => {
                parallel = match args.next() {
                    Some(arg) => match arg.parse() {
                        Ok(n) => n,
                        Err(_) => {
                            eprintln!("Error: Parallel count must be a positive number");
                            print_usage();
                            process::exit(1);
                        }
                    },
                    None => {
                        eprintln!("Error: Missing parallel count argument");
                        print_usage();
                        process::exit(1);
                    }
                }
            }
            "-r" => randomize = true,
            "-g" => {
                grace_seconds = match args.next() {
                    Some(arg) => match arg.parse() {
                        Ok(n) => n,
                        Err(_) => {
                            eprintln!("Error: Grace seconds must be a positive number");
                            print_usage();
                            process::exit(1);
                        }
                    },
                    None => {
                        eprintln!("Error: Missing grace seconds argument");
                        print_usage();
                        process::exit(1);
                    }
                }
            }
            "-t" => {
                connect_timeout = match args.next() {
                    Some(arg) => match arg.parse::<u64>() {
                        Ok(n) if n > 0 => Some(n),
                        _ => {
                            eprintln!("Error: Connect timeout must be a positive number");
                            print_usage();
                            process::exit(1);
                        }
                    },
                    None => {
                        eprintln!("Error: Missing connect timeout argument");
                        print_usage();
                        process::exit(1);
                    }
                }
            }
            "--insecure-skip-verify" => {
                insecure_skip_verify = true;
            }
            _ => {
                if config_file.is_none() {
                    config_file = Some(arg);
                } else {
                    eprintln!("Unexpected argument: {}", arg);
                    print_usage();
                    process::exit(1);
                }
            }
        }
    }

    if config_file.is_none() {
        eprintln!("Missing config file argument");
        print_usage();
        process::exit(1);
    }

    // Check mutual exclusivity of -s and -l flags
    if stdout && log_file.is_some() {
        eprintln!("Error: -s and -l flags are mutually exclusive");
        print_usage();
        process::exit(1);
    }

    CliArgs {
        delete,
        log_file,
        stdout,
        config_file,
        parallel,
        randomize,
        grace_seconds,
        connect_timeout,
        insecure_skip_verify,
    }
}
