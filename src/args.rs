use std::env;
use std::process;

fn print_usage() {
    println!(
        "Usage: {} [-h] [-v] [-d] [-x \"*.xml\"] [-l logfile] config_file",
        super::PROGRAM_NAME
    );
}

pub fn parse_args() -> (bool, Option<String>, Option<String>, Option<String>) {
    let mut log_file = None;
    let mut delete = false;
    let mut config_file = None;
    let mut ext = None;


    let mut args = env::args();
    args.next(); // Skip program name

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" => {
                print_usage();
                process::exit(0);
            }
            "-v" => {
                println!("{} version {}", super::PROGRAM_NAME, super::PROGRAM_VERSION);
                process::exit(0);
            }
            "-d" => delete = true,
            "-l" => log_file = Some(args.next().expect("Missing log file argument")),
            "-x" => ext = Some(args.next().expect("Missing file extension argument")),
            _ => {
                config_file = Some(arg);
            }
        }
    }

    if config_file.is_none() {
        eprintln!("Missing config file argument");
        print_usage();
        process::exit(1);
    }

    if ext.is_none() {
        ext = Some("*.xml".to_string());
    }

    (delete, log_file, config_file, ext)
}

