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
/// Patterns use the workspace regex engine; paths use normalized root-relative `/` separators.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Literal,
    Regex,
}

fn pattern(value: &str) -> Result<regex::Regex> {
    if value.is_empty() || value.len() > 4096 {
        return Err(Error::Invalid("search pattern ceiling"));
    }
    regex::RegexBuilder::new(value)
        .size_limit(1024 * 1024)
        .dfa_size_limit(1024 * 1024)
        .build()
        .map_err(|_| Error::Invalid("invalid or oversized search regex"))
}

pub(crate) fn file(
    root: &Root,
    path: &str,
    max_bytes: u64,
    start: Option<usize>,
    end: Option<usize>,
) -> Result<(serde_json::Value, Probe)> {
    checked_path(path, false)?;
    if max_bytes == 0 || max_bytes > 1024 * 1024 {
        return Err(Error::Invalid("read ceiling"));
    }
    let ranged = start.is_some() || end.is_some();
    let first = start.unwrap_or(1);
    if first == 0 || end.is_some_and(|last| last < first) {
        return Err(Error::Invalid("read line range"));
    }
    // Capture and hash the complete bounded source, even when exposing only a range.
    let source = root.read(
        Path::new(path),
        if ranged { 64 * 1024 * 1024 } else { max_bytes },
    )?;
    let text = std::str::from_utf8(&source.bytes)
        .map_err(|_| Error::Invalid("read requires UTF-8 text; binary capture is not enabled"))?;
    let dependency = probe(root, path, Some(source.version.clone()));
    if !ranged {
        return Ok((
            serde_json::json!({"text":text,"version":source.version,"complete":true}),
            dependency,
        ));
    }
    let total_lines = text.split_inclusive('\n').count();
    if first > total_lines && !(first == 1 && total_lines == 0) {
        return Err(Error::Invalid("read range starts beyond end of file"));
    }
    let mut returned = String::new();
    let mut last = None;
    for (index, line) in text.split_inclusive('\n').enumerate().skip(first - 1) {
        let number = index + 1;
        if number > end.unwrap_or(usize::MAX)
            || returned.len() as u64 + line.len() as u64 > max_bytes
        {
            break;
        }
        returned.push_str(line);
        last = Some(number);
    }
    if total_lines != 0 && last.is_none() {
        return Err(Error::Invalid("read line exceeds byte ceiling"));
    }
    let next_line = last.filter(|last| *last < total_lines).map(|last| last + 1);
    Ok((
        serde_json::json!({"text":returned,"version":source.version,"complete":first == 1 && next_line.is_none(),"returned_range":last.map(|last| serde_json::json!({"start_line":first,"end_line":last})),"next_line":next_line,"total_lines":total_lines}),
        dependency,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn search(
    root: &Root,
    query: &str,
    max: usize,
    mode: Option<SearchMode>,
    path_pattern: Option<&str>,
    max_files: Option<usize>,
    max_scan_bytes: Option<u64>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(serde_json::Value, Vec<Probe>)> {
    if query.is_empty() || query.len() > 4096 || max == 0 || max > 10_000 {
        return Err(Error::Invalid("search limits"));
    }
    let regex = match mode.unwrap_or(SearchMode::Literal) {
        SearchMode::Literal => None,
        SearchMode::Regex => Some(pattern(query)?),
    };
    let path_regex = path_pattern.map(pattern).transpose()?;
    let mut limits = vcp_repository::discovery::Limits::default();
    if let Some(max_files) = max_files {
        if max_files == 0 || max_files > limits.entries {
            return Err(Error::Invalid("search file ceiling"));
        }
        limits.entries = max_files;
    }
    if let Some(bytes) = max_scan_bytes {
        if bytes == 0 || bytes > limits.total_bytes {
            return Err(Error::Invalid("search byte ceiling"));
        }
        limits.total_bytes = bytes;
    }
    let scan = root.discover_filtered_cancellable(&limits, cancelled, &|path| {
        path_regex.as_ref().is_none_or(|regex| regex.is_match(path))
    })?;
    let mut probes = scan.ignore_dependencies;
    let mut matches = vec![];
    let mut complete = scan.complete;
    for source in scan.sources {
        if cancelled() {
            return Err(Error::Invalid("search cancelled"));
        }
        probes.push(probe(
            root,
            &source.version.path,
            Some(source.version.clone()),
        ));
        let text = std::str::from_utf8(&source.bytes)
            .map_err(|_| Error::Invalid("discovery returned nontext"))?;
        for (index, line) in text.lines().enumerate() {
            if cancelled() {
                return Err(Error::Invalid("search cancelled"));
            }
            if regex
                .as_ref()
                .map_or_else(|| line.contains(query), |regex| regex.is_match(line))
            {
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
