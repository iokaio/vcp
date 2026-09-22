// SPDX-License-Identifier: Apache-2.0
use crate::{
    artifact::{immutable_file, read_bounded},
    contract::*,
    Error, Result,
};
use serde::{Deserialize, Serialize};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
    Connection, Row, SqliteConnection,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use vcp_protocol::{canonical_bytes, digest_bytes};

const MAGIC: &[u8; 8] = b"VCPJ0001";
const COMMITTED: &[u8; 8] = b"VCPCMIT1";
const HEADER: usize = 8 + 4 + 4 + 64;
const TRAILER: usize = 64 + 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    Sqlite,
    Files,
}
impl std::str::FromStr for BackendKind {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "sqlite" => Ok(Self::Sqlite),
            "files" => Ok(Self::Files),
            _ => Err(Error::Incompatible),
        }
    }
}
pub(crate) enum Backend {
    Sqlite(SqliteConnection),
    Files(Journal),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Barrier {
    Prepared,
    BeforeCommit,
    AfterCommit,
    BeforeReply,
    BeforeValidation,
    BeforeActivation,
    AfterActivation,
    BeforeCleanupFile,
    AfterCleanupFile,
}

#[cfg(feature = "qualification")]
pub type Observer = std::sync::Arc<dyn Fn(Barrier) + Send + Sync>;

pub(crate) struct Journal {
    file: File,
    root: PathBuf,
    chain: String,
    base: State,
    initial_chain: String,
    // Unit-test fault injection only; never a runtime capacity or policy setting.
    #[cfg(test)]
    pub(crate) write_budget: Option<usize>,
}
fn sql_error(error: sqlx::Error) -> Error {
    if let sqlx::Error::Database(database) = &error {
        if matches!(database.code().as_deref(), Some("5" | "6" | "261" | "262")) {
            return Error::Unavailable(
                "SQLite busy/locked; retry after a fresh snapshot and deadline check",
            );
        }
        if matches!(database.code().as_deref(), Some("11" | "26")) {
            return Error::Corruption("SQLite integrity");
        }
    }
    Error::Database(error)
}
impl Backend {
    pub(crate) async fn close(self) -> Result<()> {
        match self {
            Self::Sqlite(connection) => connection.close().await.map_err(sql_error),
            Self::Files(journal) => journal.file.sync_all().map_err(Error::from),
        }
    }

