// SPDX-License-Identifier: Apache-2.0
//! Frozen filesystem experiment, not a model quality or runtime toolchain claim.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs, io::Write, time::Instant};
use vcp_domain::{Revision, RootId, WorkspaceId};
use vcp_extensions::{activation, discovery, skill_manifest::*};
use vcp_repository::RootIdentity;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const MANIFEST: &[u8] = include_bytes!("../../../evals/skills/manifest.json");
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    revision: String,
    declared_before_execution: bool,
    catalog_sizes: Vec<usize>,
    body_bytes: usize,
    resource_bytes: usize,
    matching_cue: String,
    nonmatching_cue: String,
    required_tool: String,
    required_assertion_rate_bps: u32,
    model_calls: u32,
    purpose: String,
}
fn run_case(manifest: &Manifest, count: usize) -> Result<Value> {
    let temp = tempfile::tempdir()?;
    let body = "x".repeat(manifest.body_bytes);
    let resource = vec![42u8; manifest.resource_bytes];
    for index in 0..count {
        let package = temp.path().join(format!("skill-{index:04}"));
        fs::create_dir(&package)?;
        let descriptor = SkillDescriptor {
            schema_version: 1,
            id: format!("skill-{index:04}"),
            version: "1.0.0".into(),
            description: format!("Synthetic Rust discovery fixture {index}"),
            source: "original-vcp-qualification-fixture".into(),
            license: "Apache-2.0".into(),
            vcp_version: 1,
            cues: BTreeSet::from([manifest.matching_cue.clone()]),
            environments: BTreeSet::from(["windows".into()]),
            required_tools: BTreeSet::from([manifest.required_tool.clone()]),
            body: ContentRef {
                path: "SKILL.md".into(),
                sha256: vcp_protocol::digest_bytes(body.as_bytes()),
            },
            resources: vec![ContentRef {
                path: "reference.bin".into(),
                sha256: vcp_protocol::digest_bytes(&resource),
            }],
        };
        fs::write(
            package.join(DESCRIPTOR_NAME),
            serde_json::to_vec(&descriptor)?,
        )?;
        fs::write(package.join("SKILL.md"), &body)?;
        fs::write(package.join("reference.bin"), &resource)?;
    }
    let registry = SourceRegistry {
        version: 1,
        revision: Revision::ZERO,
        sources: vec![SkillSource {
            id: "fixture".into(),
            kind: SourceKind::Workspace,
            enabled: true,
            root: RootIdentity {
                workspace: WorkspaceId::new(),
                root: RootId::new(),
                repository: "qualification".into(),
                worktree: "qualification".into(),
                binding: Revision::ZERO,
            },
            path: temp.path().to_path_buf(),
        }],
        disabled: BTreeSet::new(),
    };
    let limits = discovery::Limits::default();
    let started = Instant::now();
    let catalog = discovery::discover(&registry, &limits)?;
    let discovery_ns = started.elapsed().as_nanos();
    let context = discovery::MatchContext {
        environment: "windows".into(),
        tools: BTreeSet::from([manifest.required_tool.clone()]),
        cues: BTreeSet::from([manifest.matching_cue.clone()]),
    };
    let matching = catalog.matching(&context)?.len();
    let mut nonmatching = context.clone();
    nonmatching.cues = BTreeSet::from([manifest.nonmatching_cue.clone()]);
    let nonmatching_count = catalog.matching(&nonmatching)?.len();
    let listing: Vec<_> = catalog
        .skills
        .iter()
        .map(|skill| {
            json!({
                "qualified_id": skill.qualified_id,
                "description": skill.descriptor.description,
                "cues": skill.descriptor.cues,
                "environments": skill.descriptor.environments,
                "required_tools": skill.descriptor.required_tools
            })
        })
        .collect();
    let activation_started = Instant::now();
    let activated = activation::activate(
        &registry,
        &catalog,
        "skill-0000",
        &context,
        "explicit frozen qualification selection",
        &limits,
    )?;
    let activation_ns = activation_started.elapsed().as_nanos();
    let assertions = json!({
        "all_descriptors": catalog.skills.len() == count && catalog.reads.descriptors == count as u64,
        "no_diagnostics": catalog.diagnostics.is_empty(),
        "discovery_loads_no_content": catalog.reads.bodies == 0 && catalog.reads.resources == 0,
        "matching_project": matching == count,
        "nonmatching_project": nonmatching_count == 0,
        "single_body": activated.reads.bodies == 1 && activated.body.bytes == body.as_bytes(),
        "single_resource": activated.reads.resources == 1 && activated.resources.first().is_some_and(|value| value.bytes == resource),
        "descriptor_cost_below_eager_body_bytes": catalog.reads.descriptor_bytes < (count * manifest.body_bytes) as u64
    });
    let pass = assertions
        .as_object()
        .unwrap()
        .values()
        .all(|value| value == true);
    Ok(
        json!({"catalog_size":count,"pass":pass,"assertions":assertions,
        "discovery_reads":catalog.reads,"activation_reads":activated.reads,
        "descriptor_listing_json_bytes":serde_json::to_vec(&listing)?.len(),
        "available_body_bytes":count*manifest.body_bytes,
        "activated_body_bytes":activated.body.bytes.len(),
        "discovery_wall_ns":discovery_ns,"activation_wall_ns":activation_ns,
        "matching":matching,"nonmatching":nonmatching_count}),
    )
}
fn main() -> Result<()> {
    let manifest: Manifest = serde_json::from_slice(MANIFEST)?;
    if !manifest.declared_before_execution
        || manifest.required_assertion_rate_bps != 10000
        || manifest.model_calls != 0
        || manifest.catalog_sizes.is_empty()
    {
        return Err("invalid frozen experiment declaration".into());
    }
    let attempts: Vec<_> = manifest
        .catalog_sizes
        .iter()
        .map(|count| match run_case(&manifest, *count) {
            Ok(value) => value,
            Err(error) => json!({"catalog_size":count,"pass":false,"error":error.to_string()}),
        })
        .collect();
    let pass = attempts.iter().all(|attempt| attempt["pass"] == true);
    let report = json!({"schema":manifest.schema,"revision":manifest.revision,"purpose":manifest.purpose,
        "manifest_sha256":vcp_protocol::digest_bytes(MANIFEST),"attempts":attempts,"pass":pass,
        "model_calls":0,"model_spend_usd":"0","model_quality":"not_run",
        "limitations":["Single native host; cold/cache effects are not controlled.",
            "Listing byte counts are the declared projection, not a model token estimate.",
            "Body/resource dependency revalidation reads are reported separately.",
            "Skills are synthetic data; no toolchain or model task usefulness is qualified."]});
    let bytes = serde_json::to_vec_pretty(&report)?;
    if let Some(path) = std::env::args_os().nth(1) {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
    } else {
        std::io::stdout().write_all(&bytes)?;
    }
    if !pass {
        std::process::exit(1);
    }
    Ok(())
}
