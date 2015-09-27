
use repo::Repo;
use result::*;
use config::*;
use db::*;
use git2;
use chrono::*;

pub struct Index;

impl Index {
    pub fn new_for_config(_config: &Config) -> Index {
        Index
    }
    
    pub fn index_repo(&self, db: &Db, repo: &Repo) -> RepoResult<()> {

        let commits = try!(db.find_commits_not_indexed(&repo.id));

        for commit in commits {
            info!("indexing {:?}", commit);

            try!(self.index_commit(db, repo, &commit));
        }
        
        Ok(())
    }

    pub fn index_commit(&self, db: &Db, repo: &Repo, commit_id: &str) -> RepoResult<()> {
        let git_repo = try!(repo.git_repo());
        
        let commit = try!(repo.get_commit(commit_id));

        let commit_time = commit.time();
        let author = commit.author();
        let committer = commit.committer();
        
        let time = Self::datetime_convert_git_to_chrono(&commit_time);

        info!("commit {:?} {:?} {:?} {:?} {} {:?}", author.name(), author.email(), committer.name(), committer.email(), time, commit.message());

        let mut diff_opts = git2::DiffOptions::new();
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
        
        Ok(())
    }

    pub fn datetime_convert_git_to_chrono(git_time: &git2::Time) -> DateTime<offset::fixed::FixedOffset> {
        let tz = offset::fixed::FixedOffset::east(git_time.offset_minutes() * 60);
        
        let time = tz.timestamp(git_time.seconds(), 0);

        time
    }
}