    pub(crate) async fn open(root: &Path, kind: BackendKind) -> Result<(Self, State, Vec<Commit>)> {
        let base = crate::replay_base::ReplayBase::load(root)?;
        let seed = base.as_ref().map(|b| b.state.clone()).unwrap_or_default();
        let initial_chain = base
            .as_ref()
            .map(|b| b.chain())
            .transpose()?
            .unwrap_or_else(|| "0".repeat(64));
        match kind {
            BackendKind::Sqlite => {
                let options = SqliteConnectOptions::new()
                    .filename(root.join("canonical.sqlite"))
                    .create_if_missing(true)
                    .journal_mode(SqliteJournalMode::Wal)
                    .synchronous(SqliteSynchronous::Full)
                    .foreign_keys(true)
                    .busy_timeout(Duration::from_millis(100));
                let mut db = SqliteConnection::connect_with(&options)
                    .await
                    .map_err(sql_error)?;
                let version: i64 = sqlx::query_scalar("PRAGMA user_version")
                    .fetch_one(&mut db)
                    .await?;
                let schema_version = if base.is_some() {
                    2
                } else {
                    FORMAT_VERSION as i64
                };
                if version != 0 && version != schema_version {
                    return Err(Error::Incompatible);
                }
                if version == 0 {
                    let count: i64 =
                        sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE type='table'")
                            .fetch_one(&mut db)
                            .await?;
                    if count != 0 {
                        return Err(Error::Incompatible);
                    }
                    let mut tx = db.begin_with("BEGIN IMMEDIATE").await.map_err(sql_error)?;
                    for statement in [
                        "CREATE TABLE commits (watermark INTEGER PRIMARY KEY CHECK(watermark>0), id TEXT NOT NULL UNIQUE, payload BLOB NOT NULL, digest TEXT NOT NULL)",
                        "CREATE TABLE records (key TEXT PRIMARY KEY, workspace TEXT NOT NULL, revision TEXT NOT NULL, collection TEXT NOT NULL, payload BLOB NOT NULL, digest TEXT NOT NULL)",
                        "CREATE TABLE edges (source TEXT NOT NULL REFERENCES records(key) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED, target TEXT NOT NULL REFERENCES records(key) DEFERRABLE INITIALLY DEFERRED, PRIMARY KEY(source,target))",
                        "CREATE TABLE events (id TEXT PRIMARY KEY, session TEXT NOT NULL, seq TEXT NOT NULL, workspace TEXT NOT NULL, watermark INTEGER NOT NULL REFERENCES commits(watermark), payload BLOB NOT NULL, UNIQUE(session,seq))",
                        "CREATE TABLE commands (workspace TEXT NOT NULL, id TEXT NOT NULL, digest TEXT NOT NULL, watermark INTEGER NOT NULL REFERENCES commits(watermark), payload BLOB NOT NULL, PRIMARY KEY(workspace,id))",
                        "PRAGMA user_version=1",
                    ] {
                        let statement = if base.is_some() && statement.starts_with("CREATE TABLE events") {
                            "CREATE TABLE events (id TEXT PRIMARY KEY, session TEXT NOT NULL, seq TEXT NOT NULL, workspace TEXT NOT NULL, watermark INTEGER NOT NULL, payload BLOB NOT NULL, UNIQUE(session,seq))"
                        } else if base.is_some() && statement.starts_with("CREATE TABLE commands") {
                            "CREATE TABLE commands (workspace TEXT NOT NULL, id TEXT NOT NULL, digest TEXT NOT NULL, watermark INTEGER NOT NULL, payload BLOB NOT NULL, PRIMARY KEY(workspace,id))"
                        } else if base.is_some() && statement == "PRAGMA user_version=1" { "PRAGMA user_version=2" } else {statement};
                        sqlx::query(statement).execute(&mut *tx).await?;
                    }
                    materialize_base(&mut tx, &seed).await?;
                    tx.commit().await.map_err(sql_error)?;
                }
                let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
                    .fetch_one(&mut db)
                    .await?;
                if integrity != "ok" {
                    return Err(Error::Corruption("SQLite integrity check"));
                }
                let mut state = seed;
                let mut commits = Vec::new();
                // Bound each load and keyset-page the log instead of fetching the history as one SQL result.
                loop {
                    let row = sqlx::query("SELECT watermark,id,payload,digest FROM commits WHERE watermark>? ORDER BY watermark LIMIT 1")
                        .bind(i64::try_from(state.watermark.get()).map_err(|_| Error::Limit("SQLite sequence"))?).fetch_optional(&mut db).await?;
                    let Some(row) = row else { break };
                    let payload: Vec<u8> = row.try_get("payload")?;
                    if payload.len() > MAX_COMMIT_BYTES
                        || digest_bytes(&payload) != row.try_get::<String, _>("digest")?
                    {
                        return Err(Error::Corruption("SQLite commit checksum"));
                    }
                    let commit: Commit = serde_json::from_slice(&payload)?;
                    if commit.receipt.watermark.get() != row.try_get::<i64, _>("watermark")? as u64
                        || commit.transaction.id.as_str() != row.try_get::<String, _>("id")?
                    {
                        return Err(Error::Corruption("SQLite commit identity"));
                    }
                    state.replay(&commit)?;
                    commits.push(commit);
                }
                let mut backend = Self::Sqlite(db);
                backend.verify_materialized(&state).await?;
                backend.configuration().await?;
                Ok((backend, state, commits))
            }
            BackendKind::Files => {
                let mut journal = Journal {
                    file: OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(false)
                        .open(root.join("canonical.frames"))?,
                    root: root.to_owned(),
                    chain: initial_chain.clone(),
                    initial_chain,
                    base: seed,
                    #[cfg(test)]
                    write_budget: None,
                };
                let (state, commits) = journal.replay()?;
                journal.verify_checkpoint(&state, &commits)?;
                Ok((Self::Files(journal), state, commits))
            }
        }
    }
    pub(crate) async fn configuration(&mut self) -> Result<serde_json::Value> {
        match self {
            Self::Sqlite(db) => {
                let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
                    .fetch_one(&mut *db)
                    .await?;
                let sync: i64 = sqlx::query_scalar("PRAGMA synchronous")
                    .fetch_one(&mut *db)
                    .await?;
                let foreign: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
                    .fetch_one(&mut *db)
                    .await?;
                let timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
                    .fetch_one(&mut *db)
                    .await?;
                let version: String = sqlx::query_scalar("SELECT sqlite_version()")
                    .fetch_one(&mut *db)
                    .await?;
                if mode != "wal" || sync != 2 || foreign != 1 || timeout != 100 {
                    return Err(Error::Corruption("SQLite connection settings"));
                }
                Ok(
                    serde_json::json!({"backend":"sqlite","sqlite":version,"journal":mode,"synchronous":sync,"foreign_keys":foreign,"busy_timeout_ms":timeout}),
                )
            }
            Self::Files(_) => Ok(
                serde_json::json!({"backend":"files","frame":"VCPJ0001","commit_marker":"VCPCMIT1","durability":"sync_all before receipt"}),
            ),
        }
    }
    pub(crate) async fn append(
        &mut self,
        commit: &Commit,
        state: &State,
        observe: impl Fn(Barrier),
    ) -> Result<()> {
        let payload = canonical_bytes(commit)?;
        if payload.len() > MAX_COMMIT_BYTES {
            return Err(Error::Limit("commit bytes"));
        }
        match self {
            Self::Sqlite(db) => {
                let mut tx = db.begin_with("BEGIN IMMEDIATE").await.map_err(sql_error)?;
                let watermark = i64::try_from(commit.receipt.watermark.get())
                    .map_err(|_| Error::Limit("SQLite watermark"))?;
                sqlx::query("INSERT INTO commits(watermark,id,payload,digest) VALUES(?,?,?,?)")
                    .bind(watermark)
                    .bind(commit.transaction.id.as_str())
                    .bind(&payload)
                    .bind(digest_bytes(&payload))
                    .execute(&mut *tx)
                    .await
                    .map_err(sql_error)?;
                for mutation in &commit.transaction.mutations {
                    match mutation {
                        Mutation::Put { expected, record } => {
                            let bytes = canonical_bytes(record)?;
                            let changed = if let Some(expected) = expected {
                                sqlx::query("UPDATE records SET revision=?,payload=?,digest=? WHERE key=? AND revision=? AND workspace=?")
                                    .bind(record.revision.get().to_string()).bind(&bytes).bind(digest_bytes(&bytes)).bind(record.key())
                                    .bind(expected.get().to_string()).bind(record.workspace.as_str()).execute(&mut *tx).await.map_err(sql_error)?.rows_affected()
                            } else {
                                sqlx::query("INSERT INTO records(key,workspace,revision,collection,payload,digest) VALUES(?,?,?,?,?,?)")
                                    .bind(record.key()).bind(record.workspace.as_str()).bind(record.revision.get().to_string()).bind(record.collection.name())
                                    .bind(&bytes).bind(digest_bytes(&bytes)).execute(&mut *tx).await.map_err(sql_error)?.rows_affected()
                            };
                            if changed != 1 {
                                return Err(Error::Conflict("SQLite compare-and-set"));
                            }
                            sqlx::query("DELETE FROM edges WHERE source=?")
                                .bind(record.key())
                                .execute(&mut *tx)
                                .await?;
                            for reference in record.required_references()? {
                                sqlx::query("INSERT INTO edges(source,target) VALUES(?,?)")
                                    .bind(record.key())
                                    .bind(reference)
                                    .execute(&mut *tx)
                                    .await?;
                            }
                        }
                        Mutation::DropProjection { id, expected } => {
                            let changed = sqlx::query("DELETE FROM records WHERE key=? AND revision=? AND collection='projection'")
                                .bind(key(Collection::Projection, id)).bind(expected.get().to_string()).execute(&mut *tx).await?.rows_affected();
                            if changed != 1 {
                                return Err(Error::Conflict("projection compare-and-set"));
                            }
                        }
                    }
                }
                for event in state
                    .events
                    .iter()
                    .filter(|event| event.watermark == commit.receipt.watermark)
                {
                    sqlx::query("INSERT INTO events(id,session,seq,workspace,watermark,payload) VALUES(?,?,?,?,?,?)")
                        .bind(event.event.id.as_str()).bind(event.event.session.as_str()).bind(event.sequence.get().to_string())
                        .bind(event.event.workspace.as_str()).bind(watermark).bind(canonical_bytes(event)?).execute(&mut *tx).await?;
                }
                if let Some(command) = &commit.receipt.command {
                    sqlx::query("INSERT INTO commands(workspace,id,digest,watermark,payload) VALUES(?,?,?,?,?)")
                        .bind(command.workspace.as_str()).bind(command.command.as_str()).bind(&command.digest).bind(watermark)
                        .bind(canonical_bytes(command)?).execute(&mut *tx).await?;
                }
                observe(Barrier::BeforeCommit);
                tx.commit().await.map_err(sql_error)?;
                observe(Barrier::AfterCommit);
            }
            Self::Files(journal) => journal.append(&payload, commit.receipt.watermark, &observe)?,
        }
        Ok(())
    }
    pub(crate) async fn verify_materialized(&mut self, state: &State) -> Result<()> {
        if let Self::Sqlite(db) = self {
            let rows = sqlx::query(
                "SELECT key,payload,digest,workspace,revision,collection FROM records ORDER BY key",
            )
            .fetch_all(&mut *db)
            .await?;
            if rows.len() != state.records.len() {
                return Err(Error::Corruption("SQLite record count"));
            }
            for row in rows {
                let key: String = row.try_get("key")?;
                let payload: Vec<u8> = row.try_get("payload")?;
                let expected = state
                    .records
                    .get(&key)
                    .ok_or(Error::Corruption("SQLite unexpected record"))?;
                if payload != canonical_bytes(expected)?
                    || digest_bytes(&payload) != row.try_get::<String, _>("digest")?
                    || expected.workspace.as_str() != row.try_get::<String, _>("workspace")?
                    || expected.revision.get().to_string()
                        != row.try_get::<String, _>("revision")?
                    || expected.collection.name() != row.try_get::<String, _>("collection")?
                {
                    return Err(Error::Corruption("SQLite materialized record"));
                }
            }
            for (query, expected) in [
                ("SELECT count(*) FROM events", state.events.len()),
                ("SELECT count(*) FROM commands", state.commands.len()),
            ] {
                let count: i64 = sqlx::query_scalar(query).fetch_one(&mut *db).await?;
                if count as usize != expected {
                    return Err(Error::Corruption("SQLite projection count"));
                }
            }
            for event in &state.events {
                let bytes: Vec<u8> = sqlx::query_scalar("SELECT payload FROM events WHERE id=?")
                    .bind(event.event.id.as_str())
                    .fetch_one(&mut *db)
                    .await?;
                if bytes != canonical_bytes(event)? {
                    return Err(Error::Corruption("SQLite event content"));
                }
            }
            for receipt in state.commands.values() {
                let bytes: Vec<u8> =
                    sqlx::query_scalar("SELECT payload FROM commands WHERE workspace=? AND id=?")
                        .bind(receipt.workspace.as_str())
                        .bind(receipt.command.as_str())
                        .fetch_one(&mut *db)
                        .await?;
                if bytes != canonical_bytes(receipt)? {
                    return Err(Error::Corruption("SQLite receipt content"));
                }
            }
            let fk = sqlx::query("PRAGMA foreign_key_check")
                .fetch_optional(&mut *db)
                .await?;
            if fk.is_some() {
                return Err(Error::Corruption("SQLite foreign references"));
            }
        }
        Ok(())
    }
    pub(crate) fn checkpoint(&mut self, state: &State) -> Result<()> {
        if let Self::Files(journal) = self {
            let bytes = canonical_bytes(state)?;
            let file = format!("checkpoint-{:020}.json", state.watermark.get());
            let path = journal.root.join(&file);
            if !path.exists() {
                immutable_file(&path, &bytes)?;
            } else if read_bounded(&path, 64 * 1024 * 1024)? != bytes {
                return Err(Error::Corruption("checkpoint collision"));
            }
            let seal = serde_json::json!({"version":1,"watermark":state.watermark,"file":file,"sha256":digest_bytes(&bytes),"chain":journal.chain});
            let pointer = journal
                .root
                .join(format!("checkpoint-{:020}.active", state.watermark.get()));
            if !pointer.exists() {
                immutable_file(&pointer, &canonical_bytes(&seal)?)?;
            }
        }
        Ok(())
    }
}
impl Journal {
    fn replay(&mut self) -> Result<(State, Vec<Commit>)> {
        // A published durable tip distinguishes truncation of acknowledged data
        // from a writer that died before appending its commit marker.
        let mut tips = fs::read_dir(&self.root)?
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|entry| {
                entry.file_name().to_string_lossy().starts_with("commit-")
                    && entry.path().extension().is_some_and(|e| e == "tip")
            })
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        tips.sort();
        let tip = if let Some(path) = tips.last() {
            Some(serde_json::from_slice::<serde_json::Value>(&read_bounded(
                path, 4096,
            )?)?)
        } else {
            None
        };
        if let Some(tip) = &tip {
            let end = tip["end"]
                .as_str()
                .and_then(|n| n.parse::<u64>().ok())
                .ok_or(Error::Corruption("journal tip extent"))?;
            if tip["version"] != 1 || self.file.metadata()?.len() < end {
                return Err(Error::Corruption("acknowledged journal was truncated"));
            }
        }
        let mut state = self.base.clone();
        let mut commits = Vec::new();
        loop {
            let start = self.file.stream_position()?;
            let remaining = self
                .file
                .metadata()?
                .len()
                .checked_sub(start)
                .ok_or(Error::Corruption("journal shrank"))?;
            if remaining == 0 {
                break;
            }
            if remaining < HEADER as u64 {
                self.quarantine_tail(start)?;
                break;
            }
            let mut header = [0; HEADER];
            self.file.read_exact(&mut header)?;
            let len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
            let inverse = u32::from_le_bytes(header[12..16].try_into().unwrap());
            if &header[..8] != MAGIC
                || len == 0
                || len > MAX_COMMIT_BYTES
                || inverse != !(len as u32)
                || header[16..] != *self.chain.as_bytes()
            {
                return Err(Error::Corruption("journal header or chain"));
            }
            if remaining < (HEADER + len + TRAILER) as u64 {
                self.quarantine_tail(start)?;
                break;
            }
            let mut payload = vec![0; len];
            self.file.read_exact(&mut payload)?;
            let mut trailer = [0; TRAILER];
            self.file.read_exact(&mut trailer)?;
            let hash = digest_bytes(&[header.as_slice(), payload.as_slice()].concat());
            if trailer[..64] != *hash.as_bytes() || &trailer[64..] != COMMITTED {
                return Err(Error::Corruption("committed journal bytes"));
            }
            let commit: Commit = serde_json::from_slice(&payload)?;
            state.replay(&commit)?;
            commits.push(commit);
            self.chain = hash;
            if let Some(tip) = &tip {
                if tip["watermark"] == serde_json::to_value(state.watermark)?
                    && (tip["chain"] != self.chain
                        || tip["end"] != self.file.stream_position()?.to_string())
                {
                    return Err(Error::Corruption("acknowledged journal tip differs"));
                }
            }
        }
        if let Some(tip) = &tip {
            let watermark: vcp_domain::Watermark =
                serde_json::from_value(tip["watermark"].clone())?;
            if watermark > state.watermark {
                return Err(Error::Corruption("journal tip missing"));
            }
        }
        self.file.seek(SeekFrom::End(0))?;
        Ok((state, commits))
    }
    fn quarantine_tail(&mut self, start: u64) -> Result<()> {
        self.file.seek(SeekFrom::Start(start))?;
        let mut tail = Vec::new();
        Read::by_ref(&mut self.file)
            .take((MAX_COMMIT_BYTES + HEADER + TRAILER) as u64)
            .read_to_end(&mut tail)?;
        if !tail.is_empty() {
            let path = self.root.join(format!(
                "torn-tail-{}.bin",
                vcp_domain::TransactionId::new()
            ));
            immutable_file(&path, &tail)?;
        }
        self.file.set_len(start)?;
        self.file.sync_all()?;
        Ok(())
    }
    fn append(
        &mut self,
        payload: &[u8],
        watermark: vcp_domain::Watermark,
        observe: &impl Fn(Barrier),
    ) -> Result<()> {
        let mut header = Vec::with_capacity(HEADER);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        header.extend_from_slice(&(!(payload.len() as u32)).to_le_bytes());
        header.extend_from_slice(self.chain.as_bytes());
        let hash = digest_bytes(&[header.as_slice(), payload].concat());
        self.write_bytes(&header)?;
        self.write_bytes(payload)?;
        self.write_bytes(hash.as_bytes())?;
        observe(Barrier::BeforeCommit);
        self.write_bytes(COMMITTED)?;
        self.file.sync_all()?;
        self.chain = hash;
        let tip = serde_json::json!({"version":1,"watermark":watermark,"end":self.file.stream_position()?.to_string(),"chain":self.chain});
        immutable_file(
            &self
                .root
                .join(format!("commit-{:020}.tip", watermark.get())),
            &canonical_bytes(&tip)?,
        )?;
        observe(Barrier::AfterCommit);
        Ok(())
    }
    fn write_bytes(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        #[cfg(test)]
        if let Some(remaining) = &mut self.write_budget {
            let written = bytes.len().min(*remaining);
            self.file.write_all(&bytes[..written])?;
            *remaining -= written;
            if written < bytes.len() {
                return Err(std::io::ErrorKind::StorageFull.into());
            }
            return Ok(());
        }
        self.file.write_all(bytes)
    }
    fn verify_checkpoint(&self, state: &State, commits: &[Commit]) -> Result<()> {
        let mut pointers = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("checkpoint-") && name.ends_with(".active") {
                pointers.push(entry.path());
            }
        }
        pointers.sort();
        if let Some(pointer) = pointers.last() {
            let seal: serde_json::Value = serde_json::from_slice(&read_bounded(pointer, 4096)?)?;
            let watermark: vcp_domain::Watermark =
                serde_json::from_value(seal["watermark"].clone())?;
            let name = format!("checkpoint-{:020}.json", watermark.get());
            if seal["version"] != 1 || seal["file"] != name || watermark > state.watermark {
                return Err(Error::Corruption("checkpoint pointer"));
            }
            let bytes = read_bounded(&self.root.join(name), 64 * 1024 * 1024)?;
            if seal["sha256"] != digest_bytes(&bytes) {
                return Err(Error::Corruption("checkpoint seal"));
            }
            let checkpoint: State = serde_json::from_slice(&bytes)?;
            let mut expected = self.base.clone();
            if watermark < expected.watermark {
                return Err(Error::Corruption("checkpoint before replay base"));
            }
            let mut chain = self.initial_chain.clone();
            for commit in commits
                .iter()
                .take_while(|c| c.receipt.watermark <= watermark)
            {
                expected.replay(commit)?;
                let payload = canonical_bytes(commit)?;
                let mut header = Vec::new();
                header.extend_from_slice(MAGIC);
                header.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                header.extend_from_slice(&(!(payload.len() as u32)).to_le_bytes());
                header.extend_from_slice(chain.as_bytes());
                header.extend_from_slice(&payload);
                chain = digest_bytes(&header);
            }
            if seal["chain"] != chain {
                return Err(Error::Corruption("checkpoint journal boundary"));
            }
            if expected != checkpoint {
                return Err(Error::Corruption(
                    "checkpoint differs from canonical history",
                ));
            }
        }
        // The retained full journal is an unambiguous recovery source when an
        // unpublished/torn checkpoint has no active marker. No history is deleted.
        Ok(())
    }
}

