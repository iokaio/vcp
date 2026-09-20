// SPDX-License-Identifier: Apache-2.0
//! Native catalog contracts over frozen projects, not a model usefulness grade.
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs, io::Write, path::Path, time::Instant};
use vcp_domain::{Revision, RootId, WorkspaceId};
use vcp_extensions::{activation, catalog, discovery, skill_manifest::*};
use vcp_repository::{Root, RootIdentity};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const FIXTURE: &[u8] = include_bytes!("../../../evals/skills/builtin/manifest.json");
fn identity(root: &str) -> RootIdentity {
    RootIdentity {
        workspace: WorkspaceId::new(),
        root: RootId::parse(root).unwrap(),
        repository: "builtin-qualification".into(),
        worktree: "fixture".into(),
        binding: Revision::ZERO,
    }
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("missing fixture string {key}").into())
}
fn strings(value: &Value) -> Result<BTreeSet<String>> {
    value
        .as_array()
        .ok_or("fixture array required")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "fixture string required".into())
        })
        .collect()
}
fn snapshot(root: &Root, expected: &Value) -> Result<Vec<Value>> {
    let files = expected
        .as_array()
        .ok_or("fixture file inventory required")?;
    if files.is_empty() || files.len() > 32 {
        return Err("fixture file count bound".into());
    }
    files.iter().map(|file| {
        let relative = string(file, "path")?;
        let observed = root.read(Path::new(relative), 64 * 1024)?;
        if observed.bytes.len() as u64 != file["bytes"].as_u64().ok_or("fixture size required")? ||
            observed.version.sha256 != string(file, "sha256")? {
            return Err(format!("fixture identity changed: {relative}").into());
        }
        Ok(json!({"path":relative,"bytes":observed.bytes.len(),"sha256":observed.version.sha256}))
    }).collect()
}
fn run_case(
    case: &Value,
    fixtures: &Path,
    registry: &SourceRegistry,
    discovered: &discovery::Catalog,
) -> Result<Value> {
    let project = string(case, "project")?;
    if !project.starts_with("projects/")
        || Path::new(project)
            .components()
            .any(|p| !matches!(p, std::path::Component::Normal(_)))
    {
        return Err("fixture project path must remain relative".into());
    }
    let root = Root::open(identity("fixture-project"), &fixtures.join(project))?;
    let before = snapshot(&root, &case["expected"]["preserve_files"])?;
    let context = vcp_lifecycle::foundation::skills::actual_match_context(
        &root,
        strings(&case["context"]["tools"])?,
    )?;
    let selected = discovered.resolve(string(case, "skill")?, &context)?;
    let started = Instant::now();
    let active = activation::activate(
        registry,
        discovered,
        &selected.qualified_id,
        &context,
        "explicit frozen builtin fixture selection",
        &discovery::Limits::default(),
    )?;
    let activation_ns = started.elapsed().as_nanos();
    let after = snapshot(&root, &case["expected"]["preserve_files"])?;
    let assertions = json!({
        "environment": context.environment == string(&case["context"], "environment")?,
        "observed_cues": context.cues == strings(&case["expected"]["observed_root_cues"])?,
        "suggestion": selected.matches(&context) == case["expected"]["automatic_suggestion"].as_bool().ok_or("suggestion expectation required")?,
        "analysis_requirements": selected.descriptor.required_tools == strings(&case["expected"]["required_analysis_tools"])?,
        "explicit_activation": case["expected"]["explicit_activation"] == true && active.qualified_id == selected.qualified_id,
        "pinned_body": active.body.version.sha256 == selected.descriptor.body.sha256,
        "one_lazy_body": active.reads.bodies == 1 && active.reads.resources == selected.descriptor.resources.len() as u64,
        "preserved_project": before == after,
    });
    let pass = assertions
        .as_object()
        .ok_or("assertion object")?
        .values()
        .all(|v| v == true);
    Ok(
        json!({"id":case["id"],"skill":case["skill"],"kind":case["kind"],"pass":pass,
        "assertions":assertions,"context":context,"activation_reads":active.reads,
        "activation_wall_ns":activation_ns,"body_sha256":active.body.version.sha256,
        "project_files":after,"command_execution":"not_run","behavior_rubric":"not_graded","live_quality":"not_run"}),
    )
}
fn main() -> Result<()> {
    let declared: Value = serde_json::from_slice(FIXTURE)?;
    let cases = declared["cases"]
        .as_array()
        .ok_or("frozen cases required")?;
    if declared["schema_version"] != 1
        || declared["declared_before_execution"] != true
        || declared["model_calls"] != 0
        || declared["required_contract_pass_rate_bps"] != 10000
        || declared["case_count"].as_u64() != Some(cases.len() as u64)
        || cases.len() != 42
    {
        return Err("invalid frozen builtin experiment".into());
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../skills/builtin")
        .canonicalize()?;
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/skills/builtin");
    let source_identity = identity(catalog::ROOT_ID);
    let root = Root::open(source_identity.clone(), &source)?;
    let integrity = catalog::verify(&root)?;
    let registry = SourceRegistry {
        version: 1,
        revision: Revision::ZERO,
        disabled: BTreeSet::new(),
        sources: vec![SkillSource {
            id: catalog::SOURCE_ID.into(),
            kind: SourceKind::Builtin,
            enabled: true,
            root: source_identity,
            path: source,
        }],
    };
    let started = Instant::now();
    let discovered = discovery::discover(&registry, &discovery::Limits::default())?;
    let discovery_ns = started.elapsed().as_nanos();
    catalog::verify_discovery(&integrity, &discovered)?;
    let attempts: Vec<_> = cases
        .iter()
        .map(
            |case| match run_case(case, &fixtures, &registry, &discovered) {
                Ok(result) => result,
                Err(error) => json!({"id":case["id"],"pass":false,"error":error.to_string()}),
            },
        )
        .collect();
    let pass = discovered.reads.bodies == 0
        && discovered.reads.resources == 0
        && attempts.iter().all(|a| a["pass"] == true);
    let report = json!({"schema":"p7-02-builtin-contract-experiment/1","revision":declared["revision"],
        "fixture_sha256":vcp_protocol::digest_bytes(FIXTURE),"catalog_sha256":integrity.catalog_sha256,
        "integrity_reads":integrity.reads,"discovery_reads":discovered.reads,"discovery_wall_ns":discovery_ns,
        "attempts":attempts,"pass":pass,"model_calls":0,"model_spend_usd":"0",
        "live_quality":"not_run","command_selection":"not_graded","toolchain_execution":"not_run",
        "limitations":["Descriptor suggestions are not proof of project semantics or useful model output.",
        "No compiler, package manager, Git index mutation, database, or external service was invoked.",
        "Read costs include separate catalog integrity metadata, normal discovery and activation dependency revalidation.",
        "Project cue and fixture-preservation reads are not included in catalog counters."]});
    let output = std::env::args_os()
        .nth(1)
        .ok_or("new report path required")?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.write_all(b"\n")?;
    if !pass {
        std::process::exit(1);
    }
    Ok(())
}
