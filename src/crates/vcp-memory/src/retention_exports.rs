// SPDX-License-Identifier: Apache-2.0
//! Export copies follow immutable canonical source proof through later changes.
use crate::{retention::Target, Result};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::WorkspaceId;
use vcp_store::{
    contract::{key, Collection},
    Store,
};

pub(super) fn extend(
    store: &Store,
    workspace: &WorkspaceId,
    dependencies: &mut BTreeMap<Target, BTreeSet<Target>>,
) -> Result<()> {
    for export in vcp_store::export_contract::retention_dependencies(store.state(), workspace)? {
        let sources: BTreeSet<_> = export
            .records
            .into_iter()
            .map(Target::Record)
            .chain(export.events.into_iter().map(Target::Event))
            .collect();
        for artifact in export.artifacts {
            dependencies
                .entry(Target::Record(key(Collection::Artifact, artifact.as_str())))
                .or_default()
                .extend(sources.iter().cloned());
        }
    }
    Ok(())
}
