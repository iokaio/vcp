// SPDX-License-Identifier: Apache-2.0
//! Immutable current-record snapshots, distinct from canonical history evidence.
use crate::{
    contract::{key, Collection, Record, State},
    Error, Result,
};
use serde::Serialize;
use std::collections::BTreeMap;
use vcp_domain::{SessionId, SessionSeq, Watermark, WorkspaceId};

/// A hot read projection. Its digest commits to these current records and
/// sequence watermarks only, never to historical events or retry receipts.
/// It cannot be supplied as a canonical replay base or completion capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CurrentState {
    schema_version: u32,
    pub watermark: Watermark,
    pub records: BTreeMap<String, Record>,
    pub sequences: BTreeMap<SessionId, SessionSeq>,
}
impl CurrentState {
    pub(crate) fn from_state(state: &State) -> Self {
        Self {
            schema_version: 1,
            watermark: state.watermark,
            records: state.records.clone(),
            sequences: state.sequences.clone(),
        }
    }
    pub fn record(
        &self,
        collection: Collection,
        id: &str,
        workspace: &WorkspaceId,
    ) -> Result<&Record> {
        let record = self
            .records
            .get(&key(collection, id))
            .ok_or(Error::Conflict("record not found"))?;
        if &record.workspace != workspace {
            return Err(Error::Access);
        }
        Ok(record)
    }
    /// Use the canonical collection-key prefix rather than scanning unrelated
    /// record collections. Workspace filtering still applies to every row.
    pub fn records_in<'a>(
        &'a self,
        collection: Collection,
        workspace: &'a WorkspaceId,
    ) -> impl Iterator<Item = &'a Record> + 'a {
        let lower = format!("{}:", collection.name());
        let upper = format!("{};", collection.name());
        self.records
            .range(lower..upper)
            .map(|(_, record)| record)
            .filter(move |record| &record.workspace == workspace)
    }
    /// Domain-separated by this type's schema field and exact serialized shape.
    pub fn projection_digest(&self) -> Result<String> {
        Ok(vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(
            self,
        )?))
    }
}
