use std::path::Path;

use config::Config;
use db::Db;
use result::*;
use repo::*;
use index::*;

fn open_db(config: &Config) -> RepoResult<Db> {
    let dbpath = Path::new(&config.data_dir).join("db.sqlite");
    info!("opening db");
    let database = try!(Db::open(dbpath.as_path()).map_err(|e| RepoError::SqlError(e)));
    database.migrate();
    Ok(database)
}

pub fn init(config: &Config) -> RepoResult<()> {
    info!("initialising");
    let _db = try!(open_db(config));

    Ok(())
}

/// 1. find repo dir and check consistency against sqlite db:
/// 2. if dir doesn't exist, clone it
/// 3. if sqlite commit id doesn't exist in repo clear it
/// 4. git fetch all to manually sync with remote
/// 5. if local and remote branches have diverged, find latest commit that we have in common,
///    and delete from the search index all local commits since then
/// 6. now we can fast forward through the remote commits and add them to the search index,
///    updating sqlite with the processed commit id as we go
/// 7. any files that were deleted would have been removed from the index when processing commits
/// 8. spider the entire repo and add all the files to the index, replacing any existing docs in index


fn ensure_cloned(_config: &Config, db: &Db, repo: &mut Repo) -> RepoResult<()> {
    info!("ensuring cloned {:?}", repo);
    let _git_repo = try!(repo.clone_repo());

    try!(repo.revwalk(db));
    
    repo.update_repo_in_db(db)
}

fn ensure_fetched(config: &Config, db: &Db, repo: &mut Repo) -> RepoResult<()> {
    info!("ensuring fetched {:?}", repo);
    if repo.is_cloned() {
        try!(repo.open_repo());
        
        try!(repo.pull_repo());

        try!(repo.revwalk(db));
        
        repo.update_repo_in_db(db)
    } else {        
        ensure_cloned(config, db, repo)
    }
}

fn ensure_indexed(config: &Config, db: &Db, repo: &mut Repo) -> RepoResult<()> {
    info!("ensuring indexed {:?}", repo);
    try!(ensure_fetched(&config, db, repo));

    let index = Index::new_for_config(config);

    try!(index.index_repo(db, repo));

    Ok(())
}

/// open db
/// calc repo dir location
/// create basic db entry if it doesn't exist
/// clone project if it isn't already
/// otherwise:
///   check remote url matches
///   fetch branch
///   checkout branch
/// update db as we go
pub fn fetch_repo(config: &Config) -> RepoResult<()> {    
    let db = try!(open_db(config));
    
    let mut repo = try!(Repo::new_for_config(&config));

    try!(repo.probe_fs());
    try!(repo.update_repo_in_db(&db));

    try!(ensure_fetched(&config, &db, &mut repo));

    Ok(())
}

pub fn index_repo(config: &Config) -> RepoResult<()> {
    let db = try!(open_db(config));
    
    let mut repo = try!(Repo::new_for_config(&config));

    try!(repo.probe_fs());
    try!(repo.update_repo_in_db(&db));
    
    try!(ensure_indexed(&config, &db, &mut repo));

    Ok(())
}

pub fn run_sync(_config: &Config) -> RepoResult<()> {
    Ok(())
}
