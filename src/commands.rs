use super::config::Config;
use super::db::Db;
use std::path::Path;

fn create_db(config: &Config) -> Db {
    let database = Db::open(Path::new(&config.data_dir).join("db.sqlite").as_path()).unwrap();
    database.migrate();
    database
}

pub fn init(config: &Config) {
    create_db(config);
}

pub fn fetch_repo(config: &Config) {
}

pub fn index_repo(config: &Config) {
}

pub fn run_sync(config: &Config) {
}
