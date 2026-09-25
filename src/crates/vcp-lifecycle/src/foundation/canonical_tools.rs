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
impl CanonicalTools {
    pub fn contains(&self, name: &str) -> bool {
        self.0.contains(name)
    }
    pub fn is_all(&self) -> bool {
        self.0.len() == NAMES.len()
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
    pub fn allowed_tools(&self) -> AllowedTools {
        AllowedTools(self.names().map(ToolName::plain).collect())
    }
    #[cfg(windows)]
    pub fn schemas(&self) -> Value {
        let mut definitions = super::coding::schemas();
        if let Some(definitions) = definitions.as_array_mut() {
            definitions.retain(|value| {
                value["name"]
                    .as_str()
                    .is_some_and(|name| self.contains(name))
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
            BTreeSet::from(["vcp_read", "vcp_list", "vcp_search"])
        );
        assert!(!tools
            .allowed_tools()
            .contains(&ToolName::plain("vcp_verify")));
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
