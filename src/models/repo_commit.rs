use rusqlite::{SqliteConnection,SqliteResult,SqliteRow};
use schemamama_rusqlite::{SqliteMigration};
use std::str::FromStr;
use result::*;

#[derive(Debug,Copy,Clone)]
pub enum CommitState {
    Indexed,
    NotIndexed,
}

impl FromStr for CommitState {
    type Err = RepoError;
    fn from_str(s: &str) -> Result<CommitState, Self::Err> {
        match s {
            "Indexed" => Ok(CommitState::Indexed),
            "NotIndexed" => Ok(CommitState::NotIndexed),
            _ => Err(RepoError::EnumParseError(s.to_string()))
        }
    }
}

impl ToString for CommitState {
    fn to_string(&self) -> String {
        match *self {
            CommitState::Indexed => "Indexed".to_string(),
            CommitState::NotIndexed => "NotIndexed".to_string(),
        }
    }
}

#[derive(Debug,Clone)]
pub struct RepoCommit {
    pub id: String,
    pub repo_id: String,
    pub state: CommitState,
}

impl RepoCommit {
    pub fn new(id: String, repo_id: String, state: CommitState) -> RepoCommit {
        RepoCommit {
            id: id,
            repo_id: repo_id,
            state: state,
        }
    }
    
    pub fn new_from_sql_row(row0: &SqliteRow) -> RepoResult<RepoCommit> {
        let commit_state: String = row0.get(2);
        
        Ok(RepoCommit {
            id: row0.get(0),
            repo_id: row0.get(1),
            state: try!(CommitState::from_str(&commit_state)),
        })
    }
}

pub struct CreateCommitsTable;
migration!(CreateCommitsTable, 3, "create commits table");

impl SqliteMigration for CreateCommitsTable {
    fn up(&self, conn: &SqliteConnection) -> SqliteResult<()> {
        const CREATE_COMMITS: &'static str = "\
        CREATE TABLE commits ( \
        id TEXT, \
        repo_id TEXT, \
        state TEXT \
        );";

        const CREATE_COMMITS_PKEY: &'static str = "\
        CREATE UNIQUE INDEX commits_repo_id_id_idx ON commits(repo_id,id)";

        Ok(())
            .and(conn.execute(CREATE_COMMITS, &[]))
            .and(conn.execute(CREATE_COMMITS_PKEY, &[]))
            .map(|_| (()))
    }

    fn down(&self, conn: &SqliteConnection) -> SqliteResult<()> {
        conn.execute("DROP TABLE commits;", &[]).map(|_| ())
    }
}
