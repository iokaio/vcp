# Support

VCP is an experiment in design and planning. There is no supported application release, installation package, compatibility matrix, or production support service yet. The [Apache 2.0 license](LICENSE) does not provide a support commitment.

## Where to ask

| Request | Venue |
|---|---|
| Design or contribution question | [GitHub Issues](https://github.com/iokaio/vcp/issues) |
| Documentation defect or reproducible bug | GitHub Issues using the bug report template |
| Feature or architecture proposal | GitHub Issues using the feature request template |
| Suspected vulnerability or exposed secret | Private channels in [SECURITY.md](SECURITY.md) |
| Conduct concern | Private channel in [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) |

Discussions may be used if enabled later. Issues are the default public venue today. Community assistance is offered as maintainers and contributors have time; there is no guaranteed response or resolution schedule.

## Make a report useful

Identify the relevant document and plan task, or the commit/release and component if code is involved. Describe what you expected, what happened, and a minimal reproduction. For future runtime reports, include Windows version, shell, backend, relevant configuration without secrets, and the exact failing command and exit status.

Use synthetic examples. Review logs for provider tokens, private source, prompts, workspace history, and recovery material before sharing them. Do not upload full local data directories or portable snapshots to a public issue.

## Scope and expectations

The first planned delivery target is a native Windows CLI. Other platforms, public APIs, SDKs, and editor integration remain deferred as described in the [plan](docs/plan/README.md). Design examples are not supported commands, and research tables are not a current provider or model compatibility promise.

Maintainers may ask for a smaller reproduction, refer an upstream defect to its project, defer work, or close a proposal that falls outside current scope. A release support matrix and real installation/troubleshooting instructions will accompany implemented, qualified releases.

Adapted from [Munarium's support-file structure](https://github.com/iokaio/munarium/blob/8da666067000ca1ee9c131bc67e70b978862faa3/SUPPORT.md). VCP does not inherit that project's commercial offerings or service commitments. See [attribution](THIRD_PARTY_NOTICES.md).