async fn materialize_base(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    state: &State,
) -> Result<()> {
    for record in state.records.values() {
        let bytes = canonical_bytes(record)?;
        sqlx::query("INSERT INTO records(key,workspace,revision,collection,payload,digest) VALUES(?,?,?,?,?,?)")
            .bind(record.key()).bind(record.workspace.as_str()).bind(record.revision.get().to_string()).bind(record.collection.name()).bind(&bytes).bind(digest_bytes(&bytes)).execute(&mut **tx).await?;
    }
    for record in state.records.values() {
        for reference in record.required_references()? {
            sqlx::query("INSERT INTO edges(source,target) VALUES(?,?)")
                .bind(record.key())
                .bind(reference)
                .execute(&mut **tx)
                .await?;
        }
    }
    for event in &state.events {
        sqlx::query(
            "INSERT INTO events(id,session,seq,workspace,watermark,payload) VALUES(?,?,?,?,?,?)",
        )
        .bind(event.event.id.as_str())
        .bind(event.event.session.as_str())
        .bind(event.sequence.get().to_string())
        .bind(event.event.workspace.as_str())
        .bind(
            i64::try_from(event.watermark.get())
                .map_err(|_| Error::Limit("SQLite base watermark"))?,
        )
        .bind(canonical_bytes(event)?)
        .execute(&mut **tx)
        .await?;
    }
    for command in state.commands.values() {
        sqlx::query(
            "INSERT INTO commands(workspace,id,digest,watermark,payload) VALUES(?,?,?,?,?)",
        )
        .bind(command.workspace.as_str())
        .bind(command.command.as_str())
        .bind(&command.digest)
        .bind(
            i64::try_from(command.watermark.get())
                .map_err(|_| Error::Limit("SQLite base watermark"))?,
        )
        .bind(canonical_bytes(command)?)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}
