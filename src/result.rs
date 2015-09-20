use std::result::Result;
use rusqlite::SqliteError;
use std::convert::From;
use db;

pub type RepoResult<T> = Result<T, RepoError>;

#[derive(Debug)]
pub enum RepoError {
    DbError(db::DbError),
    SqlError(SqliteError),
    NoRemote,
    NotCloned,
}

impl From<SqliteError> for RepoError {
    fn from(err: SqliteError) -> RepoError {
        RepoError::SqlError(err)
    }
}

impl From<db::DbError> for RepoError {
    fn from(err: db::DbError) -> RepoError {
        RepoError::DbError(err)
    }
}

    
