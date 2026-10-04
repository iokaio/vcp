// SPDX-License-Identifier: Apache-2.0
//! The existing job's ciphertext publication, without a whole-object buffer.
use super::*;
use sha2::{Digest, Sha256};
use std::io::Read;

pub(super) fn persist_ciphertext(path: &Path, ciphertext: &mut FinalizedCiphertext) -> Result<()> {
    if path.exists() {
        let mut input = private_paths::PublicCiphertext::open(path, 65 * 1024 * 1024)?;
        let mut digest = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes = bytes
                .checked_add(count as u64)
                .ok_or(Error::Limit("snapshot ciphertext size"))?;
            digest.update(&buffer[..count]);
        }
        input.finish()?;
        if bytes == ciphertext.bytes() && format!("{:x}", digest.finalize()) == ciphertext.sha256()
        {
            return Ok(());
        }
        return Err(Error::Conflict("owned snapshot object differs"));
    }
    let temporary = path.with_extension(format!("{}.partial", TransactionId::new()));
    // The finalized capability checks its exact held input's length/hash while
    // copying. Only a complete, synced output reaches create-only publication.
    private_paths::write_private_with(&temporary, |file| ciphertext.copy_ciphertext(file))?;
    if fs::hard_link(&temporary, path).is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(Error::Conflict("snapshot object publication failed"));
    }
    // Match the existing job's local residue obligation after publication.
    let _ = fs::remove_file(&temporary);
    Ok(())
}
