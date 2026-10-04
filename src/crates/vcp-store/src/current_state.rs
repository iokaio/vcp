// SPDX-License-Identifier: Apache-2.0
//! Immutable current-record snapshots, distinct from canonical history evidence.
use crate::{
    contract::{key, Collection, Record, SharedStateValue, State},
    Error, Result,
};
use serde::{ser::SerializeStruct, Serialize, Serializer};
use std::collections::BTreeMap;
use vcp_domain::{SessionId, SessionSeq, Watermark, WorkspaceId};

/// Borrowed current-record input for domain checks. This view carries no
/// historical events or receipts and does not materialize an archival State.
#[derive(Clone, Copy, Debug)]
pub struct CurrentStateView<'a> {
    pub watermark: Watermark,
    pub records: &'a BTreeMap<String, Record>,
    pub sequences: &'a BTreeMap<SessionId, SessionSeq>,
}
// The borrowed view has exactly the same versioned projection encoding as the
// owned current snapshot. It never supplies legacy whole-State commitments.
impl Serialize for CurrentStateView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut value = serializer.serialize_struct("CurrentState", 4)?;
        value.serialize_field("schema_version", &1u32)?;
        value.serialize_field("watermark", &self.watermark)?;
        value.serialize_field("records", self.records)?;
        value.serialize_field("sequences", self.sequences)?;
        value.end()
    }
}
impl<'a> From<&'a State> for CurrentStateView<'a> {
    fn from(state: &'a State) -> Self {
        Self {
            watermark: state.watermark,
            records: &state.records,
            sequences: &state.sequences,
        }
    }
}
impl<'a> From<&'a CurrentState> for CurrentStateView<'a> {
    fn from(state: &'a CurrentState) -> Self {
        Self {
            watermark: state.watermark,
            records: &state.records,
            sequences: &state.sequences,
        }
    }
}
impl<'a> CurrentStateView<'a> {
    pub fn record(
        self,
        collection: Collection,
        id: &str,
        workspace: &WorkspaceId,
    ) -> Result<&'a Record> {
        let record = self
            .records
            .get(&key(collection, id))
            .ok_or(Error::Conflict("record not found"))?;
        if &record.workspace != workspace {
            return Err(Error::Access);
        }
        Ok(record)
    }
    pub fn records_in(
        self,
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
}

/// A hot read projection. Its digest commits to these current records and
/// sequence watermarks only, never to historical events or retry receipts.
/// It cannot be supplied as a canonical replay base or completion capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CurrentState {
    schema_version: u32,
    pub watermark: Watermark,
    pub records: SharedStateValue<BTreeMap<String, Record>>,
    pub sequences: BTreeMap<SessionId, SessionSeq>,
}
impl CurrentState {
    pub(crate) fn from_state(state: &State) -> Self {
        Self::from_parts(
            state.watermark,
            state.records.clone(),
            state.sequences.clone(),
        )
    }
    /// Build the current portion of a candidate. Construction alone confers no
    /// canonical validity; the preparation pipeline checks it before publication.
    pub(crate) fn from_parts(
        watermark: Watermark,
        records: SharedStateValue<BTreeMap<String, Record>>,
        sequences: BTreeMap<SessionId, SessionSeq>,
    ) -> Self {
        Self {
            schema_version: 1,
            watermark,
            records,
            sequences,
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
