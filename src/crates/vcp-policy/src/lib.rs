// SPDX-License-Identifier: Apache-2.0
//! Pure authority decisions. Store transactions and native effects belong to
//! the canonical host/broker; money is deliberately absent from this API.
use std::collections::BTreeSet;
use vcp_domain::{accounting::valid_hash, policy::*, workspace::*, *};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid prepared authority input: {0}")]
    Invalid(&'static str),
    #[error("authority serialization: {0}")]
    Json(#[from] serde_json::Error),
}

/// Private immutable identity, not an execution capability. Only the trusted
/// registered tool prepares effect/resource classifications for this object.
#[derive(Clone, Debug)]
pub struct Prepared {
    operation: Operation,
    digest: String,
}
impl Prepared {
    pub fn new(operation: Operation) -> Result<Self> {
        validate_operation(&operation)?;
        let digest = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&operation)?);
        Ok(Self { operation, digest })
    }
    pub fn operation(&self) -> &Operation {
        &self.operation
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// Native observations supplied by the host at the current admission point.
/// These facts cannot be deserialized from a request or model/tool content.
pub struct Facts<'a> {
    pub workspace: &'a Workspace,
    pub scope: &'a Scope,
    pub actor: &'a ActorId,
    pub steering: SteeringRevision,
    pub policy: PolicyRevision,
    pub now: Timestamp,
    pub owner_current: bool,
    pub task_running: bool,
    pub resources_current: bool,
    pub registered_roots: &'a BTreeSet<RootId>,
    pub isolation: &'a BTreeSet<Isolation>,
    pub host_denials: &'a [Denial],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow {
        origin: String,
        grant: Option<GrantId>,
    },
    Deny {
        origin: String,
        reason: String,
    },
    Question {
        digest: String,
        reason: String,
    },
}
fn deny(origin: &str, reason: &str) -> Decision {
    Decision::Deny {
        origin: origin.into(),
        reason: reason.into(),
    }
}

pub fn evaluate(
    prepared: &Prepared,
    policy: &Policy,
    grants: &[Grant],
    facts: &Facts<'_>,
) -> Result<Decision> {
    validate_policy(policy)?;
    let op = prepared.operation();
    if !facts.owner_current || !facts.task_running {
        return Ok(deny("controller", "owner or task admission is held"));
    }
    if &op.scope != facts.scope
        || &op.actor != facts.actor
        || op.scope.workspace != facts.workspace.id
        || policy.workspace != facts.workspace.id
        || op.host != facts.workspace.binding.host
        || op.binding != facts.workspace.binding.revision
        || op.authority != facts.workspace.authority
        || op.steering != facts.steering
        || op.policy != facts.policy
        || policy.revision != facts.policy
        || !facts.resources_current
        || op
            .resources
            .iter()
            .any(|r| !facts.registered_roots.contains(&r.root))
    {
        return Ok(deny(
            "current identity",
            "scope, host, policy, steering or source observation changed",
        ));
    }
    if facts.workspace.trust != Trust::Trusted {
        return Ok(deny(
            "workspace trust",
            "workspace is not trusted for tool dispatch",
        ));
    }
    if op.timeout_ms > policy.timeout_ceiling_ms || op.output_bytes > policy.output_ceiling_bytes {
        return Ok(deny(
            "resource ceiling",
            "operation exceeds configured time or output limits",
        ));
    }
    if !op.required_isolation.is_subset(facts.isolation) {
        return Ok(deny(
            "platform capabilities",
            "requested isolation is unavailable",
        ));
    }
    // A user-configured document never changes the host's separate hard rules.
    for rule in facts.host_denials.iter().chain(policy.denials.iter()) {
        validate_denial(rule)?;
        if denies(rule, op) {
            return Ok(deny(&rule.id, &rule.reason));
        }
    }
    if policy.mode == Autonomy::Plan && op.effects != BTreeSet::from([EffectClass::Read]) {
        return Ok(deny(
            "plan",
            "planning mode does not dispatch mutations or processes",
        ));
    }
    for grant in grants {
        validate_grant(grant)?;
        if !grant.revoked
            && grant.actor == op.actor
            && grant.scope.contains(&op.scope)
            && grant.host == op.host
            && grant.binding == op.binding
            && grant.authority == op.authority
            && grant.policy == op.policy
            && grant.expires_at > facts.now
            && matches_target(&grant.target, prepared)
        {
            return Ok(Decision::Allow {
                origin: grant.reason.clone(),
                grant: Some(grant.id.clone()),
            });
        }
    }
    let inside = !op.resources.is_empty()
        && op
            .resources
            .iter()
            .all(|r| policy.workspace_roots.contains(&r.root));
    let local = matches!(op.invocation, Invocation::Local);
    let read = op.effects == BTreeSet::from([EffectClass::Read]);
    let edit = op
        .effects
        .is_subset(&BTreeSet::from([EffectClass::Read, EffectClass::Write]));
    let automatic = inside && local && (read || (policy.mode == Autonomy::Workspace && edit))
        || policy.mode == Autonomy::Autonomous
            && inside
            && op.effects.is_subset(&policy.automatic_effects);
    if automatic {
        return Ok(Decision::Allow {
            origin: format!("{:?} preset", policy.mode),
            grant: None,
        });
    }
    Ok(Decision::Question {
        digest: prepared.digest().into(),
        reason: "operation needs scoped user authority".into(),
    })
}

