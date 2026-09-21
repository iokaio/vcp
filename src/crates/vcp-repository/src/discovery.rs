// SPDX-License-Identifier: Apache-2.0
use crate::*;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Limits {
    pub entries: usize,
    pub depth: usize,
    pub file_bytes: u64,
    pub total_bytes: u64,
    pub skip_generated: bool,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: 10_000,
            depth: 32,
            file_bytes: 1024 * 1024,
            total_bytes: 8 * 1024 * 1024,
            skip_generated: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    Ignored,
    PathFilter,
    Generated,
    Binary,
    Reparse,
    Oversize,
    Entries,
    Depth,
    TotalBytes,
    Unavailable,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exclusion {
    pub path: String,
    pub reason: ExclusionReason,
}
pub struct Discovery {
    pub sources: Vec<Source>,
    pub exclusions: Vec<Exclusion>,
    pub ignore_dependencies: Vec<crate::instructions::Probe>,
    pub complete: bool,
}
struct Walker<'a> {
    root: &'a Root,
    cancelled: &'a dyn Fn() -> bool,
    include: &'a dyn Fn(&str) -> bool,
    limits: &'a Limits,
    visited: usize,
    bytes: u64,
    result: Discovery,
}
impl Root {
    pub fn discover(&self, limits: &Limits) -> Result<Discovery> {
        self.discover_cancellable(limits, &|| false)
    }
    /// Check trusted cancellation between filesystem operations during bounded discovery.
    pub fn discover_cancellable(
        &self,
        limits: &Limits,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Discovery> {
        self.discover_filtered_cancellable(limits, cancelled, &|_| true)
    }
    /// Restrict source capture while retaining the same traversal, ignores and entry ceiling.
    pub fn discover_filtered_cancellable(
        &self,
        limits: &Limits,
        cancelled: &dyn Fn() -> bool,
        include: &dyn Fn(&str) -> bool,
    ) -> Result<Discovery> {
        if limits.entries == 0
            || limits.entries > 100_000
            || limits.depth == 0
            || limits.depth > 128
            || limits.file_bytes == 0
            || limits.file_bytes > 64 * 1024 * 1024
            || limits.total_bytes > 64 * 1024 * 1024
        {
            return Err(Error::Limit("discovery configuration"));
        }
        let mut walker = Walker {
            root: self,
            cancelled,
            include,
            limits,
            visited: 0,
            bytes: 0,
            result: Discovery {
                sources: vec![],
                exclusions: vec![],
                ignore_dependencies: vec![],
                complete: true,
            },
        };
        walker.visit(Path::new(""), 0, &[])?;
        walker
            .result
            .sources
            .sort_by(|a, b| a.version.path.cmp(&b.version.path));
        walker.result.exclusions.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(walker.result)
    }
}
impl Walker<'_> {
    fn check_cancelled(&self) -> Result<()> {
        if (self.cancelled)() {
            return Err(Error::Limit("discovery cancelled"));
        }
        Ok(())
    }
    fn exclude(&mut self, path: &Path, reason: ExclusionReason) {
        self.result.exclusions.push(Exclusion {
            path: path.to_string_lossy().replace('\\', "/"),
            reason,
        });
    }
    fn visit(&mut self, relative: &Path, depth: usize, inherited: &[Gitignore]) -> Result<()> {
        self.check_cancelled()?;
        if depth > self.limits.depth {
            self.result.complete = false;
            self.exclude(relative, ExclusionReason::Depth);
            return Ok(());
        }
        let _held = match self.root.hold(
            if relative.as_os_str().is_empty() {
                None
            } else {
                Some(relative)
            },
            true,
        ) {
            Ok(held) => held,
            Err(Error::Link) => {
                self.exclude(relative, ExclusionReason::Reparse);
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let mut rules = inherited.to_vec();
        let ignore_path = relative.join(".gitignore");
        match self.root.read(&ignore_path, 64 * 1024) {
            Ok(source) => {
                let text = std::str::from_utf8(&source.bytes)
                    .map_err(|_| Error::Unsupported("non-UTF8 ignore rules"))?;
                let mut builder = GitignoreBuilder::new(self.root.path.join(relative));
                for line in text.lines() {
                    builder
                        .add_line(Some(self.root.path.join(&ignore_path)), line)
                        .map_err(|_| Error::Unsupported("invalid ignore pattern"))?;
                }
                rules.push(
                    builder
                        .build()
                        .map_err(|_| Error::Unsupported("invalid ignore rules"))?,
                );
                self.result
                    .ignore_dependencies
                    .push(crate::instructions::Probe {
                        root: self.root.identity.root.clone(),
                        binding: self.root.identity.binding,
                        path: path::relative(&ignore_path)?,
                        observed: Some(source.version),
                    });
            }
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                self.result
                    .ignore_dependencies
                    .push(crate::instructions::Probe {
                        root: self.root.identity.root.clone(),
                        binding: self.root.identity.binding,
                        path: path::relative(&ignore_path)?,
                        observed: None,
                    });
            }
            Err(error) => return Err(error),
        }
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(self.root.path.join(relative))? {
            self.check_cancelled()?;
            if self.visited == self.limits.entries {
                self.result.complete = false;
                self.exclude(relative, ExclusionReason::Entries);
                break;
            }
            self.visited += 1;
            let entry = entry?;
            let name = entry.file_name();
            if name.to_str().is_none() {
                self.result.complete = false;
                self.exclude(relative, ExclusionReason::Unavailable);
                continue;
            }
            entries.push((name, entry.file_type()?));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, kind) in entries {
            self.check_cancelled()?;
            let path = relative.join(&name);
            if name == ".git" {
                self.exclude(&path, ExclusionReason::Ignored);
                continue;
            }
            let mut ignored = false;
            for rule in &rules {
                let matched =
                    rule.matched_path_or_any_parents(self.root.path.join(&path), kind.is_dir());
                if matched.is_ignore() {
                    ignored = true;
                }
                if matched.is_whitelist() {
                    ignored = false;
                }
            }
            if ignored {
                self.exclude(&path, ExclusionReason::Ignored);
                continue;
            }
            if kind.is_symlink() {
                self.exclude(&path, ExclusionReason::Reparse);
                continue;
            }
            if kind.is_dir() {
                if self.limits.skip_generated
                    && matches!(
                        name.to_str(),
                        Some("node_modules" | "target" | "dist" | ".next")
                    )
                {
                    self.exclude(&path, ExclusionReason::Generated);
                    continue;
                }
                self.visit(&path, depth + 1, &rules)?;
                continue;
            }
            if !kind.is_file() {
                self.exclude(&path, ExclusionReason::Unavailable);
                continue;
            }
            if !(self.include)(&path::relative(&path)?) {
                self.exclude(&path, ExclusionReason::PathFilter);
                continue;
            }
            let remaining = self.limits.total_bytes.saturating_sub(self.bytes);
            if remaining == 0 {
                self.result.complete = false;
                self.exclude(&path, ExclusionReason::TotalBytes);
                continue;
            }
            match self.root.read(&path, self.limits.file_bytes.min(remaining)) {
                Ok(source) => {
                    self.bytes += source.version.bytes.get();
                    if source.bytes.contains(&0) || std::str::from_utf8(&source.bytes).is_err() {
                        self.exclude(&path, ExclusionReason::Binary);
                    }
                    self.result.sources.push(source);
                }
                Err(Error::Link) => self.exclude(&path, ExclusionReason::Reparse),
                Err(Error::Limit(_)) => {
                    self.result.complete = false;
                    self.exclude(
                        &path,
                        if remaining < self.limits.file_bytes {
                            ExclusionReason::TotalBytes
                        } else {
                            ExclusionReason::Oversize
                        },
                    );
                }
                Err(Error::Io(_)) => {
                    self.result.complete = false;
                    self.exclude(&path, ExclusionReason::Unavailable);
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}
