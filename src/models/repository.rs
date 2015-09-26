use std::path::PathBuf;
use rusqlite::{SqliteConnection,SqliteResult,SqliteRow};
use schemamama_rusqlite::SqliteMigration;
use std::str::FromStr;
use time;
use time::Timespec;
use result::*;
use repo::SyncState;
use models::types;

#[derive(Debug,Clone)]
pub struct Repository {
    pub id: String,
    pub uri: String,
    pub path: PathBuf,
    pub sync_state: SyncState,
    pub added_datetime: Option<Timespec>,
    pub fetched_datetime: Option<Timespec>,
    pub indexed_datetime: Option<Timespec>,
}

impl Repository {
    pub fn new_from_remote(id: String, uri: String, path: PathBuf) -> Repository {
        Repository {
            id: id,
            uri: uri,
            path: path,
            sync_state: SyncState::NotCloned,
            added_datetime: Some(time::get_time()),
            fetched_datetime: None,
            indexed_datetime: None,
        }
    }
    
    pub fn new_from_sql_row(row0: &SqliteRow) -> RepoResult<Repository> {
        let sync_state: String = row0.get(3);

        Ok(Repository {
            id: row0.get(0),
            uri: row0.get(1),
            path: types::path_buf_from_bytes_vec(row0.get(2)),
            sync_state: try!(SyncState::from_str(&sync_state)),
            added_datetime: row0.get(4),
            fetched_datetime: row0.get(5),
            indexed_datetime: row0.get(6),
        })
    }
}

pub struct CreateRepositoriesTable;
migration!(CreateRepositoriesTable, 1, "create repositories table");

impl SqliteMigration for CreateRepositoriesTable {
    fn up(&self, conn: &SqliteConnection) -> SqliteResult<()> {
        const CREATE_REPOS: &'static str = "\
        CREATE TABLE repositories ( \
        id TEXT, \
        uri TEXT, \
        path TEXT,
        sync_state TEXT, \
        added_datetime DATETIME,
        fetched_datetime DATETIME, \
        indexed_datetime DATETIME \
        );";

        const CREATE_REPOS_PKEY: &'static str = "\
        CREATE UNIQUE INDEX repositories_id_idx ON repositories(id)";

        const CREATE_REPOS_NATURAL_KEY: &'static str = "\
        CREATE UNIQUE INDEX repositories_uri_idx ON repositories(uri)";

        Ok(())
            .and(conn.execute(CREATE_REPOS, &[]))
            .and(conn.execute(CREATE_REPOS_PKEY, &[]))
            .and(conn.execute(CREATE_REPOS_NATURAL_KEY, &[]))
            .map(|_| (()))
    }

    fn down(&self, conn: &SqliteConnection) -> SqliteResult<()> {
        conn.execute("DROP TABLE repositories;", &[]).map(|_| ())
    }
}
