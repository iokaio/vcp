// SPDX-License-Identifier: Apache-2.0
//! Output consumer lifetime is part of the canonical owner's lifetime.
use crate::jsonl::{Jsonl, Payload};
use std::{
    io::{self, Write},
    sync::mpsc,
    time::Duration,
};
use tokio::sync::oneshot;
use vcp_domain::{ids::CommandId, revision::SessionSeq, workspace::Scope};
use vcp_lifecycle::foundation::{CanonicalHost, CanonicalOwner};
use vcp_protocol::subscription::EventPage;

const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
type Frame = (Vec<u8>, oneshot::Sender<io::Result<()>>);

/// A blocked OS pipe must not block interruption or canonical storage. The
/// writer owns only bytes, never an owner/host capability. There is at most one
/// queued frame and the caller awaits its acknowledgement before producing more.
pub struct OwnedJsonl {
    stream: Jsonl<Vec<u8>>,
    sender: mpsc::SyncSender<Frame>,
    owner: Option<CanonicalOwner>,
    failed: bool,
    finishing: bool,
}

impl OwnedJsonl {
    /// Emit a finite durable snapshot, then release its cursor. A producer can
    /// keep committing while this consumer is slow without growing a UI queue.
    pub async fn drain_events(
        &mut self,
        host: &CanonicalHost,
        correlation: &CommandId,
        after: SessionSeq,
    ) -> Result<SessionSeq, String> {
        let mut cursor = host.subscribe_events(after, 32)?;
        let snapshot = cursor.snapshot.clone();
        let result = async {
            loop {
                match host.events(cursor.clone())? {
                    EventPage::Events {
                        events,
                        next_cursor,
                        at_end,
                        ..
                    } => {
                        for event in &events {
                            let scope = event.event.task.as_ref().map(|task| Scope {
                                workspace: event.event.workspace.clone(),
                                session: event.event.session.clone(),
                                task: task.clone(),
                            });
                            self.emit(correlation, scope.as_ref(), Payload::Event { event })
                                .await?;
                        }
                        cursor = next_cursor;
                        if at_end {
                            return Ok(cursor.after);
                        }
                    }
                    EventPage::Gap { reason, .. } => {
                        let reason = serde_json::to_string(&reason).map_err(|e| e.to_string())?;
                        self.emit(correlation, None, Payload::CursorGap { reason: &reason })
                            .await?;
                        return Err("durable event cursor must be restarted from a snapshot".into());
                    }
                }
            }
        }
        .await;
        let released = host.unsubscribe_events(snapshot);
        match result {
            Ok(after) => {
                released?;
                Ok(after)
            }
            Err(error) => Err(error),
        }
    }

    pub fn new<W: Write + Send + 'static>(output: W, owner: CanonicalOwner) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Frame>(1);
        std::thread::Builder::new()
            .name("vcp-cli-output".into())
            .spawn(move || {
                let mut output = output;
                while let Ok((frame, reply)) = receiver.recv() {
                    let result = output.write_all(&frame).and_then(|()| output.flush());
                    let failed = result.is_err();
                    let _ = reply.send(result);
                    if failed {
                        break;
                    }
                }
            })?;
        Ok(Self {
            stream: Jsonl::new(Vec::new()),
            sender,
            owner: Some(owner),
            failed: false,
            finishing: false,
        })
    }

    pub async fn emit(
        &mut self,
        correlation: &CommandId,
        scope: Option<&Scope>,
        payload: Payload<'_>,
    ) -> Result<(), String> {
        if self.failed || (self.owner.is_none() && !self.finishing) {
            return Err("output owner is closed".into());
        }
        let result = async {
            self.stream
                .emit(correlation, scope, payload)
                .map_err(|e| e.to_string())?;
            let (reply, acknowledged) = oneshot::channel();
            self.sender
                .try_send((self.stream.take_frame(), reply))
                .map_err(|_| "output consumer unavailable".to_owned())?;
            tokio::time::timeout(WRITE_TIMEOUT, acknowledged)
                .await
                .map_err(|_| "output consumer timed out".to_owned())?
                .map_err(|_| "output consumer stopped".to_owned())?
                .map_err(|e| e.to_string())
        }
        .await;
        if let Err(error) = result {
            self.failed = true;
            // Close invokes the existing durable owner-loss policy. It cannot
            // be bypassed by a broken pipe, partial write, or serialization error.
            if let Err(close) = self.close().await {
                return Err(format!("{error}; owner shutdown: {close}"));
            }
            return Err(error);
        }
        Ok(())
    }

    pub async fn close(&mut self) -> Result<(), String> {
        if let Some(owner) = self.owner.take() {
            owner.close().await?;
        }
        Ok(())
    }

    /// Stop and reconcile the owner before selecting a final outcome. The
    /// output worker remains usable for this one final drain; it owns no live
    /// execution authority. A failed final write cannot restart the owner.
    pub async fn finish(
        &mut self,
        host: &CanonicalHost,
        correlation: &CommandId,
        scope: &Scope,
        after: SessionSeq,
    ) -> Result<u8, String> {
        self.finish_with_error(host, correlation, scope, after, false)
            .await
    }

    pub async fn finish_with_error(
        &mut self,
        host: &CanonicalHost,
        correlation: &CommandId,
        scope: &Scope,
        after: SessionSeq,
        internal_failure: bool,
    ) -> Result<u8, String> {
        if self.failed || self.owner.is_none() || self.finishing {
            return Err("output owner is closed".into());
        }
        self.close().await?;
        self.finishing = true;
        let result = async {
            let mut outcome = crate::outcome::Outcome::read(host, scope)?;
            outcome.conditions.internal_failure |= internal_failure;
            self.drain_events(host, correlation, after).await?;
            for approval in &outcome.approvals {
                self.emit(
                    correlation,
                    Some(&approval.scope),
                    Payload::RequiredInput {
                        approval: &approval.id,
                    },
                )
                .await?;
            }
            let exit_code = outcome.conditions.code();
            self.emit(
                correlation,
                Some(scope),
                Payload::Result {
                    conditions: &outcome.conditions,
                    exit_code,
                    receipt: Some(&outcome.receipt),
                },
            )
            .await?;
            Ok(exit_code)
        }
        .await;
        self.finishing = false;
        result
    }
}
