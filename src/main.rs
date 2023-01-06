mod conf;
mod log;
mod transf;
mod args;

const PROGRAM_NAME: &str = "iftpfm2";
const PROGRAM_VERSION: &str = "0.1.0";

fn main() {
    println!("Begin of main()");
    let (delete, log_file, config_file) = args::parse_args();
    let config_file = config_file.unwrap();
    let configs = conf::parse_config(&config_file).unwrap();
    if let Some(log_file) = log_file {
            log::set_log_file(log_file);
    }
    for cf in configs {
        transf::transfer_files(&cf, delete);
    }
    println!("End of main()");
}


