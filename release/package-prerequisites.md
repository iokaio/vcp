# Internal beta prerequisites

The native VCP executable contains its Rust/native runtime libraries. Node, Rust,
MSVC, Python, provider credentials and model weights are not included in this
package. No optional tool, model or browser is acquired automatically.

| Operation | User-provided prerequisite | Missing prerequisite behavior |
| --- | --- | --- |
| Native CLI and local editor connection | Declared Windows x64 beta target; configured private data directory and initialized workspace | Follow the packaged setup/quickstart diagnostics. No provider credential is needed merely to inspect local state. |
| Installer lifecycle and explicit model acquisition | PowerShell 7; network access only for explicit acquisition | Installer diagnoses unavailable PowerShell. Model provisioning fails without publishing unverified files. |
| Editor extension | Matching native candidate and VS Code version from the release pair; explicit User settings | Incompatible or missing native connection is refused. |
| Provider execution | Owner-selected current provider snapshot, qualified configuration, credential and admitted budget | Run `vcp setup check`; no secret or spending authority is embedded in the package. |
| Local memory build/query | Explicitly acquired MiniLM files matching `models/minilm-assets.json` | Missing/corrupt model assets are refused; weights remain outside the installation. |
| Skill authoring validator | Authorized Node process profile | Materialize `skill-authoring/scripts/validate.cjs`; report not run if unavailable. |
| PDF helper | Authorized Python 3.10+ environment with the shipped `pdf-workflows/requirements.txt` pins: pypdf, ReportLab, Pillow, charset-normalizer | Materialize the helper before running; missing dependencies are diagnosed. No OCR/rendering engine is bundled. |
| Spreadsheet helper | Authorized Python 3.10+ environment with the shipped `spreadsheet-workflows/requirements.txt` pins: openpyxl, defusedxml, formualizer, et-xmlfile | Materialize the helper before running; dependencies and formula/recalculation limitations remain explicit. |
| Browser helper | Authorized Node profile, project's installed Playwright and its browser; optional project-installed axe-core | No browser/tool download. Missing dependencies are reported. The helper uses a separate context and the documented loopback restrictions. |
| MCP pagination helper | Authorized Python profile; standard library only | Materialize the helper; no MCP server or credential is installed by this resource. |
| Project-specific build/test tools | The project's explicitly authorized process profiles and dependencies | A skill cannot grant tools or report unexecuted checks as passed. |

Use Python 3.12+ for the PDF/spreadsheet helpers when explicit Windows junction
detection is needed; their documented older-Python limitation still applies.
These are prerequisite declarations, not evidence that a clean Windows machine or
installed-product helper qualification passed. Refer to the exact candidate's
qualification scorecard for executed checks and remaining limitations.

`component-inventory.json` identifies the actual locked Windows production
normal/build packages, packaged and embedded assets, license provenance and
the relevant distinction between build tools and runtime components. Original
MPL-2.0 source archives accompany their license texts in `licenses/sources`.
Some published Apache packages omit a standalone license file; the bundle
retains their declarations/authors and the complete selected Apache-2.0 terms.
The inventory separately records the debugserver-types MIT declaration/text
limitation and supplies its unchanged original source archive.
