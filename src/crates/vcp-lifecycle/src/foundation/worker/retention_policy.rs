// SPDX-License-Identifier: Apache-2.0
use super::*;
impl Context {
    pub(super) fn apply_startup_retention_policy(&mut self) -> Result<()> {
        if self.capture_admission_blocked() {
            return Ok(());
        }
        let access = self.memory_access();
        self.runtime
            .block_on(vcp_memory::retention_policy::resume_cleanup(
                self.engine.store_mut(),
                &access,
                now(),
            ))?;
        self.runtime
            .block_on(vcp_memory::retention_policy::run_due(
                self.engine.store_mut(),
                &access,
                now(),
            ))?;
        Ok(())
    }
}
