#[macro_use]
extern crate log;
extern crate clap;
extern crate codelauf;

use codelauf::config;
use std::path::Path;
use clap::{Arg, App, SubCommand, ArgMatches};
use codelauf::db;
use std::io::{Read,Result,Error,ErrorKind};
use std::fs::File;

fn create_db() -> db::Db {
    let database = db::Db::open(Path::new("db.sqlite")).unwrap();
    database.migrate();
    database
}

fn main() {
    let matches = config::parse_args();

    let config = config::get_config(&matches);
    println!("using config:\n {:?}", config);

    match matches.subcommand_name() {
        Some("init") => {
            create_db();
        },
        Some("index") => {
        },
        Some("sync") => {
        },
        _ => {
            println!("{}", matches.usage());
        }
    }
    

}
