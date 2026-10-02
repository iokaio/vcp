// SPDX-License-Identifier: Apache-2.0
//! Built-in model sets (ADR-077). A set only suggests models, endpoints and
//! profile limits. Every member still passes the live catalog check and the
//! accounted two-call qualification; an unavailable member is never replaced.
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub const SCHEMA: &str = "vcp-model-sets/1";
/// Request-fee ceiling for every member; the verified catalogs list no fee.
pub const REQUEST_PRICE_LIMIT: &str = "0.001";

/// Profile limits for a set. Measured for Qwen 3.8 Max
/// (docs/evaluations/p7-qwen38-reasoning-budget-2026-09-22.md); they are upper
/// bounds that profile preparation clamps to each endpoint's maximum.
pub struct Limits {
    pub output_tokens: u32,
    pub provider_timeout_seconds: u32,
    pub deadline_seconds: u32,
    pub max_requests: u32,
}
pub const LIMITS: Limits = Limits {
    output_tokens: 16_384,
    provider_timeout_seconds: 180,
    deadline_seconds: 900,
    max_requests: 16,
};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, clap::ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Main,
    Child,
    Compaction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Default,
    Vendor,
    Level,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub role: Role,
    pub model: String,
    pub endpoint: String,
    /// Research label from docs/architecture/model-groups.md, not VCP evidence.
    pub level: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Set {
    pub id: String,
    pub category: Category,
    pub title: String,
    pub description: String,
    pub members: Vec<Member>,
    pub notes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema: String,
    endpoints_verified: String,
    research: String,
    sets: Vec<Set>,
}

impl Set {
    /// The member assigned to a role; an unassigned role uses the main model.
    pub fn member(&self, role: Role) -> Option<&Member> {
        self.members
            .iter()
            .find(|member| member.role == role)
            .or_else(|| self.members.iter().find(|member| member.role == Role::Main))
    }

    /// Each distinct model and endpoint once, with every role it serves.
    pub fn distinct(&self) -> Vec<(Vec<Role>, &Member)> {
        let mut rows: Vec<(Vec<Role>, &Member)> = Vec::new();
        for member in &self.members {
            match rows
                .iter_mut()
                .find(|(_, seen)| seen.model == member.model && seen.endpoint == member.endpoint)
            {
                Some((roles, _)) => roles.push(member.role),
                None => rows.push((vec![member.role], member)),
            }
        }
        rows
    }
}

fn identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 256
        && text
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-._/".contains(&c))
        && text
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}

fn validate(catalog: &Catalog) -> Result<(), String> {
    if catalog.schema != SCHEMA
        || catalog.endpoints_verified.is_empty()
        || catalog.research.is_empty()
        || catalog.sets.is_empty()
        || catalog.sets.len() > 32
    {
        return Err("built-in model set catalog is invalid".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for set in &catalog.sets {
        let mut roles = std::collections::BTreeSet::new();
        if !crate::profile_selection::valid_set(&set.id)
            || !ids.insert(set.id.as_str())
            || set.title.is_empty()
            || set.members.is_empty()
            || set.members.len() > 3
            || !set.members.iter().any(|member| member.role == Role::Main)
            || !set.members.iter().all(|member| {
                roles.insert(member.role)
                    && identifier(&member.model)
                    && identifier(&member.endpoint)
                    && matches!(
                        member.level.as_str(),
                        "frontier" | "high" | "medium" | "low" | "unrated"
                    )
            })
        {
            return Err(format!("built-in model set {} is invalid", set.id));
        }
    }
    Ok(())
}

/// Every built-in set, validated once.
pub fn catalog() -> Result<&'static [Set], String> {
    static CATALOG: OnceLock<Result<Vec<Set>, String>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            let catalog: Catalog = serde_json::from_str(include_str!("model_sets.json"))
                .map_err(|_| "built-in model set catalog is malformed".to_owned())?;
            validate(&catalog)?;
            Ok(catalog.sets)
        })
        .as_deref()
        .map_err(Clone::clone)
}

pub fn find(id: &str) -> Result<&'static Set, String> {
    catalog()?.iter().find(|set| set.id == id).ok_or_else(|| {
        let known: Vec<_> = catalog()
            .map(|sets| sets.iter().map(|set| set.id.as_str()).collect())
            .unwrap_or_default();
        format!(
            "unknown model set {id}; choose one of: {}",
            known.join(", ")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_sets_are_valid_and_cover_the_planned_groups() {
        let sets = catalog().unwrap();
        let ids: Vec<_> = sets.iter().map(|set| set.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "quick",
                "qwen",
                "openai",
                "anthropic",
                "glm",
                "frontier",
                "high",
                "medium"
            ]
        );
        let quick = find("quick").unwrap();
        assert_eq!(quick.category, Category::Default);
        let main = quick.member(Role::Main).unwrap();
        assert_eq!(main.model, "qwen/qwen3.8-max-0902");
        assert_eq!(main.endpoint, "alibaba");
        assert_eq!(quick.member(Role::Child), Some(main));
        assert_eq!(quick.distinct().len(), 1);
        for set in sets {
            assert!(matches!(
                set.category,
                Category::Default | Category::Vendor | Category::Level
            ));
            // Every member must be selectable by the provider setup command.
            for member in &set.members {
                assert!(crate::provider_setup::production::valid_selection(
                    &member.model,
                    &member.endpoint,
                    REQUEST_PRICE_LIMIT,
                    "25"
                )
                .is_ok());
            }
        }
        let glm = find("glm").unwrap();
        let distinct = glm.distinct();
        assert_eq!(distinct.len(), 2);
        assert_eq!(distinct[1].0, [Role::Child, Role::Compaction]);
        assert!(find("fable").unwrap_err().contains("quick, qwen"));
    }

    #[test]
    fn invalid_catalogs_are_rejected() {
        let valid: Catalog = serde_json::from_str(include_str!("model_sets.json")).unwrap();
        let mut duplicate_role = Catalog {
            sets: valid.sets.clone(),
            ..valid
        };
        let repeated = duplicate_role.sets[0].members[0].clone();
        duplicate_role.sets[0].members.push(repeated);
        assert!(validate(&duplicate_role).is_err());
        let mut no_main = Catalog {
            sets: duplicate_role.sets.clone(),
            schema: SCHEMA.into(),
            endpoints_verified: "2026-10-02".into(),
            research: "docs".into(),
        };
        no_main.sets[0].members = vec![Member {
            role: Role::Child,
            model: "qwen/qwen3.8-flash".into(),
            endpoint: "alibaba".into(),
            level: "medium".into(),
        }];
        assert!(validate(&no_main).is_err());
        no_main.sets[0].members[0].role = Role::Main;
        no_main.sets[0].members[0].endpoint = "../alibaba".into();
        assert!(validate(&no_main).is_err());
    }
}
