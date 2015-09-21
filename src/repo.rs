use std::rc::Rc;
use std::path::{PathBuf,Path};
use std::str;
use std::str::FromStr;
use std::fs;
use std::fmt;
use git2;
use sha1::Sha1;
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

#[derive(Clone)]
pub struct Repo {
    pub id: String,
    pub path: PathBuf,
    pub uri: String,
    pub branch: String,
    pub sync_state: SyncState,
    pub git_repo: Option<Rc<git2::Repository>>,
    pub commit: Option<String>,
}

impl fmt::Debug for Repo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Repo ({:?}, {}, {}, {:?})", self.path, self.uri, self.branch, self.sync_state)
    }
}

impl Repo {
    pub fn new_for_config(config: &Config) -> RepoResult<Repo> {
        let repo_loc = try!(config.repo_location.as_ref().ok_or(RepoError::NoRemote));

        let uri = try!(repo_loc.remote.as_ref().ok_or(RepoError::NoRemote));
        let branch = repo_loc.branch.clone();

        Ok(Repo::new(try!(Repo::get_repo_path(config, repo_loc)), uri.clone(), branch, SyncState::NotCloned))
    }
    
    pub fn new(path: PathBuf, uri: String, branch: Option<String>, sync_state: SyncState) -> Repo {
        let branch = branch.unwrap_or("master".to_string());
        Repo {
            id: Repo::id(&uri, &branch),
            path: path,
            uri: uri,
            branch: branch,
            sync_state: sync_state,
            git_repo: None,
            commit: None,
        }
    }

    fn new_git_callbacks<'a>() -> git2::RemoteCallbacks<'a> {
        let mut grcs = git2::RemoteCallbacks::<'a>::new();

        grcs
            .transfer_progress(|prog| {
                info!("total: {} received: {} indexed: {}",
                      prog.total_objects(),
                      prog.received_objects(),
                      prog.indexed_objects());
                true
            })
            .sideband_progress(|data| {
                match str::from_utf8(data) {
                    Ok(v) => println!("{}", v),
                    Err(e) => println!("not utf8 data: {:?}", e)
                };
                true
            });

        grcs
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
                
                let new_repo = db::Repository::new_from_remote(self.id.clone(), remote_uri.clone(), remote_branch.clone(), repo_path.to_string());
                try!(db.insert_repo(&new_repo));
                
                info!("created db repo entry {:?}", new_repo);
                
                Ok(new_repo)
            }
        }
    }

    pub fn update_repo_in_db(&mut self, db: &db::Db) -> RepoResult<()> {
        info!("updating db repo entry to match cloned repo...");

        let mut db_repo = try!(self.find_or_create_in_db(db));

        if self.commit.is_none() {
            self.commit = db_repo.indexed_commit.clone();
        }
        
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

    pub fn clone_repo(&mut self) -> RepoResult<()> {
        self.git_repo = Some(Rc::new(try!(git2::Repository::clone(&self.uri, self.path.clone()))));

        self.sync_state = SyncState::Cloned;

        Ok(())
    }

    pub fn open_repo(&mut self) -> RepoResult<()> {
        self.git_repo = Some(Rc::new(try!(git2::Repository::open(self.path.clone()))));

        Ok(())
    }

    fn find_or_create_git_remote<'a> (&'a self, repo: &'a git2::Repository) -> RepoResult<git2::Remote> {
        // TODO: ensure returned remote has correct uri
        repo.find_remote("origin").map_err(|e| RepoError::GitError(e))
    }

    pub fn fetch_repo(&mut self) -> RepoResult<()> {
        let git_repo = try!(self.git_repo());

        let mut fo = git2::FetchOptions::new();
        let grcs = Repo::new_git_callbacks();
        
        fo.prune(git2::FetchPrune::On);
        fo.remote_callbacks(grcs);

        let mut remote = try!(self.find_or_create_git_remote(&git_repo));

        info!("fetching from remote");
        try!(remote.fetch(&[&self.branch], Some(&mut fo), None));

        Ok(())
    }

    pub fn checkout_head(&mut self) -> RepoResult<()> {
        let git_repo = try!(self.git_repo());

        let branch = try!(git_repo.find_branch(&self.branch, git2::BranchType::Local));
        let branch_fullname = try!(branch.get().name().ok_or(RepoError::StringUnicodeError));

        info!("branch full name {}", branch_fullname);
        try!(git_repo.set_head(branch_fullname));
        
        let mut cb = git2::build::CheckoutBuilder::new();
        cb.force();

        info!("checkout {}", self.branch);
        try!(git_repo.checkout_head(Some(&mut cb)).map_err(|e| RepoError::GitError(e)));

        Ok(())
    }

    /// like git update-ref refs/heads/master refs/remotes/origin/master
    pub fn repoint_head_to_origin(&mut self) -> RepoResult<()> {
        let git_repo = try!(self.git_repo());
        
        let remote = try!(self.find_or_create_git_remote(&git_repo));

        let remote_name = remote.name().unwrap();
        let branch_name = &self.branch;
        let remote_ref = format!("refs/remotes/{}/{}", remote_name, branch_name);
        let local_ref = format!("refs/heads/{}", branch_name);
        let local_oid = try!(git_repo.refname_to_id(&local_ref));
        let remote_oid = try!(git_repo.refname_to_id(&remote_ref));

        let reflog_msg = format!("update-ref: moving {} from {} to {}", local_ref, local_oid, remote_oid);
        try!(git_repo.reference(&local_ref, remote_oid, true, &reflog_msg));

        Ok(())
    }

    pub fn pull_repo(&mut self) -> RepoResult<()> {
        try!(self.fetch_repo());

        try!(self.repoint_head_to_origin());
        
        try!(self.checkout_head());

        Ok(())
    }

    /// walks commits from current head to merge-base of self.commit if any
    pub fn revwalk(&mut self) -> RepoResult<()> {
        let git_repo = try!(self.git_repo());

        let mut revwalk = try!(git_repo.revwalk());

        try!(revwalk.push_head());

        if self.commit.is_some() {
            let old_head = try!(git_repo.revparse_single(self.commit.as_ref().unwrap())).id();
            let current_head = try!(git_repo.head()).target().unwrap();
            
            let base = try!(git_repo.merge_base(old_head, current_head));
            
            try!(revwalk.hide(base));
        }

        info!("commit history:");
        for oid in revwalk {
            info!("{:?}", oid);
        }
        
        Ok(())
    }

    pub fn git_repo(&self) -> RepoResult<Rc<git2::Repository>> {
        match self.git_repo.as_ref() {
            Some(gr) => Ok(gr.clone()),
            None => Err(RepoError::InvalidState("git repo not opened".to_string())),
        }
    }
    
    pub fn set_state(&mut self, new_state: SyncState) {
        info!("repo {} {:?} --> {:?}", self.uri, self.sync_state, new_state);
        self.sync_state = new_state;
    }

    pub fn get_repo_path(config: &Config, repo_loc: &RepoLocation) -> RepoResult<PathBuf> {
        let id = Repo::id(try!(repo_loc.get_remote()), repo_loc.get_branch_or_default());
        Ok(Path::new(&config.data_dir).join("repos").join(id))
    }

    pub fn id(remote: &str, branch: &str) -> String {
        let mut h = Sha1::new();
        h.update(remote.as_bytes());
        h.update(branch.as_bytes());
        h.hexdigest()
    }
        
}




