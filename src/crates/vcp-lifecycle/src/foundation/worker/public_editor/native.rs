// SPDX-License-Identifier: Apache-2.0
//! Editor claims never substitute for the host's scoped native file observation.
use super::*;
use std::path::Path;

pub(super) fn observe(
    root: &vcp_repository::Root,
    document: &methods::DocumentObservation,
) -> Result<vcp_repository::Source> {
    let relative = vcp_repository::path::relative(Path::new(&document.relative_path))?;
    if relative != document.relative_path {
        return Err("editor document path must be canonical root-relative".into());
    }
    let uri = url::Url::parse(&document.uri)?;
    if uri.scheme() != "file"
        || uri.query().is_some()
        || uri.fragment().is_some()
        || uri
            .host_str()
            .is_some_and(|host| !host.is_empty() && host != "localhost")
    {
        return Err("editor document must be a local file URI".into());
    }
    let file = uri
        .to_file_path()
        .map_err(|_| "editor URI is not a native file path")?;
    // Root::read rejects links/reparse components and pins native identity. The
    // independent URI comparison prevents a valid relative proof authorizing a
    // different document, including a hard-link alias outside this root.
    let source = root.read(Path::new(&relative), 256 * 1024)?;
    if std::fs::canonicalize(file)? != std::fs::canonicalize(root.path().join(&relative))? {
        return Err("editor URI does not identify its root-relative resource".into());
    }
    root.revalidate(&source.version)?;
    if document
        .disk_sha256
        .as_ref()
        .is_some_and(|hash| hash != &source.version.sha256)
    {
        return Err("editor disk observation is stale".into());
    }
    Ok(source)
}

pub(super) fn logical_disk(bytes: &[u8], encoding: &wire::Encoding) -> Result<String> {
    match encoding {
        wire::Encoding::Utf8 => Ok(std::str::from_utf8(bytes)?.to_owned()),
        wire::Encoding::Utf8Bom => {
            let bytes = bytes
                .strip_prefix(&[0xef, 0xbb, 0xbf])
                .ok_or("UTF-8 BOM missing")?;
            Ok(std::str::from_utf8(bytes)?.to_owned())
        }
        wire::Encoding::Utf16Le | wire::Encoding::Utf16Be => {
            let little = matches!(encoding, wire::Encoding::Utf16Le);
            let bom = if little { [0xff, 0xfe] } else { [0xfe, 0xff] };
            let bytes = bytes.strip_prefix(&bom).ok_or("UTF-16 BOM missing")?;
            if bytes.len() % 2 != 0 {
                return Err("odd UTF-16 disk byte count".into());
            }
            let words = bytes
                .chunks_exact(2)
                .map(|pair| {
                    if little {
                        u16::from_le_bytes([pair[0], pair[1]])
                    } else {
                        u16::from_be_bytes([pair[0], pair[1]])
                    }
                })
                .collect::<Vec<_>>();
            Ok(String::from_utf16(&words)?)
        }
        wire::Encoding::Unknown => {
            let detected = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
                wire::Encoding::Utf8Bom
            } else if bytes.starts_with(&[0xff, 0xfe]) {
                wire::Encoding::Utf16Le
            } else if bytes.starts_with(&[0xfe, 0xff]) {
                wire::Encoding::Utf16Be
            } else {
                wire::Encoding::Utf8
            };
            logical_disk(bytes, &detected)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_observation_binds_uri_path_bytes_and_file_identity() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("one.txt");
        let alias = directory.path().join("alias.txt");
        std::fs::write(&file, b"original\r\n").unwrap();
        std::fs::hard_link(&file, &alias).unwrap();
        let root = vcp_repository::Root::open(
            vcp_repository::RootIdentity {
                workspace: WorkspaceId::new(),
                root: RootId::new(),
                repository: "fixture".into(),
                worktree: "fixture".into(),
                binding: Revision::ZERO,
            },
            directory.path(),
        )
        .unwrap();
        let mut document = methods::DocumentObservation {
            host: id(HostId::new()).unwrap(),
            open_id: id(ArtifactId::new()).unwrap(),
            uri: url::Url::from_file_path(&file).unwrap().to_string(),
            relative_path: "one.txt".into(),
            version: 1u64.into(),
            content_sha256: vcp_protocol::digest_bytes(b"original\r\n"),
            dirty: false,
            content: None,
            disk_sha256: None,
            language: "plaintext".into(),
            eol: wire::EndOfLine::Crlf,
            encoding: wire::Encoding::Unknown,
            selections: vec![],
            capture: false,
            diagnostics: None,
        };
        let before = observe(&root, &document).unwrap();
        document.uri = url::Url::from_file_path(&alias).unwrap().to_string();
        assert!(
            observe(&root, &document).is_err(),
            "even a same-inode alias is a different editor document"
        );
        document.uri = url::Url::from_file_path(&file).unwrap().to_string();
        document.relative_path = "../one.txt".into();
        assert!(observe(&root, &document).is_err());
        document.relative_path = "one.txt".into();
        document.disk_sha256 = Some("0".repeat(64));
        assert!(observe(&root, &document).is_err());
        document.disk_sha256 = None;
        std::fs::write(&file, b"changed\r\n").unwrap();
        assert!(root.revalidate(&before.version).is_err());
        assert_ne!(
            observe(&root, &document).unwrap().version.sha256,
            before.version.sha256
        );
        std::fs::remove_file(&file).unwrap();
        assert!(observe(&root, &document).is_err());
    }
    #[test]
    fn logical_hash_preserves_line_endings_and_requires_declared_encoding() {
        assert_eq!(
            logical_disk(b"x\r\n", &wire::Encoding::Utf8).unwrap(),
            "x\r\n"
        );
        assert_eq!(
            logical_disk(&[0xef, 0xbb, 0xbf, b'x'], &wire::Encoding::Utf8Bom).unwrap(),
            "x"
        );
        assert_eq!(
            logical_disk(&[0xff, 0xfe, 0x3d, 0xd8, 0, 0xde], &wire::Encoding::Utf16Le).unwrap(),
            "😀"
        );
        assert_eq!(
            logical_disk(&[0xfe, 0xff, 0xd8, 0x3d, 0xde, 0], &wire::Encoding::Utf16Be).unwrap(),
            "😀"
        );
        assert_eq!(logical_disk(b"x", &wire::Encoding::Unknown).unwrap(), "x");
        assert!(logical_disk(&[0x80], &wire::Encoding::Unknown).is_err());
        assert!(logical_disk(b"x", &wire::Encoding::Utf8Bom).is_err());
        assert!(logical_disk(&[0xff, 0xfe, 0], &wire::Encoding::Utf16Le).is_err());
        assert!(logical_disk(&[0xff, 0xfe, 0x3d, 0xd8], &wire::Encoding::Utf16Le).is_err());
    }
}
