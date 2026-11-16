//! Read-only identity checks for opt-in historical capture.
use anyhow::{ensure, Result};
use std::path::{Path, PathBuf};

pub fn identity(connection: &rusqlite::Connection) -> Result<PathBuf> {
    let path = Path::new(
        connection
            .path()
            .ok_or_else(|| anyhow::anyhow!("non-file SQLite store"))?,
    );
    #[cfg(unix)]
    {
        let mut moved: std::os::raw::c_int = 0;
        // SAFETY: the shared connection is alive for this synchronous call;
        // GraphStore is !Sync and MetaStore holds its mutex. HAS_MOVED only
        // queries the SQLite VFS identity and writes the supplied integer.
        let result = unsafe {
            rusqlite::ffi::sqlite3_file_control(
                connection.handle(),
                c"main".as_ptr(),
                rusqlite::ffi::SQLITE_FCNTL_HAS_MOVED,
                (&mut moved as *mut std::os::raw::c_int).cast(),
            )
        };
        ensure!(
            result == rusqlite::ffi::SQLITE_OK,
            "SQLite VFS cannot establish store identity"
        );
        ensure!(moved == 0, "store file replaced since SQLite open");
        path.canonicalize().map_err(Into::into)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        anyhow::bail!("authoritative store identity unavailable on this platform")
    }
}
