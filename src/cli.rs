use std::env;
use std::process;

/// Command line arguments parsed into a struct
#[derive(Debug, PartialEq)]
pub struct CliArgs {
    pub delete: bool,
    pub log_file: Option<String>,
    pub config_file: Option<String>,
    pub parallel: usize,
    pub randomize: bool,
    pub grace_seconds: u64,
    pub connect_timeout: Option<u64>,
    pub insecure_skip_verify: bool,
    pub size_check: bool,
}

/// Prints usage instructions for the program.
///
/// Uses `PROGRAM_NAME` constant from `crate` for the executable name.
pub fn print_usage() {
    println!(
        "Usage: {} [-h] [-v] [-d] [-r] [-l logfile] [-p parallel] [-g grace_seconds] [-t connect_timeout] [--insecure-skip-verify] [--size-check] config_file",
        crate::PROGRAM_NAME // Now using PROGRAM_NAME from lib.rs
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
    let mut config_file = None;
    let mut parallel = 1;
    let mut randomize = false;
    let mut grace_seconds = 30; // Default grace period
    let mut connect_timeout: Option<u64> = None; // Default 30 seconds will be applied in ftp_ops
    let mut insecure_skip_verify = false; // Default: verify certificates
    let mut size_check = false; // Default: no size check

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
            "--size-check" => {
                size_check = true;
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

    CliArgs {
        delete,
        log_file,
        config_file,
        parallel,
        randomize,
        grace_seconds,
        connect_timeout,
        insecure_skip_verify,
        size_check,
    }
}
