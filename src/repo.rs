use std::path::{PathBuf,Path};
use std::str::FromStr;
use std::fs;
use git2;
use super::config::{Config,RepoLocation};
use super::result::*;
use super::db;

#[derive(Debug,Copy,Clone)]
pub enum SyncState {
    NotCloned,
    Cloned,
    Corrupted,
}

impl FromStr for SyncState {
    type Err = RepoError;
    fn from_str(s: &str) -> Result<SyncState, Self::Err> {
        match s {
            "NotCloned" => Ok(SyncState::NotCloned),
            "Cloned" => Ok(SyncState::Cloned),
            "Corrupted" => Ok(SyncState::Corrupted),
            _ => Err(RepoError::EnumParseError(s.to_string()))
        }
    }
}

impl ToString for SyncState {
    fn to_string(&self) -> String {
        match *self {
            SyncState::NotCloned => "NotCloned".to_string(),
            SyncState::Cloned => "Cloned".to_string(),
            SyncState::Corrupted => "Corrupted".to_string(),
        }
    }
}

#[derive(Debug,Clone)]
pub struct Repo {
    pub path: PathBuf,
    pub uri: String,
    pub branch: String,
    pub sync_state: SyncState,
}

impl Repo {
    pub fn new_for_config(config: &Config) -> RepoResult<Repo> {
        let repo_loc = try!(config.repo_location.as_ref().ok_or(RepoError::NoRemote));

        let uri = try!(repo_loc.remote.as_ref().ok_or(RepoError::NoRemote));
        let branch = repo_loc.branch.clone();

        Ok(Repo::new(Repo::get_repo_path(config, repo_loc), uri.clone(), branch, SyncState::NotCloned))
    }
    
    pub fn new(path: PathBuf, uri: String, branch: Option<String>, sync_state: SyncState) -> Repo {
        Repo {
            path: path,
            uri: uri,
            branch: branch.unwrap_or("master".to_string()),
            sync_state: sync_state,
        }
    }

    pub fn is_cloned(&self) -> bool {
        match self.sync_state {
            SyncState::NotCloned => false,
            _ => true
        }
    }
    
    pub fn dot_git_path(&self) -> PathBuf {
        self.path.join(".git")
    }
    
    pub fn dot_git_exists(&self) -> bool {
        match fs::metadata(self.dot_git_path().as_path()) {
            Ok(_) => true,
            Err(_) => {
                info!("repo doesn't exist at {:?}", self.path);
                false
            }
        }
    }

    pub fn find_in_db(&self, db: &db::Db) -> RepoResult<Option<db::Repository>> {
        db.find_repo_by_remote(&self.uri, &self.branch)
    }
    
    pub fn find_or_create_in_db(&mut self, db: &db::Db) -> RepoResult<db::Repository> {
        let maybe_repo = try!(self.find_in_db(db));

        match maybe_repo {
            Some(existing_repo) => {                
                Ok(existing_repo)
            }
            None => {
                info!("creating new db repo entry for {:?}", self);
                
                let remote_uri = &self.uri;
                let remote_branch = &self.branch;
                let repo_path = try!(self.path.to_str().ok_or(RepoError::PathUnicodeError));
                
                let new_repo = db::Repository::new_from_remote(remote_uri.clone(), remote_branch.clone(), repo_path.to_string());
                try!(db.insert_repo(&new_repo));
                
                info!("created db repo entry {:?}", new_repo);
                
                Ok(new_repo)
            }
        }
    }

    pub fn update_repo_in_db(&mut self, db: &db::Db) -> RepoResult<()> {
        info!("updating db repo entry to match cloned repo...");

        let mut db_repo = try!(self.find_or_create_in_db(db));

        match db_repo.sync_state {
            SyncState::NotCloned => {
                db_repo.sync_state = self.sync_state;
            },
            SyncState::Cloned => {
                db_repo.sync_state = self.sync_state;
            },
            other_state => {
                self.sync_state = other_state;
            }
        }
        
        try!(db.update_repo(&db_repo));
        
        Ok(())
    }
    
    pub fn probe_fs(&mut self) -> RepoResult<()> {
        info!("probing cloned repo {}", self.uri);

        if !self.dot_git_exists() {
            self.set_state(SyncState::NotCloned);
            Ok(())
        } else {
            match self.sync_state {
                SyncState::NotCloned => {
                    self.set_state(SyncState::Cloned);
                }
                _ => {}
            }
            Ok(())
        }
    }

    pub fn clone_repo(&mut self) -> RepoResult<git2::Repository> {
        let result = try!(git2::Repository::clone(&self.uri, self.path.clone()));

        self.sync_state = SyncState::Cloned;

        Ok(result)
    }
    
    pub fn set_state(&mut self, new_state: SyncState) {
        info!("repo {} {:?} --> {:?}", self.uri, self.sync_state, new_state);
        self.sync_state = new_state;
    }

    fn get_repo_path(config: &Config, _repo_loc: &RepoLocation) -> PathBuf {
        //TODO: either use hash or derive dir name from repo name in uri
        Path::new(&config.data_dir).join("the_repo".to_string())
    }
}