fn matches_target(target: &GrantTarget, prepared: &Prepared) -> bool {
    let op = prepared.operation();
    match target {
        GrantTarget::Exact { digest } => digest == prepared.digest(),
        GrantTarget::Configured {
            tool,
            schema,
            arguments_digest,
            invocation,
            effects,
            roots,
            paths,
            isolation,
            timeout_ms,
            output_bytes,
        } => {
            tool == &op.tool
                && schema == &op.schema
                && arguments_digest == &vcp_protocol::digest_bytes(op.arguments.as_bytes())
                && invocation == &op.invocation
                && &op.effects == effects
                && &op.required_isolation == isolation
                && op.timeout_ms <= *timeout_ms
                && op.output_bytes <= *output_bytes
                && !op.resources.is_empty()
                && op
                    .resources
                    .iter()
                    .all(|r| roots.contains(&r.root) && paths.iter().any(|p| within(&r.path, p)))
        }
    }
}
fn denies(rule: &Denial, op: &Operation) -> bool {
    if rule.tool.as_ref().is_some_and(|tool| tool != &op.tool) {
        return false;
    }
    // Opaque operations have no complete resource/effect closure. A caller's
    // declared inputs cannot prove that a scoped denial is irrelevant.
    if op.effects.contains(&EffectClass::Opaque) {
        return true;
    }
    if !rule.effects.is_empty() && rule.effects.is_disjoint(&op.effects) {
        return false;
    }
    if rule.roots.is_empty() && rule.paths.is_empty() {
        return true;
    }
    op.resources.iter().any(|r| {
        (rule.roots.is_empty() || rule.roots.contains(&r.root))
            && (rule.paths.is_empty()
                || rule
                    .paths
                    .iter()
                    .any(|p| !p.is_ascii() || !r.path.is_ascii() || within(&r.path, p)))
    })
}
fn within(path: &str, prefix: &str) -> bool {
    // Exact Unicode spelling for grants; ambiguous Unicode denial comparisons
    // conservatively deny the root above. Do not invent Windows case folding.
    if !path.is_ascii() || !prefix.is_ascii() {
        return prefix.is_empty()
            || path == prefix
            || path
                .strip_prefix(prefix)
                .is_some_and(|tail| tail.starts_with('/'));
    }
    let path = path.to_lowercase();
    let prefix = prefix.to_lowercase();
    prefix.is_empty()
        || path == prefix
        || path
            .strip_prefix(&prefix)
            .is_some_and(|tail| tail.starts_with('/'))
}
pub fn relative(path: &str) -> bool {
    path.len() <= 32768
        && (path.is_empty()
            || path.split('/').all(|part| {
                !part.is_empty()
                    && !matches!(part, "." | "..")
                    && !part.ends_with(['.', ' '])
                    && !part.chars().any(|c| {
                        c.is_control()
                            || matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
                    })
            }))
}
fn text(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && !value.contains('\0')
}
fn validate_invocation(invocation: &Invocation) -> Result<()> {
    match invocation {
        Invocation::Local => (),
        Invocation::Process {
            executable,
            executable_identity,
            arguments,
            directory,
            environment_digest,
            ..
        } => {
            if !text(executable, 32768)
                || !valid_hash(executable_identity)
                || !text(directory, 32768)
                || !valid_hash(environment_digest)
                || arguments.len() > 256
                || arguments
                    .iter()
                    .any(|a| a.len() > 32768 || a.contains('\0'))
            {
                return Err(Error::Invalid("process identity"));
            }
        }
        Invocation::Remote {
            server_identity,
            endpoint,
        } => {
            if !valid_hash(server_identity) || !text(endpoint, 4096) {
                return Err(Error::Invalid("remote identity"));
            }
        }
    }
    Ok(())
}
pub fn validate_operation(op: &Operation) -> Result<()> {
    if !text(&op.tool, 128)
        || !valid_hash(&op.schema)
        || op.arguments.len() > 256 * 1024
        || op.resources.len() > 1024
        || op.effects.is_empty()
        || op.timeout_ms == Units::ZERO
        || op.output_bytes == ByteCount::ZERO
    {
        return Err(Error::Invalid("operation limits"));
    }
    let args: serde_json::Value = serde_json::from_str(&op.arguments)?;
    if !args.is_object() || vcp_protocol::canonical_bytes(&args)? != op.arguments.as_bytes() {
        return Err(Error::Invalid("canonical arguments"));
    }
    validate_invocation(&op.invocation)?;
    let mut seen = BTreeSet::new();
    for resource in &op.resources {
        if !relative(&resource.path)
            || !valid_hash(&resource.version)
            || !seen.insert((resource.root.clone(), resource.path.to_lowercase()))
            || resource.write && !op.effects.contains(&EffectClass::Write)
        {
            return Err(Error::Invalid("resource classification"));
        }
    }
    match op.invocation {
        Invocation::Local if op.effects.contains(&EffectClass::Execute) => {
            return Err(Error::Invalid("local executable classification"))
        }
        Invocation::Process { .. }
            if !op.effects.contains(&EffectClass::Execute)
                || !op.effects.contains(&EffectClass::Opaque) =>
        {
            return Err(Error::Invalid("process effects must be conservative"))
        }
        Invocation::Remote { .. }
            if !op.effects.contains(&EffectClass::Network)
                || !op.effects.contains(&EffectClass::Opaque) =>
        {
            return Err(Error::Invalid("remote effects must be conservative"))
        }
        _ => (),
    }
    Ok(())
}
fn validate_denial(rule: &Denial) -> Result<()> {
    if !text(&rule.id, 128)
        || !text(&rule.reason, 4096)
        || rule.paths.len() > 128
        || rule.paths.iter().any(|p| !relative(p))
        || rule.roots.len() > 64
    {
        return Err(Error::Invalid("denial rule"));
    }
    Ok(())
}
pub fn validate_policy(policy: &Policy) -> Result<()> {
    if policy.denials.len() > 256
        || policy.workspace_roots.len() > 64
        || policy.timeout_ceiling_ms == Units::ZERO
        || policy.timeout_ceiling_ms.get() > 3_600_000
        || policy.output_ceiling_bytes == ByteCount::ZERO
        || policy.output_ceiling_bytes.get() > 1024 * 1024 * 1024
    {
        return Err(Error::Invalid("policy limits"));
    }
    let mut ids = BTreeSet::new();
    for rule in &policy.denials {
        validate_denial(rule)?;
        if !ids.insert(&rule.id) {
            return Err(Error::Invalid("duplicate rule"));
        }
    }
    Ok(())
}
pub fn validate_grant(grant: &Grant) -> Result<()> {
    if !text(&grant.reason, 4096) || grant.expires_at == Timestamp::ZERO {
        return Err(Error::Invalid("grant provenance/expiry"));
    }
    match &grant.target {
        GrantTarget::Exact { digest } if !valid_hash(digest) => {
            return Err(Error::Invalid("grant operation hash"))
        }
        GrantTarget::Configured {
            tool,
            schema,
            arguments_digest,
            invocation,
            effects,
            roots,
            paths,
            timeout_ms,
            output_bytes,
            ..
        } => {
            validate_invocation(invocation)?;
            if !text(tool, 128)
                || !valid_hash(schema)
                || !valid_hash(arguments_digest)
                || effects.is_empty()
                || roots.is_empty()
                || roots.len() > 64
                || paths.is_empty()
                || paths.len() > 128
                || paths.iter().any(|p| !relative(p))
                || timeout_ms == &Units::ZERO
                || output_bytes == &ByteCount::ZERO
            {
                return Err(Error::Invalid("configured grant"));
            }
        }
        _ => (),
    }
    Ok(())
}
