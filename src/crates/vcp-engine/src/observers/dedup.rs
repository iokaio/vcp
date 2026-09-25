// SPDX-License-Identifier: Apache-2.0
use super::{subscription::Input, Error, Result, VERSION};
/// Advancing delivery watermark and deadline are deliberately absent. Identical
/// evidence cannot repeatedly alert merely because another event was delivered.
pub fn key(input: &Input) -> Result<String> {
    input.validate()?;
    let bytes = vcp_protocol::canonical_bytes(&(
        VERSION,
        &input.root,
        &input.task,
        "verification_repeated",
        input.steering,
        input.task_revision,
        input.authority,
        input.deletion,
        &input.input_digest,
        &input.pattern_digest,
    ))
    .map_err(|_| Error::Invalid("identity encoding"))?;
    Ok(vcp_protocol::digest_bytes(&bytes))
}
