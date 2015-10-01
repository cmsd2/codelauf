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
pub struct RepoTreeEntry {
    pub entry: git2::TreeEntry<'static>,
    pub path: PathBuf,
}

impl RepoTreeEntry {
    pub fn new(entry: git2::TreeEntry<'static>, path: PathBuf) -> RepoTreeEntry {
        RepoTreeEntry {
            entry: entry,
            path: path
        }
    }

    pub fn from_ref(entry: &git2::TreeEntry, path: &Path) -> Option<RepoTreeEntry> {
        let maybe_name = entry.name();
        
        maybe_name.map(|name| {
            RepoTreeEntry::new(entry.to_owned(), path.join(name).to_owned())
        })
    }
}

pub struct RecursiveTreeIter<'a> {
    entries: Vec<RepoTreeEntry>,
    repo: &'a git2::Repository,
}

impl<'a> Iterator for RecursiveTreeIter<'a> {
    type Item = RepoTreeEntry;
    
    fn next(&mut self) -> Option<RepoTreeEntry> {
        if self.entries.is_empty() {
            None
        } else {
            let repo_entry = self.entries.remove(0);
            let entry = &repo_entry.entry;
            
            match entry.kind() {
                Some(git2::ObjectType::Tree) => {
                    let obj: git2::Object<'a> = entry.to_object(self.repo).unwrap();
                    
                    let tree: &git2::Tree<'a> = obj.as_tree().unwrap();
                    
                    for entry in tree.iter() {
                        let child_repo_entry = RepoTreeEntry::from_ref(&entry, &repo_entry.path);

                        if child_repo_entry.is_some() {
                            self.entries.push(child_repo_entry.unwrap());
                        }
                    }
                }
                _ => {}
            }
            
            Some(repo_entry.clone())
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

        let branch_commit = try!(self.branch_commit_id(branch_name));
        
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
    pub fn revwalk(&self, db: &db::Db) -> RepoResult<()> {
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
            try!(self.add_commit(db, &oid));
        }
        
        Ok(())
    }

    pub fn get_commit<'a>(&'a self, commit_id: &str) -> RepoResult<git2::Commit<'a> > {
        info!("getting commit {:?}", commit_id);
        let git_repo = try!(self.git_repo());

        let oid = try!(git2::Oid::from_str(commit_id));
        
        let commit = try!(git_repo.find_commit(oid));

        Ok(commit)
    }

    pub fn treediff(&self, db: &db::Db, indexed_commit_id: &str, branch_commit_id: &str) -> RepoResult<()> {
        Ok(())
    }

    pub fn treewalks(&self, db: &db::Db) -> RepoResult<()> {
        for branch in self.branches.iter() {
            let branch_commit_id = try!(self.branch_commit_id(&branch.name));
            let branch_commit_id_str = format!("{}", branch_commit_id);
            
            // get commit id for last time we indexed the repo
            let repo_branch = try!(db.find_branch(&self.id, &branch.name)).unwrap();
            let indexed_commit_id = repo_branch.indexed_commit_id;
            
            if indexed_commit_id.is_some() {
                // tree-to-tree diff it and head, adding changed files to table:

                try!(self.treediff(db, indexed_commit_id.as_ref().unwrap(), &branch_commit_id_str));
            } else {
                // add all files to files table
                try!(self.treewalk(db, &repo_branch.name, &branch_commit_id_str));
            }
        }

        Ok(())
    }

    pub fn treewalk(&self, db: &db::Db, branch: &str, commit_id: &str) -> RepoResult<()> {
        //let git_repo = try!(self.git_repo());

        let commit = try!(self.get_commit(commit_id));

        let tree = try!(commit.tree());
        
        let iter = self.tree_iter(&tree);

        for repo_entry in iter {
            let entry = repo_entry.entry;
            
            match entry.kind() {
                Some(git2::ObjectType::Blob) => {
                    //let obj: git2::Object = entry.to_object(git_repo).unwrap();

                    //todo get contents of file from blob
                    //let blob: &git2::Blob = obj.as_blob().unwrap();

                    try!(db.upsert_file(&self.id, branch, &repo_entry.path, Some(commit_id)));
                },
                _ => {}
            }
        }

        Ok(())
    }

    pub fn tree_iter<'tree,'repo>(&'repo self, tree: &'tree git2::Tree<'tree>) -> RecursiveTreeIter<'repo> {
        let repo = self.git_repo().unwrap();
        
        let mut initial = vec![];
        
        for entry in tree.iter() {
            let repo_entry = RepoTreeEntry::from_ref(&entry, Path::new(""));

            if repo_entry.is_some() {
                initial.push(repo_entry.unwrap());
            }
        }
        
        RecursiveTreeIter {
            entries: initial,
            repo: repo,
        }
    }

    pub fn add_commit(&self, db: &db::Db, oid: &git2::Oid) -> RepoResult<()> {
        info!("adding commit {:?}", oid);

        let commit_id = format!("{}", oid);
        
        try!(db.create_commit_unless_exists(&commit_id, &self.id));
        
        Ok(())
    }

    pub fn head_commit_id(&self) -> RepoResult<String> {
        let git_repo = try!(self.git_repo());

        let head = try!(git_repo.head());

        let oid = try!(head.target().ok_or(RepoError::HeadRefHasNoDirectTarget));
        
        let commit = try!(git_repo.find_commit(oid));

        Ok(format!("{}", commit.id()))
    }

    pub fn branch_commit_id(&self, branch: &str) -> RepoResult<git2::Oid> {
        let git_repo = try!(self.git_repo());

        let branch_fullname = try!(self.find_branch(git_repo, branch));
        
        let id = try!(git_repo.refname_to_id(&branch_fullname));

        return Ok(id);
    }

    pub fn git_repo<'a>(&'a self) -> RepoResult<&'a git2::Repository> {
        match self.git_repo.as_ref() {
            Some(gr) => Ok(gr),
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




