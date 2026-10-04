// SPDX-License-Identifier: Apache-2.0
//! Immutable owner-selected subset of registered canonical model tools.
use codex_extension_api::{AllowedTools, ToolName};
#[cfg(windows)]
use serde_json::Value;
use std::collections::BTreeSet;

const NAMES: [&str; 7] = [
    "vcp_read",
    "vcp_list",
    "vcp_search",
    "vcp_patch",
    "vcp_exec",
    "vcp_verify",
    "vcp_mcp",
];

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct CanonicalTools(BTreeSet<String>);
impl Default for CanonicalTools {
    fn default() -> Self {
        Self(NAMES.into_iter().map(str::to_owned).collect())
    }
}
impl<'de> serde::Deserialize<'de> for CanonicalTools {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let names = <Vec<String> as serde::Deserialize>::deserialize(deserializer)?;
        let mut selected = BTreeSet::new();
        for name in names {
            if !NAMES.contains(&name.as_str()) || !selected.insert(name) {
                return Err(serde::de::Error::custom(
                    "canonical tool ceiling contains an unknown or duplicate name",
                ));
            }
        }
        Ok(Self(selected))
    }
}
/// Model tools implied by a recorded ceiling name rather than recorded
/// themselves. `vcp_skill` reads verified skill references and, with the
/// patch ceiling, materializes helpers as ordinary `vcp_patch` creations
/// (ADR-070/071), so it follows the read ceiling and leaves recorded ceilings
/// and legacy defaults unchanged.
const IMPLIED: [(&str, &str); 2] = [("vcp_skill", "vcp_read"), ("vcp_artifact_read", "vcp_read")];

impl CanonicalTools {
    pub fn contains(&self, name: &str) -> bool {
        self.0.contains(name)
    }
    /// Tool names a skill's `required_tools` may rely on: permitted canonical
    /// tools, `vcp_exec` and available process profiles when execution is in
    /// the ceiling, and `vcp_verify` when verification is configured. Shared by
    /// host activation and offline CLI inspection so the two cannot disagree.
    pub fn skill_match_tools(
        &self,
        process_profiles: impl IntoIterator<Item = String>,
        verification_configured: bool,
    ) -> BTreeSet<String> {
        let mut tools: BTreeSet<String> = [
            "vcp_read",
            "vcp_list",
            "vcp_search",
            "vcp_patch",
            "vcp_skill",
        ]
        .into_iter()
        .filter(|name| self.permits(name))
        .map(str::to_owned)
        .collect();
        if self.contains("vcp_exec") {
            let profiles: Vec<String> = process_profiles.into_iter().collect();
            if !profiles.is_empty() {
                tools.insert("vcp_exec".into());
                tools.extend(profiles);
            }
        }
        if verification_configured && self.contains("vcp_verify") {
            tools.insert("vcp_verify".into());
        }
        tools
    }
    /// Whether the model may call `name` under this ceiling.
    pub fn permits(&self, name: &str) -> bool {
        self.contains(name)
            || IMPLIED
                .iter()
                .any(|(implied, by)| *implied == name && self.contains(by))
    }
    pub fn is_all(&self) -> bool {
        self.0.len() == NAMES.len()
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
    pub fn allowed_tools(&self) -> AllowedTools {
        AllowedTools(
            self.names()
                .chain(
                    IMPLIED
                        .iter()
                        .filter(|(_, by)| self.contains(by))
                        .map(|(implied, _)| *implied),
                )
                .map(ToolName::plain)
                .collect(),
        )
    }
    #[cfg(windows)]
    pub fn schemas(&self) -> Value {
        let mut definitions = super::coding::schemas();
        if let Some(definitions) = definitions.as_array_mut() {
            definitions.retain(|value| {
                value["name"]
                    .as_str()
                    .is_some_and(|name| self.permits(name))
            });
        }
        definitions
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn canonical_tool_ceiling_validates_exact_names_and_filters_schemas() {
        let tools: CanonicalTools =
            serde_json::from_value(json!(["vcp_read", "vcp_list", "vcp_search"])).unwrap();
        let selected = tools.schemas();
        let names: BTreeSet<_> = selected
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            BTreeSet::from(["vcp_read", "vcp_list", "vcp_search", "vcp_skill", "vcp_artifact_read"])
        );
        assert!(!tools
            .allowed_tools()
            .contains(&ToolName::plain("vcp_verify")));
        assert!(tools.permits("vcp_skill"));
        assert!(tools
            .allowed_tools()
            .contains(&ToolName::plain("vcp_skill")));
        let patch: CanonicalTools = serde_json::from_value(json!(["vcp_patch"])).unwrap();
        assert!(!patch.permits("vcp_skill"));
        assert!(serde_json::from_value::<CanonicalTools>(json!(["vcp_skill"])).is_err());
        // One rule for skill required_tools at activation and in CLI inspection.
        let all = CanonicalTools::default();
        let listed = all.skill_match_tools(["python".to_owned()], false);
        assert!(
            listed.contains("vcp_exec")
                && listed.contains("python")
                && listed.contains("vcp_skill")
        );
        assert!(!listed.contains("vcp_verify"));
        assert!(all.skill_match_tools([], true).contains("vcp_verify"));
        assert!(!all.skill_match_tools([], true).contains("vcp_exec"));
        assert!(!tools
            .skill_match_tools(["python".to_owned()], true)
            .contains("vcp_exec"));
        assert_eq!(
            CanonicalTools::default().schemas(),
            crate::foundation::coding::schemas()
        );
        assert!(serde_json::from_value::<CanonicalTools>(json!([]))
            .unwrap()
            .schemas()
            .as_array()
            .unwrap()
            .is_empty());
        for invalid in [
            json!(["vcp_read", "vcp_read"]),
            json!(["read"]),
            json!(["functions.vcp_read"]),
            json!(["vcp_unknown"]),
            json!(null),
        ] {
            assert!(serde_json::from_value::<CanonicalTools>(invalid).is_err());
        }
    }
}
