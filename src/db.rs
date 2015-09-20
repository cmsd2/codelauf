use std::path::Path;
use rusqlite::{SqliteConnection,SqliteResult};
use schemamama::{Migrator};
use schemamama_rusqlite::{SqliteAdapter,SqliteMigration};

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
