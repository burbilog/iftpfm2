use std::env;
use std::process;

/// Prints usage instructions for the program.
///
/// Uses `PROGRAM_NAME` constant from `crate` for the executable name.
pub fn print_usage() {
    println!(
        "Usage: {} [-h] [-v] [-d] [-r] [-l logfile] [-p parallel] [-g grace_seconds] [-t connect_timeout] [--insecure-skip-verify] config_file",
        crate::PROGRAM_NAME // Now using PROGRAM_NAME from lib.rs
    );
}

/// Parses command line arguments and returns configuration options
///
/// # Returns
/// A tuple containing:
/// - `bool`: Whether to delete source files after transfer
/// - `Option<String>`: Path to log file (None for stdout).
/// - `Option<String>`: Path to config file.
/// - `usize`: Number of parallel threads.
/// - `bool`: Whether to randomize processing order.
/// - `u64`: Grace period in seconds for shutdown.
/// - `Option<u64>`: Connection timeout in seconds (None = 30s default).
/// - `bool`: Whether to skip TLS certificate verification (FTPS only).
///
/// # Panics
/// - If required arguments are missing
/// - If numeric arguments can't be parsed
///
/// # Example
/// ```text
/// // let (delete, log_file, config_file, parallel, randomize, grace_seconds, connect_timeout, insecure_skip_verify) = parse_args();
/// ```
pub fn parse_args() -> (bool, Option<String>, Option<String>, usize, bool, u64, Option<u64>, bool) {
    let mut log_file = None;
    let mut delete = false;
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

    (delete, log_file, config_file, parallel, randomize, grace_seconds, connect_timeout, insecure_skip_verify)
}
