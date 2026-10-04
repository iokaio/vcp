// SPDX-License-Identifier: Apache-2.0
//! Exact byte accounting for the current projection, independent of retained
//! historical payloads. Legacy State capacity remains a separate contract.
use crate::{contract::MAX_RECORDS, CurrentStateView, Error, Result};
use serde::Serialize;
use std::collections::BTreeSet;
use vcp_domain::Watermark;

pub(crate) const MAX_CURRENT_STATE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub(crate) struct CurrentSize {
    watermark: Watermark,
    bytes: usize,
}
impl CurrentSize {
    pub(crate) fn measure(current: CurrentStateView<'_>) -> Result<Self> {
        Ok(Self {
            watermark: current.watermark,
            bytes: count(&current)?,
        })
    }
    pub(crate) fn bytes(self) -> usize {
        self.bytes
    }
    pub(crate) fn validate(self, current: CurrentStateView<'_>) -> Result<()> {
        if self.watermark != current.watermark {
            return Err(Error::Corruption("current size watermark"));
        }
        if current.records.len() > MAX_RECORDS {
            return Err(Error::Limit("canonical record count"));
        }
        if self.bytes > MAX_CURRENT_STATE_BYTES {
            return Err(Error::Limit("current projection bytes"));
        }
        Ok(())
    }
    /// The shared mutation preparer supplies the exact touched-key set. Sequence
    /// maps are small current metadata and are counted directly; no history is read.
    pub(crate) fn next(
        self,
        prior: CurrentStateView<'_>,
        next: CurrentStateView<'_>,
        touched: &BTreeSet<String>,
    ) -> Result<Self> {
        if self.watermark != prior.watermark || next.watermark != prior.watermark.next()? {
            return Err(Error::Corruption("current size transition"));
        }
        let mut removed = count(&prior.watermark)?;
        let mut added = count(&next.watermark)?;
        checked_add(&mut removed, count(prior.sequences)?)?;
        checked_add(&mut added, count(next.sequences)?)?;
        checked_add(&mut removed, prior.records.len().saturating_sub(1))?;
        checked_add(&mut added, next.records.len().saturating_sub(1))?;
        for key in touched {
            if let Some(value) = prior.records.get(key) {
                checked_add(&mut removed, entry(key, value)?)?;
            }
            if let Some(value) = next.records.get(key) {
                checked_add(&mut added, entry(key, value)?)?;
            }
        }
        let bytes = self
            .bytes
            .checked_sub(removed)
            .and_then(|value| value.checked_add(added))
            .ok_or(Error::Corruption("current size arithmetic"))?;
        Ok(Self {
            watermark: next.watermark,
            bytes,
        })
    }
}
fn checked_add(total: &mut usize, value: usize) -> Result<()> {
    *total = total
        .checked_add(value)
        .ok_or(Error::Limit("current size overflow"))?;
    Ok(())
}
fn entry(key: &impl Serialize, value: &impl Serialize) -> Result<usize> {
    let mut bytes = count(key)?;
    checked_add(&mut bytes, 1)?;
    checked_add(&mut bytes, count(value)?)?;
    Ok(bytes)
}
fn count(value: &impl Serialize) -> Result<usize> {
    #[derive(Default)]
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("current size overflow"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter::default();
    serde_json::to_writer(&mut counter, value)?;
    Ok(counter.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        contract::{Collection, Record},
        CurrentState,
    };
    use std::collections::BTreeMap;
    use vcp_domain::{Revision, WorkspaceId};

    #[test]
    fn current_capacity_bounds_actual_records_without_archival_materialization() {
        let workspace = WorkspaceId::new();
        let mut records = BTreeMap::new();
        let payload = "x".repeat(1024 * 1024 - 2048);
        for index in 0..65 {
            let row = Record::typed(
                Collection::Projection,
                format!("capacity-{index}"),
                workspace.clone(),
                Revision::ZERO,
                &serde_json::json!({"schema_version":1,"payload":payload}),
            )
            .unwrap();
            row.validate_shape().unwrap();
            records.insert(row.key(), row);
            if index >= 63 {
                let view = CurrentStateView {
                    watermark: Watermark::ZERO,
                    records: &records,
                    sequences: &BTreeMap::new(),
                };
                let size = CurrentSize::measure(view).unwrap();
                assert_eq!(size.bytes() <= MAX_CURRENT_STATE_BYTES, index == 63);
                assert_eq!(size.validate(view).is_ok(), index == 63);
            }
        }
        let current = CurrentState::from_parts(Watermark::ZERO, records.into(), BTreeMap::new());
        let view = CurrentStateView::from(&current);
        let size = CurrentSize::measure(view).unwrap();
        assert!(matches!(
            size.validate(view),
            Err(Error::Limit("current projection bytes"))
        ));
        let wrong_cut = CurrentStateView {
            watermark: Watermark::ZERO.next().unwrap(),
            ..view
        };
        assert!(matches!(
            size.validate(wrong_cut),
            Err(Error::Corruption("current size watermark"))
        ));
    }
}
