#[macro_use]
extern crate log;
extern crate clap;
extern crate codelauf;
extern crate env_logger;

use codelauf::config;
use codelauf::commands;
use codelauf::result::*;
use std::process;

fn run() -> RepoResult<()> {
    env_logger::init().unwrap();
    
    let args = config::parse_args();

    let config = config::get_config(&args).unwrap();
    println!("using config:\n {:?}", config);

    
    match args.subcommand_name() {
        Some("init") => {
            commands::init(&config)
        },
        Some("index") => {
            commands::index_repo(&config)
        },
        Some("fetch") => {
            commands::fetch_repo(&config)
        },
        Some("sync") => {
            commands::run_sync(&config)
        },
        _ => {
            println!("{}", args.usage());
            Err(RepoError::InvalidArgs("unrecognised command".to_string()))
        }
    }
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(e) => {
            println!("error: {:?}", e);
            process::exit(1);
        }
    }
}
