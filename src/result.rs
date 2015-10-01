use std::num;
use std::io;
use std::result::Result;
use rusqlite::SqliteError;
use std::convert::From;
use db;
use git2;
use url;
use rs_es;

pub type RepoResult<T> = Result<T, RepoError>;

#[derive(Debug)]
pub enum RepoError {
    InvalidArgs(String),
    EnumParseError(String),
    DbError(db::DbError),
    SqlError(SqliteError),
    NoRemote,
    NoElasticSearch,
    NotCloned,
    PathUnicodeError,
    StringUnicodeError,
    GitError(git2::Error),
    InvalidState(String),
    FromUtf8Error,
    UrlParseError(url::ParseError),
    ElasticSearchError(rs_es::error::EsError),
    ParseIntError(num::ParseIntError),
    NoTreeEntryName,
    HeadRefHasNoDirectTarget,
    IoError(io::Error),
    BranchNotFound,
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

impl From<url::ParseError> for RepoError {
    fn from(err: url::ParseError) -> RepoError {
        RepoError::UrlParseError(err)
    }
}

impl From<rs_es::error::EsError> for RepoError {
    fn from(err: rs_es::error::EsError) -> RepoError {
        RepoError::ElasticSearchError(err)
    }
}

impl From<num::ParseIntError> for RepoError {
    fn from(err: num::ParseIntError) -> RepoError {
        RepoError::ParseIntError(err)
    }
}

impl From<io::Error> for RepoError {
    fn from(err: io::Error) -> RepoError {
        RepoError::IoError(err)
    }
}

    
