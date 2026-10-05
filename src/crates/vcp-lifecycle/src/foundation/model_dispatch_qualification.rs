// SPDX-License-Identifier: Apache-2.0
//! Explicit native qualification probes, absent from production builds.
use super::{worker::Context, CanonicalHost};
use std::sync::Arc;
use vcp_domain::AttemptId;
use vcp_store::contract::State;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Point {
    BeforeSendIntent,
    BeforeTransport,
    BeforeSettlement,
}

pub(super) type Observer =
    Arc<dyn Fn(Point, &AttemptId, &State) -> Result<(), String> + Send + Sync>;

impl CanonicalHost {
    /// Inspect the next allocation using the production selector without
    /// preparing context, reserving funds, or granting a provider send.
    pub fn qualification_coding_allocation(
        &self,
        thread: codex_protocol::ThreadId,
        snapshot: vcp_models::catalog::Snapshot,
    ) -> Result<vcp_domain::request_allocation::Allocation, String> {
        let binding = self.binding(thread)?;
        self.worker
            .run(move |context| context.coding_request_allocation(&binding, &snapshot))
    }

    /// Runs synchronously on the canonical worker. The observer must not reenter
    /// this host; its snapshot describes the exact durable boundary observed.
    pub fn qualification_observe_model_dispatch(
        &self,
        observer: impl Fn(Point, &AttemptId, &State) -> Result<(), String> + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.worker.run(move |context| {
            context.model_dispatch_observer = Some(Arc::new(observer));
            Ok(())
        })
    }
}

impl Context {
    pub(super) fn qualification_model_dispatch_point(
        &self,
        point: Point,
        attempt: &AttemptId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(observer) = &self.model_dispatch_observer {
            let archive = self.runtime.block_on(self.engine.store().archive_state())?;
            observer(point, attempt, &archive)?;
        }
        Ok(())
    }
}
