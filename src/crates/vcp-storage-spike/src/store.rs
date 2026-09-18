// SPDX-License-Identifier: Apache-2.0
use crate::{digest, object_id, reject, Result, View, LIMIT};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
    Connection, Row, SqliteConnection,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub enum Backend {
    Sqlite(SqliteConnection),
    Files(File),
}
pub struct Store {
    backend: Backend,
    root: PathBuf,
    _lock: File,
    last: Option<View>,
    chain: String,
    failed: bool,
    commands: std::collections::BTreeSet<String>,
}
impl Store {
    pub async fn open(root: &Path, kind: &str) -> Result<Self> {
        if !matches!(kind, "sqlite" | "files") {
            return Err(reject("unknown backend"));
        }
        fs::create_dir_all(root)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("owner.lock"))?;
        lock.try_lock()?;
        // A root's backend cannot silently change on reopen.
        let marker = root.join("backend");
        if marker.exists() {
            if fs::read_to_string(&marker)? != kind {
                return Err(reject("backend identity mismatch"));
            }
        } else {
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker)?;
            f.write_all(kind.as_bytes())?;
            f.sync_all()?;
        }
        fs::create_dir_all(root.join("objects"))?;
        let mut views = Vec::new();
        let mut chain = "0".repeat(64);
        let backend = if kind == "sqlite" {
            let options = SqliteConnectOptions::new()
                .filename(root.join("state.sqlite"))
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .synchronous(SqliteSynchronous::Full);
            let mut db = SqliteConnection::connect_with(&options).await?;
            sqlx::query("CREATE TABLE IF NOT EXISTS commits (seq INTEGER PRIMARY KEY, command TEXT UNIQUE NOT NULL, payload BLOB NOT NULL, digest TEXT NOT NULL)").execute(&mut db).await?;
            for row in sqlx::query("SELECT seq, command, payload, digest FROM commits ORDER BY seq")
                .fetch_all(&mut db)
                .await?
            {
                let bytes: Vec<u8> = row.try_get("payload")?;
                let hash: String = row.try_get("digest")?;
                if bytes.len() > LIMIT || digest(&bytes) != hash {
                    return Err(reject("SQLite record checksum"));
                }
                let view: View = serde_json::from_slice(&bytes)?;
                if view.sequence != row.try_get::<i64, _>("seq")? as u64
                    || view.command != row.try_get::<String, _>("command")?
                {
                    return Err(reject("SQLite identity mismatch"));
                }
                views.push(view);
            }
            Backend::Sqlite(db)
        } else {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(root.join("state.frames"))?;
            loop {
                let start = file.stream_position()?;
                let remaining = file.metadata()?.len() - start;
                if remaining == 0 {
                    break;
                }
                if remaining < 8 {
                    file.set_len(start)?;
                    file.sync_all()?;
                    break;
                }
                let mut header = [0u8; 8];
                file.read_exact(&mut header)?;
                let size = u32::from_le_bytes(header[..4].try_into()?) as usize;
                if size == 0
                    || size > LIMIT
                    || (size as u32) != !u32::from_le_bytes(header[4..].try_into()?)
                {
                    return Err(reject("frame header corrupt"));
                }
                if remaining < (8 + size + 64) as u64 {
                    file.set_len(start)?;
                    file.sync_all()?;
                    break;
                }
                let mut bytes = vec![0; size];
                file.read_exact(&mut bytes)?;
                let mut hash = [0; 64];
                file.read_exact(&mut hash)?;
                let next = digest(
                    &[
                        b"vcp-p0-frames-v1\0".as_slice(),
                        chain.as_bytes(),
                        header.as_slice(),
                        bytes.as_slice(),
                    ]
                    .concat(),
                );
                if next.as_bytes() != hash {
                    return Err(reject("committed frame corrupt"));
                }
                chain = next;
                views.push(serde_json::from_slice::<View>(&bytes)?);
            }
            file.seek(SeekFrom::End(0))?;
            Backend::Files(file)
        };
        let mut previous = None;
        let mut commands = std::collections::BTreeSet::new();
        for view in &views {
            validate_next(previous, view)?;
            if !commands.insert(view.command.clone()) {
                return Err(reject("duplicate command"));
            }
            previous = Some(view);
        }
        let result = Self {
            backend,
            root: root.into(),
            _lock: lock,
            last: views.pop(),
            chain,
            failed: false,
            commands,
        };
        if let Some(view) = &result.last {
            result.validate_artifacts(view)?;
        }
        Ok(result)
    }
    pub fn view(&self) -> Option<View> {
        self.last.clone()
    }
    pub async fn configuration(&mut self) -> Result<serde_json::Value> {
        if let Backend::Sqlite(db) = &mut self.backend {
            let version: String = sqlx::query_scalar("SELECT sqlite_version()")
                .fetch_one(&mut *db)
                .await?;
            let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
                .fetch_one(&mut *db)
                .await?;
            let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
                .fetch_one(&mut *db)
                .await?;
            if journal != "wal" || synchronous != 2 {
                return Err(reject("SQLite durability configuration differs"));
            }
            Ok(
                serde_json::json!({"sqlite":version,"journal_mode":journal,"synchronous":synchronous}),
            )
        } else {
            Ok(serde_json::json!({"format":"vcp-p0-frames-v1","sync":"sync_all-before-ack"}))
        }
    }
    pub fn put_artifact(&self, bytes: &[u8]) -> Result<String> {
        if bytes.len() > LIMIT {
            return Err(reject("artifact too large"));
        }
        let id = digest(bytes);
        let dest = self.root.join("objects").join(&id);
        if dest.exists() {
            if fs::read(&dest)? != bytes {
                return Err(reject("artifact collision/corruption"));
            }
        } else {
            let mut f = OpenOptions::new().write(true).create_new(true).open(dest)?;
            f.write_all(bytes)?;
            f.sync_all()?;
        }
        Ok(id)
    }
    pub fn artifact(&self, id: &str) -> Result<Vec<u8>> {
        if !object_id(id) {
            return Err(reject("invalid artifact id"));
        }
        let bytes = fs::read(self.root.join("objects").join(id))?;
        if bytes.len() > LIMIT || digest(&bytes) != id {
            return Err(reject("artifact corrupt"));
        }
        Ok(bytes)
    }
    fn validate_artifacts(&self, view: &View) -> Result<()> {
        for (id, size) in &view.artifacts {
            if self.artifact(id)?.len() as u64 != *size {
                return Err(reject("artifact size"));
            }
        }
        Ok(())
    }
    pub async fn commit(&mut self, expected: u64, view: View) -> Result<()> {
        self.commit_observed(expected, view, |_| {}).await
    }
    /// Fault observation points are called synchronously by the qualification
    /// child process. The parent kills that process at the selected barrier.
    pub async fn commit_observed(
        &mut self,
        expected: u64,
        view: View,
        mut observe: impl FnMut(&str),
    ) -> Result<()> {
        if self.failed {
            return Err(reject("writer requires reopen"));
        }
        if self.last.as_ref() == Some(&view) {
            return Ok(());
        }
        if self.commands.contains(&view.command) {
            return Err(reject("command identity conflict"));
        }
        if self.last.as_ref().map_or(0, |v| v.sequence) != expected {
            return Err(reject("stale revision"));
        }
        validate_next(self.last.as_ref(), &view)?;
        self.validate_artifacts(&view)?;
        let bytes = serde_json::to_vec(&view)?;
        if bytes.len() > LIMIT {
            return Err(reject("view too large"));
        }
        // Once persistence begins, any failure requires recovery; never continue
        // appending after an ambiguous short write or database commit failure.
        self.failed = true;
        observe("before_write");
        match &mut self.backend {
            Backend::Sqlite(db) => {
                let mut tx = db.begin().await?;
                sqlx::query("INSERT INTO commits (seq,command,payload,digest) VALUES (?,?,?,?)")
                    .bind(view.sequence as i64)
                    .bind(&view.command)
                    .bind(&bytes)
                    .bind(digest(&bytes))
                    .execute(&mut *tx)
                    .await?;
                observe("before_commit");
                tx.commit().await?;
            }
            Backend::Files(file) => {
                let size = bytes.len() as u32;
                let header = [size.to_le_bytes(), (!size).to_le_bytes()].concat();
                let next = digest(
                    &[
                        b"vcp-p0-frames-v1\0".as_slice(),
                        self.chain.as_bytes(),
                        &header,
                        &bytes,
                    ]
                    .concat(),
                );
                file.write_all(&header)?;
                let split = bytes.len() / 2;
                file.write_all(&bytes[..split])?;
                observe("before_commit");
                file.write_all(&bytes[split..])?;
                file.write_all(next.as_bytes())?;
                file.sync_all()?;
                self.chain = next;
            }
        }
        observe("after_commit");
        self.commands.insert(view.command.clone());
        self.last = Some(view);
        self.failed = false;
        Ok(())
    }
    pub async fn close(mut self) -> Result<()> {
        if let Backend::Sqlite(db) = self.backend {
            db.close().await?;
        }
        self.last = None;
        Ok(())
    }
}
fn validate_next(old: Option<&View>, view: &View) -> Result<()> {
    view.validate()?;
    if view.sequence > i64::MAX as u64
        || old.is_some_and(|v| {
            view.sequence != v.sequence + 1 || view.deletion_epoch < v.deletion_epoch
        })
    {
        return Err(reject("nonsequential commit or deletion rollback"));
    }
    Ok(())
}
