// SPDX-License-Identifier: Apache-2.0
//! Connection-owned pull projections. No background producer or raw fact queue.
use super::public_connection::PublicConnection;
use super::*;
use std::collections::BTreeMap;
use vcp_engine::{
    snapshot::{RestartReason, SnapshotCursor, SnapshotError},
    ProjectedEvents,
};
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    methods::{self, Call, ResultValue},
    subscription::{Cursor, GapReason},
};

const MAX_SUBSCRIPTIONS: usize = 8;
const MAX_BYTES: usize = 256 * 1024;

#[derive(Default)]
pub(super) struct Subscriptions(BTreeMap<String, Subscription>);
struct Subscription {
    cursor: Cursor,
    event_token: String,
    snapshot: Option<(String, SnapshotCursor)>,
    // Exactly one response is retained for an immediate same-token retry.
    cached: Option<(String, ResultValue)>,
}
fn error(code: Code, explanation: &str) -> RpcError {
    ApplicationError {
        code,
        retry: Retry::AfterRevalidation,
        operation: None,
        explanation: explanation.into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn unavailable() -> RpcError {
    error(Code::StoreUnavailable, "canonical observation unavailable")
}
fn denied() -> RpcError {
    error(Code::PolicyDenied, "current observation scope denied")
}
fn id(value: &str) -> std::result::Result<methods::Id, RpcError> {
    value
        .to_owned()
        .try_into()
        .map_err(|_| RpcError::invalid_params())
}
fn token(subscription: &str) -> String {
    format!("{subscription}.{}", SnapshotId::new())
}
fn public_gap(reason: GapReason) -> methods::GapReason {
    match reason {
        GapReason::SnapshotExpired => methods::GapReason::CursorExpired,
        GapReason::ScopeChanged => methods::GapReason::AuthorityChanged,
        GapReason::RetentionChanged => methods::GapReason::RetentionChanged,
        GapReason::SequenceUnavailable | GapReason::CursorChanged => {
            methods::GapReason::SequenceUnavailable
        }
    }
}
fn gap(
    context: &Context,
    access: &Access,
    subscription: &str,
    reason: methods::GapReason,
) -> std::result::Result<ResultValue, RpcError> {
    Ok(ResultValue::Gap(methods::EventGap {
        subscription: id(subscription)?,
        reason,
        snapshot_sequence: context
            .engine
            .store()
            .state()
            .sequences
            .get(&access.session)
            .copied()
            .unwrap_or_default()
            .get()
            .into(),
        resubscribe_required: true,
    }))
}
impl Subscriptions {
    fn remove(&mut self, context: &mut Context, subscription: &str) {
        if let Some(owned) = self.0.remove(subscription) {
            context.engine.unsubscribe(&owned.cursor.snapshot);
        }
    }
    fn prune(&mut self, context: &mut Context, now: Timestamp) {
        let expired: Vec<_> = self
            .0
            .iter()
            .filter(|(_, owned)| now >= owned.cursor.expires_at)
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            self.remove(context, &id);
        }
    }
    fn room(&self) -> std::result::Result<(), RpcError> {
        if self.0.len() >= MAX_SUBSCRIPTIONS {
            Err(error(Code::ResourceLimit, "connection subscription limit"))
        } else {
            Ok(())
        }
    }
}

impl PublicConnection {
    pub(super) fn event_call(
        &mut self,
        call: Call,
        current: &Access,
    ) -> std::result::Result<ResultValue, RpcError> {
        call.validate().map_err(|_| RpcError::invalid_params())?;
        let (host, access, connection, _) = self.rpc_context(current).map_err(|_| denied())?;
        let scope = match &call {
            Call::SessionSnapshot(p) => &p.scope,
            Call::EventsSubscribe(p) => &p.scope,
            Call::EventsNext(p) => &p.scope,
            Call::EventsUnsubscribe(p) => &p.scope,
            _ => return Err(RpcError::invalid_params()),
        };
        if scope.workspace.as_str() != access.workspace.as_str()
            || scope.session.as_str() != access.session.as_str()
        {
            return Err(denied());
        }
        let subscriptions = self.subscriptions.clone();
        let connected = self.connected.clone();
        host.worker
            .run_cleanup(move |context| {
                Ok((|| {
                    if !connected.load(Ordering::SeqCst) {
                        return Err(denied());
                    }
                    context
                        .public_authorize(&access, &connection, None, false)
                        .map_err(|_| denied())?;
                    let mut subscriptions = subscriptions.lock().map_err(|_| unavailable())?;
                    let now = now();
                    subscriptions.prune(context, now);
                    let result = dispatch(context, &access, &mut subscriptions, call, now)?;
                    if vcp_protocol::jsonrpc::encode_frame(&result, MAX_BYTES).is_err() {
                        return Err(error(Code::ResourceLimit, "subscriber response byte limit"));
                    }
                    Ok(result)
                })())
            })
            .map_err(|_| unavailable())?
    }

    pub(super) fn clear_public_events(&self) {
        let ids = match self.subscriptions.lock() {
            Ok(mut owned) => std::mem::take(&mut owned.0)
                .into_values()
                .map(|subscription| subscription.cursor.snapshot)
                .collect::<Vec<_>>(),
            Err(_) => return, // Engine registrations still have their bounded TTL.
        };
        if ids.is_empty() {
            return;
        }
        let job: Job = Box::new(move |context| {
            for id in ids {
                context.engine.unsubscribe(&id);
            }
        });
        let Some(mut job) = enqueue_cleanup(&self.host.worker, job) else {
            return;
        };
        let worker = self.host.worker.clone();
        // Closing an observer cannot fence the canonical writer because its
        // cleanup queue is full. At most the global cursor quota can own these
        // jobs; TTL reclamation remains authoritative if shutdown wins.
        drop(self.reactor.spawn(async move {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
            loop {
                if tokio::time::Instant::now() >= deadline {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
                match enqueue_cleanup(&worker, job) {
                    None => return,
                    Some(pending) => job = pending,
                }
            }
        }));
    }
}

fn enqueue_cleanup(worker: &Worker, job: Job) -> Option<Job> {
    let Ok(sender) = worker.0.sender.lock() else {
        return None;
    };
    let Some(sender) = sender.as_ref() else {
        return None;
    };
    match sender.try_send(job) {
        Ok(()) | Err(std::sync::mpsc::TrySendError::Disconnected(_)) => None,
        Err(std::sync::mpsc::TrySendError::Full(job)) => Some(job),
    }
}

fn dispatch(
    context: &mut Context,
    access: &Access,
    subscriptions: &mut Subscriptions,
    call: Call,
    now: Timestamp,
) -> std::result::Result<ResultValue, RpcError> {
    match call {
        Call::EventsUnsubscribe(request) => {
            subscriptions.remove(context, request.subscription.as_str());
            Ok(ResultValue::Unsubscribed {
                subscription: request.subscription,
            })
        }
        Call::EventsSubscribe(request) => {
            subscriptions.room()?;
            let after = SessionSeq::new(
                request
                    .after_sequence
                    .as_str()
                    .parse()
                    .map_err(|_| RpcError::invalid_params())?,
            );
            let cursor = context
                .engine
                .subscribe(access, after, request.limit, now)
                .map_err(engine_error)?;
            let subscription = cursor.snapshot.as_str().to_owned();
            let mut owned = Subscription {
                cursor,
                event_token: token(&subscription),
                snapshot: None,
                cached: None,
            };
            let result = page(context, access, &subscription, &mut owned, now)?;
            if matches!(result, ResultValue::Gap(_)) {
                context.engine.unsubscribe(&owned.cursor.snapshot);
            } else {
                subscriptions.0.insert(subscription, owned);
            }
            Ok(result)
        }
        Call::EventsNext(request) => {
            let subscription = request.subscription.as_str();
            let Some(owned) = subscriptions.0.get_mut(subscription) else {
                return gap(
                    context,
                    access,
                    subscription,
                    methods::GapReason::CursorExpired,
                );
            };
            if let Some(reason) = context
                .engine
                .event_cursor_gap(access, &owned.cursor, now)
                .map_err(engine_error)?
            {
                let result = gap(context, access, subscription, public_gap(reason));
                subscriptions.remove(context, subscription);
                return result;
            }
            if let Some((previous, result)) = &owned.cached {
                if previous == &request.cursor && matches!(result, ResultValue::Events(_)) {
                    return Ok(result.clone());
                }
            }
            if owned.snapshot.is_some() {
                return Err(error(
                    Code::VersionConflict,
                    "complete or discard snapshot pagination before replay",
                ));
            }
            if request.cursor != owned.event_token {
                let result = gap(
                    context,
                    access,
                    subscription,
                    methods::GapReason::SequenceUnavailable,
                );
                subscriptions.remove(context, subscription);
                return result;
            }
            if owned.cursor.after == owned.cursor.end {
                owned.cursor = context
                    .engine
                    .advance_event_window(access, &owned.cursor, now)
                    .map_err(engine_error)?;
            }
            let result = page(context, access, subscription, owned, now)?;
            if matches!(result, ResultValue::Gap(_)) {
                subscriptions.remove(context, subscription);
            } else {
                owned.cached = Some((request.cursor, result.clone()));
            }
            Ok(result)
        }
        Call::SessionSnapshot(request) => {
            if let Some(requested) = request.cursor {
                let subscription = requested
                    .split_once('.')
                    .map(|(id, _)| id)
                    .ok_or_else(RpcError::invalid_params)?;
                let Some(owned) = subscriptions.0.get_mut(subscription) else {
                    return gap(
                        context,
                        access,
                        subscription,
                        methods::GapReason::CursorExpired,
                    );
                };
                if let Some(reason) = context
                    .engine
                    .event_cursor_gap(access, &owned.cursor, now)
                    .map_err(engine_error)?
                {
                    let result = gap(context, access, subscription, public_gap(reason));
                    subscriptions.remove(context, subscription);
                    return result;
                }
                if request.limit != owned.cursor.limit {
                    return Err(RpcError::invalid_params());
                }
                if let Some((previous, result)) = &owned.cached {
                    if previous == &requested && matches!(result, ResultValue::Snapshot(_)) {
                        return Ok(result.clone());
                    }
                }
                let Some((expected, native)) = &owned.snapshot else {
                    return Err(RpcError::invalid_params());
                };
                if expected != &requested {
                    return Err(RpcError::invalid_params());
                }
                let result =
                    match context
                        .engine
                        .snapshot_page(access, request.limit, Some(native), now)
                    {
                        Ok(mut snapshot) => {
                            bind_snapshot(&mut snapshot, subscription, owned)?;
                            ResultValue::Snapshot(snapshot)
                        }
                        Err(SnapshotError::Restart { reason, .. }) => {
                            let result = gap(
                                context,
                                access,
                                subscription,
                                match reason {
                                    RestartReason::RetentionChanged => {
                                        methods::GapReason::RetentionChanged
                                    }
                                    RestartReason::CursorExpired => {
                                        methods::GapReason::CursorExpired
                                    }
                                    RestartReason::CursorChanged | RestartReason::SourceChanged => {
                                        methods::GapReason::SequenceUnavailable
                                    }
                                },
                            );
                            subscriptions.remove(context, subscription);
                            return result;
                        }
                        Err(error) => return Err(snapshot_error(error)),
                    };
                owned.cached = Some((requested, result.clone()));
                Ok(result)
            } else {
                subscriptions.room()?;
                let mut snapshot = context
                    .engine
                    .snapshot_page(access, request.limit, None, now)
                    .map_err(snapshot_error)?;
                let cursor: Cursor =
                    serde_json::from_str(&snapshot.event_cursor).map_err(|_| unavailable())?;
                let subscription = cursor.snapshot.as_str().to_owned();
                let mut owned = Subscription {
                    cursor,
                    event_token: token(&subscription),
                    snapshot: None,
                    cached: None,
                };
                bind_snapshot(&mut snapshot, &subscription, &mut owned)?;
                subscriptions.0.insert(subscription, owned);
                Ok(ResultValue::Snapshot(snapshot))
            }
        }
        _ => Err(RpcError::invalid_params()),
    }
}

fn bind_snapshot(
    snapshot: &mut methods::SessionSnapshot,
    subscription: &str,
    owned: &mut Subscription,
) -> std::result::Result<(), RpcError> {
    owned.snapshot = snapshot
        .next_cursor
        .as_ref()
        .map(|cursor| {
            serde_json::from_str::<SnapshotCursor>(cursor)
                .map(|native| (token(subscription), native))
        })
        .transpose()
        .map_err(|_| unavailable())?;
    snapshot.subscription = id(subscription)?;
    snapshot.event_cursor = owned.event_token.clone();
    snapshot.next_cursor = owned.snapshot.as_ref().map(|(token, _)| token.clone());
    Ok(())
}

fn page(
    context: &Context,
    access: &Access,
    subscription: &str,
    owned: &mut Subscription,
    now: Timestamp,
) -> std::result::Result<ResultValue, RpcError> {
    match context
        .engine
        .projected_events(access, &owned.cursor, now, MAX_BYTES)
        .map_err(engine_error)?
    {
        ProjectedEvents::Gap(reason) => gap(context, access, subscription, public_gap(reason)),
        ProjectedEvents::TooLarge => gap(
            context,
            access,
            subscription,
            methods::GapReason::SlowConsumer,
        ),
        ProjectedEvents::Page {
            events,
            next,
            at_end,
        } => {
            owned.cursor = next;
            owned.event_token = token(subscription);
            Ok(ResultValue::Events(methods::EventBatch {
                subscription: id(subscription)?,
                snapshot_sequence: owned.cursor.end.get().into(),
                cursor: owned.event_token.clone(),
                events,
                at_end,
            }))
        }
    }
}
fn engine_error(failure: vcp_engine::Error) -> RpcError {
    match failure {
        vcp_engine::Error::Access => denied(),
        vcp_engine::Error::Protocol(_) => {
            error(Code::ResourceLimit, "subscription capacity or page limit")
        }
        vcp_engine::Error::Target => RpcError::invalid_params(),
        _ => unavailable(),
    }
}
fn snapshot_error(failure: SnapshotError) -> RpcError {
    match failure {
        SnapshotError::Access => denied(),
        SnapshotError::Limit => error(Code::ResourceLimit, "snapshot capacity or page limit"),
        SnapshotError::Restart { .. } => {
            error(Code::CursorGap, "discard partial snapshot and restart")
        }
        SnapshotError::InvalidData => unavailable(),
    }
}
