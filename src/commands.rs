use super::config::{Config,RepoLocation};
use super::db::{Db,Repository};
use super::result::*;
use std::path::{Path,PathBuf};
use std::fs;
use rusqlite::SqliteError;
use git2;

fn open_db(config: &Config) -> RepoResult<Db> {
    let dbpath = Path::new(&config.data_dir).join("db.sqlite");
    info!("opening db");
    let database = try!(Db::open(dbpath.as_path()).map_err(|e| RepoError::SqlError(e)));
    database.migrate();
    Ok(database)
}

pub fn init(config: &Config) {
    info!("initialising");
    open_db(config);
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

#[derive(Debug,Clone)]
struct RepoSpec {
    pub path: PathBuf,
    pub location: RepoLocation,
}

impl RepoSpec {
    pub fn new_for_config(config: &Config) -> RepoResult<RepoSpec> {
        let repo_loc = try!(config.repo_location.as_ref().ok_or(RepoError::NoRemote));

        Ok(RepoSpec::new(get_repo_path(config, repo_loc), repo_loc.clone()))
    }
    
    pub fn new(path: PathBuf, location: RepoLocation) -> RepoSpec {
        RepoSpec {
            path: path,
            location: location,
        }
    }

    /// return true if /path/to/repo/.git directory exists
    pub fn is_cloned(&self) -> bool {
        match fs::metadata(self.dot_git_path().as_path()) {
            Ok(m) => true,
            Err(e) => {
                info!("repo doesn't exist at {:?}", self.path);
                false
            }
        }
    }

    pub fn dot_git_path(&self) -> PathBuf {
        self.path.join(".git")
    }
}

fn get_repo_path(config: &Config, repo_loc: &RepoLocation) -> PathBuf {
    //TODO: either use hash or derive dir name from repo name in uri
    Path::new(&config.data_dir).join("the_repo".to_string())
}

/// find or create repo entry in db
fn get_repo_state(config: &Config, db: &Db, repo: &RepoSpec) -> RepoResult<Repository> {
    let maybe_repo = try!(db.find_repo_by_remote(&repo.location));

    match maybe_repo {
        Some(existing_repo) => Ok(existing_repo),
        None => {
            info!("creating new db repo entry for {:?}", repo);
            
            let default_branch = "master".to_string();

            let remote_uri = try!(repo.location.remote.as_ref().ok_or(RepoError::NoRemote));
            let remote_branch = repo.location.branch.as_ref().unwrap_or(&default_branch);
            let repo_path = try!(repo.path.to_str().ok_or(RepoError::PathUnicodeError));
            
            let new_repo = Repository::new_from_remote(remote_uri.clone(), remote_branch.clone(), repo_path.to_string());
            try!(db.insert_repo(&new_repo));

            info!("created db repo entry {:?}", new_repo);
            
            Ok(new_repo)
        }
    }
}

fn probe_repo_clone(config: &Config, repo: &RepoSpec) -> RepoResult<RepoSpec> {
    info!("probing cloned repo {:?}", repo);
    Ok(repo.clone())
}

/// update db entry to match
/// return resulting state
fn update_repo_state(config: &Config, db: &Db, repo: &RepoSpec) -> RepoResult<()> {
    info!("updating db repo entry to match cloned repo...");
    
    if !repo.is_cloned() {
        return Err(RepoError::NotCloned);
    }

    // get remote url and branch from clone
    let repo = try!(probe_repo_clone(config, repo));

    // find matching row in db
    let repo_stat = try!(get_repo_state(&config, &db, &repo));

    Ok(())
}

fn clone_repo(repo: &RepoSpec) -> RepoResult<git2::Repository> {
    let remote_uri = try!(repo.location.remote.as_ref().ok_or(RepoError::NoRemote));
    
    let result = try!(git2::Repository::clone(remote_uri, repo.path.clone()));

    Ok(result)
}

fn ensure_cloned(config: &Config, db: &Db, repo: &RepoSpec) -> RepoResult<()> {
    info!("ensuring cloned {:?}", repo);
    let git_repo = try!(clone_repo(repo));
    
    update_repo_state(config, db, repo)
}

fn ensure_fetched(config: &Config, db: &Db, repo: &RepoSpec) -> RepoResult<()> {
    info!("ensuring fetched {:?}", repo);
    if repo.is_cloned() {
        update_repo_state(config, db, repo)
    } else {        
        ensure_cloned(config, db, repo)
    }
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
    
    let repo = try!(RepoSpec::new_for_config(&config));

    ensure_fetched(&config, &db, &repo)
}

pub fn index_repo(config: &Config) {
}

pub fn run_sync(config: &Config) {
}
