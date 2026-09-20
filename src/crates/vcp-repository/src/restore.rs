// SPDX-License-Identifier: Apache-2.0
//! Native whole-directory publication for explicitly admitted workspace restore.
//! No destination is overwritten and no failure triggers recursive cleanup.
use crate::{
    path::{native, HeldPath},
    Error, Result, Root,
};
use std::{
    fs::{self, File, OpenOptions},
    os::windows::fs::OpenOptionsExt,
    path::Path,
};
use windows_sys::Win32::Storage::FileSystem::*;

pub struct Staging {
    parent: Root,
    root: Root,
    stage: HeldPath,
    _parent: HeldPath,
}
/// The destination namespace remains pinned until the consumer has completed
/// its own revision-checked binding/activation protocol.
pub struct Published {
    root: Root,
    held: HeldPath,
    _parent: HeldPath,
}
fn component(name: &str) -> Result<()> {
    if crate::path::relative(Path::new(name))? != name || name.contains('/') || name.contains('\\')
    {
        return Err(Error::Scope(
            "one normal directory component required".into(),
        ));
    }
    Ok(())
}
fn opaque(name: &str) -> Result<()> {
    component(name)?;
    if name.len() != 36
        || name.bytes().enumerate().any(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b != b'-'
            } else {
                !(b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            }
        })
    {
        return Err(Error::Scope("generated staging UUID required".into()));
    }
    Ok(())
}
fn deleting_directory(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .access_mode(DELETE | FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let info = native::info(&file)?;
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(Error::Link);
    }
    if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
        return Err(Error::Scope("staging directory required".into()));
    }
    Ok(file)
}
impl Staging {
    pub fn create(parent: &Root, name: &str) -> Result<Self> {
        opaque(name)?;
        let held = parent.hold(None, true)?;
        let path = parent.path().join(name);
        // CREATE_NEW semantics for a directory: even an empty collision fails.
        fs::create_dir(&path)?;
        let root = Root::open(parent.identity.clone(), &path)?;
        let stage = root.hold(None, true)?;
        Ok(Self {
            parent: parent.clone(),
            root,
            stage,
            _parent: held,
        })
    }
    /// Reopen only the exact independently recorded native directory identity.
    /// The caller's durable operation journal determines which stage is owned.
    pub fn reopen(parent: &Root, name: &str, expected_identity: &str) -> Result<Self> {
        opaque(name)?;
        let held = parent.hold(None, true)?;
        let root = Root::open(parent.identity.clone(), &parent.path().join(name))?;
        let stage = root.hold(None, true)?;
        if stage.native_identity != expected_identity {
            return Err(Error::Stale);
        }
        Ok(Self {
            parent: parent.clone(),
            root,
            stage,
            _parent: held,
        })
    }
    pub fn root(&self) -> &Root {
        &self.root
    }
    pub fn path(&self) -> &Path {
        self.root.path()
    }
    pub fn native_identity(&self) -> &str {
        &self.stage.native_identity
    }
    /// Call only after descendant write/read guards have been released and all
    /// output files flushed. The parent stays pinned across handle replacement.
    /// Any source swap during that short gap is detected before native rename.
    pub fn publish(self, destination_name: &str) -> Result<Published> {
        component(destination_name)?;
        let destination = self.parent.path().join(destination_name);
        match fs::symlink_metadata(&destination) {
            Ok(_) => return Err(Error::Scope("restore destination already exists".into())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        let Self {
            parent,
            root,
            stage,
            _parent,
        } = self;
        let expected = stage.native_identity.clone();
        let source = root.path().to_owned();
        drop(stage);
        let file = deleting_directory(&source)?;
        if native::identity(&file)? != expected || native::final_path(&file)? != source {
            return Err(Error::Stale);
        }
        // FileRenameInfo uses ReplaceIfExists=false and addresses the source by
        // its retained handle. A destination appearing after preflight still
        // causes failure without changing that destination or its contents.
        crate::mutation::rename(&file, &destination)?;
        if native::identity(&file)? != expected || native::final_path(&file)? != destination {
            return Err(Error::Stale);
        }
        drop(file);
        let root = Root::open(parent.identity.clone(), &destination)?;
        let held = root.hold(None, true)?;
        if held.native_identity != expected {
            return Err(Error::Stale);
        }
        Ok(Published {
            root,
            held,
            _parent,
        })
    }
}
impl Published {
    pub fn root(&self) -> &Root {
        &self.root
    }
    pub fn path(&self) -> &Path {
        self.root.path()
    }
    pub fn native_identity(&self) -> &str {
        &self.held.native_identity
    }
    pub fn into_parts(self) -> (Root, HeldPath) {
        (self.root, self.held)
    }
    /// Recover a completed rename after a lost caller reply; neither source
    /// names nor file content can substitute for the expected native identity.
    pub fn reopen(parent: &Root, name: &str, expected_identity: &str) -> Result<Self> {
        component(name)?;
        let held_parent = parent.hold(None, true)?;
        let root = Root::open(parent.identity.clone(), &parent.path().join(name))?;
        let held = root.hold(None, true)?;
        if held.native_identity != expected_identity {
            return Err(Error::Stale);
        }
        Ok(Self {
            root,
            held,
            _parent: held_parent,
        })
    }
}
