
use std::path::{Path,PathBuf};
use std::ffi::OsStr;
use result::{RepoResult,RepoError};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
    

pub fn path_buf_from_bytes(bytes: &[u8]) -> PathBuf {
    let os_str: &OsStr = OsStr::from_bytes(bytes);

    PathBuf::from(os_str)
}

pub fn path_buf_from_bytes_vec(bytes: Vec<u8>) -> PathBuf {
    path_buf_from_bytes(&bytes[..])
}


#[cfg(unstable)]
pub fn path_to_bytes<'a>(path: &'a Path) -> RepoResult<&'a [u8]> {
    path.as_os_str().to_bytes().ok_or(RepoError::PathUnicodeError)
}

#[cfg(not(unstable))]
pub fn path_to_bytes<'a>(path: &'a Path) -> RepoResult<&'a [u8]> {
    path.as_os_str().to_str().map(|s| s.as_bytes()).ok_or(RepoError::PathUnicodeError)
}

pub fn path_to_bytes_vec(path: &Path) -> RepoResult<Vec<u8>> {
    let mut result: Vec<u8> = vec![];
    
    path_to_bytes(path).map(|bytes| {
        for b in bytes {
            result.push(*b);
        }
        
        result
    })
}
