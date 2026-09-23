// SPDX-License-Identifier: Apache-2.0
//! Revision-bound retention previews, never deletion grants. Runtime requires
//! memory/retention/1. A page is a slice of one exact cached selection; applying
//! memory/forget must not rerun its selector or silently truncate that selection.
use crate::{
    memory,
    memory_governance::Digest,
    methods::{Acceptance, Counter, Id, Scope, TaskStatus},
};
use serde::{Deserialize, Serialize};

pub const CAPABILITY: &str = "memory/retention/1";
pub const MAX_INPUT_BYTES: usize = 256 * 1024;
pub const MAX_TARGETS: u32 = 128;
pub const MAX_OFFSET: u32 = 8192;
pub const PREVIEW_TTL_MS: u32 = 60_000;

macro_rules! dto {
    ($name:ident { $($(#[$meta:meta])* $field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        #[serde(deny_unknown_fields)]
        pub struct $name { $($(#[$meta])* pub $field: $ty),* }
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Exclude,
    RestoreRecall,
    Compact,
    Purge,
}

dto!(PreviewRequest {
    scope: Scope,
    task: Id,
    selector: Selector,
    action: Action,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 128)))]
    limit: u32
});
dto!(PreviewPageRequest {
    scope: Scope,
    task: Id,
    preview: Id,
    #[cfg_attr(feature = "schema", schemars(range(max = 8192)))]
    offset: u32,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 128)))]
    limit: u32
});
dto!(JobRead {
    scope: Scope,
    task: Id,
    job: Id
});

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Target {
    Record(#[cfg_attr(feature = "schema", schemars(length(min = 1, max = 512)))] String),
    Event(Id),
}
dto!(PreviewTarget {
    target: Target, selected: bool,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))] protected_reason: Option<String>
});
dto!(PreviewPage {
    scope: Scope, task: Id, preview: Id, digest: Digest, action: Action,
    watermark: Counter, authority: Counter, deletion: Counter,
    selected_count: Counter, dependent_count: Counter, protected_count: Counter,
    retained_bytes: Counter, bytes_are_exact: bool,
    #[cfg_attr(feature = "schema", schemars(range(max = 8192)))] offset: u32,
    #[cfg_attr(feature = "schema", schemars(range(max = 8192)))] next_offset: Option<u32>,
    #[cfg_attr(feature = "schema", schemars(length(max = 128)))] targets: Vec<PreviewTarget>,
    #[cfg_attr(feature = "schema", schemars(range(max = 60000)))] expires_in_ms: u32
});
dto!(JobView {
    scope: Scope,
    task: Id,
    job: Id,
    action: Action,
    deletion: Counter,
    logical_unavailable: bool,
    rewrite_complete: bool,
    local_cleanup_complete: bool,
    pending_generations_count: Counter,
    backup_copies_count: Counter,
    cleanup_required: bool
});
dto!(ForgetResult {
    acceptance: Acceptance,
    job: JobView
});

