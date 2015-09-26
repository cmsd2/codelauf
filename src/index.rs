
use repo::Repo;
use result::*;
use config::*;

pub struct Index;

impl Index {
    pub fn new_for_config(_config: &Config) -> Index {
        Index
    }
    
    pub fn index_repo(&self, _repo: &Repo) -> RepoResult<()> {
        Ok(())
    }
}
