// SPDX-License-Identifier: Apache-2.0
//! Physical page I/O for the existing backends. These adapters neither select
//! an authoritative root nor enable layout 3; the owner publishes that root.
use super::{Pages, MAX_PAGE_BYTES};
use crate::{
    artifact::{immutable_file, read_bounded},
    private_paths::Directory,
    Error, Result,
};
use sqlx::SqliteConnection;
use vcp_protocol::digest_bytes;

pub(crate) struct Files<'a> {
    directory: &'a Directory,
}
impl<'a> Files<'a> {
    /// A pre-admitted directory capability keeps native ancestors pinned.
    pub(crate) fn new(directory: &'a Directory) -> Self {
        Self { directory }
    }
}
impl Pages for Files<'_> {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        check_key(digest)?;
        check_limit(limit)?;
        read_bounded(&self.directory.path.join(format!("{digest}.json")), limit)
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        check_write(digest, bytes)?;
        let path = self.directory.path.join(format!("{digest}.json"));
        if path.exists() {
            return same(&read_bounded(&path, MAX_PAGE_BYTES)?, bytes);
        }
        match immutable_file(&path, bytes) {
            Ok(()) => Ok(()),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                same(&read_bounded(&path, MAX_PAGE_BYTES)?, bytes)
            }
            Err(error) => Err(error),
        }
    }
}

pub(crate) struct Sqlite<'a> {
    connection: &'a mut SqliteConnection,
}
impl<'a> Sqlite<'a> {
    /// Borrow the existing owner's connection/transaction; never open another
    /// database or create one as a side effect of a historical read.
    pub(crate) fn new(connection: &'a mut SqliteConnection) -> Self {
        Self { connection }
    }
    /// Called only while creating/migrating layout 3, before root publication.
    pub(crate) async fn initialize(connection: &mut SqliteConnection) -> Result<()> {
        sqlx::query("CREATE TABLE IF NOT EXISTS history_index_pages(digest TEXT PRIMARY KEY NOT NULL, payload BLOB NOT NULL) WITHOUT ROWID")
            .execute(connection).await?;
        Ok(())
    }
}
impl Pages for Sqlite<'_> {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        check_key(digest)?;
        check_limit(limit)?;
        // Refuse oversized/incorrect storage types before SQLx allocates a blob.
        let row: Option<Option<Vec<u8>>> = sqlx::query_scalar("SELECT CASE WHEN typeof(payload)='blob' AND length(payload) BETWEEN 1 AND ? THEN payload ELSE NULL END FROM history_index_pages WHERE digest=?")
            .bind(limit as i64).bind(digest).fetch_optional(&mut *self.connection).await?;
        match row {
            Some(Some(bytes)) => Ok(bytes),
            Some(None) => Err(Error::Corruption("history index page storage bound")),
            None => Err(Error::Corruption("history index page missing")),
        }
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        check_write(digest, bytes)?;
        sqlx::query("INSERT INTO history_index_pages(digest,payload) VALUES(?,?) ON CONFLICT(digest) DO NOTHING")
            .bind(digest).bind(bytes).execute(&mut *self.connection).await?;
        same(&self.read(digest, MAX_PAGE_BYTES).await?, bytes)
    }
}
fn same(prior: &[u8], next: &[u8]) -> Result<()> {
    if prior != next {
        return Err(Error::Corruption("immutable history index page differs"));
    }
    Ok(())
}
fn check_key(digest: &str) -> Result<()> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Corruption("history index page key"));
    }
    Ok(())
}
fn check_limit(limit: usize) -> Result<()> {
    if limit == 0 || limit > MAX_PAGE_BYTES {
        return Err(Error::Limit("history index page read"));
    }
    Ok(())
}
fn check_write(digest: &str, bytes: &[u8]) -> Result<()> {
    check_key(digest)?;
    if bytes.is_empty() || bytes.len() > MAX_PAGE_BYTES {
        return Err(Error::Limit("history index page write"));
    }
    if digest_bytes(bytes) != digest {
        return Err(Error::Corruption("history index page write digest"));
    }
    Ok(())
}
