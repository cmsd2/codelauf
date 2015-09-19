extern crate codelauf;

use std::path::Path;
use codelauf::db;

fn main() {
    println!("Hello, world!");

    let database = db::Db::open(Path::new("db.sqlite")).unwrap();
    database.migrate();
}
