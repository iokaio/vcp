// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    cell::RefCell,
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use vcp_domain::{ids::*, revision::*};
use vcp_protocol::event::{EventEnvelope, EventInput, EventKind};

enum Page {
    Events(Vec<u64>),
    Gap(GapReason),
    Failure,
}
struct Source {
    cursors: RefCell<VecDeque<Cursor>>,
    pages: RefCell<VecDeque<Page>>,
    requests: RefCell<Vec<SessionSeq>>,
    released: RefCell<Vec<SnapshotId>>,
}
impl event_drain::Source for Source {
    fn subscribe(&self, after: SessionSeq, limit: u32) -> Result<Cursor, String> {
        self.requests.borrow_mut().push(after);
        let cursor = self
            .cursors
            .borrow_mut()
            .pop_front()
            .expect("unexpected renewal");
        assert_eq!(cursor.after, after);
        assert_eq!(cursor.limit, limit);
        Ok(cursor)
    }
    fn page(&self, mut cursor: Cursor) -> Result<EventPage, String> {
        match self
            .pages
            .borrow_mut()
            .pop_front()
            .expect("unexpected page")
        {
            Page::Gap(reason) => Ok(EventPage::Gap {
                reason,
                restart_from_snapshot: true,
            }),
            Page::Failure => Err("authenticated history read failed".into()),
            Page::Events(sequences) => {
                let events = sequences
                    .into_iter()
                    .map(|sequence| {
                        assert_eq!(sequence, cursor.after.get() + 1);
                        cursor.after = SessionSeq::new(sequence);
                        EventEnvelope {
                            version: 1,
                            sequence: cursor.after,
                            watermark: Watermark::new(sequence),
                            redaction: None,
                            event: EventInput {
                                id: EventId::new(),
                                workspace: cursor.workspace.clone(),
                                session: cursor.session.clone(),
                                task: Some(TaskId::new()),
                                actor: ActorId::new(),
                                correlation: CommandId::new(),
                                causation: None,
                                timestamp: Timestamp::ZERO,
                                kind: EventKind::Diagnostic,
                                artifacts: vec![],
                                data: serde_json::json!({}),
                                metadata: None,
                            },
                        }
                    })
                    .collect();
                Ok(EventPage::Events {
                    events,
                    at_end: cursor.after == cursor.end,
                    snapshot_watermark: cursor.watermark,
                    next_cursor: cursor,
                })
            }
        }
    }
    fn unsubscribe(&self, snapshot: SnapshotId) -> Result<(), String> {
        self.released.borrow_mut().push(snapshot);
        Ok(())
    }
}
fn cursor(end: u64) -> Cursor {
    Cursor {
        version: 1,
        snapshot: SnapshotId::new(),
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        after: SessionSeq::ZERO,
        end: SessionSeq::new(end),
        watermark: Watermark::new(end),
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
        expires_at: Timestamp::new(60_000),
        limit: 32,
    }
}
fn renewed(original: &Cursor, after: u64, end: u64) -> Cursor {
    Cursor {
        snapshot: SnapshotId::new(),
        after: SessionSeq::new(after),
        end: SessionSeq::new(end),
        watermark: Watermark::new(end),
        expires_at: Timestamp::new(120_000),
        ..original.clone()
    }
}
fn fixture_source(cursors: Vec<Cursor>, pages: Vec<Page>) -> Source {
    Source {
        cursors: RefCell::new(cursors.into()),
        pages: RefCell::new(pages.into()),
        requests: RefCell::new(vec![]),
        released: RefCell::new(vec![]),
    }
}
fn fixture_output(fail_sequence: Option<u64>) -> (OwnedJsonl, Arc<Mutex<Vec<serde_json::Value>>>) {
    let (sender, receiver) = mpsc::sync_channel::<Frame>(1);
    let frames = Arc::new(Mutex::new(vec![]));
    let captured = frames.clone();
    std::thread::spawn(move || {
        while let Ok((bytes, reply)) = receiver.recv() {
            let frame: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            if fail_sequence.is_some_and(|seq| frame["event"]["sequence"] == seq.to_string()) {
                let _ = reply.send(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "fixture writer failed",
                )));
                break;
            }
            captured.lock().unwrap().push(frame);
            let _ = reply.send(Ok(()));
        }
    });
    (
        OwnedJsonl {
            stream: Jsonl::new(Vec::new()),
            sender,
            owner: None,
            failed: false,
            finishing: true,
        },
        frames,
    )
}
fn sequences(frames: &Arc<Mutex<Vec<serde_json::Value>>>) -> Vec<u64> {
    frames
        .lock()
        .unwrap()
        .iter()
        .filter(|f| f["type"] == "event")
        .map(|f| f["event"]["sequence"].as_str().unwrap().parse().unwrap())
        .collect()
}

