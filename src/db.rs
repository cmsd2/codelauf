use std::path::Path;
use rusqlite::{SqliteConnection,SqliteResult,SqliteRow};
use schemamama::{Migrator};
use schemamama_rusqlite::{SqliteAdapter,SqliteMigration};
use std::str::FromStr;
use time;
use time::Timespec;
use super::result::*;
use super::repo::SyncState;

#[derive(Debug,Clone)]
pub enum DbError {
    EnumParseError(String)
}

#[derive(Debug,Clone)]
pub struct Repository {
    pub id: String,
    pub uri: String,
    pub branch: String,
    pub path: String,
    pub sync_state: SyncState,
    pub added_datetime: Option<Timespec>,
    pub fetched_datetime: Option<Timespec>,
    pub indexed_commit: Option<String>,
    pub indexed_datetime: Option<Timespec>,
}

impl Repository {
    pub fn new_from_remote(id: String, uri: String, branch: String, path: String) -> Repository {
        Repository {
            id: id,
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

    pub fn find_repo_by_remote(&self, remote: &String, branch: &String) -> RepoResult<Option<Repository>> {
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

    pub fn update_repo(&self, repo: &Repository) -> RepoResult<()> {
        let mut stmt = try!(self.conn.prepare("UPDATE repositories SET \
                                               path=?, sync_state=?, \
                                               fetched_datetime=?, \
                                               indexed_commit=?, \
                                               indexed_datetime=? \
                                               WHERE id=?").map_err(|e| RepoError::SqlError(e)));
        try!(stmt.execute(&[
            &repo.path,
            &repo.sync_state.to_string(),
            &repo.fetched_datetime,
            &repo.indexed_commit,
            &repo.indexed_datetime,
            &repo.id]));
        Ok(())
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
