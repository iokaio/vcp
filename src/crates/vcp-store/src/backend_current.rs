// SPDX-License-Identifier: Apache-2.0
//! Layout-3 native publication qualification. Original Commit payloads remain
//! byte-for-byte journal bodies; the new header authenticates the staged root.
use super::*;
use crate::{admitted_history::AdmittedCut, contract::current_transition::PreparedCurrent};
#[path = "backend_current_replay.rs"]
mod replay;
#[path = "backend_current_sqlite.rs"]
mod sqlite;
pub(crate) use replay::replay_current;

pub(crate) async fn initialize_sqlite(db: &mut SqliteConnection) -> Result<()> {
    crate::history_index::io::Sqlite::initialize(db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS history_publications(watermark INTEGER PRIMARY KEY REFERENCES commits(watermark), digest TEXT NOT NULL)")
        .execute(db).await?;
    Ok(())
}

pub(crate) async fn append_sqlite(
    db: &mut SqliteConnection,
    source: &AdmittedCut,
    prepared: &PreparedCurrent,
    observe: &impl Fn(Barrier),
) -> Result<AdmittedCut> {
    let commit = prepared.commit();
    let payload = canonical_bytes(commit)?;
    if payload.len() > MAX_COMMIT_BYTES {
        return Err(Error::Limit("commit bytes"));
    }
    let mut transaction = db.begin_with("BEGIN IMMEDIATE").await.map_err(sql_error)?;
    let (next, publication) = {
        let mut pages = crate::history_index::io::Sqlite::new(&mut transaction);
        crate::history_publication::stage(&mut pages, source, prepared, &payload).await?
    };
    sqlite::insert_rows(
        &mut transaction,
        commit,
        &payload,
        &prepared.proposed().events,
    )
    .await?;
    let watermark = i64::try_from(commit.receipt.watermark.get())
        .map_err(|_| Error::Limit("SQLite watermark"))?;
    sqlx::query("INSERT INTO history_publications(watermark,digest) VALUES(?,?)")
        .bind(watermark)
        .bind(publication)
        .execute(&mut *transaction)
        .await?;
    observe(Barrier::BeforeCommit);
    transaction.commit().await.map_err(sql_error)?;
    observe(Barrier::AfterCommit);
    Ok(next)
}

impl Journal {
    pub(crate) fn append_current(
        &mut self,
        payload: &[u8],
        watermark: vcp_domain::Watermark,
        publication: &str,
        observe: &impl Fn(Barrier),
    ) -> Result<()> {
        let header = crate::journal_frame::header(payload, &self.chain, Some(publication))?;
        let hash = digest_bytes(&[header.as_slice(), payload].concat());
        self.write_bytes(&header)?;
        self.write_bytes(payload)?;
        self.write_bytes(hash.as_bytes())?;
        observe(Barrier::BeforeCommit);
        self.write_bytes(crate::journal_frame::COMMITTED)?;
        self.file.sync_all()?;
        self.chain = hash;
        let tip = serde_json::json!({
            "version":3,
            "watermark":watermark,
            "end":self.file.stream_position()?.to_string(),
            "chain":self.chain,
            "publication":publication,
        });
        immutable_file(
            &self
                .root
                .join(format!("commit-{:020}.tip", watermark.get())),
            &canonical_bytes(&tip)?,
        )?;
        observe(Barrier::AfterCommit);
        Ok(())
    }
}

#[path = "backend_current_tests.rs"]
mod tests;
