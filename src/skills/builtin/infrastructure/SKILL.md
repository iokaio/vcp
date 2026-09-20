# Infrastructure, containers, and CI configuration

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Identify the configuration system

Read the relevant Terraform/provider lock, container build/compose files, CI workflow, environment selection, and repository deployment guidance. Activate explicitly where current root cues do not detect these files. Do not treat all YAML as one schema or infer the target account from a sample value.

Trace inputs, secrets references, artifact provenance, runner permissions and network boundaries. Review immutable dependency pins, destructive replacements, state ownership, cache isolation and deployment ordering. A plan can refresh remote state or evaluate external providers, so it is not automatically an offline read.

Use the installed declared formatter/schema validator or bounded fixture when authorized. Container builds can fetch bases and execute build steps; Terraform init/plan/apply and CI reruns may have external effects. Never use discovered credentials or contact a remote environment without its explicit scope.

## Output

Return the affected resource/workflow, evidence-backed risk, proposed bounded change, exact local checks and remaining deployment validation. Do not report a plan, build, or deployment as successful without its actual receipt.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
