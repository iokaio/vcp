# VCP skill authoring

Original VCP guidance, version 1.0.1. Create or revise a VCP skill package when a
reusable workflow needs guidance beyond ordinary project instructions. A README
edit or a one-off task does not by itself need a skill.

## Define the useful difference

Inspect the existing catalog and nearest skill before adding another. Identify
the workflow's output, the decisions where guidance helps, and realistic requests
that should and should not select it. Preserve the user's chosen stack and scope.
Do not expand a narrowly useful skill into a catchall for adjacent tasks.

Write original guidance from the project's contracts and authorized sources.
Do not copy third-party skill prose, templates, assets or evaluators. Instructions
should add task-specific judgment or a concrete procedure; omit generic advice
the model already knows. State relevant limitations without repeating every host
policy. A package describes work; it cannot grant tools, network access, credentials,
budget or permission to publish.

## Build within the existing package contract

Read [the package-format reference](references/package-format.md) when creating or
changing a descriptor, content inventory or version. It records VCP's format,
not a foreign skill loader. Use existing project validators and packaging helpers
where available; missing tooling must produce a not-run result, not a guessed pass.

Keep the description specific enough for selection. Keep the body concise and
self-contained for its common workflow. Add a resource only for useful conditional
detail or deterministic reusable work, and register its exact bytes. VCP currently
reads all declared resources on activation: splitting prose into files does not
reduce activation cost. Do not add scripts, templates or a new execution loop
without a concrete need and their own validation.

For an update, inspect callers and existing fixtures before removing resources.
Bump the package version when its content changes, update its hashes and any owning
catalog inventory, and preserve unrelated packages and historical receipts. A user
or workspace override is intentional; do not overwrite it to force the builtin
version to win. Metadata discovery must remain separate from body activation.
For metadata-only discovery, assert that neither bodies nor resources were read;
checking body reads alone leaves the resource boundary untested. Activation must
then verify the declared content under the target runtime's limits.

## Evaluate observable behavior

Before evaluating, write realistic normal, boundary, hostile-input, missing-tool
and near-miss tasks with independent expected outcomes. Separate authoring examples
from held-out fixtures. Hash the package, fixture inputs and rubric before runs;
keep the expected answers outside the task workspace.

Translate every required behavior into an explicit fixture condition, operation
and observable assertion. A checklist naming a behavior is not a test of it.
Check that the assertions reject a plausible incorrect implementation while
accepting valid behavior; record any requirement that remains unexercised.

For precedence work, isolate each required transition with competing sources and
observable selected identity, then change only the relevant source condition.
Exercise removal, disabling or invalidation where the target contract requires
it; do not infer those outcomes from one successful override. For each relevant
size limit, test the nearest representable values below, exactly at and above
the boundary using the contract's units. Include multibyte content for byte limits
and combined content where an aggregate limit applies. Derive limits and expected failure behavior from the
target implementation or contract, never from guessed constants.

Compare no skill, the nearest existing skill and the candidate with the same task
and tool authority. Check the actual artifact, preservation of other files,
unsupported-operation diagnostics and unwanted actions. Descriptor validity or
matching phrases in the response does not prove usefulness. Keep failed baseline
and candidate runs in the results. Record costs, latency, interventions and checks
not run, including any limits on a reviewer or model's observations.

Use the project's existing native discovery, activation, revocation and package
checks. Verify changed/missing bodies or resources fail integrity validation, a
near-miss does not acquire a new cue merely to improve selection statistics, and
version changes retain prior identities. Freeze any changed evaluation expectations
under a new version rather than rewriting a failed result.

Return the package/version, the behavior it adds, observed validation and remaining
qualification. Follow the owning work item's promotion gate; an unqualified
candidate is not an accepted default. A live comparison needs its own approved
budget and exact inputs. Do not spend or replay a failed campaign merely because
this workflow calls for evidence.
