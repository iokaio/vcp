// SPDX-License-Identifier: Apache-2.0
//! SQLite roots never replace semantic replay. Original payloads and final
//! materialization are checked against the jointly admitted owner.
use super::*;
use crate::{history_index::io::Sqlite, replay_base::ReplayBase};
use vcp_domain::Watermark;

pub(crate) async fn replay_sqlite_all(
    db: &mut SqliteConnection,
    origin: &Origin,
    base: Option<&ReplayBase>,
    diagnostics: &mut crate::StoreDiagnostics,
) -> Result<DurableOwner> {
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut *db)
        .await?;
    if integrity != "ok" {
        return Err(Error::Corruption("SQLite integrity check"));
    }
    let mut state = base.map(|base| base.state.clone()).unwrap_or_default();
    if state.watermark > origin.watermark() {
        return Err(Error::Corruption("history origin before base"));
    }
    let mut size = StateSize::measure(&state)?;
    while state.watermark < origin.watermark() {
        let (commit, payload) = read_commit(db, state.watermark)
            .await?
            .ok_or(Error::Corruption("history origin legacy prefix missing"))?;
        state = state.into_replayed_observed(&commit, diagnostics, &mut size)?;
        origin
            .verify_original(&mut Sqlite::new(db), &commit, &payload)
            .await?;
        observed(diagnostics, payload.len());
    }
    let mut owner = origin.admit(&mut Sqlite::new(db), &state, base).await?;
    drop(state);
    let mut suffix = 0i64;
    while let Some((commit, payload)) =
        read_commit(db, owner.semantic().current().watermark).await?
    {
        let watermark = i64::try_from(commit.receipt.watermark.get())
            .map_err(|_| Error::Limit("SQLite watermark"))?;
        let publication:Option<String>=sqlx::query_scalar("SELECT CASE WHEN typeof(digest)='text' AND length(digest)=64 THEN digest ELSE NULL END FROM history_publications WHERE watermark=?")
            .bind(watermark).fetch_optional(&mut *db).await?.flatten();
        let publication =
            publication.ok_or(Error::Corruption("SQLite history publication missing"))?;
        owner = super::replay::replay_payload(&mut Sqlite::new(db), &owner, &payload, &publication)
            .await?;
        observed(diagnostics, payload.len());
        suffix = suffix
            .checked_add(1)
            .ok_or(Error::Limit("SQLite publication count"))?;
    }
    let publications: i64 = sqlx::query_scalar("SELECT count(*) FROM history_publications")
        .fetch_one(&mut *db)
        .await?;
    if publications != suffix {
        return Err(Error::Corruption("SQLite publication count"));
    }
    // Prefix rows at/below a retained base are unavailable, never silently
    // ignored. There is exactly one original row for each post-base watermark.
    let commits: i64 = sqlx::query_scalar("SELECT count(*) FROM commits")
        .fetch_one(&mut *db)
        .await?;
    if u64::try_from(commits).ok() != Some(owner.originals().root().count()) {
        return Err(Error::Corruption("SQLite original row count"));
    }
    super::materialized::verify(db, &owner).await?;
    Ok(owner)
}
async fn read_commit(
    db: &mut SqliteConnection,
    after: Watermark,
) -> Result<Option<(Commit, Vec<u8>)>> {
    let row=sqlx::query("SELECT watermark,id,CASE WHEN typeof(payload)='blob' AND length(payload) BETWEEN 1 AND ? THEN payload ELSE NULL END AS payload,digest FROM commits WHERE watermark>? ORDER BY watermark LIMIT 1")
        .bind(MAX_COMMIT_BYTES as i64).bind(i64::try_from(after.get()).map_err(|_|Error::Limit("SQLite watermark"))?).fetch_optional(&mut *db).await?;
    let Some(row) = row else { return Ok(None) };
    let bytes: Vec<u8> = row
        .try_get::<Option<Vec<u8>>, _>("payload")?
        .ok_or(Error::Corruption("SQLite commit length"))?;
    if digest_bytes(&bytes) != row.try_get::<String, _>("digest")? {
        return Err(Error::Corruption("SQLite commit checksum"));
    }
    let commit: Commit = serde_json::from_slice(&bytes)?;
    if commit.transaction.id.as_str() != row.try_get::<String, _>("id")?
        || i64::try_from(commit.receipt.watermark.get()).ok() != Some(row.try_get("watermark")?)
        || commit.receipt.watermark != after.next()?
    {
        return Err(Error::Corruption("SQLite commit identity"));
    }
    Ok(Some((commit, bytes)))
}
fn observed(diagnostics: &mut crate::StoreDiagnostics, bytes: usize) {
    diagnostics.replayed_commits = diagnostics.replayed_commits.saturating_add(1);
    diagnostics.replay_payload_bytes = diagnostics
        .replay_payload_bytes
        .saturating_add(bytes as u64);
}
