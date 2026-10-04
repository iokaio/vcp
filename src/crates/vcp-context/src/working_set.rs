// SPDX-License-Identifier: Apache-2.0
//! Rebuildable source/range observations. This index never grants read authority.
use crate::manifest::{Content, Error, Kind, Part, Result, Revisions};
use serde::Serialize;
use std::collections::BTreeMap;
use vcp_domain::{ArtifactId, RootId};
use vcp_repository::{instructions::Probe, FileVersion};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Applicability {
    Current,
    Changed,
    Unavailable,
}
#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub source: FileVersion,
    pub start_line: u64,
    pub end_line: u64,
    pub complete: bool,
    pub next_line: Option<u64>,
    pub artifact: ArtifactId,
    pub origin_call: String,
    pub applicability: Applicability,
}
#[derive(Serialize)]
pub struct Index {
    pub schema_version: u32,
    pub revisions: Revisions,
    pub entries: Vec<Entry>,
    pub omitted_observations: usize,
}

/// Callers validate retained sources and current authority before rebuilding.
/// A supplied version reader is invoked once per distinct path; successful
/// observations become ordinary send-fence dependencies.
pub fn rebuild(
    history: &[Part],
    revisions: &Revisions,
    root: &RootId,
    mut current: impl FnMut(&str) -> Option<FileVersion>,
) -> Result<(Index, Vec<Probe>)> {
    let history: Vec<_> = history
        .iter()
        .filter(|part| matches!(part.kind, Kind::ToolCall | Kind::ToolResult))
        .collect();
    let mut entries = Vec::new();
    for pair in history.chunks_exact(2) {
        let (
            Content::ToolCall {
                id,
                name,
                arguments,
            },
            Content::ToolResult {
                id: result_id,
                output,
            },
        ) = (&pair[0].content, &pair[1].content)
        else {
            return Err(Error::Invalid("working-set completed pair"));
        };
        if id != result_id {
            return Err(Error::Invalid("working-set call identity"));
        }
        for part in pair {
            part.validate(&revisions.scope)?;
        }
        if name != "vcp_read" {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(output)?;
        let result = &value["result"];
        if result["version"].is_null() || value.get("error").is_some() {
            continue;
        }
        let source: FileVersion = serde_json::from_value(result["version"].clone())?;
        let path = arguments["path"]
            .as_str()
            .ok_or(Error::Invalid("working-set source path"))?;
        let normalized = vcp_repository::path::relative(std::path::Path::new(path))
            .map_err(|_| Error::Invalid("working-set relative path"))?;
        if source.root != *root || source.binding != revisions.binding || source.path != normalized
        {
            // Historical reads under an old binding cannot describe this root.
            continue;
        }
        let complete = result["complete"].as_bool() == Some(true);
        let (start_line, end_line) = if let (Some(start), Some(end)) = (
            result["returned_range"]["start_line"].as_u64(),
            result["returned_range"]["end_line"].as_u64(),
        ) {
            (start, end)
        } else if complete {
            (
                1,
                result["text"]
                    .as_str()
                    .unwrap_or_default()
                    .split_inclusive('\n')
                    .count() as u64,
            )
        } else {
            continue;
        };
        if start_line == 0 || (end_line < start_line && !(complete && end_line == 0)) {
            return Err(Error::Invalid("working-set source range"));
        }
        // A newer version supersedes older source/range observations. Multiple
        // useful ranges of the same exact source remain independently visible.
        entries.retain(|entry: &Entry| entry.source.path != source.path || entry.source == source);
        entries.push(Entry {
            source,
            start_line,
            end_line,
            complete,
            next_line: result["next_line"].as_u64(),
            artifact: pair[1].artifact.clone(),
            origin_call: id.clone(),
            applicability: Applicability::Unavailable,
        });
    }
    if history.len() % 2 != 0 {
        return Err(Error::Invalid("working-set incomplete pair"));
    }
    let omitted_observations = entries.len().saturating_sub(128);
    entries.drain(..omitted_observations);
    let mut versions = BTreeMap::new();
    let mut probes = Vec::new();
    for entry in &mut entries {
        let observed = versions
            .entry(entry.source.path.clone())
            .or_insert_with(|| current(&entry.source.path));
        entry.applicability = match observed {
            Some(version) if *version == entry.source => Applicability::Current,
            Some(_) => Applicability::Changed,
            None => Applicability::Unavailable,
        };
    }
    for (path, version) in versions {
        if let Some(version) = version {
            probes.push(Probe {
                root: root.clone(),
                binding: revisions.binding,
                path,
                observed: Some(version),
            });
        }
    }
    Ok((
        Index {
            schema_version: 1,
            revisions: revisions.clone(),
            entries,
            omitted_observations,
        },
        probes,
    ))
}
