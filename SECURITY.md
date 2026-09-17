# Security policy

Report suspected vulnerabilities privately. Do not publish exploit details, credentials, or sensitive workspace content in a GitHub issue or pull request.

## Reporting a vulnerability

Email **info@ioka.io** with `VCP security` in the subject. If GitHub private vulnerability reporting is available on the [repository Security tab](https://github.com/iokaio/vcp/security), you may use its **Report a vulnerability** action instead. Email remains the fallback if that action is unavailable.

Include the affected commit or release, operating environment, impacted component or design section, expected and observed behavior, and the smallest safe reproduction. Explain the likely impact and any workaround. Use synthetic data and test credentials; do not send live provider keys, recovery keys, private repositories, or unredacted task history. Test only systems and data you are authorized to use.

Maintainers will assess the report, coordinate next steps through the private channel, and discuss attribution and disclosure with the reporter. There is no guaranteed response time, paid bounty program, or published security support window at this stage.

## Supported versions

VCP currently has design documentation and a plan, with no supported runtime release. Reports about unsafe design assumptions, repository content, or development tooling are welcome. A supported-version and update policy must be published with the first release; this file does not promise maintenance of unreleased snapshots or future versions.

## Areas of concern

The planned design treats the following as significant security boundaries:

- Tool or process execution outside the task's granted authority, including path/junction escapes and delegated effects.
- Untrusted repository, model, or MCP content gaining instruction or execution authority.
- Workspace scope violations or retrieval of pruned or restricted material.
- Provider credentials or recovery material leaking through prompts, logs, reports, or exports.
- Plaintext snapshot contents entering a cloud destination, acceptance of tampered state, or bypass of trusted-writer checks.
- Incorrect evidence or accounting that conceals external effects or unadmitted model spend.

Active local files and state are intentionally plaintext, while cloud-bound VCP snapshots must be encrypted before publication. Coding model requests use OpenRouter; embeddings are intended to remain local. These are planned boundaries, not a claim that security controls have been implemented or audited. See the [architecture](docs/architecture/vcp-what.md) for the full contracts.

If you accidentally disclose a credential, revoke or rotate it promptly and report the exposure privately. Removing a file from the latest revision alone does not undo disclosure.

Adapted from [Munarium's security guidance](https://github.com/iokaio/munarium/blob/8da666067000ca1ee9c131bc67e70b978862faa3/SECURITY.md) for VCP's scope and current development stage; see [attribution](THIRD_PARTY_NOTICES.md).