dto!(InstantSpec {
    #[cfg_attr(feature = "schema", schemars(length(min = 10, max = 29)))]
    entered: String,
    #[cfg_attr(feature = "schema", schemars(range(min = -840, max = 840)))]
    offset_minutes: i16,
    utc: Counter
});
dto!(Bound {
    instant: InstantSpec,
    inclusive: bool
});
dto!(TimeWindow { lower: Option<Bound>, upper: Option<Bound> });

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Status {
    Task(TaskStatus),
    Claim(memory::Outcome),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    Command,
    ModuleRelationship,
    Architecture,
    EnvironmentConstraint,
    VerifiedFix,
    UserPreference,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Criterion {
    Date(TimeWindow),
    Workspace(Id),
    Root(Id),
    Path(#[cfg_attr(feature = "schema", schemars(length(min = 1, max = 4096)))] String),
    Task(Id),
    Actor(Id),
    Agent(Id),
    Model(#[cfg_attr(feature = "schema", schemars(length(min = 1, max = 256)))] String),
    Provider(#[cfg_attr(feature = "schema", schemars(length(min = 1, max = 256)))] String),
    Event(#[cfg_attr(feature = "schema", schemars(length(min = 1, max = 256)))] String),
    Claim(ClaimKind),
    Status(Status),
    Superseded(bool),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(
    tag = "operator",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Tree {
    All(#[cfg_attr(feature = "schema", schemars(length(min = 1, max = 64)))] Vec<Tree>),
    Any(#[cfg_attr(feature = "schema", schemars(length(min = 1, max = 64)))] Vec<Tree>),
    Not(Box<Tree>),
    Match(Criterion),
}
dto!(Selector {
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 1)))]
    schema_version: u32,
    tree: Tree
});

// Check structural limits before serde conversion so caller-constructed deep
// trees cannot allocate/recursively clone an unbounded domain selector first.
fn tree_bound(tree: &Tree, depth: usize, count: &mut usize) -> Result<(), &'static str> {
    *count += 1;
    if depth > 16 || *count > 256 {
        return Err("retention selector tree bound");
    }
    match tree {
        Tree::All(children) | Tree::Any(children) => {
            if children.is_empty() || children.len() > 64 {
                return Err("retention selector branch bound");
            }
            for child in children {
                tree_bound(child, depth + 1, count)?;
            }
        }
        Tree::Not(child) => tree_bound(child, depth + 1, count)?,
        Tree::Match(_) => (),
    }
    Ok(())
}
impl Selector {
    /// Domain normalization validates dates, UTC equivalence, paths, names,
    /// tree complexity and three-valued predicate semantics without reinterpretation.
    pub fn normalized(&self) -> Result<vcp_domain::retention_selector::Selector, &'static str> {
        tree_bound(&self.tree, 0, &mut 0)?;
        input_bound(self)?;
        let value = serde_json::to_value(self).map_err(|_| "retention selector encoding")?;
        let selector: vcp_domain::retention_selector::Selector =
            serde_json::from_value(value).map_err(|_| "retention selector shape")?;
        selector
            .normalized()
            .map_err(|_| "invalid retention selector")
    }
}
fn input_bound(value: &impl Serialize) -> Result<(), &'static str> {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.checked_sub(bytes.len()).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "retention input bound")
            })?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Count(MAX_INPUT_BYTES), value).map_err(|_| "retention input bound")
}
fn page_bound(offset: u32, limit: u32) -> Result<(), &'static str> {
    if offset > MAX_OFFSET || limit == 0 || limit > MAX_TARGETS {
        Err("retention page bound")
    } else {
        Ok(())
    }
}
impl PreviewRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        page_bound(0, self.limit)?;
        // Bound depth before the borrowing serialization preflight as well.
        tree_bound(&self.selector.tree, 0, &mut 0)?;
        input_bound(self)?;
        self.selector.normalized()?;
        Ok(())
    }
}
impl PreviewPageRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        page_bound(self.offset, self.limit)
    }
}
impl PreviewPage {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.offset > MAX_OFFSET
            || self.targets.len() > MAX_TARGETS as usize
            || self.expires_in_ms > PREVIEW_TTL_MS
            || self.next_offset.is_some_and(|n| {
                n > MAX_OFFSET
                    || n <= self.offset
                    || n as usize != self.offset as usize + self.targets.len()
            })
        {
            return Err("retention preview page bound");
        }
        for entry in &self.targets {
            if let Target::Record(id) = &entry.target {
                if id.is_empty() || id.len() > 512 || id.contains('\0') {
                    return Err("retention target bound");
                }
            }
            if entry
                .protected_reason
                .as_ref()
                .is_some_and(|s| s.trim().is_empty() || s.len() > 4096 || s.contains('\0'))
            {
                return Err("retention protection reason bound");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn selector(tree: Tree) -> Selector {
        Selector {
            schema_version: 1,
            tree,
        }
    }
    fn leaf() -> Tree {
        Tree::Match(Criterion::Path("src/main.rs".into()))
    }
    #[test]
    fn domain_normalization_and_complexity_are_preserved() {
        let normalized = selector(Tree::All(vec![leaf(), leaf()]))
            .normalized()
            .unwrap();
        assert!(matches!(
            normalized.tree,
            vcp_domain::retention_selector::Tree::Match(_)
        ));
        for tree in [
            Tree::All(vec![]),
            Tree::Any(vec![leaf(); 65]),
            Tree::Match(Criterion::Path("../outside".into())),
            Tree::Match(Criterion::Provider(" padded ".into())),
        ] {
            assert!(selector(tree).normalized().is_err());
        }
        let mut deep = leaf();
        for _ in 0..17 {
            deep = Tree::Not(Box::new(deep));
        }
        assert!(selector(deep).normalized().is_err());
        assert!(selector(Tree::All(vec![Tree::Any(vec![leaf(); 64]); 4]))
            .normalized()
            .is_err());
    }
    #[test]
    fn dates_require_exact_resolved_utc_and_nonempty_window() {
        let domain =
            vcp_domain::retention_selector::InstantSpec::parse("2024-02-29T12:00:00-07:00", None)
                .unwrap();
        let instant: InstantSpec =
            serde_json::from_value(serde_json::to_value(&domain).unwrap()).unwrap();
        let date = |instant| {
            selector(Tree::Match(Criterion::Date(TimeWindow {
                lower: Some(Bound {
                    instant,
                    inclusive: true,
                }),
                upper: None,
            })))
        };
        assert!(date(instant.clone()).normalized().is_ok());
        let mut forged = instant.clone();
        forged.utc = 0.into();
        assert!(date(forged).normalized().is_err());
        let mut forged = instant;
        forged.offset_minutes = 0;
        assert!(date(forged).normalized().is_err());
        assert!(selector(Tree::Match(Criterion::Date(TimeWindow {
            lower: None,
            upper: None
        })))
        .normalized()
        .is_err());
    }
    #[test]
    fn pages_and_nested_wire_objects_fail_closed() {
        assert!(page_bound(8192, 128).is_ok());
        for (offset, limit) in [(8193, 1), (0, 0), (0, 129)] {
            assert!(page_bound(offset, limit).is_err());
        }
        let mut value = serde_json::to_value(selector(leaf())).unwrap();
        value["tree"]["value"]["grant"] = true.into();
        assert!(serde_json::from_value::<Selector>(value).is_err());
        assert!(serde_json::from_str::<Target>(
            r#"{"kind":"record","id":"claim:a","actor":"forged"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<Action>(r#""delete_everything""#).is_err());
        assert!(serde_json::from_str::<InstantSpec>(
            r#"{"entered":"1970-01-01","offset_minutes":0,"utc":0}"#
        )
        .is_err());
        assert_eq!(
            serde_json::to_value(Target::Record("claim:a".into())).unwrap(),
            serde_json::json!({"kind":"record","id":"claim:a"})
        );
        assert!(input_bound(&"x".repeat(MAX_INPUT_BYTES)).is_err());
        assert!(input_bound(&"x".repeat(MAX_INPUT_BYTES - 2)).is_ok());
    }
}
