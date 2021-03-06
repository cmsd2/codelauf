use std::env;
use clap::{App, SubCommand, ArgMatches};
use toml::{Table, Parser};
use std::io::{Read,Result,Error,ErrorKind};
use std::fs::File;
use super::result::*;

#[derive(Debug,Clone)]
pub struct Config {
    pub data_dir: String, // where to create database and repo clones
    pub zookeeper: Option<String>, // e.g. localhost:2181/codelauf
    pub elasticsearch: Option<String>, // e.g. localhost:9200
    pub index_config: IndexConfig,
    pub sync_config: SyncConfig,
    pub repo_location: Option<RepoLocation>,
}

impl Config {
    pub fn new() -> Config {
        Config {
            data_dir: ".".to_string(),
            zookeeper: None,
            elasticsearch: None,
            index_config: IndexConfig::new(),
            sync_config: SyncConfig::new(),
            repo_location: None,
        }
    }
    
    pub fn new_from_table(table: &Table) -> Config {
        let mut cfg = Self::new();
        cfg.data_dir = table
            .get("data_dir")
            .map(|m| m.as_str().unwrap().to_string())
            .unwrap_or(cfg.data_dir);
        cfg.zookeeper = table
            .get("zookeeper")
            .map(|m| m.as_str().unwrap().to_string());
        cfg.elasticsearch = table
            .get("elasticsearch")
            .map(|m| m.as_str().unwrap().to_string());
        cfg.index_config = table
            .get("index")
            .map(|m| IndexConfig::new_from_table(m.as_table().unwrap()) )
            .unwrap_or(cfg.index_config);
        cfg.sync_config = table
            .get("sync")
            .map(|m| SyncConfig::new_from_table(m.as_table().unwrap()) )
            .unwrap_or(cfg.sync_config);
        cfg
    }
}

#[derive(Debug,Clone)]
pub struct IndexConfig;

impl IndexConfig {
    pub fn new() -> IndexConfig {
        IndexConfig
    }
    
    pub fn new_from_table(_table: &Table) -> IndexConfig {
        let cfg = Self::new();
        cfg
    }
}

#[derive(Debug,Clone)]
pub struct SyncConfig;

impl SyncConfig {
    pub fn new() -> SyncConfig {
        SyncConfig
    }

    pub fn new_from_table(_table: &Table) -> SyncConfig {
        let cfg = Self::new();
        cfg
    }
}

#[derive(Debug,Clone)]
pub struct RepoLocation {
    pub remote: Option<String>,
    pub branches: Vec<String>,
    pub dir: Option<String>,
}

impl RepoLocation {
    pub fn new() -> RepoLocation {
        RepoLocation {
            remote: None,
            branches: vec![],
            dir: None,
        }
    }

    pub fn get_remote<'a>(&'a self) -> RepoResult<&'a str> {
        self.remote.as_ref().map(|s| s as &str).ok_or(RepoError::NoRemote)
    }

    pub fn new_from_args<'a,'b>(args: &ArgMatches<'a,'b>) -> Option<RepoLocation> {
        if args.is_present("REMOTE") || args.is_present("REPO_DIR") {
            let mut repo_loc = RepoLocation::new();
            
            repo_loc.remote = get_config_str(args, "REMOTE")
                .or(repo_loc.remote);

            match args.values_of("BRANCH") {
                Some(branches) => {
                    for branch in branches {
                        repo_loc.branches.push(branch.to_string());
                    }
                },
                None => {
                    repo_loc.branches.push("master".to_string());
                }
            }
            
            repo_loc.dir = get_config_str(args, "REPO_DIR")
                .or(repo_loc.dir);
            
            Some(repo_loc)
        } else {
            None
        }
    }
}

