#[macro_use]
extern crate log;
extern crate clap;
extern crate codelauf;
extern crate toml;

use std::path::Path;
use clap::{Arg, App, SubCommand, ArgMatches};
use codelauf::db;
use toml::{Table, Parser};
use std::io::{Read,Result,Error,ErrorKind};
use std::fs::File;

fn parse_args<'a,'b>() -> ArgMatches<'a,'b> {
    App::new("codelauf")
        .version("1.0")
        .author("Chris Dawes <cmsd2@cantab.net>")
        .about("Codelauf indexes git repositories for search")
        .args_from_usage(
            "-c --config=[CONFIG] 'Sets a custom config file'")
        .subcommand(SubCommand::with_name("init")
                    .about("creates the local database and exits")
                    .args_from_usage(
                        "-v --verbose 'Print stuff'")
                    )
        .get_matches()
}

#[derive(Debug,Clone)]
struct Config {
    data_dir: String
}

impl Config {
    pub fn new() -> Config {
        Config {
            data_dir: ".".to_string()
        }
    }
    
    pub fn new_from_table(table: Table) -> Config {
        let mut cfg = Self::new();
        cfg
    }
}

fn parse_config(path: &str) -> Result<Config> {
    let mut f = try!(File::open("foo.txt"));
    
    let mut s = String::new();
    try!(f.read_to_string(&mut s));
    
    let mut p = Parser::new(&s);

    p.parse().map(Config::new_from_table).ok_or(Error::new(ErrorKind::Other, "oh no!"))
}

fn read_config(config: Option<&str>) -> Result<Config> {
    match config {
        Some(path) => parse_config(path),
        None => Ok(Config::new())
    }
}

fn apply_config<'a,'b>(cfg: Config, args: &ArgMatches<'a,'b>) -> Config {
    cfg
}

fn get_config<'a,'b>(args: &ArgMatches<'a,'b>) -> Result<Config> {
    let mut maybe_config = read_config(args.value_of("CONFIG"));

    maybe_config.map_err(|err| {
        info!("error reading config file: {:?}", err);
        err
    }).map(|config| {
        apply_config(config, args)
    })
}

fn main() {
    let matches = parse_args();

    let config = get_config(&matches);
    println!("using config:\n {:?}", config);
    
    let database = db::Db::open(Path::new("db.sqlite")).unwrap();
    database.migrate();
}
