// SPDX-License-Identifier: Apache-2.0
//! Offline native visibility qualification, using registered recovered children.
//! This is not a live-provider usefulness or hard-close process-marker result.
use super::*;
use vcp_protocol::subscription::EventPage;

pub(super) async fn flood_and_recover_cursor(
    host: &CanonicalHost,
    config: &Config,
    root: codex_protocol::ThreadId,
    noisy: codex_protocol::ThreadId,
    noisy_task: &TaskId,
    quiet_task: &TaskId,
) {
    let original = host.snapshot().unwrap();
    let quiet: Task = original
        .record(Collection::Task, quiet_task.as_str(), &config.workspace)
        .unwrap()
        .decode()
        .unwrap();
    let before_accounting = original
        .records
        .values()
        .filter(|row| {
            matches!(
                row.collection,
                Collection::Attempt | Collection::Reservation | Collection::Ledger
            )
        })
        .map(|row| (row.id.clone(), row.value.clone()))
        .collect::<Vec<_>>();
    let frozen = host.subscribe_events(SessionSeq::ZERO, 3).unwrap();
    let cutoff = frozen.end;
    let mut published = Vec::new();
    for index in 0..48 {
        let thread = if index % 4 == 0 { root } else { noisy };
        let text = format!("attributed progress {index}: {}", "x".repeat(1024));
        published.push(
            host.capture(thread, Channel::ChildTranscript, text.into_bytes())
                .unwrap(),
        );
    }
    // Many chunks remain one retained artifact; rendering cannot force the
    // producer to enqueue an unbounded sequence of UI strings.
    let stream = host.open_output(noisy, Channel::ChildTranscript).unwrap();
    let chunk = vec![b'x'; 4096];
    for _ in 0..64 {
        stream.write(&chunk).unwrap();
    }
    let stream = stream.finish().unwrap();
    assert_eq!(
        host.read_artifact(stream.spec.id.clone()).unwrap(),
        vec![b'x'; 256 * 1024]
    );

    let mut cursor = frozen.clone();
    loop {
        let EventPage::Events {
            events,
            next_cursor,
            at_end,
            ..
        } = host.events(cursor).unwrap()
        else {
            panic!("live bounded snapshot unexpectedly lost");
        };
        assert!(events.len() <= 3);
        assert!(events.iter().all(|event| event.sequence <= cutoff));
        cursor = next_cursor;
        if at_end {
            break;
        }
    }
    // Simulate an abandoned UI cursor, then recover from its durable checkpoint.
    host.unsubscribe_events(frozen.snapshot).unwrap();
    assert!(matches!(
        host.events(cursor).unwrap(),
        EventPage::Gap {
            restart_from_snapshot: true,
            ..
        }
    ));
    let mut cursor = host.subscribe_events(cutoff, 7).unwrap();
    let snapshot = cursor.snapshot.clone();
    let mut seen = Vec::new();
    let mut referenced = std::collections::BTreeSet::new();
    let mut attributed = std::collections::BTreeSet::new();
    loop {
        let EventPage::Events {
            events,
            next_cursor,
            at_end,
            ..
        } = host.events(cursor).unwrap()
        else {
            panic!("fresh recovery cursor must replay retained evidence");
        };
        assert!(events.len() <= 7);
        for event in events {
            seen.push(event.sequence);
            referenced.extend(event.event.artifacts);
            attributed.extend(event.event.task);
        }
        cursor = next_cursor;
        if at_end {
            break;
        }
    }
    host.unsubscribe_events(snapshot).unwrap();
    assert!(seen
        .windows(2)
        .all(|pair| pair[0].get() + 1 == pair[1].get()));
    assert!(attributed.contains(&config.root_task) && attributed.contains(noisy_task));
    for artifact in published.iter().chain(std::iter::once(&stream)) {
        assert!(referenced.contains(&artifact.spec.id));
    }
    let final_state = host.snapshot().unwrap();
    let expected = final_state
        .events
        .iter()
        .filter(|event| event.event.session == config.session && event.sequence > cutoff)
        .map(|event| event.sequence)
        .collect::<Vec<_>>();
    assert_eq!(
        seen, expected,
        "flood or cursor loss must not skip a durable transition"
    );
    let after_quiet: Task = final_state
        .record(Collection::Task, quiet_task.as_str(), &config.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(after_quiet.state, quiet.state);
    assert_eq!(
        after_quiet.reason, quiet.reason,
        "quiet sibling's wait reason survives noisy output"
    );
    let after_accounting = final_state
        .records
        .values()
        .filter(|row| {
            matches!(
                row.collection,
                Collection::Attempt | Collection::Reservation | Collection::Ledger
            )
        })
        .map(|row| (row.id.clone(), row.value.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        before_accounting, after_accounting,
        "inspection/flood must not dispatch model work or consume its budget"
    );
}
