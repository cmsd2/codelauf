use super::config::{Config,RepoLocation};
use super::db::Db;
use std::path::{Path,PathBuf};
use std::fs;
use rusqlite::SqliteError;

type RepoResult<T> = Result<T, RepoError>;

#[derive(Debug)]
pub enum RepoError {
    SqlError(SqliteError),
    NoRemote,
    NotCloned,
}

fn open_db(config: &Config) -> RepoResult<Db> {
    let database = try!(Db::open(Path::new(&config.data_dir).join("db.sqlite").as_path()).map_err(|e| RepoError::SqlError(e)));
    database.migrate();
    Ok(database)
}

pub fn init(config: &Config) {
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


struct Repo {
    path: PathBuf,
    location: RepoLocation,
}

impl Repo {
    pub fn new(path: PathBuf, location: RepoLocation) -> Repo {
        Repo {
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
    Path::new(&config.data_dir).join("the_repo".to_string())
}

/// calc path to repo given data_dir and repo details
fn get_repo(config: &Config, repo_loc: &RepoLocation) -> Repo {
    Repo::new(get_repo_path(config, repo_loc), repo_loc.clone())
}

/// find or create repo entry in db
fn get_repo_state(config: &Config, db: &Db, repo: &Repo) {
}

/// probe cloned repo
/// update db entry to match
/// return resulting state
fn update_repo_state(config: &Config, db: &Db, repo: &Repo) -> RepoResult<()> {
    if !repo.is_cloned() {
        return Err(RepoError::NotCloned);
    }

    Ok(())
}

fn ensure_cloned(config: &Config, db: &Db, repo: &Repo) -> RepoResult<()> {
    update_repo_state(config, db, repo)
}

fn ensure_fetched(config: &Config, db: &Db, repo: &Repo) -> RepoResult<()> {
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
pub fn fetch_repo(config: &Config) -> Result<(), RepoError> {
    let db = try!(open_db(config));
    let repo_loc = try!(config.repo_location.as_ref().ok_or(RepoError::NoRemote));
    
    let repo = get_repo(&config, &repo_loc);
    let repo_stat = get_repo_state(&config, &db, &repo);

    ensure_fetched(&config, &db, &repo)
}

pub fn index_repo(config: &Config) {
}

pub fn run_sync(config: &Config) {
}
