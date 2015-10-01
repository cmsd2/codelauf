use repo::Repo;
use result::*;
use config::*;
use db::*;
use git2;
use chrono::*;
use rs_es;
use sha1::Sha1;
use std::fs::File;
use std::path::{Path,PathBuf};
use std::cell::RefCell;
use std::io::Read;
use encoding::{Encoding, DecoderTrap};
use encoding::all::UTF_8;

#[derive(Debug,Clone,RustcEncodable,RustcDecodable)]
pub struct CommitId {
    pub id: String
}

impl CommitId {
    pub fn new_for_git_commit(commit: &git2::Commit) -> CommitId {
        CommitId {
            id: format!("{}", commit.id())
        }
    }
}

#[derive(Debug,Clone,RustcEncodable,RustcDecodable)]
pub struct Signature {
    pub name: Option<String>,
    pub email: Option<String>,
}

impl Signature {
    pub fn new_for_git_signature(sig: &git2::Signature) -> Signature {
        Signature {
            name: sig.name().map(|s| s.to_owned()),
            email: sig.email().map(|s| s.to_owned()),
        }
    }
}

#[derive(Debug,Clone,RustcEncodable,RustcDecodable)]
pub struct Commit {
    pub parents: Vec<CommitId>,
    pub repo_id: String,
    pub author: Signature,
    pub committer: Signature,
    pub commit_date: String,
    pub message: Option<String>,
}

impl Commit {
    pub fn new_for_git_commit(repo_id: &str, commit: &git2::Commit) -> RepoResult<Commit> {
        let time = Index::datetime_convert_git_to_chrono(&commit.time());

        let mut parents = vec![];

        for parent in commit.parents() {
            parents.push(CommitId::new_for_git_commit(&parent));
        }
        
        Ok(Commit {
            parents: parents,
            repo_id: repo_id.to_owned(),
            author: Signature::new_for_git_signature(&commit.author()),
            committer: Signature::new_for_git_signature(&commit.committer()),
            commit_date: time.to_rfc3339(),
            message: commit.message().map(|s| s.to_owned()),
        })
    }
}

#[derive(Debug,Clone,RustcEncodable,RustcDecodable)]
pub struct IndexedFile {
    pub repo_id: String,
    pub path: PathBuf,
    pub text: Option<String>,
    pub keywords: Option<String>,
    pub changed_commit_id: Option<String>,
    pub changed_date: Option<String>,
}

impl IndexedFile {
    pub fn new(repo_id: String, path: PathBuf) -> IndexedFile {
        IndexedFile {
            repo_id: repo_id,
            path: path,
            text: None,
            keywords: None,
            changed_commit_id: None,
            changed_date: None,
        }
    }

    pub fn id(&self) -> String {
        let mut h = Sha1::new();
        h.update(self.repo_id.as_bytes());
        h.update(path_to_bytes(&self.path).unwrap());
        h.hexdigest()
    }
}

pub struct Index {
    pub es_client: RefCell<rs_es::Client>,
}

impl Index {
    pub fn new_for_config(config: &Config) -> RepoResult<Index> {
        let es_url_str: &str = try!(config.elasticsearch.as_ref().ok_or(RepoError::NoElasticSearch));

        let mut es_url_parts = es_url_str.split(":");
        
        let es_host = try!(es_url_parts.next().ok_or(RepoError::NoElasticSearch));
        let es_port = try!(es_url_parts.next().map(|s| s.parse::<u32>()).unwrap_or(Ok(9200)));

        info!("es host: {} port: {}", es_host, es_port);

        Ok(Index {
            es_client: RefCell::new(rs_es::Client::new(es_host, es_port)),
        })
    }

    pub fn index_tree(&self, db: &Db, repo: &Repo) -> RepoResult<()> {
        let files = try!(db.find_files_not_indexed(&repo.id));

        for file in files {
            match self.index_file(db, repo, &file.path, &file.changed_commit_id) {
                Err(err) => {
                    info!("error indexing file {:?}: {:?}", file.path, err);
                },
                _ => {}
            }
        }

        Ok(())
    }

    pub fn index_file(&self, db: &Db, repo: &Repo, path: &Path, commit_id: &str) -> RepoResult<()> {
        info!("indexing file {:?}", path);

        let mut f = try!(File::open(path));
        let mut s = String::new();
        try!(f.read_to_string(&mut s));
        //todo analyse file instead of sending verbatim

        let mut indexed_file = IndexedFile::new(repo.id.clone(), path.to_owned());
        indexed_file.text = Some(s);
        indexed_file.changed_commit_id = Some(commit_id.to_owned());
        let file_id = indexed_file.id();
        
        let mut es_client = self.es_client.borrow_mut();
        let mut op = es_client.index("codelauf", "file");
        
        try!(op
             .with_id(&file_id)
             .with_doc(&indexed_file)
             .send());

        try!(db.mark_file_as_indexed(&repo.id, path, commit_id));

        Ok(())
    }

