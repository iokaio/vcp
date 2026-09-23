// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_engine::{rpc::RpcHost, Access};
use vcp_protocol::{
    command::Command,
    methods::{self, Call, ResultValue},
};

fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}
fn access(config: &Config) -> Access {
    Access {
        actor: config.actor.clone(),
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: false,
        bootstrap: false,
    }
}
fn scope(config: &Config) -> methods::Scope {
    methods::Scope {
        workspace: id(config.workspace.as_str()),
        session: id(config.session.as_str()),
    }
}
fn subscribe(config: &Config) -> Call {
    Call::EventsSubscribe(methods::EventsSubscribe {
        scope: scope(config),
        after_sequence: 0.into(),
        limit: 128,
    })
}
fn next(config: &Config, subscription: methods::Id, cursor: String) -> Call {
    Call::EventsNext(methods::EventsNext {
        scope: scope(config),
        subscription,
        cursor,
    })
}
fn inspect(host: &CanonicalHost) {
    host.command(Command::Inspect, None, Revision::ZERO)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn snapshot_replay_and_rolling_same_cursor_retry_do_not_lose_commits() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let access = access(&config);
        let mut observer = host.public_connection(access.clone()).unwrap();
        let before = host.snapshot().unwrap();
        let ResultValue::Snapshot(snapshot) = observer
            .call(
                Call::SessionSnapshot(methods::SessionSnapshotRead {
                    scope: scope(&config),
                    limit: 128,
                    cursor: None,
                }),
                &access,
            )
            .await
            .unwrap()
        else {
            panic!("snapshot");
        };
        assert!(snapshot.complete);
        assert_eq!(host.snapshot().unwrap(), before);
        let s: u64 = snapshot.sequence.as_str().parse().unwrap();
        inspect(&host);
        let request = next(
            &config,
            snapshot.subscription.clone(),
            snapshot.event_cursor,
        );
        let ResultValue::Events(first) = observer.call(request.clone(), &access).await.unwrap()
        else {
            panic!("events");
        };
        assert_eq!(first.events.len(), 1);
        assert_eq!(first.events[0].sequence.as_str(), (s + 1).to_string());
        assert!(first.at_end);
        assert!(first.events[0].outcome.is_none());
        let serialized = serde_json::to_string(&first).unwrap();
        assert!(!serialized.contains("\"data\""));
        assert!(!serialized.contains("\"actor\""));
        inspect(&host);
        assert_eq!(
            observer.call(request, &access).await.unwrap(),
            ResultValue::Events(first.clone())
        );
        let ResultValue::Events(second) = observer
            .call(
                next(&config, first.subscription.clone(), first.cursor),
                &access,
            )
            .await
            .unwrap()
        else {
            panic!("events");
        };
        assert_eq!(second.events.len(), 1);
        assert_eq!(second.events[0].sequence.as_str(), (s + 2).to_string());
        let state = host.snapshot().unwrap();
        observer
            .call(
                Call::EventsUnsubscribe(methods::EventsUnsubscribe {
                    scope: scope(&config),
                    subscription: second.subscription.clone(),
                }),
                &access,
            )
            .await
            .unwrap();
        assert!(matches!(
            observer
                .call(next(&config, second.subscription, second.cursor), &access)
                .await
                .unwrap(),
            ResultValue::Gap(methods::EventGap {
                reason: methods::GapReason::CursorExpired,
                ..
            })
        ));
        assert_eq!(host.snapshot().unwrap(), state);
        observer.disconnect().unwrap().wait().await.unwrap();
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn subscriber_capacity_and_abandoned_consumers_do_not_block_writes_or_steal_cursors() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let access = access(&config);
        let mut one = host.public_connection(access.clone()).unwrap();
        let mut two = host.public_connection(access.clone()).unwrap();
        let mut three = host.public_connection(access.clone()).unwrap();
        let mut first = None;
        for _ in 0..8 {
            let ResultValue::Events(page) = one.call(subscribe(&config), &access).await.unwrap()
            else {
                panic!("events");
            };
            first.get_or_insert(page);
            two.call(subscribe(&config), &access).await.unwrap();
        }
        assert!(one.call(subscribe(&config), &access).await.is_err());
        assert!(three.call(subscribe(&config), &access).await.is_err());
        let first = first.unwrap();
        assert!(matches!(
            three
                .call(
                    next(&config, first.subscription.clone(), first.cursor.clone()),
                    &access
                )
                .await
                .unwrap(),
            ResultValue::Gap(_)
        ));
        three
            .call(
                Call::EventsUnsubscribe(methods::EventsUnsubscribe {
                    scope: scope(&config),
                    subscription: first.subscription.clone(),
                }),
                &access,
            )
            .await
            .unwrap();
        inspect(&host); // Neither a producer queue nor subscriber capacity blocks the writer.
        let ResultValue::Events(page) = one
            .call(next(&config, first.subscription, first.cursor), &access)
            .await
            .unwrap()
        else {
            panic!("owner cursor was stolen");
        };
        assert_eq!(page.events.len(), 1);
        drop(one); // Drop owns cleanup even for an abandoned observer.
        assert!(three.call(subscribe(&config), &access).await.is_ok());
        assert!(host
            .snapshot()
            .unwrap()
            .records
            .values()
            .all(|row| row.collection != vcp_store::contract::Collection::Task));
        two.disconnect().unwrap().wait().await.unwrap();
        three.disconnect().unwrap().wait().await.unwrap();
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cached_observations_reauthorize_and_authority_changes_require_a_fresh_snapshot() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let current = access(&config);
        let mut observer = host.public_connection(current.clone()).unwrap();
        let ResultValue::Events(first) = observer.call(subscribe(&config), &current).await.unwrap()
        else {
            panic!("events");
        };
        let request = next(&config, first.subscription, first.cursor);
        observer.call(request.clone(), &current).await.unwrap();
        let mut denied = current.clone();
        denied.read = false;
        assert!(observer.call(request.clone(), &denied).await.is_err());
        let mut other_scope = request.clone();
        if let Call::EventsNext(value) = &mut other_scope {
            value.scope.session = id("another-session");
        }
        assert!(observer.call(other_scope, &current).await.is_err());
        host.command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        assert!(observer.call(request.clone(), &current).await.is_err());
        let mut refreshed = current.clone();
        refreshed.authority = AuthorityRevision::new(1);
        assert!(matches!(
            observer.call(request, &refreshed).await.unwrap(),
            ResultValue::Gap(methods::EventGap {
                reason: methods::GapReason::AuthorityChanged,
                ..
            })
        ));
        assert!(matches!(
            observer
                .call(
                    Call::SessionSnapshot(methods::SessionSnapshotRead {
                        scope: scope(&config),
                        limit: 128,
                        cursor: None
                    }),
                    &refreshed
                )
                .await
                .unwrap(),
            ResultValue::Snapshot(_)
        ));
        observer.disconnect().unwrap().wait().await.unwrap();
        owner.close().await.unwrap();
    }
}
