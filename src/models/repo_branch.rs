use rusqlite::{SqliteConnection,SqliteResult,SqliteRow};
use schemamama_rusqlite::SqliteMigration;
use result::*;

#[derive(Debug,Clone)]
pub struct RepoBranch {
    pub repo_id: String,
    pub name: String,
    pub indexed_commit_id: Option<String>,
}

impl RepoBranch {
    pub fn new(repo_id: String, name: String, indexed_commit_id: Option<String>) -> RepoBranch {
        RepoBranch {
            repo_id: repo_id,
            name: name,
            indexed_commit_id: indexed_commit_id,
        }
    }
    
    pub fn new_from_sql_row(row0: &SqliteRow) -> RepoResult<RepoBranch> {
        Ok(RepoBranch {
            repo_id: row0.get(0),
            name: row0.get(1),
            indexed_commit_id: row0.get(2),
        })
    }
}

pub struct CreateBranchesTable;
migration!(CreateBranchesTable, 2, "create branches table");

impl SqliteMigration for CreateBranchesTable {
    fn up(&self, conn: &SqliteConnection) -> SqliteResult<()> {
        info!("creating branches table");
        
        const CREATE_BRANCHES: &'static str = "\
        CREATE TABLE branches ( \
        repo_id TEXT, \
        name TEXT, \
        indexed_commit_id TEXT \
        );";

        const CREATE_BRANCHES_NATURAL_KEY: &'static str = "\
        CREATE UNIQUE INDEX branches_repo_id_name_idx ON branches(repo_id,name)";

        Ok(())
            .and(conn.execute(CREATE_BRANCHES, &[]))
            .and(conn.execute(CREATE_BRANCHES_NATURAL_KEY, &[]))
            .map(|_| (()))
    }

    fn down(&self, conn: &SqliteConnection) -> SqliteResult<()> {
        conn.execute("DROP TABLE branches;", &[]).map(|_| ())
    }
}
