// SPDX-License-Identifier: Apache-2.0
//! Exact all-row comparison with authenticated durable history. Each database
//! payload is bounded before allocation; no resident copy of history is built.
use super::*;
use crate::history_index::io::Sqlite;
use vcp_domain::{CommandId, EventId, WorkspaceId};
#[path = "backend_current_materialized_pages.rs"]
mod pages;

pub(super) async fn verify(db: &mut SqliteConnection, owner: &DurableOwner) -> Result<()> {
    let current = owner.semantic().current();
    let catalog = owner.semantic().catalog();
    for (query, expected) in [
        ("SELECT count(*) FROM records", current.records.len() as u64),
        ("SELECT count(*) FROM events", catalog.event_count()),
        ("SELECT count(*) FROM commands", catalog.command_count()),
    ] {
        let count: i64 = sqlx::query_scalar(query).fetch_one(&mut *db).await?;
        if u64::try_from(count).ok() != Some(expected) {
            return Err(Error::Corruption("SQLite projection count"));
        }
    }
    let mut after = String::new();
    let mut records = 0usize;
    loop {
        let rows = pages::records(db, &after).await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let key: String = row.try_get("key")?;
            let expected = current
                .records
                .get(&key)
                .ok_or(Error::Corruption("SQLite unexpected record"))?;
            if key <= after {
                return Err(Error::Corruption("SQLite record order"));
            }
            let payload = payload(&row)?;
            if payload != canonical_bytes(expected)?
                || digest_bytes(&payload) != row.try_get::<String, _>("digest")?
                || expected.workspace.as_str() != row.try_get::<String, _>("workspace")?
                || expected.revision.get().to_string() != row.try_get::<String, _>("revision")?
                || expected.collection.name() != row.try_get::<String, _>("collection")?
            {
                return Err(Error::Corruption("SQLite materialized record"));
            }
            after = key;
            records += 1;
        }
    }
    if records != current.records.len() {
        return Err(Error::Corruption("SQLite projection count"));
    }
    let mut after = String::new();
    let mut count = 0u64;
    loop {
        let rows = pages::events(db, &after).await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let id: String = row.try_get("id")?;
            let payload = payload(&row)?;
            let event = catalog
                .event(&mut Sqlite::new(db), &EventId::parse(&id)?)
                .await?
                .ok_or(Error::Corruption("SQLite unexpected event"))?;
            if id <= after || payload != canonical_bytes(&event)? {
                return Err(Error::Corruption("SQLite event content"));
            }
            after = id;
            count = count
                .checked_add(1)
                .ok_or(Error::Limit("SQLite event count"))?;
        }
    }
    if count != catalog.event_count() {
        return Err(Error::Corruption("SQLite projection count"));
    }
    let mut workspace = String::new();
    let mut id = String::new();
    let mut count = 0u64;
    loop {
        let rows = pages::commands(db, &workspace, &id).await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let next_workspace: String = row.try_get("workspace")?;
            let next_id: String = row.try_get("id")?;
            let payload = payload(&row)?;
            let receipt = catalog
                .command_unchecked_meaning(
                    &mut Sqlite::new(db),
                    &WorkspaceId::parse(&next_workspace)?,
                    &CommandId::parse(&next_id)?,
                )
                .await?
                .ok_or(Error::Corruption("SQLite unexpected receipt"))?;
            if (&next_workspace, &next_id) <= (&workspace, &id)
                || payload != canonical_bytes(&receipt)?
            {
                return Err(Error::Corruption("SQLite receipt content"));
            }
            workspace = next_workspace;
            id = next_id;
            count = count
                .checked_add(1)
                .ok_or(Error::Limit("SQLite receipt count"))?;
        }
    }
    if count != catalog.command_count() {
        return Err(Error::Corruption("SQLite projection count"));
    }
    if sqlx::query("PRAGMA foreign_key_check")
        .fetch_optional(&mut *db)
        .await?
        .is_some()
    {
        return Err(Error::Corruption("SQLite foreign references"));
    }
    Ok(())
}
fn payload(row: &sqlx::sqlite::SqliteRow) -> Result<Vec<u8>> {
    row.try_get::<Option<Vec<u8>>, _>("payload")?
        .ok_or(Error::Corruption("SQLite materialized payload bound"))
}
