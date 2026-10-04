// SPDX-License-Identifier: Apache-2.0
//! Test-only extraction of the exact existing SQLite row publication body.
//! Native layout-3 publication supplies only fully validated appended envelopes.
use super::*;
use vcp_protocol::event::EventEnvelope;

pub(super) async fn insert_rows(
    db: &mut SqliteConnection,
    commit: &Commit,
    payload: &[u8],
    events: &[EventEnvelope],
) -> Result<()> {
    let watermark = i64::try_from(commit.receipt.watermark.get())
        .map_err(|_| Error::Limit("SQLite watermark"))?;
    sqlx::query("INSERT INTO commits(watermark,id,payload,digest) VALUES(?,?,?,?)")
        .bind(watermark)
        .bind(commit.transaction.id.as_str())
        .bind(payload)
        .bind(digest_bytes(&payload))
        .execute(&mut *db)
        .await
        .map_err(sql_error)?;
    for mutation in &commit.transaction.mutations {
        match mutation {
            Mutation::Put { expected, record } => {
                let bytes = canonical_bytes(record)?;
                let changed = if let Some(expected) = expected {
                    sqlx::query("UPDATE records SET revision=?,payload=?,digest=? WHERE key=? AND revision=? AND workspace=?")
                        .bind(record.revision.get().to_string()).bind(&bytes).bind(digest_bytes(&bytes)).bind(record.key())
                        .bind(expected.get().to_string()).bind(record.workspace.as_str()).execute(&mut *db).await.map_err(sql_error)?.rows_affected()
                } else {
                    sqlx::query("INSERT INTO records(key,workspace,revision,collection,payload,digest) VALUES(?,?,?,?,?,?)")
                        .bind(record.key()).bind(record.workspace.as_str()).bind(record.revision.get().to_string()).bind(record.collection.name())
                        .bind(&bytes).bind(digest_bytes(&bytes)).execute(&mut *db).await.map_err(sql_error)?.rows_affected()
                };
                if changed != 1 {
                    return Err(Error::Conflict("SQLite compare-and-set"));
                }
                sqlx::query("DELETE FROM edges WHERE source=?")
                    .bind(record.key())
                    .execute(&mut *db)
                    .await?;
                for reference in record.required_references()? {
                    sqlx::query("INSERT INTO edges(source,target) VALUES(?,?)")
                        .bind(record.key())
                        .bind(reference)
                        .execute(&mut *db)
                        .await?;
                }
            }
            Mutation::DropProjection { id, expected } => {
                let changed = sqlx::query(
                    "DELETE FROM records WHERE key=? AND revision=? AND collection='projection'",
                )
                .bind(key(Collection::Projection, id))
                .bind(expected.get().to_string())
                .execute(&mut *db)
                .await?
                .rows_affected();
                if changed != 1 {
                    return Err(Error::Conflict("projection compare-and-set"));
                }
            }
        }
    }
    for event in events {
        sqlx::query(
            "INSERT INTO events(id,session,seq,workspace,watermark,payload) VALUES(?,?,?,?,?,?)",
        )
        .bind(event.event.id.as_str())
        .bind(event.event.session.as_str())
        .bind(event.sequence.get().to_string())
        .bind(event.event.workspace.as_str())
        .bind(watermark)
        .bind(canonical_bytes(event)?)
        .execute(&mut *db)
        .await?;
    }
    if let Some(command) = &commit.receipt.command {
        sqlx::query(
            "INSERT INTO commands(workspace,id,digest,watermark,payload) VALUES(?,?,?,?,?)",
        )
        .bind(command.workspace.as_str())
        .bind(command.command.as_str())
        .bind(&command.digest)
        .bind(watermark)
        .bind(canonical_bytes(command)?)
        .execute(&mut *db)
        .await?;
    }
    Ok(())
}
