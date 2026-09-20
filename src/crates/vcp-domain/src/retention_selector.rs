// SPDX-License-Identifier: Apache-2.0
//! Shared retention predicates. Normalization never reads the machine timezone.
//! Selection is only a planner input, never permission to delete matching data.
use crate::{
    memory::{ClaimKind, Outcome},
    task::TaskState,
    *,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstantSpec {
    pub entered: String,
    /// Fixed UTC offset selected by the caller, not an inferred IANA timezone.
    pub offset_minutes: i16,
    pub utc: Timestamp,
}
impl InstantSpec {
    /// Dates require an explicit fixed offset and mean local midnight. Timestamps
    /// require Z or +/-HH:MM. Named zones, absent offsets, unknown -00:00, leap
    /// seconds and submillisecond precision are rejected, never guessed/rounded.
    pub fn parse(input: &str, date_offset: Option<i16>) -> Result<Self> {
        if !input.is_ascii() || input.len() < 10 || input.len() > 29 {
            return Err(Error::Invalid("explicit retention timestamp"));
        }
        let number = |text: &str| -> Result<i64> {
            if text.is_empty() || !text.bytes().all(|c| c.is_ascii_digit()) {
                return Err(Error::Invalid("retention date digits"));
            }
            text.parse()
                .map_err(|_| Error::Invalid("retention date number"))
        };
        if &input[4..5] != "-" || &input[7..8] != "-" {
            return Err(Error::Invalid("retention date format"));
        }
        let year = number(&input[..4])?;
        let month = number(&input[5..7])?;
        let day = number(&input[8..10])?;
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let months = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        if !(1970..=9999).contains(&year)
            || !(1..=12).contains(&month)
            || day < 1
            || day > months[(month - 1) as usize]
        {
            return Err(Error::Invalid("retention calendar date"));
        }
        let mut time_ms = 0i64;
        let offset = if input.len() == 10 {
            date_offset.ok_or(Error::Invalid("date requires explicit UTC offset"))?
        } else {
            if input.len() < 20
                || &input[10..11] != "T"
                || &input[13..14] != ":"
                || &input[16..17] != ":"
            {
                return Err(Error::Invalid("retention timestamp format"));
            }
            let hour = number(&input[11..13])?;
            let minute = number(&input[14..16])?;
            let second = number(&input[17..19])?;
            if hour > 23 || minute > 59 || second > 59 {
                return Err(Error::Invalid("retention timestamp clock"));
            }
            let mut zone = 19;
            let mut millis = 0;
            if input.as_bytes()[zone] == b'.' {
                zone += 1;
                let first = zone;
                while zone < input.len() && input.as_bytes()[zone].is_ascii_digit() {
                    zone += 1;
                }
                let length = zone - first;
                if !(1..=3).contains(&length) {
                    return Err(Error::Invalid("retention millisecond precision"));
                }
                millis = number(&input[first..zone])? * 10i64.pow((3 - length) as u32);
            }
            time_ms = ((hour * 60 + minute) * 60 + second) * 1000 + millis;
            let suffix = &input[zone..];
            let offset = if suffix == "Z" {
                0
            } else {
                if suffix.len() != 6
                    || !matches!(suffix.as_bytes()[0], b'+' | b'-')
                    || &suffix[3..4] != ":"
                    || suffix == "-00:00"
                {
                    return Err(Error::Invalid("explicit resolved UTC offset required"));
                }
                let hours = number(&suffix[1..3])?;
                let minutes = number(&suffix[4..6])?;
                if hours > 14 || minutes > 59 || (hours == 14 && minutes != 0) {
                    return Err(Error::Invalid("UTC offset outside supported range"));
                }
                ((hours * 60 + minutes) * if suffix.starts_with('-') { -1 } else { 1 }) as i16
            };
            if date_offset.is_some_and(|supplied| supplied != offset) {
                return Err(Error::Invalid("conflicting explicit UTC offsets"));
            }
            offset
        };
        if !(-840..=840).contains(&offset) {
            return Err(Error::Invalid("UTC offset outside supported range"));
        }
        let leap_days = |year: i64| {
            let y = year - 1;
            y / 4 - y / 100 + y / 400
        };
        let days = (year - 1970) * 365 + leap_days(year) - leap_days(1970)
            + months[..(month - 1) as usize].iter().sum::<i64>()
            + day
            - 1;
        let utc = days * 86_400_000 + time_ms - i64::from(offset) * 60_000;
        let utc =
            u64::try_from(utc).map_err(|_| Error::Invalid("retention time predates Unix epoch"))?;
        Ok(Self {
            entered: input.into(),
            offset_minutes: offset,
            utc: Timestamp::new(utc),
        })
    }
    pub fn validate(&self) -> Result<()> {
        if Self::parse(&self.entered, Some(self.offset_minutes))? != *self {
            return Err(Error::Invalid("retention UTC normalization mismatch"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bound {
    pub instant: InstantSpec,
    pub inclusive: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeWindow {
    pub lower: Option<Bound>,
    pub upper: Option<Bound>,
}
impl TimeWindow {
    pub fn validate(&self) -> Result<()> {
        if self.lower.is_none() && self.upper.is_none() {
            return Err(Error::Invalid("unbounded retention date predicate"));
        }
        for bound in [&self.lower, &self.upper].into_iter().flatten() {
            bound.instant.validate()?;
        }
        if self
            .lower
            .as_ref()
            .zip(self.upper.as_ref())
            .is_some_and(|(a, b)| {
                a.instant.utc > b.instant.utc
                    || (a.instant.utc == b.instant.utc && !(a.inclusive && b.inclusive))
            })
        {
            return Err(Error::Invalid("empty retention time window"));
        }
        Ok(())
    }
    fn matches(&self, value: Timestamp) -> bool {
        self.lower
            .as_ref()
            .is_none_or(|b| value > b.instant.utc || (b.inclusive && value == b.instant.utc))
            && self
                .upper
                .as_ref()
                .is_none_or(|b| value < b.instant.utc || (b.inclusive && value == b.instant.utc))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Status {
    Task(TaskState),
    Claim(Outcome),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Criterion {
    Date(TimeWindow),
    Workspace(WorkspaceId),
    Root(RootId),
    Path(String),
    Task(TaskId),
    Actor(ActorId),
    Agent(AgentId),
    Model(String),
    Provider(String),
    Event(String),
    Claim(ClaimKind),
    Status(Status),
    Superseded(bool),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "operator",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Tree {
    All(Vec<Tree>),
    Any(Vec<Tree>),
    Not(Box<Tree>),
    Match(Criterion),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    pub schema_version: u32,
    pub tree: Tree,
}
impl Selector {
    pub fn normalized(self) -> Result<Self> {
        if self.schema_version != 1 {
            return Err(Error::Invalid("retention selector version"));
        }
        let mut count = 0;
        Ok(Self {
            schema_version: 1,
            tree: normalize(self.tree, 0, &mut count)?,
        })
    }
    /// Only Match may enter a preview's exact selected IDs. Unknown metadata
    /// remains unknown under negation, preventing accidental broad deletion.
    pub fn evaluate(&self, facts: &Facts<'_>) -> Result<Truth> {
        self.clone().normalized()?;
        Ok(evaluate(&self.tree, facts))
    }
}
fn normalize(tree: Tree, depth: usize, count: &mut usize) -> Result<Tree> {
    *count += 1;
    if depth > 16 || *count > 256 {
        return Err(Error::Invalid("retention selector tree bound"));
    }
    match tree {
        Tree::All(children) => normalize_list(children, true, depth, count),
        Tree::Any(children) => normalize_list(children, false, depth, count),
        Tree::Not(child) => Ok(Tree::Not(Box::new(normalize(*child, depth + 1, count)?))),
        Tree::Match(criterion) => {
            match &criterion {
                Criterion::Date(window) => window.validate()?,
                Criterion::Path(path) => {
                    if path.is_empty()
                        || path.len() > 4096
                        || path.starts_with('/')
                        || path.contains(['\\', ':', '\0'])
                        || path.split('/').any(|s| matches!(s, "" | "." | ".."))
                    {
                        return Err(Error::Invalid(
                            "canonical root-relative retention path required",
                        ));
                    }
                }
                Criterion::Model(value) | Criterion::Provider(value) | Criterion::Event(value)
                    if value.is_empty()
                        || value.len() > 256
                        || value != value.trim()
                        || value.contains('\0') =>
                {
                    return Err(Error::Invalid("retention selector name"));
                }
                _ => (),
            }
            Ok(Tree::Match(criterion))
        }
    }
}
fn normalize_list(children: Vec<Tree>, all: bool, depth: usize, count: &mut usize) -> Result<Tree> {
    if children.is_empty() || children.len() > 64 {
        return Err(Error::Invalid("retention selector branch bound"));
    }
    let mut result = Vec::new();
    for child in children {
        let normalized = normalize(child, depth + 1, count)?;
        let additions = match normalized {
            Tree::All(items) if all => items,
            Tree::Any(items) if !all => items,
            other => vec![other],
        };
        for child in additions {
            if !result.contains(&child) {
                result.push(child);
            }
        }
    }
    // Preserve caller ordering for preview display; no implicit case/path folding.
    if result.len() == 1 {
        return result.pop().ok_or(Error::Invalid("empty retention branch"));
    }
    if result.len() > 64 {
        return Err(Error::Invalid("normalized retention branch bound"));
    }
    Ok(if all {
        Tree::All(result)
    } else {
        Tree::Any(result)
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Truth {
    Match,
    NoMatch,
    Unknown,
}
impl Truth {
    fn not(self) -> Self {
        match self {
            Self::Match => Self::NoMatch,
            Self::NoMatch => Self::Match,
            Self::Unknown => Self::Unknown,
        }
    }
}
/// Metadata only. Payload scanning and implicit inference from text are forbidden.
pub struct Facts<'a> {
    pub workspace: &'a WorkspaceId,
    pub timestamp: Option<Timestamp>,
    pub roots: Option<&'a [RootId]>,
    pub paths: Option<&'a [String]>,
    pub task: Option<&'a TaskId>,
    pub actor: Option<&'a ActorId>,
    pub agent: Option<&'a AgentId>,
    pub model: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub event: Option<&'a str>,
    pub claim: Option<ClaimKind>,
    pub task_status: Option<TaskState>,
    pub claim_status: Option<Outcome>,
    pub superseded: Option<bool>,
}
fn known(value: Option<bool>) -> Truth {
    match value {
        Some(true) => Truth::Match,
        Some(false) => Truth::NoMatch,
        None => Truth::Unknown,
    }
}
fn evaluate(tree: &Tree, facts: &Facts<'_>) -> Truth {
    match tree {
        Tree::Not(child) => evaluate(child, facts).not(),
        Tree::All(children) | Tree::Any(children) => {
            let all = matches!(tree, Tree::All(_));
            let mut unknown = false;
            for child in children {
                match evaluate(child, facts) {
                    Truth::NoMatch if all => return Truth::NoMatch,
                    Truth::Match if !all => return Truth::Match,
                    Truth::Unknown => unknown = true,
                    _ => (),
                }
            }
            if unknown {
                Truth::Unknown
            } else if all {
                Truth::Match
            } else {
                Truth::NoMatch
            }
        }
        Tree::Match(criterion) => known(match criterion {
            Criterion::Date(window) => facts.timestamp.map(|v| window.matches(v)),
            Criterion::Workspace(id) => Some(id == facts.workspace),
            Criterion::Root(id) => facts.roots.map(|roots| roots.contains(id)),
            Criterion::Path(path) => facts.paths.map(|paths| paths.contains(path)),
            Criterion::Task(id) => facts.task.map(|v| v == id),
            Criterion::Actor(id) => facts.actor.map(|v| v == id),
            Criterion::Agent(id) => facts.agent.map(|v| v == id),
            Criterion::Model(name) => facts.model.map(|v| v == name),
            Criterion::Provider(name) => facts.provider.map(|v| v == name),
            Criterion::Event(name) => facts.event.map(|v| v == name),
            Criterion::Claim(kind) => facts.claim.map(|v| v == *kind),
            Criterion::Status(Status::Task(status)) => facts.task_status.map(|v| v == *status),
            Criterion::Status(Status::Claim(status)) => facts.claim_status.map(|v| v == *status),
            Criterion::Superseded(value) => facts.superseded.map(|v| v == *value),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn any_declared_root_matches_and_absent_roots_stay_unknown_under_negation() {
        let workspace = WorkspaceId::parse("workspace").unwrap();
        let roots = [
            RootId::parse("first").unwrap(),
            RootId::parse("second").unwrap(),
        ];
        let mut facts = Facts {
            workspace: &workspace,
            timestamp: None,
            roots: Some(&roots),
            paths: None,
            task: None,
            actor: None,
            agent: None,
            model: None,
            provider: None,
            event: None,
            claim: None,
            task_status: None,
            claim_status: None,
            superseded: None,
        };
        let selector = Selector {
            schema_version: 1,
            tree: Tree::Match(Criterion::Root(roots[1].clone())),
        };
        assert_eq!(selector.evaluate(&facts).unwrap(), Truth::Match);
        facts.roots = None;
        let negated = Selector {
            schema_version: 1,
            tree: Tree::Not(Box::new(selector.tree)),
        };
        assert_eq!(negated.evaluate(&facts).unwrap(), Truth::Unknown);
    }
}
