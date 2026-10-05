// SPDX-License-Identifier: Apache-2.0
//! Current ingestion records own the requirements; history arrives separately.
use super::*;

pub(crate) struct Inputs<'a> {
    current: crate::CurrentStateView<'a>,
    cursors: BTreeMap<CommandId, Cursor>,
    jobs: BTreeMap<CommandId, Job>,
}
impl<'a> Inputs<'a> {
    pub(crate) fn new(current: crate::CurrentStateView<'a>) -> Result<Self> {
        let mut cursors = BTreeMap::new();
        let mut jobs = BTreeMap::new();
        for record in current.records.values() {
            match kind(record)? {
                Some("vcp_ingestion_cursor_v1") => {
                    let cursor: Cursor = record.decode()?;
                    cursors.insert(cursor.id.clone(), cursor);
                }
                Some("vcp_ingestion_job_v1") => {
                    let job: Job = record.decode()?;
                    jobs.insert(job.id.clone(), job);
                }
                _ => (),
            }
        }
        Ok(Self {
            current,
            cursors,
            jobs,
        })
    }
    pub(crate) fn needs_history(&self) -> bool {
        !self.cursors.is_empty() || !self.jobs.is_empty()
    }
    pub(crate) fn history(&self, expected_count: usize) -> IngestionHistory<'_> {
        ingestion_history::IngestionHistory::new(
            self.current.records,
            &self.cursors,
            &self.jobs,
            expected_count,
        )
    }
    pub(crate) fn finish(&self, history: History) -> Result<()> {
        validate_history(self.current, &self.cursors, &self.jobs, history)
    }
}
