// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "qualification")]
mod common;
use sqlx::{Connection, Row};
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use vcp_store::{contract::*, *};

#[tokio::test]
async fn independent_process_kills_recover_atomic_receipt_and_release_real_owner_lock() {
    for kind in [BackendKind::Sqlite, BackendKind::Files] {
        for barrier in ["prepared", "before_commit", "after_commit", "before_reply"] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("canonical");
            let marker = temporary.path().join("reached");
            let kind_text = if kind == BackendKind::Sqlite {
                "sqlite"
            } else {
                "files"
            };
            let mut child = Command::new(env!("CARGO_BIN_EXE_vcp-store-crash-fixture"))
                .arg(&root)
                .arg(kind_text)
                .arg(barrier)
                .arg(&marker)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let start = Instant::now();
            while !marker.exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("child exited before {barrier}: {status}");
                }
                if start.elapsed() > Duration::from_secs(20) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("barrier timeout: {barrier}");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(
                Store::open(&root, kind, &[]).await.is_err(),
                "second process must not acquire live owner"
            );
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            let committed = matches!(barrier, "after_commit" | "before_reply");
            // Observe physical persistence independently of Store's claimed state.
            if kind == BackendKind::Sqlite {
                let options = sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(root.join("canonical.sqlite"))
                    .read_only(true);
                let mut db = sqlx::SqliteConnection::connect_with(&options)
                    .await
                    .unwrap();
                let row=sqlx::query("SELECT (SELECT count(*) FROM commits) AS commits,(SELECT count(*) FROM commands) AS commands,(SELECT count(*) FROM events) AS events,(SELECT count(*) FROM records) AS records").fetch_one(&mut db).await.unwrap();
                for column in ["commits", "commands", "events"] {
                    assert_eq!(
                        row.get::<i64, _>(column),
                        i64::from(committed),
                        "{barrier} {column}"
                    );
                }
                assert_eq!(row.get::<i64, _>("records"), if committed { 3 } else { 0 });
                db.close().await.unwrap();
            } else {
                let bytes = std::fs::read(root.join("canonical.frames")).unwrap();
                assert_eq!(bytes.ends_with(b"VCPCMIT1"), committed);
            }
            let mut recovered = Store::open(&root, kind, &[]).await.unwrap();
            assert_eq!(recovered.state().watermark.get(), u64::from(committed));
            let first = recovered.transact(common::initial()).await.unwrap();
            let retry = recovered.transact(common::initial()).await.unwrap();
            assert_eq!(first, retry);
            assert_eq!(recovered.state().watermark.get(), 1);
            assert_eq!(recovered.state().events.len(), 1);
            assert_eq!(recovered.state().commands.len(), 1);
        }
    }
}

#[tokio::test]
async fn migration_kills_leave_one_active_backend_and_an_unchanged_recovery_root() {
    for kind in [BackendKind::Sqlite, BackendKind::Files] {
        for barrier in ["before_validation", "before_activation", "after_activation"] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("active");
            let marker = temporary.path().join("reached");
            let other = if kind == BackendKind::Sqlite {
                BackendKind::Files
            } else {
                BackendKind::Sqlite
            };
            let mut child = Command::new(env!("CARGO_BIN_EXE_vcp-store-crash-fixture"))
                .arg(&root)
                .arg(if kind == BackendKind::Sqlite {
                    "sqlite"
                } else {
                    "files"
                })
                .arg(barrier)
                .arg(&marker)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let start = Instant::now();
            while !marker.exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("migration fixture exited: {status}");
                }
                if start.elapsed() > Duration::from_secs(20) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("migration barrier timeout");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(migration::ActiveRoot::open(&root, None, &[]).await.is_err());
            child.kill().unwrap();
            child.wait().unwrap();
            let active = migration::ActiveRoot::open(&root, None, &[]).await.unwrap();
            assert_eq!(
                active.store().kind(),
                if barrier == "after_activation" {
                    other
                } else {
                    kind
                }
            );
            assert_eq!(active.store().state().watermark.get(), 1);
            assert_eq!(active.store().state().commands.len(), 1);
            let first: serde_json::Value = serde_json::from_slice(
                &std::fs::read(root.join("activation-00000000000000000000.json")).unwrap(),
            )
            .unwrap();
            let original = root.join("roots").join(first["root"].as_str().unwrap());
            if barrier == "after_activation" {
                let source = Store::open(&original, kind, &[]).await.unwrap();
                assert_eq!(source.state(), active.store().state());
            }
            assert!(original.is_dir());
        }
    }
}
