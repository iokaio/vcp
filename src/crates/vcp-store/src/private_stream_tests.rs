// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn streamed_owner_files_are_create_only_and_failed_producers_remove_only_their_output() {
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("existing");
    write_private(&existing, b"preserved").unwrap();
    let mut invoked = false;
    assert!(write_private_with(&existing, |_| {
        invoked = true;
        Ok(())
    })
    .is_err());
    assert!(!invoked);
    assert_eq!(fs::read(&existing).unwrap(), b"preserved");
    let failed = temp.path().join("failed");
    assert!(matches!(
        write_private_with(&failed, |file| {
            file.write_all(b"incomplete owned bytes")?;
            Err(Error::Unavailable("producer interrupted"))
        }),
        Err(Error::Unavailable("producer interrupted"))
    ));
    assert!(!failed.exists());
    assert_eq!(fs::read(&existing).unwrap(), b"preserved");
    let output = temp.path().join("complete");
    write_private_with(&output, |file| {
        for _ in 0..80 {
            file.write_all(&[0x71; 65536])?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(fs::metadata(&output).unwrap().len(), 80 * 65536);
    assert_eq!(fs::read(&output).unwrap(), vec![0x71; 80 * 65536]);
}
