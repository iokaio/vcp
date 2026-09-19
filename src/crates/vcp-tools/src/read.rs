// SPDX-License-Identifier: Apache-2.0
use crate::*;
use std::fs;
pub(crate) fn list(root: &Root, path: &str, max: usize) -> Result<serde_json::Value> {
    checked_path(path, true)?;
    if max == 0 || max > 10_000 {
        return Err(Error::Invalid("list entry ceiling"));
    }
    let held = root.hold(
        if path.is_empty() {
            None
        } else {
            Some(Path::new(path))
        },
        true,
    )?;
    let mut rows = vec![];
    for entry in fs::read_dir(root.path().join(path))? {
        if rows.len() == max {
            return Err(Error::Invalid(
                "list exceeds entry ceiling; select a narrower directory",
            ));
        }
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| Error::Invalid("non-Unicode name"))?;
        let metadata = fs::symlink_metadata(entry.path())?;
        #[cfg(windows)]
        let link = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let link = metadata.is_symlink();
        rows.push(serde_json::json!({"name":name,"kind":if link{"reparse"}else if metadata.is_dir(){"directory"}else{"file"}}));
    }
    rows.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(
        serde_json::json!({"path":path,"directory_identity":held.native_identity,"entries":rows,"complete":true}),
    )
}
pub(crate) fn search(
    root: &Root,
    query: &str,
    max: usize,
) -> Result<(serde_json::Value, Vec<Probe>)> {
    if query.is_empty() || query.len() > 4096 || max == 0 || max > 10_000 {
        return Err(Error::Invalid("search limits"));
    }
    let scan = root.discover(&vcp_repository::discovery::Limits::default())?;
    let mut probes = scan.ignore_dependencies;
    let mut matches = vec![];
    let mut complete = scan.complete;
    for source in scan.sources {
        probes.push(probe(
            root,
            &source.version.path,
            Some(source.version.clone()),
        ));
        let text = std::str::from_utf8(&source.bytes)
            .map_err(|_| Error::Invalid("discovery returned nontext"))?;
        for (index, line) in text.lines().enumerate() {
            if line.contains(query) {
                if matches.len() == max {
                    complete = false;
                    break;
                }
                matches.push(serde_json::json!({"path":source.version.path,"line":index+1,"text":line,"version":source.version}));
            }
        }
    }
    Ok((
        serde_json::json!({"matches":matches,"complete":complete,"exclusions":scan.exclusions,"query":query}),
        probes,
    ))
}
