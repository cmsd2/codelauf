use std::result::Result;
use rusqlite::SqliteError;
use std::convert::From;
use db;
use git2;

pub type RepoResult<T> = Result<T, RepoError>;

#[derive(Debug)]
pub enum RepoError {
    DbError(db::DbError),
    SqlError(SqliteError),
    NoRemote,
    NotCloned,
    PathUnicodeError,
    GitError(git2::Error),
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

impl From<git2::Error> for RepoError {
    fn from(err: git2::Error) -> RepoError {
        RepoError::GitError(err)
    }
}

    
