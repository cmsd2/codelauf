#[macro_use]
extern crate log;
extern crate clap;
extern crate codelauf;
extern crate env_logger;

use codelauf::config;
use codelauf::commands;

fn main() {
    env_logger::init().unwrap();
    
    let matches = config::parse_args();

    let config = config::get_config(&matches).unwrap();
    println!("using config:\n {:?}", config);

    match matches.subcommand_name() {
        Some("init") => {
            commands::init(&config);
        },
        Some("index") => {
            commands::index_repo(&config);
        },
        Some("fetch") => {
            commands::fetch_repo(&config).map_err(|e| {
                error!("error fetching repo: {:?}", e)
            });
        },
        Some("sync") => {
            commands::run_sync(&config);
        },
        _ => {
            println!("{}", matches.usage());
        }
    }
    

}
