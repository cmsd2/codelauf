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

#[derive(Debug,Clone)]
pub struct Branch {
    pub name: String,
    pub indexed_commit: Option<String>,
}

impl Branch {
    pub fn new(name: String, indexed_commit: Option<String>) -> Branch {
        Branch {
            name: name,
            indexed_commit: indexed_commit,
        }
    }
}

#[derive(Clone)]
pub struct Repo {
    pub id: String,
    pub path: PathBuf,
    pub uri: String,
    pub branches: Vec<Branch>,
    pub sync_state: SyncState,
    pub git_repo: Option<Rc<git2::Repository>>,
}

impl fmt::Debug for Repo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Repo ({:?}, {}, {:?}, {:?})", self.path, self.uri, self.branches, self.sync_state)
    }
}

impl Repo {
    pub fn new_for_config(config: &Config) -> RepoResult<Repo> {
        let repo_loc = try!(config.repo_location.as_ref().ok_or(RepoError::NoRemote));

        let uri = try!(repo_loc.remote.as_ref().ok_or(RepoError::NoRemote));
        let branches = repo_loc.branches.iter().map(|b| Branch::new(b.clone(), None) ).collect();

        Ok(Repo::new(try!(Repo::get_repo_path(config, repo_loc)), uri.clone(), branches, SyncState::NotCloned))
    }
    
    pub fn new(path: PathBuf, uri: String, branches: Vec<Branch>, sync_state: SyncState) -> Repo {
        Repo {
            id: Repo::id(&uri),
            path: path,
            uri: uri,
            branches: branches,
            sync_state: sync_state,
            git_repo: None,
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
        db.find_repo_by_remote(&self.uri)
    }

    pub fn create_in_db(&self, db: &db::Db) -> RepoResult<db::Repository> {
        info!("creating new db repo entry for {:?}", self);

        let remote_uri = &self.uri;
        
        let new_repo = db::Repository::new_from_remote(self.id.clone(), remote_uri.clone(), self.path.clone());
        try!(db.insert_repo(&new_repo));
        
        info!("created db repo entry {:?}", new_repo);

        for branch in &self.branches {
            let new_branch = db::RepoBranch::new(new_repo.id.clone(), branch.name.clone(), None);
            
            try!(db.insert_branch(&new_branch));
            
            info!("created db repo branch entry {:?}", new_branch);
        }

        Ok(new_repo)
    }
    
    pub fn find_or_create_in_db(&mut self, db: &db::Db) -> RepoResult<db::Repository> {
        let maybe_repo = try!(self.find_in_db(db));

        match maybe_repo {
            Some(existing_repo) => {                
                Ok(existing_repo)
            }
            None => {                
                let new_repo = try!(self.create_in_db(db));
                
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

    pub fn fetch_repo(&self) -> RepoResult<()> {
        let git_repo = try!(self.git_repo());

        let mut fo = git2::FetchOptions::new();
        let grcs = Repo::new_git_callbacks();
        
        fo.prune(git2::FetchPrune::On);
        fo.remote_callbacks(grcs);

        let mut remote = try!(self.find_or_create_git_remote(&git_repo));

        info!("fetching from remote");
        let branch_names: Vec<&str> = self.branches.iter().map(|s| &s.name[..]).collect();
        try!(remote.fetch(&branch_names, Some(&mut fo), None));
        info!("fetched.");

        Ok(())
    }

    pub fn find_branch(&self, git_repo: &git2::Repository, branch_name: &str) -> RepoResult<String> {
        info!("finding branch {}", branch_name);
        
        let branch = try!(git_repo.find_branch(branch_name, git2::BranchType::Local));
        
        let branch_fullname = try!(branch.get().name().ok_or(RepoError::StringUnicodeError).map(|s| s.to_string()));

        info!("found branch {}", branch_fullname);

        Ok(branch_fullname)
    }

    pub fn checkout_branch(&mut self, branch_name: &str) -> RepoResult<()> {
        let git_repo = try!(self.git_repo());

        let branch_fullname = try!(self.find_branch(&git_repo, branch_name));

        info!("setting head to {}", branch_fullname);
        try!(git_repo.set_head(&branch_fullname));
        
        let mut cb = git2::build::CheckoutBuilder::new();
        cb.force();

        info!("checkout {}", branch_name);
        try!(git_repo.checkout_head(Some(&mut cb)).map_err(|e| RepoError::GitError(e)));

        Ok(())
    }

    /// like git update-ref refs/heads/master refs/remotes/origin/master
    pub fn repoint_branch_to_origin(&self, branch_name: &str) -> RepoResult<()> {
        let git_repo = try!(self.git_repo());
        
        let remote = try!(self.find_or_create_git_remote(&git_repo));

        let remote_name = remote.name().unwrap();
        let remote_ref = format!("refs/remotes/{}/{}", remote_name, branch_name);
        let local_ref = format!("refs/heads/{}", branch_name);

        info!("getting commit id for local branch {}", local_ref);
        let local_oid = try!(git_repo.refname_to_id(&local_ref));

        info!("getting commit id for remote branch {}", remote_ref);
        let remote_oid = try!(git_repo.refname_to_id(&remote_ref));

        let reflog_msg = format!("update-ref: moving {} from {} to {}", local_ref, local_oid, remote_oid);
        try!(git_repo.reference(&local_ref, remote_oid, true, &reflog_msg));

        Ok(())
    }

    pub fn pull_repo(&self) -> RepoResult<()> {
        try!(self.fetch_repo());

        for branch in &self.branches {
            try!(self.repoint_branch_to_origin(&branch.name));
        }
        
        //try!(self.checkout_head());

        Ok(())
    }

    pub fn revwalk_add_branch(&self, git_repo: &git2::Repository, revwalk: &mut git2::Revwalk, branch_name: &str, indexed_commit: &Option<String>) -> RepoResult<()> {
        let branch_fullname = try!(self.find_branch(git_repo, branch_name));
        
        let branch_commit = try!(git_repo.refname_to_id(&branch_fullname));

        if indexed_commit.is_some() {
            let indexed_commit_id = try!(git_repo.revparse_single(indexed_commit.as_ref().unwrap())).id();
            
            let bases = try!(git_repo.merge_bases(branch_commit, indexed_commit_id));
            
            for base in bases.iter() {
                try!(revwalk.hide(*base));
            }
        }
        
        try!(revwalk.push(branch_commit));

        Ok(())
    }

    /// walks commits from current head to merge-base of self.commit if any
    pub fn revwalk(&self) -> RepoResult<()> {
        info!("walking revision tree");
        
        let git_repo = try!(self.git_repo());

        let mut revwalk = try!(git_repo.revwalk());

        if self.branches.is_empty() {
            try!(self.revwalk_add_branch(&git_repo, &mut revwalk, "master", &None));
        } else {
            for branch in &self.branches {
                try!(self.revwalk_add_branch(&git_repo, &mut revwalk, &branch.name, &branch.indexed_commit));
            }
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
        let id = Repo::id(try!(repo_loc.get_remote()));
        Ok(Path::new(&config.data_dir).join("repos").join(id))
    }

    pub fn id(remote: &str) -> String {
        let mut h = Sha1::new();
        h.update(remote.as_bytes());
        h.hexdigest()
    }
}




