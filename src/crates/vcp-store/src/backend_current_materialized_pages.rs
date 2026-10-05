// SPDX-License-Identifier: Apache-2.0
//! Select a keyset prefix in one SQLite read snapshot. Only bounded metadata is
//! windowed; payloads cross SQLx after the cumulative byte budget is applied.
use super::*;
use sqlx::sqlite::SqliteRow;

pub(super) async fn records(db: &mut SqliteConnection, after: &str) -> Result<Vec<SqliteRow>> {
    Ok(sqlx::query("WITH candidates AS (SELECT key,CASE WHEN typeof(payload)='blob' AND length(payload) BETWEEN 1 AND ?1 THEN length(payload) ELSE 1 END AS cost FROM records WHERE key>?2 ORDER BY key LIMIT 256), bounded AS (SELECT key,SUM(cost) OVER (ORDER BY key ROWS UNBOUNDED PRECEDING) AS bytes FROM candidates) SELECT r.key,r.workspace,r.revision,r.collection,r.digest,CASE WHEN typeof(r.payload)='blob' AND length(r.payload) BETWEEN 1 AND ?1 THEN r.payload ELSE NULL END AS payload FROM bounded b JOIN records r ON r.key=b.key WHERE b.bytes<=?3 ORDER BY b.key")
        .bind(MAX_RECORD_BYTES as i64).bind(after).bind(MAX_COMMIT_BYTES as i64).fetch_all(db).await?)
}
pub(super) async fn events(db: &mut SqliteConnection, after: &str) -> Result<Vec<SqliteRow>> {
    Ok(sqlx::query("WITH candidates AS (SELECT id,CASE WHEN typeof(payload)='blob' AND length(payload) BETWEEN 1 AND ?1 THEN length(payload) ELSE 1 END AS cost FROM events WHERE id>?2 ORDER BY id LIMIT 256), bounded AS (SELECT id,SUM(cost) OVER (ORDER BY id ROWS UNBOUNDED PRECEDING) AS bytes FROM candidates) SELECT e.id,CASE WHEN typeof(e.payload)='blob' AND length(e.payload) BETWEEN 1 AND ?1 THEN e.payload ELSE NULL END AS payload FROM bounded b JOIN events e ON e.id=b.id WHERE b.bytes<=?1 ORDER BY b.id")
        .bind(MAX_COMMIT_BYTES as i64).bind(after).fetch_all(db).await?)
}
pub(super) async fn commands(
    db: &mut SqliteConnection,
    workspace: &str,
    id: &str,
) -> Result<Vec<SqliteRow>> {
    Ok(sqlx::query("WITH candidates AS (SELECT workspace,id,CASE WHEN typeof(payload)='blob' AND length(payload) BETWEEN 1 AND ?1 THEN length(payload) ELSE 1 END AS cost FROM commands WHERE (workspace,id)>(?2,?3) ORDER BY workspace,id LIMIT 256), bounded AS (SELECT workspace,id,SUM(cost) OVER (ORDER BY workspace,id ROWS UNBOUNDED PRECEDING) AS bytes FROM candidates) SELECT c.workspace,c.id,CASE WHEN typeof(c.payload)='blob' AND length(c.payload) BETWEEN 1 AND ?1 THEN c.payload ELSE NULL END AS payload FROM bounded b JOIN commands c ON (c.workspace,c.id)=(b.workspace,b.id) WHERE b.bytes<=?1 ORDER BY b.workspace,b.id")
        .bind(MAX_COMMIT_BYTES as i64).bind(workspace).bind(id).fetch_all(db).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn materialized_pages_bound_rows_and_aggregate_payload_and_surface_invalid_rows() {
        let mut db = SqliteConnection::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE events(id TEXT PRIMARY KEY,payload BLOB)")
            .execute(&mut db)
            .await
            .unwrap();
        for index in 0..300 {
            sqlx::query("INSERT INTO events VALUES(?,zeroblob(4))")
                .bind(format!("e{index:04}"))
                .execute(&mut db)
                .await
                .unwrap();
        }
        let page = events(&mut db, "").await.unwrap();
        assert_eq!(page.len(), 256);
        assert_eq!(page[255].try_get::<String, _>("id").unwrap(), "e0255");
        assert_eq!(events(&mut db, "e0255").await.unwrap().len(), 44);
        sqlx::query("DELETE FROM events")
            .execute(&mut db)
            .await
            .unwrap();
        let bytes = MAX_COMMIT_BYTES / 3 + 1;
        for index in 0..4 {
            sqlx::query("INSERT INTO events VALUES(?,zeroblob(?))")
                .bind(format!("e{index}"))
                .bind(bytes as i64)
                .execute(&mut db)
                .await
                .unwrap();
        }
        let page = events(&mut db, "").await.unwrap();
        assert_eq!(page.len(), 2);
        let total: usize = page
            .iter()
            .map(|row| row.try_get::<Vec<u8>, _>("payload").unwrap().len())
            .sum();
        assert!(total <= MAX_COMMIT_BYTES);
        assert_eq!(events(&mut db, "e1").await.unwrap().len(), 2);
        // Invalid values consume a metadata slot and produce NULL for the
        // validator. Filtering them out would conceal corruption or fake EOF.
        for statement in [
            "UPDATE events SET payload=NULL WHERE id='e0' AND ?1>0",
            "UPDATE events SET payload='' WHERE id='e0' AND ?1>0",
            "UPDATE events SET payload=zeroblob(0) WHERE id='e0' AND ?1>0",
            "UPDATE events SET payload=zeroblob(?1) WHERE id='e0'",
        ] {
            sqlx::query(statement)
                .bind((MAX_COMMIT_BYTES + 1) as i64)
                .execute(&mut db)
                .await
                .unwrap();
            let page = events(&mut db, "").await.unwrap();
            assert_eq!(page[0].try_get::<String, _>("id").unwrap(), "e0");
            assert!(page[0]
                .try_get::<Option<Vec<u8>>, _>("payload")
                .unwrap()
                .is_none());
        }
    }
    #[tokio::test]
    async fn materialized_pages_keep_record_and_composite_receipt_order_across_row_boundary() {
        let mut db = SqliteConnection::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE records(key TEXT PRIMARY KEY,workspace TEXT,revision TEXT,collection TEXT,digest TEXT,payload BLOB)").execute(&mut db).await.unwrap();
        sqlx::query(
            "CREATE TABLE commands(workspace TEXT,id TEXT,payload BLOB,PRIMARY KEY(workspace,id))",
        )
        .execute(&mut db)
        .await
        .unwrap();
        for index in 0..300 {
            sqlx::query("INSERT INTO records VALUES(?,'w','0','projection','digest',zeroblob(8))")
                .bind(format!("r{index:04}"))
                .execute(&mut db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO commands VALUES(?,?,zeroblob(8))")
                .bind(format!("w{}", index / 100))
                .bind(format!("c{:03}", index % 100))
                .execute(&mut db)
                .await
                .unwrap();
        }
        let rows = records(&mut db, "").await.unwrap();
        assert_eq!(rows.len(), 256);
        assert_eq!(rows[255].try_get::<String, _>("key").unwrap(), "r0255");
        assert_eq!(records(&mut db, "r0255").await.unwrap().len(), 44);
        let rows = commands(&mut db, "", "").await.unwrap();
        assert_eq!(rows.len(), 256);
        assert_eq!(rows[255].try_get::<String, _>("workspace").unwrap(), "w2");
        assert_eq!(rows[255].try_get::<String, _>("id").unwrap(), "c055");
        assert_eq!(commands(&mut db, "w2", "c055").await.unwrap().len(), 44);
    }
}
