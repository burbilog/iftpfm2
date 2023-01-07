mod conf;
mod log;
mod transf;
mod args;

const PROGRAM_NAME: &str = "iftpfm2";
const PROGRAM_VERSION: &str = "2.0.0";

fn main() {
    // Parse arguments and setup logging
    let (delete, log_file, config_file, ext) = args::parse_args();
    if let Some(log_file) = log_file {
            log::set_log_file(log_file);
    }

    log::log("Begin of main()").unwrap();

    // Parse config file
    let config_file = config_file.unwrap();
    let configs = conf::parse_config(&config_file).unwrap();

    // Loop over each line in config file
    for cf in configs {
        transf::transfer_files(&cf, delete, ext.clone());
    }

    log::log("End of main()").unwrap();
}


