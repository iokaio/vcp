// SPDX-License-Identifier: Apache-2.0
//! Explicit native fault observers, absent from production builds.
use super::{worker::Context, CanonicalHost};
use std::sync::Arc;
use vcp_domain::AttemptId;
use vcp_store::contract::State;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Point {
    BeforeTransport,
    BeforeSettlement,
}

pub(super) type Observer =
    Arc<dyn Fn(Point, &AttemptId, &State) -> Result<(), String> + Send + Sync>;

impl CanonicalHost {
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
            observer(point, attempt, self.engine.store().state())?;
        }
        Ok(())
    }
}