#[tokio::test]
async fn expired_lease_renews_acknowledged_sequence_and_keeps_original_finite_cut() {
    let first = cursor(4);
    let second = renewed(&first, 2, 7);
    let source = fixture_source(
        vec![first.clone(), second.clone()],
        vec![
            Page::Events(vec![1, 2]),
            Page::Gap(GapReason::SnapshotExpired),
            Page::Events(vec![3, 4, 5, 6, 7]),
        ],
    );
    let (mut output, frames) = fixture_output(None);
    assert_eq!(
        output
            .drain_events_from(&source, &CommandId::new(), SessionSeq::ZERO)
            .await
            .unwrap(),
        SessionSeq::new(4)
    );
    assert_eq!(sequences(&frames), vec![1, 2, 3, 4]);
    assert_eq!(
        *source.requests.borrow(),
        vec![SessionSeq::ZERO, SessionSeq::new(2)]
    );
    assert_eq!(
        *source.released.borrow(),
        vec![first.snapshot, second.snapshot]
    );
}

#[tokio::test]
async fn renewal_rejects_authority_retention_scope_and_cut_regressions_before_delivery() {
    for mode in 0..6 {
        let first = cursor(4);
        let mut second = renewed(&first, 2, 5);
        match mode {
            0 => second.authority = AuthorityRevision::new(1),
            1 => second.deletion = DeletionEpoch::new(1),
            2 => second.workspace = WorkspaceId::new(),
            3 => second.session = SessionId::new(),
            4 => second.watermark = Watermark::new(3),
            _ => second.end = SessionSeq::new(3),
        }
        let source = fixture_source(
            vec![first.clone(), second.clone()],
            vec![
                Page::Events(vec![1, 2]),
                Page::Gap(GapReason::SnapshotExpired),
            ],
        );
        let (mut output, frames) = fixture_output(None);
        assert!(output
            .drain_events_from(&source, &CommandId::new(), SessionSeq::ZERO)
            .await
            .is_err());
        assert_eq!(sequences(&frames), vec![1, 2]);
        assert_eq!(
            *source.released.borrow(),
            vec![first.snapshot, second.snapshot]
        );
        let frames = frames.lock().unwrap();
        assert_eq!(frames.last().unwrap()["type"], "cursor_gap");
        let reason = frames.last().unwrap()["reason"].as_str().unwrap();
        assert_eq!(
            reason,
            if mode == 1 {
                "\"retention_changed\""
            } else if mode <= 3 {
                "\"scope_changed\""
            } else {
                "\"cursor_changed\""
            }
        );
    }
}

#[tokio::test]
async fn repeated_expiry_without_progress_stops_and_other_gaps_never_renew() {
    let first = cursor(2);
    let second = renewed(&first, 0, 2);
    let source = fixture_source(
        vec![first, second],
        vec![
            Page::Gap(GapReason::SnapshotExpired),
            Page::Gap(GapReason::SnapshotExpired),
        ],
    );
    let (mut output, _) = fixture_output(None);
    assert!(output
        .drain_events_from(&source, &CommandId::new(), SessionSeq::ZERO)
        .await
        .is_err());
    assert_eq!(source.requests.borrow().len(), 2);
    for reason in [
        GapReason::ScopeChanged,
        GapReason::RetentionChanged,
        GapReason::SequenceUnavailable,
        GapReason::CursorChanged,
    ] {
        let source = fixture_source(vec![cursor(2)], vec![Page::Gap(reason)]);
        let (mut output, _) = fixture_output(None);
        assert!(output
            .drain_events_from(&source, &CommandId::new(), SessionSeq::ZERO)
            .await
            .is_err());
        assert_eq!(source.requests.borrow().len(), 1);
    }
}

#[tokio::test]
async fn mid_page_writer_failure_and_history_failure_do_not_retry_or_skip() {
    let source = fixture_source(vec![cursor(3)], vec![Page::Events(vec![1, 2, 3])]);
    let (mut output, frames) = fixture_output(Some(2));
    assert!(output
        .drain_events_from(&source, &CommandId::new(), SessionSeq::ZERO)
        .await
        .unwrap_err()
        .contains("fixture writer failed"));
    assert_eq!(sequences(&frames), vec![1]);
    assert!(output.failed);
    assert_eq!(source.requests.borrow().len(), 1);
    assert_eq!(source.released.borrow().len(), 1);
    let source = fixture_source(vec![cursor(3)], vec![Page::Failure]);
    let (mut output, frames) = fixture_output(None);
    assert_eq!(
        output
            .drain_events_from(&source, &CommandId::new(), SessionSeq::ZERO)
            .await
            .unwrap_err(),
        "authenticated history read failed"
    );
    assert!(sequences(&frames).is_empty());
    assert_eq!(source.requests.borrow().len(), 1);
}
