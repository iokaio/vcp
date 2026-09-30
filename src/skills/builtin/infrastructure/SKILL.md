# Infrastructure, containers, and CI configuration

Original VCP guidance. This package supplies instructions, not a tool executor or authority.

## Inspect first

- CI workflow files (for example `.github/workflows`, GitLab CI, Azure Pipelines, Jenkinsfiles), their triggers, runner images, permissions, required checks and secret references.
- The IaC tool actually in use: Terraform/OpenTofu, Pulumi, CloudFormation/CDK, Kubernetes manifests, Kustomize, Helm charts, Dockerfiles and compose files.
- State and backends: remote state configuration, locking, workspaces or stacks, and which environment each maps to.
- Environment selection: variable files, overlays, values files, and how development, staging and production differ.
- Secrets handling: secret stores, CI secret names, sealed or encrypted files, and anything that looks like a committed credential.
- Lock files and pins: provider locks, chart dependencies, image digests and action versions.

## Identify the configuration system

Read the relevant Terraform/provider lock, container build/compose files, CI workflow, environment selection, and repository deployment guidance. Activate explicitly where current root cues do not detect these files. Do not treat all YAML as one schema or infer the target account from a sample value.

Trace inputs, secrets references, artifact provenance, runner permissions and network boundaries. Review immutable dependency pins, destructive replacements, state ownership, cache isolation and deployment ordering. A plan can refresh remote state or evaluate external providers, so it is not automatically an offline read.

## Proceed and verify

1. Find the project's format, validate and test commands from instructions, task-runner targets and CI. Candidates to confirm include the IaC formatter and validator, a manifest or chart linter, a workflow linter, container lint, and policy checks the repository already configures.
2. Use the installed declared formatter/schema validator or bounded fixture when authorized. Prefer checks that need no credentials or backend.
3. Plan before apply. A plan, diff, preview, template render or dry-run is still a proposal; apply, deploy, push and release are separate effects that need explicit authority for the named environment.
4. Container builds can fetch bases and execute build steps; Terraform init/plan/apply and CI reruns may have external effects. Never use discovered credentials or contact a remote environment without its explicit scope.
5. Never print secret values, state files or plan output containing sensitive attributes into logs or reports; reference them by name.

Evidence is the tool reported by the host, the exact command and working directory, validator results, and a rendered or planned diff with its target environment. Local validation does not prove a deployment works.

## Pitfalls

- Resource renames or changed immutable fields forcing destroy-and-recreate; review replacements in any plan.
- Unpinned images, floating tags and actions referenced by mutable tags.
- Overly broad CI token permissions, secrets exposed to untrusted pull request triggers, and caches shared across trust levels.
- Environment drift between overlays or values files, and defaults that silently target production.
- State edits, imports or force-unlocks performed to fix a local problem; these are remote effects.
- Windows: CRLF in shell steps and container entrypoints, executable bits lost on checkout, path separators and drive letters in volume mounts, case-insensitive paths hiding duplicate manifests, and long paths in provider caches.

## Output

Return the affected resource/workflow, evidence-backed risk, proposed bounded change, exact local checks and remaining deployment validation. Do not report a plan, build, or deployment as successful without its actual receipt.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
