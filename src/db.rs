use std::path::Path;
use rusqlite::{SqliteConnection,SqliteResult,SqliteError,SqliteRow};
use schemamama::{Migrator};
use schemamama_rusqlite::{SqliteAdapter,SqliteMigration};
use std::str::FromStr;
use time;
use time::Timespec;
use super::result::*;
use super::config::RepoLocation;
use uuid::Uuid;

#[derive(Debug,Clone)]
pub enum DbError {
    EnumParseError(String)
}

#[derive(Debug,Copy,Clone)]
pub enum SyncState {
    NotCloned,
}

impl FromStr for SyncState {
    type Err = DbError;
    fn from_str(s: &str) -> Result<SyncState, Self::Err> {
        match s {
            "NotCloned" => Ok(SyncState::NotCloned),
            _ => Err(DbError::EnumParseError(s.to_string()))
        }
    }
}

impl ToString for SyncState {
    fn to_string(&self) -> String {
        match self {
            NotCloned => "NotCloned".to_string(),
        }
    }
}

#[derive(Debug,Clone)]
pub struct Repository {
    id: String,
    uri: String,
    branch: String,
    path: String,
    sync_state: SyncState,
    added_datetime: Option<Timespec>,
    fetched_datetime: Option<Timespec>,
    indexed_commit: Option<String>,
    indexed_datetime: Option<Timespec>,
}

impl Repository {
    pub fn new_from_remote(uri: String, branch: String, path: String) -> Repository {
        Repository {
            id: Uuid::new_v4().to_string(),
            uri: uri,
            branch: branch,
            path: path,
            sync_state: SyncState::NotCloned,
            added_datetime: Some(time::get_time()),
            fetched_datetime: None,
            indexed_commit: None,
            indexed_datetime: None,
        }
    }
    
    pub fn new_from_sql_row(row0: &SqliteRow) -> RepoResult<Repository> {
        let sync_state: String = row0.get(4);

        Ok(Repository {
            id: row0.get(0),
            uri: row0.get(1),
            branch: row0.get(2),
            path: row0.get(3),
            sync_state: try!(SyncState::from_str(&sync_state)),
            added_datetime: row0.get(5),
            fetched_datetime: row0.get(6),
            indexed_commit: row0.get(7),
            indexed_datetime: row0.get(8),
        })
    }
}

struct CreateRepositoriesTable;
migration!(CreateRepositoriesTable, 1, "create repositories table");

impl SqliteMigration for CreateRepositoriesTable {
    fn up(&self, conn: &SqliteConnection) -> SqliteResult<()> {
        const CREATE_REPOS: &'static str = "\
        CREATE TABLE repositories ( \
        id TEXT, \
        uri TEXT, \
        branch TEXT, \
        path TEXT,
        sync_state TEXT, \
        added_datetime DATETIME,
        fetched_datetime DATETIME, \
        indexed_commit TEXT, \
        indexed_datetime DATETIME \
        );";

        const CREATE_REPOS_PKEY: &'static str = "\
        CREATE UNIQUE INDEX repositories_id_idx ON repositories(id)";

        const CREATE_REPOS_NATURAL_KEY: &'static str = "\
        CREATE UNIQUE INDEX repositories_uri_branch_idx ON repositories(uri, branch)";

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

pub struct Db {
    conn: SqliteConnection
}

impl Db {
    pub fn open(path: &Path) -> SqliteResult<Db> {
        Ok(Db {
            conn: try!(SqliteConnection::open(&path))
        })
    }

    pub fn open_in_memory() -> SqliteResult<Db> {
        Ok(Db {
            conn: try!(SqliteConnection::open_in_memory())
        })
    }

    pub fn migrate(&self) {
        let adapter = SqliteAdapter::new(&self.conn);
        adapter.setup_schema();

        let mut migrator = Migrator::new(adapter);
        migrator.register(Box::new(CreateRepositoriesTable));

        migrator.up(1);
        assert_eq!(migrator.current_version(), Some(1));
    }

    pub fn find_repo_by_remote(&self, repo_loc: &RepoLocation) -> RepoResult<Option<Repository>> {
        let default_branch = "master".to_string();
        let remote = try!(repo_loc.remote.as_ref().ok_or(RepoError::NoRemote));
        let branch = repo_loc.branch.as_ref().unwrap_or(&default_branch);
        
        let mut stmt = try!(self.conn.prepare("SELECT * FROM repositories WHERE uri = ? AND branch = ?").map_err(|e| RepoError::SqlError(e)));
        let mut rows = try!(stmt.query(&[remote, branch]));

        match rows.next() {
            None => Ok(None),
            Some(row_result) => {
                let row = try!(row_result);
                Repository::new_from_sql_row(&row).map(|r| Some(r))
            }
        }
    }

    pub fn find_repo(&self, id: &str) -> RepoResult<Option<Repository>> {
        let mut stmt = try!(self.conn.prepare("SELECT * FROM repositories WHERE id = ?").map_err(|e| RepoError::SqlError(e)));
        let mut rows = try!(stmt.query(&[&id]));

        let row0 = try!(rows.next().unwrap());

        Repository::new_from_sql_row(&row0).map(|r| Some(r))
    }

    pub fn insert_repo(&self, repo: &Repository) -> RepoResult<()> {
        let mut stmt = try!(self.conn.prepare("INSERT INTO repositories VALUES (?,?,?,?,?,?,?,?,?)").map_err(|e| RepoError::SqlError(e)));
        try!(stmt.execute(&[
            &repo.id,
            &repo.uri,
            &repo.branch,
            &repo.path,
            &repo.sync_state.to_string(),
            &repo.added_datetime,
            &repo.fetched_datetime,
            &repo.indexed_commit,
            &repo.indexed_datetime]));
        Ok(())
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        info!("closing db");
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    pub fn test_open_in_memory() {
        let db = Db::open_in_memory().unwrap();
        db.migrate();
    }
}
