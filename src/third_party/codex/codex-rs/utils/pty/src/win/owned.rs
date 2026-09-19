// SPDX-License-Identifier: Apache-2.0
// VCP addition: owned native adapter over the retained MIT ConPTY implementation.
use super::conpty::RawConPty;
use super::{JobObject, PsuedoCon, WinChild};
use crate::TerminalSize;
use portable_pty::{Child, cmdbuilder::CommandBuilder};
use std::os::windows::io::{AsRawHandle, BorrowedHandle};
use std::{collections::BTreeMap, ffi::OsString, fs::File, io, path::Path, sync::Arc};

pub fn owned_pty_supported() -> bool {
    PsuedoCon::owned_supported()
}
pub struct OwnedPty {
    pub input: File,
    pub output: File,
    pub console: Arc<PsuedoCon>,
    pub child: OwnedPtyChild,
}
pub struct OwnedPtyChild(WinChild);
impl OwnedPtyChild {
    pub fn id(&self) -> Option<u32> {
        self.0.process_id()
    }
    pub fn wait(&mut self) -> io::Result<i32> {
        self.0.wait().map(|s| s.exit_code() as i32)
    }
}
pub fn spawn_owned_pty(
    executable: &Path,
    arguments: &[OsString],
    directory: &Path,
    environment: &BTreeMap<OsString, OsString>,
    size: TerminalSize,
    job: Arc<JobObject>,
) -> anyhow::Result<OwnedPty> {
    anyhow::ensure!(
        owned_pty_supported(),
        "owned PTY requires ReleasePseudoConsole support"
    );
    anyhow::ensure!(
        executable.is_absolute() && directory.is_absolute(),
        "absolute PTY paths required"
    );
    anyhow::ensure!(
        (1..=500).contains(&size.rows) && (1..=500).contains(&size.cols),
        "PTY size bounds"
    );
    let raw = RawConPty::new(size.cols as i16, size.rows as i16)?;
    let (mut console, input, output) = raw.into_handles();
    // Duplicate as standard owned files; source descriptors are then released.
    let input: File = unsafe { BorrowedHandle::borrow_raw(input.as_raw_handle()) }
        .try_clone_to_owned()?
        .into();
    let output: File = unsafe { BorrowedHandle::borrow_raw(output.as_raw_handle()) }
        .try_clone_to_owned()?
        .into();
    let mut command = CommandBuilder::new(executable);
    command.cwd(directory);
    command.env_clear();
    command.args(arguments);
    for (key, value) in environment {
        command.env(key, value);
    }
    let child = console.spawn_in_job(command, job, true)?;
    Ok(OwnedPty {
        input,
        output,
        console: Arc::new(console),
        child: OwnedPtyChild(child),
    })
}
