// SPDX-License-Identifier: Apache-2.0
//! Owner-only conversion of a reviewed integration plan into the normal file
//! broker representation. This is not a model-callable tool or write authority.
use crate::*;
use vcp_repository::merge::IntegrationPlan;

pub fn prepare(
    root: Root,
    metadata_owner: Root,
    identity: Identity,
    child: TaskId,
    plan: IntegrationPlan,
) -> Result<Prepared> {
    if !plan.ready()
        || plan.changes.is_empty()
        || plan.changes.len() > 64
        || root.identity.workspace != identity.scope.workspace
        || root.identity.binding != identity.binding
        || metadata_owner.identity.workspace != identity.scope.workspace
        || metadata_owner.identity.binding != identity.binding
    {
        return Err(Error::Invalid("integration scope, conflicts or file count"));
    }
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    let mut changes = Vec::new();
    for change in &plan.changes {
        checked_path(&change.path, false)?;
        if !seen.insert(change.path.to_lowercase())
            || change.parent.path != change.path
            || change.parent.root != root.identity.root
            || change.parent.binding != root.identity.binding
            || change
                .path
                .split('/')
                .any(|p| p.eq_ignore_ascii_case(".vcp-child-owner"))
            || change.before == change.after
        {
            return Err(Error::Invalid("integration path or unchanged content"));
        }
        let parent = Path::new(&change.path).parent().unwrap_or(Path::new(""));
        root.hold(
            if parent.as_os_str().is_empty() {
                None
            } else {
                Some(parent)
            },
            true,
        )?;
        vcp_repository::instructions::revalidate_probes(
            std::slice::from_ref(&change.parent),
            std::slice::from_ref(&root),
        )?;
        match (&change.parent.observed, &change.before) {
            (Some(version), Some(bytes)) => {
                let actual = root.read(Path::new(&change.path), 1024 * 1024)?;
                if &actual.version != version || &actual.bytes != bytes {
                    return Err(vcp_repository::Error::Stale.into());
                }
            }
            (None, None) => {}
            _ => return Err(Error::Invalid("integration before image")),
        }
        let length = change.after.as_ref().map_or(0, Vec::len);
        total = total
            .checked_add(length)
            .ok_or(Error::Invalid("integration byte ceiling"))?;
        if length > 1024 * 1024 || total > 4 * 1024 * 1024 {
            return Err(Error::Invalid("integration byte ceiling"));
        }
        changes.push(patch::Change {
            path: change.path.clone(),
            before: change.parent.observed.clone(),
            before_bytes: change.before.clone(),
            after: change.after.clone(),
            rename_to: None,
            probes: vec![change.parent.clone()],
        });
    }
    let integration_index = plan
        .parent_index
        .clone()
        .map(|version| (metadata_owner, version));
    let integration_parent = Some((
        plan.parent_manifest
            .clone()
            .ok_or(Error::Invalid("integration parent manifest"))?,
        plan.parent_probes.clone(),
    ));
    if let Some((owner, version)) = &integration_index {
        owner.revalidate(version)?;
    }
    let probes: Vec<_> = changes.iter().flat_map(|c| c.probes.clone()).collect();
    let arguments = serde_json::json!({"kind":"child_integration", "child":child,
        "base":plan.base_fingerprint,"result":plan.child_fingerprint,
        "parent":plan.parent_fingerprint,"plan_sha256":vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&plan)?)});
    let schema = serde_json::json!({"name":"vcp_patch","owner_only":"child_integration/1"});
    let mut resources = probes
        .iter()
        .map(|probe| {
            Ok(Resource {
                root: root.identity.root.clone(),
                path: probe.path.clone(),
                write: true,
                version: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(probe)?),
            })
        })
        .collect::<std::result::Result<Vec<_>, serde_json::Error>>()?;
    resources.push(Resource {
        root: root.identity.root.clone(),
        path: String::new(),
        write: false,
        version: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&plan.parent_manifest)?),
    });
    let authority = vcp_policy::Prepared::new(Operation {
        scope: identity.scope,
        actor: identity.actor,
        host: identity.host,
        binding: identity.binding,
        authority: identity.authority,
        steering: identity.steering,
        policy: identity.policy,
        tool: "vcp_patch".into(),
        schema: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&schema)?),
        arguments: String::from_utf8(vcp_protocol::canonical_bytes(&arguments)?)
            .map_err(|_| Error::Invalid("integration arguments"))?,
        invocation: Invocation::Local,
        resources,
        effects: BTreeSet::from([EffectClass::Read, EffectClass::Write]),
        required_isolation: BTreeSet::from([Isolation::PathContainment, Isolation::OutputLimit]),
        timeout_ms: Units::new(30_000),
        output_bytes: ByteCount::new(1024 * 1024),
    })?;
    Ok(Prepared {
        authority,
        root,
        request: Request::Patch {
            patch: String::new(),
        },
        probes,
        result: serde_json::json!({"prepared_files":changes.len(),"integration":arguments,"index":plan.parent_index}),
        changes,
        integration_index,
        integration_parent,
        integration_child: Some(child),
    })
}
