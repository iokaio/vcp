# History diagrams

The PNGs are rendered from the matching Mermaid files in [src/](src/). Each PR
diagram describes that PR's historical state, rather than the current product.
The overview timeline runs through PR #163 (2026-09-24 07:42 UTC).

Rendering was checked with `@mermaid-js/mermaid-cli` 11.17.0, headless Google
Chrome on Windows, and the shared [Mermaid configuration](mermaid-config.json).
With that CLI available, run from this directory:

```powershell
mmdc -q -p path/to/puppeteer-config.json -c mermaid-config.json -i src/overview-phase-timeline.mmd -o overview-phase-timeline.png -s 2 -b white
```

The Puppeteer configuration selects an installed browser. For example, replace
the path below with the browser executable on your machine:

```json
{"executablePath":"C:/Program Files/Google/Chrome/Application/chrome.exe"}
```

The original local renderer used Chrome's `--no-sandbox` argument; it is omitted
from the example because ordinary rendering should retain browser sandboxing.
Keep browser and font availability consistent when comparing pixels. Inspect
regenerated images for clipped labels and overlapping text before committing.

The review through PR #163 checked all 32 Mermaid/PNG pairs visually and retained
the existing PR diagrams. Only the overview source and PNG required an update:
the P4 span now includes completed inspectors and packaging through PR #163.
