// SPDX-License-Identifier: Apache-2.0
use crate::*;
use vcp_repository::FileVersion;
#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub path: String,
    pub before: Option<FileVersion>,
    pub before_bytes: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
    pub rename_to: Option<String>,
    pub probes: Vec<Probe>,
}
fn destination(root: &Root, path: &str) -> Result<Probe> {
    checked_path(path, false)?;
    let parent = Path::new(path).parent().unwrap();
    root.hold(
        if parent.as_os_str().is_empty() {
            None
        } else {
            Some(parent)
        },
        true,
    )?;
    match root.hold(Some(Path::new(path)), false) {
        Err(vcp_repository::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(probe(root, path, None))
        }
        Ok(_) => Err(Error::Invalid("destination already exists")),
        Err(e) => Err(e.into()),
    }
}
fn update(bytes: &[u8], chunks: &[codex_apply_patch::UpdateFileChunk]) -> Result<Vec<u8>> {
    let (prefix, text, encoding) =
        if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
            let little = bytes[0] == 0xff;
            if bytes.len() % 2 != 0 {
                return Err(Error::Invalid("odd UTF-16 input"));
            }
            let units: Vec<_> = bytes[2..]
                .chunks_exact(2)
                .map(|c| {
                    if little {
                        u16::from_le_bytes([c[0], c[1]])
                    } else {
                        u16::from_be_bytes([c[0], c[1]])
                    }
                })
                .collect();
            (
                &bytes[..2],
                String::from_utf16(&units).map_err(|_| Error::Invalid("invalid UTF-16"))?,
                if little { 1 } else { 2 },
            )
        } else {
            let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
            let n = if bom { 3 } else { 0 };
            (
                &bytes[..n],
                std::str::from_utf8(&bytes[n..])
                    .map_err(|_| Error::Invalid("patch requires UTF-8 or BOM-marked UTF-16"))?
                    .to_owned(),
                0,
            )
        };
    if text.contains('\0') {
        return Err(Error::Invalid("binary patch input"));
    }
    let next = codex_apply_patch::prepare_file_update(&text, chunks)
        .map_err(|e| Error::Patch(e.to_string()))?;
    let mut result = prefix.to_vec();
    if encoding == 0 {
        result.extend_from_slice(next.as_bytes());
    } else {
        for unit in next.encode_utf16() {
            result.extend(if encoding == 1 {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
    }
    if result.len() > 1024 * 1024 {
        return Err(Error::Invalid("patched file ceiling"));
    }
    Ok(result)
}
pub(crate) fn prepare(root: &Root, text: &str) -> Result<Vec<Change>> {
    if text.len() > 128 * 1024
        || !text.starts_with("*** Begin Patch\n")
        || !text.trim_end().ends_with("*** End Patch")
    {
        return Err(Error::Invalid("bounded literal patch required"));
    }
    let parsed = codex_apply_patch::parse_patch(text).map_err(|e| Error::Patch(e.to_string()))?;
    if parsed.hunks.is_empty() || parsed.hunks.len() > 64 {
        return Err(Error::Invalid("patch file count"));
    }
    let mut seen = BTreeSet::new();
    let mut changes = vec![];
    let mut bytes = 0;
    for hunk in parsed.hunks {
        use codex_apply_patch::Hunk;
        let (path, before, before_bytes, after, rename_to, probes) = match hunk {
            Hunk::AddFile { path, contents } => {
                let path = path
                    .to_str()
                    .ok_or(Error::Invalid("non-Unicode patch path"))?
                    .to_owned();
                let dependency = destination(root, &path)?;
                (
                    path,
                    None,
                    None,
                    Some(contents.into_bytes()),
                    None,
                    vec![dependency],
                )
            }
            Hunk::DeleteFile { path } => {
                let path = path
                    .to_str()
                    .ok_or(Error::Invalid("non-Unicode patch path"))?
                    .to_owned();
                checked_path(&path, false)?;
                let source = root.read(Path::new(&path), 1024 * 1024)?;
                let dependency = probe(root, &path, Some(source.version.clone()));
                (
                    path,
                    Some(source.version),
                    Some(source.bytes),
                    None,
                    None,
                    vec![dependency],
                )
            }
            Hunk::UpdateFile {
                path,
                move_path,
                chunks,
            } => {
                let path = path
                    .to_str()
                    .ok_or(Error::Invalid("non-Unicode patch path"))?
                    .to_owned();
                checked_path(&path, false)?;
                let source = root.read(Path::new(&path), 1024 * 1024)?;
                let after = update(&source.bytes, &chunks)?;
                let mut probes = vec![probe(root, &path, Some(source.version.clone()))];
                let target = move_path
                    .map(|p| {
                        p.to_str()
                            .map(str::to_owned)
                            .ok_or(Error::Invalid("non-Unicode rename"))
                    })
                    .transpose()?;
                if let Some(target) = &target {
                    checked_path(target, false)?;
                    if target.eq_ignore_ascii_case(&path) {
                        if target == &path {
                            return Err(Error::Invalid("rename must change the path"));
                        }
                        let destination = root.read(Path::new(target), 1024 * 1024)?;
                        if destination.version.native_identity != source.version.native_identity {
                            return Err(Error::Invalid(
                                "case rename destination is a different file",
                            ));
                        }
                    } else {
                        probes.push(destination(root, target)?);
                    }
                }
                (
                    path,
                    Some(source.version),
                    Some(source.bytes),
                    Some(after),
                    target,
                    probes,
                )
            }
        };
        for name in std::iter::once(&path).chain(
            rename_to
                .iter()
                .filter(|name| !name.eq_ignore_ascii_case(&path)),
        ) {
            if !seen.insert(name.to_lowercase()) {
                return Err(Error::Invalid("overlapping patch paths"));
            }
        }
        bytes += after.as_ref().map_or(0, Vec::len);
        if bytes > 4 * 1024 * 1024 {
            return Err(Error::Invalid("patch total bytes"));
        }
        changes.push(Change {
            path,
            before,
            before_bytes,
            after,
            rename_to,
            probes,
        });
    }
    Ok(changes)
}
