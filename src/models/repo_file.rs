use std::path::PathBuf;
use rusqlite::{SqliteConnection,SqliteResult,SqliteRow};
use schemamama_rusqlite::{SqliteMigration};
use result::*;
use models::types;

#[derive(Debug,Clone)]
pub struct RepoFile {
    pub repo_id: String,
    pub path: PathBuf,
    pub changed_commit_id: String,
    pub indexed_commit_id: Option<String>,
}

impl RepoFile {
    pub fn new(repo_id: String, path: PathBuf, changed_commit_id: String, indexed_commit_id: Option<String>) -> RepoFile {
        RepoFile {
            repo_id: repo_id,
            path: path,
            changed_commit_id: changed_commit_id,
            indexed_commit_id: indexed_commit_id,
        }
    }
    
    pub fn new_from_sql_row(row0: &SqliteRow) -> RepoResult<RepoFile> {
        Ok(RepoFile {
            repo_id: row0.get(0),
            path: types::path_buf_from_bytes_vec(row0.get(1)),
            changed_commit_id: row0.get(2),
            indexed_commit_id: row0.get(3),
        })
    }
}

pub struct CreateFilesTable;
migration!(CreateFilesTable, 4, "create files table");

impl SqliteMigration for CreateFilesTable {
    fn up(&self, conn: &SqliteConnection) -> SqliteResult<()> {
        const CREATE_FILES: &'static str = "\
        CREATE TABLE files ( \
        repo_id TEXT, \
        path TEXT, \
        changed_commit_id TEXT, \
        indexed_commit_id TEXT \
        );";

        const CREATE_FILES_NATURAL_KEY: &'static str = "\
        CREATE UNIQUE INDEX files_repo_id_path_idx ON files(repo_id,path)";

        Ok(())
            .and(conn.execute(CREATE_FILES, &[]))
            .and(conn.execute(CREATE_FILES_NATURAL_KEY, &[]))
            .map(|_| (()))
    }

    fn down(&self, conn: &SqliteConnection) -> SqliteResult<()> {
        conn.execute("DROP TABLE files;", &[]).map(|_| ())
    }
}
