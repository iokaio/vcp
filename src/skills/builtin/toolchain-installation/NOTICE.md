# Toolchain installation attribution and limits

The VCP instructions, platform/stack references and PowerShell ZIP helper are
original Apache-2.0 work. They use PowerShell and .NET standard facilities and
redistribute no package manager, toolchain binary, vendor installer or dependency.
The Apache-2.0 license is included as LICENSE.txt.

The existing Anthropic skills audit at revision
8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4 has no general development-toolchain
installation package to port. No upstream skill prose or code is copied here.
The official sources linked in the references are behavior documentation, checked
October 5, 2026; consult current vendor instructions for the selected version.
Downloaded tools retain their own licenses and installation requirements.

Scope: installation planning and execution guidance across common development
stacks on Windows, macOS and Linux; a separately tested PowerShell 7.4+ helper for
checksum-verified portable ZIPs. The helper does not manage system packages,
execute downloaded code, change PATH, grant profiles or install itself. Broader
vendor/OS workflows require their actual tools and existing runtime authorization.
Package availability does not certify every installer, platform or project.
The helper's path checks supplement the host's execution boundary; they are not
an OS sandbox against concurrent hostile filesystem mutation.

Owning work: P7-02 / TI-01, owner direction October 5, 2026. Functional, package
and native VCP materialization/execution evidence is recorded in the repository's
skills upgrade ledger. No general model-quality or all-platform qualification is
claimed from synthetic tests.