pub fn parse_args<'a,'b>() -> ArgMatches<'a,'b> {
    App::new("codelauf")
        .version("1.0")
        .author("Chris Dawes <cmsd2@cantab.net>")
        .about("Codelauf indexes git repositories for search")
        .args_from_usage(
            "-c --config=[CONFIG] 'Sets a custom config file'
            -z --zookeeper=[ZOOKEEPER] 'Zookeeper host:port[/dir] (env var ZOOKEEPER)'
            -e --elasticsearch=[ELASTICSEARCH] 'Elasticsearch host:port (env var ELASTICSEARCH)'
            -d --data-dir=[DATA_DIR] 'Data directory'")
        .subcommand(SubCommand::with_name("init")
                    .about("creates the local database and exits")
                    .args_from_usage("")
                    )
        .subcommand(SubCommand::with_name("index")
                    .about("indexes a single repository and exits")
                    .args_from_usage(
                        "-r --remote=[REMOTE] 'Repository remote url (required if not already cloned)'
                        -b --branch=[BRANCH] 'Branch (default master)'
                        -R --repo-dir=[REPO_DIR] 'Repo dir to use for repo (clones if it does not exist)'")
                    )
        .subcommand(SubCommand::with_name("fetch")
                    .about("clones or fetches a repository and exits")
                    .args_from_usage(
                        "-r --remote=[REMOTE] 'Repository remote url (required if not already cloned)'
                        -b --branch=[BRANCH] 'Branch (default master)'
                        -R --repo-dir=[REPO_DIR] 'Repo dir to use for repo (clones if it does not exist)'")
                    )
        .subcommand(SubCommand::with_name("sync")
                    .about("starts the worker process to mirror and index repos")
                    .args_from_usage("")
                    )
        .get_matches()
}

pub fn parse_config(path: &str) -> Result<Config> {
    let mut f = try!(File::open(path));
    
    let mut s = String::new();
    try!(f.read_to_string(&mut s));
    
    let mut p = Parser::new(&s);

    p.parse().map(|m| Config::new_from_table(&m)).ok_or(Error::new(ErrorKind::Other, "config parsing error"))
}

pub fn read_config(config: Option<String>) -> Result<Config> {
    match config {
        Some(path) => parse_config(&path),
        None => Ok(Config::new())
    }
}

pub fn get_env(name: &str) -> Option<String> {
    match env::var(name) {
        Ok(val) => Some(val),
        Err(e) => {
            info!("not using environment variable {}: {:?}", name, e);
            None
        }
    }
}

pub fn apply_config<'a,'b>(cfg: Config, args: &ArgMatches<'a,'b>) -> Config {
    let mut cfg = cfg;
    
    cfg.zookeeper = get_config_str_env(args, "ZOOKEEPER", "ZOOKEEPER")
        .or(cfg.zookeeper);

    cfg.elasticsearch = get_config_str_env(args, "ELASTICSEARCH", "ELASTICSEARCH")
        .or(cfg.elasticsearch);

    cfg.data_dir = get_config_str(args, "DATA_DIR")
        .unwrap_or(cfg.data_dir);

    match args.subcommand() {
        ("index", Some(indexargs)) => {
            cfg.repo_location = RepoLocation::new_from_args(&indexargs);
        },
        ("fetch", Some(fetchargs)) => {
            cfg.repo_location = RepoLocation::new_from_args(&fetchargs);
        },
        ("sync", Some(_syncargs)) => {
        },
        _ => {}
    }
    
    cfg
}

pub fn get_config_str<'a,'b>(args: &ArgMatches<'a,'b>, key: &str) -> Option<String> {
    args.value_of(key)
        .map(|s| s.to_string())
}

pub fn get_config_str_env<'a,'b>(args: &ArgMatches<'a,'b>, key: &str, env_key: &str) -> Option<String> {
    args.value_of(key)
        .map(|s| s.to_string())
        .or(get_env(env_key))
}

pub fn get_config<'a,'b>(args: &ArgMatches<'a,'b>) -> Result<Config> {
    let maybe_config = read_config(get_config_str(args, "CONFIG"));

    maybe_config.map_err(|err| {
        error!("error reading config file: {:?}", err);
        err
    }).map(|cfg| {
        apply_config(cfg, args)
    })
}
