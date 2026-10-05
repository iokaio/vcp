// SPDX-License-Identifier: Apache-2.0
//! Exact held-file acquisition into the existing private restore operation.
use super::*;
use std::io::{Read, Write};

pub(super) fn acquire(
    root: &Path,
    source: &Path,
    status: &Status,
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let check = || {
        if cancelled() {
            Err(Error::Unavailable("restore cancelled"))
        } else {
            Ok(())
        }
    };
    check()?;
    let destination = root.join("ciphertext.age");
    if destination.exists() {
        verify(&destination, status, &check)?;
        return Ok(());
    }
    let mut input = private_paths::PublicCiphertext::open_stream(source, status.bytes)?;
    let pending = destination.with_extension(format!("{}.partial", TransactionId::new()));
    #[cfg(feature = "qualification")]
    if let Some(prefix_bytes) = PRIVATE_WRITE_FAULT.with(|fault| {
        fault
            .borrow()
            .as_ref()
            .and_then(|(target, prefix)| (target == &destination).then_some(*prefix))
    }) {
        let mut prefix = vec![0; prefix_bytes.min(65536)];
        let used = input.read(&mut prefix)?;
        private_paths::write_private(&pending, &prefix[..used])?;
        return Err(std::io::Error::from(std::io::ErrorKind::StorageFull).into());
    }
    private_paths::write_private_with(&pending, |file| {
        let mut buffer = [0; 65536];
        loop {
            check()?;
            let used = input.read(&mut buffer)?;
            if used == 0 {
                break;
            }
            file.write_all(&buffer[..used])?;
        }
        input.finish_identity(status.bytes, &status.ciphertext)?;
        check()
    })?;
    check()?;
    fs::hard_link(&pending, &destination)?;
    let _ = fs::remove_file(pending);
    Ok(())
}
pub(super) fn verify(path: &Path, status: &Status, check: &dyn Fn() -> Result<()>) -> Result<()> {
    let mut input = private_paths::PublicCiphertext::open_stream(path, status.bytes)?;
    let mut buffer = [0; 65536];
    loop {
        check()?;
        if input.read(&mut buffer)? == 0 {
            break;
        }
    }
    input.finish_identity(status.bytes, &status.ciphertext)
}
