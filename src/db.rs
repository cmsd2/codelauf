use std::path::Path;
use rusqlite::{SqliteConnection,SqliteResult};
use schemamama::Migrator;
use schemamama_rusqlite::{SqliteAdapter,SqliteMigration};
use result::*;

pub use models::types::*;
pub use models::repository::*;
pub use models::repo_branch::*;
pub use models::repo_file::*;
pub use models::repo_commit::*;

#[derive(Debug,Clone)]
pub enum DbError {
    EnumParseError(String)
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
        migrator.register(Box::new(CreateBranchesTable));
        migrator.register(Box::new(CreateCommitsTable));
        migrator.register(Box::new(CreateFilesTable));

        migrator.up(4);
        assert_eq!(migrator.current_version(), Some(4));
    }

    pub fn find_repo_by_remote(&self, remote: &String) -> RepoResult<Option<Repository>> {
        let mut stmt = try!(self.conn.prepare("SELECT * FROM repositories WHERE uri = ?").map_err(|e| RepoError::SqlError(e)));
        let mut rows = try!(stmt.query(&[remote]));

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
        let path = try!(path_to_bytes_vec(&repo.path));
        
        let mut stmt = try!(self.conn.prepare("UPDATE repositories SET \
                                               path=?, sync_state=?, \
                                               fetched_datetime=?, \
                                               indexed_datetime=? \
                                               WHERE id=?").map_err(|e| RepoError::SqlError(e)));
        try!(stmt.execute(&[
            &path,
            &repo.sync_state.to_string(),
            &repo.fetched_datetime,
            &repo.indexed_datetime,
            &repo.id]));
        Ok(())
    }

    pub fn insert_repo(&self, repo: &Repository) -> RepoResult<()> {
        let path = try!(path_to_bytes_vec(&repo.path));
        
        let mut stmt = try!(self.conn.prepare("INSERT INTO repositories VALUES (?,?,?,?,?,?,?)").map_err(|e| RepoError::SqlError(e)));
        try!(stmt.execute(&[
            &repo.id,
            &repo.uri,
            &path,
            &repo.sync_state.to_string(),
            &repo.added_datetime,
            &repo.fetched_datetime,
            &repo.indexed_datetime]));
        Ok(())
    }

    pub fn find_branch(&self, repo_id: &str, name: &str) -> RepoResult<Option<RepoBranch>> {
        let mut stmt = try!(self.conn.prepare("SELECT * FROM branches WHERE repo_id = ? AND name = ?").map_err(|e| RepoError::SqlError(e)));
        let mut rows = try!(stmt.query(&[&repo_id, &name]));

        let row0 = try!(rows.next().unwrap());

        RepoBranch::new_from_sql_row(&row0).map(|r| Some(r))
    }

    pub fn mark_branch_as_indexed(&self, repo_id: &str, branch: &str, commit_id: &str) -> RepoResult<()> {
        let mut stmt = try!(self.conn.prepare("UPDATE branches SET \
                                               indexed_commit_id=? \
                                               WHERE repo_id=? AND name=?").map_err(|e| RepoError::SqlError(e)));
        try!(stmt.execute(&[&repo_id, &branch, &commit_id]));
        
        Ok(())
    }

    pub fn insert_branch(&self, branch: &RepoBranch) -> RepoResult<()> {
        let mut stmt = try!(self.conn.prepare("INSERT INTO branches VALUES (?,?,?)").map_err(|e| RepoError::SqlError(e)));
        try!(stmt.execute(&[
            &branch.repo_id,
            &branch.name,
            &branch.indexed_commit_id]));
        Ok(())
    }

    pub fn create_commit_unless_exists(&self, id: &str, repo_id: &str) -> RepoResult<()> {
        let mut stmt = try!(self.conn.prepare("INSERT OR IGNORE INTO commits VALUES (?,?,?)").map_err(|e| RepoError::SqlError(e)));

        try!(stmt.execute(&[
            &id,
            &repo_id,
            &CommitState::NotIndexed.to_string()]));
        
        Ok(())
    }

    pub fn find_commits_not_indexed(&self, repo_id: &str) -> RepoResult<Vec<String>> {
        let mut stmt = try!(self.conn.prepare("SELECT * FROM commits WHERE state = 'NotIndexed' AND repo_id = ?").map_err(|e| RepoError::SqlError(e)));
        
        let rows = try!(stmt.query(&[&repo_id]));

        let mut result = vec![];

        for row_result in rows {
            let row = try!(row_result);
            
            let commit_id: String = row.get(0);

            result.push(commit_id);
        }
        
        Ok(result)
    }

    pub fn mark_commit_as_indexed(&self, repo_id: &str, commit_id: &str) -> RepoResult<()> {
        let mut stmt = try!(self.conn.prepare("UPDATE commits SET \
                                               state = 'Indexed' \
                                               WHERE id=? AND repo_id=?").map_err(|e| RepoError::SqlError(e)));
        
        try!(stmt.execute(&[&commit_id, &repo_id]));
        
        Ok(())
    }

    /// create row in files table, or update changed_commit_id if it exists.
    /// indexed_commit_id will be set to null.
    pub fn upsert_file(&self, repo_id: &str, branch: &str, path: &Path, changed_commit_id: Option<&str>) -> RepoResult<()> {
        let mut stmt = try!(self.conn.prepare("INSERT OR REPLACE INTO files \
                                               (repo_id, branch, path, changed_commit_id) \
                                               VALUES (?,?,?,?)"));

        let path_bytes = try!(path_to_bytes(path));
        
        try!(stmt.execute(&[&repo_id, &branch, &path_bytes, &changed_commit_id]));

        Ok(())
    }

    pub fn find_files_not_indexed(&self, repo_id: &str) -> RepoResult<Vec<RepoFile>> {
        let mut stmt = try!(self.conn.prepare("SELECT * FROM files WHERE ((indexed_commit_id is null) or (indexed_commit_id != changed_commit_id)) AND repo_id = ?").map_err(|e| RepoError::SqlError(e)));
        
        let rows = try!(stmt.query(&[&repo_id]));

        let mut result = vec![];

        for row_result in rows {
            let row = try!(row_result);

            let file = try!(RepoFile::new_from_sql_row(&row));
            
            result.push(file);
        }
        
        Ok(result)
    }

    /// find file by repo_id and path, and set the indexed_commit_id column
    pub fn mark_file_as_indexed(&self, repo_id: &str, path: &Path, indexed_commit_id: &str) -> RepoResult<()> {
        let mut stmt = try!(self.conn.prepare("UPDATE files SET \
                                               indexed_commit_id = ? \
                                               WHERE path=? AND repo_id=?").map_err(|e| RepoError::SqlError(e)));

        let path_bytes = try!(path_to_bytes(path));
        
        try!(stmt.execute(&[&indexed_commit_id, &path_bytes, &repo_id]));
        
        Ok(())        
    }

    /// in a single transaction, delete all rows in the commits and files work tables, for the given repo
    pub fn clear_commits(&self, repo_id: &str) -> RepoResult<()> {
        let mut del_commits_stmt = try!(self.conn.prepare("DELETE FROM commits WHERE repo_id = ?"));
        try!(del_commits_stmt.execute(&[&repo_id]));

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
