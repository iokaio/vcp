// SPDX-License-Identifier: Apache-2.0
//! Descriptor inspection uses current canonical read policy and the existing
//! owner lock. It never activates content, updates authority or starts inference.
use vcp_extensions::{
    discovery::{Catalog, MatchContext},
    skill_manifest::SourceRegistry,
};

pub fn on_host(
    host: &vcp_lifecycle::foundation::CanonicalHost,
    registry: SourceRegistry,
    offset: usize,
) -> Result<serde_json::Value, String> {
    let result = host.inspect_skills(registry)?;
    let catalog: Catalog =
        serde_json::from_value(result["catalog"].clone()).map_err(|e| e.to_string())?;
    let context: MatchContext =
        serde_json::from_value(result["context"].clone()).map_err(|e| e.to_string())?;
    let mut page = super::catalog_page(&catalog, Some(&context), offset);
    page["integrity"] = result["integrity"].clone();
    page["configured"] = serde_json::json!(true);
    page["configuration"] =
        serde_json::json!("trusted profile on disk; inspected through current canonical owner");
    if let Some(next) = page["next_offset"].as_u64() {
        page["next_command"] = serde_json::json!(format!("vcp skills list --offset {next}"));
    }
    Ok(page)
}

pub async fn execute(
    profile: &crate::settings::Profile,
    entry: &crate::settings::WorkspaceEntry,
    workspace: &std::path::Path,
    pipe: &str,
    offset: usize,
) -> Result<serde_json::Value, String> {
    match vcp_store::Store::open(
        &entry.config.canonical_root,
        entry.config.backend,
        &[workspace.to_owned()],
    )
    .await
    {
        Ok(store) => {
            let result = super::inspect(profile, &entry.config, store.state(), offset);
            store.close().await.map_err(|e| e.to_string())?;
            result
        }
        Err(vcp_store::Error::Conflict("canonical root already has an owner")) => {
            let configuration = super::prepare(profile, &entry.config)?;
            crate::control::request(
                pipe,
                &crate::control::Request::Skills {
                    workspace: entry.config.workspace.clone(),
                    registry: configuration.registry,
                    offset,
                },
            )
            .await
        }
        Err(error) => Err(error.to_string()),
    }
}