    pub fn index_blob(&self, db: &Db, repo: &Repo, path: &Path, commit_id: &str, blob: &git2::Blob) -> RepoResult<()> {
        if blob.is_binary() {
            info!("not indexing binary file {:?}", path);
        } else {
            let blob_data = blob.content();

            let maybe_blob_str = UTF_8.decode(blob_data, DecoderTrap::Replace);

            if maybe_blob_str.is_err() {
                info!("error decoding blob data: {:?}", maybe_blob_str);
            } else {
                let blob_str = maybe_blob_str.unwrap();
                
                try!(self.index_blob_str(db, repo, path, commit_id, &blob_str));
            }
        }

        Ok(())
    }

    pub fn index_blob_str(&self, db: &Db, repo: &Repo, path: &Path, commit_id: &str, blob: &str) -> RepoResult<()> {
        let mut indexed_file = IndexedFile::new(repo.id.clone(), path.to_owned());
        indexed_file.text = Some(blob.to_owned());
        indexed_file.changed_commit_id = Some(commit_id.to_owned());
        let file_id = indexed_file.id();
        
        let mut es_client = self.es_client.borrow_mut();
        let mut op = es_client.index("codelauf", "file");
        
        try!(op
             .with_id(&file_id)
             .with_doc(&indexed_file)
             .send());

        try!(db.mark_file_as_indexed(&repo.id, path, commit_id));

        Ok(())
    }
    
    pub fn index_repo(&self, db: &Db, repo: &Repo) -> RepoResult<()> {
        try!(self.index_commits(db, repo));

        try!(self.index_branches(db, repo));

        Ok(())
    }

    pub fn index_branches(&self, db: &Db, repo: &Repo) -> RepoResult<()> {
        let git_repo = try!(repo.git_repo());
        
        for branch in repo.branches.iter() {
            let maybe_repo_branch = try!(db.find_branch(&repo.id, &branch.name));

            let repo_branch = try!(maybe_repo_branch.ok_or(RepoError::BranchNotFound));

            let old_tree = match repo_branch.indexed_commit_id {
                Some(commit) => {
                    let commit = try!(repo.get_commit(&commit));
                    let tree = try!(commit.tree());
                    Some(tree)
                },
                None => None
            };

            let branch_commit_id = try!(repo.branch_commit_id(&branch.name));
            let branch_commit_id_str = format!("{}", branch_commit_id);
            
            let new_tree_commit = try!(repo.get_commit(&branch_commit_id_str));
            let new_tree = try!(new_tree_commit.tree());

            let mut diff_opts = git2::DiffOptions::new();
            diff_opts.ignore_whitespace(true)
                .ignore_filemode(true)
                ;

            let diff = try!(git2::Diff::tree_to_tree(git_repo, old_tree.as_ref(), Some(&new_tree), Some(&mut diff_opts)));

            try!(self.index_diff(db, repo, &branch_commit_id_str, &diff));

            try!(db.mark_branch_as_indexed(&repo.id, &branch.name, &branch_commit_id_str));
        }

        Ok(())
    }

    pub fn index_diff(&self, db: &Db, repo: &Repo, commit_id: &str, diff: &git2::Diff) -> RepoResult<()> {
        let git_repo = try!(repo.git_repo());
        
        for delta in diff.deltas() {
            let old_file = delta.old_file();
            let new_file = delta.new_file();
            
            info!("delta: {:?} {:?} {:?} {:?} {:?}", delta.status(), old_file.id(), old_file.path(), new_file.id(), new_file.path());

            let path = new_file.path();
            
            if !new_file.id().is_zero() && path.is_some() {
                let blob = try!(git_repo.find_blob(new_file.id()));
                try!(self.index_blob(db, repo, path.unwrap(), commit_id, &blob));
            }
        }
        
        Ok(())
    }
    
    pub fn index_commits(&self, db: &Db, repo: &Repo) -> RepoResult<()> {
        let commits = try!(db.find_commits_not_indexed(&repo.id));

        for commit in commits {
            info!("indexing {:?}", commit);

            try!(self.index_commit(db, repo, &commit));
        }
        
        Ok(())
    }

    pub fn index_commit(&self, db: &Db, repo: &Repo, commit_id: &str) -> RepoResult<()> {
        let commit = try!(repo.get_commit(commit_id));

        let indexed_commit = try!(Commit::new_for_git_commit(&repo.id, &commit));

        info!("commit {:?}", indexed_commit);

        let mut es_client = self.es_client.borrow_mut();
        
        let mut op = es_client.index("codelauf", "commit");
        try!(op
             .with_id(commit_id)
             .with_doc(&indexed_commit)
             .send());

        try!(db.mark_commit_as_indexed(&repo.id, commit_id));

/*        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.ignore_whitespace(true)
            .ignore_filemode(true)
            ;
            

        for parent in commit.parents() {
            let commit_tree = try!(commit.tree());
            let parent_tree = try!(parent.tree());
            
            let diff = try!(git2::Diff::tree_to_tree(git_repo, Some(&parent_tree), Some(&commit_tree), Some(&mut diff_opts)));

            for delta in diff.deltas() {
                info!("delta: {:?} {:?} {:?}", delta.status(), delta.old_file().path(), delta.new_file().path());
            }
        }
        */
        Ok(())
    }

    pub fn datetime_convert_git_to_chrono(git_time: &git2::Time) -> DateTime<offset::fixed::FixedOffset> {
        let tz = offset::fixed::FixedOffset::east(git_time.offset_minutes() * 60);
        
        let time = tz.timestamp(git_time.seconds(), 0);

        time
    }
}
