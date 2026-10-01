// SPDX-License-Identifier: Apache-2.0
'use strict';

// Public presentation only. Artifact approval and publication are separate.
// The palette and editorial typography follow ioka.io; no remote assets load.
const escape = value => String(value).replace(/[&<>"']/g, character => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
})[character]);

function link(value) {
  if (typeof value !== 'string' || !value || /[\\\s\u0000-\u001f\u007f]/.test(value) || value.startsWith('//')) {
    throw new TypeError('Download page links require HTTPS or ordinary relative URLs');
  }
  const parsed = new URL(value, 'https://ioka.io/');
  if (parsed.protocol !== 'https:' || parsed.username || parsed.password) {
    throw new TypeError('Unsafe download page link');
  }
  return escape(value);
}

function size(bytes) {
  if (!Number.isSafeInteger(bytes) || bytes < 0) throw new TypeError('Artifact size must be a nonnegative safe integer');
  if (bytes < 1024) return `${bytes} bytes`;
  const unit = bytes >= 1024 ** 3 ? 'GiB' : bytes >= 1024 ** 2 ? 'MiB' : 'KiB';
  const divisor = unit === 'GiB' ? 1024 ** 3 : unit === 'MiB' ? 1024 ** 2 : 1024;
  return `${(bytes / divisor).toFixed(1)} ${unit}`;
}

function fileDetails(artifact) {
  return `<details class="file-details">
          <summary>SHA-256 &amp; file details</summary>
          <dl><dt>File</dt><dd><code>${escape(artifact.name)}</code></dd>
            <dt>Size</dt><dd>${escape(artifact.bytes)} bytes</dd>
            <dt>SHA-256</dt><dd><code class="hash">${escape(artifact.sha256)}</code></dd></dl>
        </details>`;
}

function renderDownloadPage(release) {
  const artifacts = new Map();
  for (const artifact of release.artifacts) {
    if (!['setup', 'zip', 'vsix'].includes(artifact.kind) || artifacts.has(artifact.kind)) {
      throw new TypeError('Expected one installer, ZIP and VSIX');
    }
    artifacts.set(artifact.kind, artifact);
  }
  if (artifacts.size !== 3) throw new TypeError('Expected one installer, ZIP and VSIX');
  const setup = artifacts.get('setup'), zip = artifacts.get('zip'), vsix = artifacts.get('vsix');
  const source = `https://github.com/iokaio/vcp/blob/${encodeURIComponent(release.commit)}`;
  const date = new Date(release.candidateAt);
  const candidateDate = Number.isNaN(date.valueOf()) ? release.candidateAt : new Intl.DateTimeFormat('en', {
    dateStyle: 'long', timeZone: 'UTC',
  }).format(date);
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="description" content="Download the matching unsigned VCP beta installer, portable ZIP and VS Code extension for Windows x64.">
  <meta name="theme-color" content="#112e3c">
  <meta name="referrer" content="strict-origin-when-cross-origin">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">
  <title>VCP beta downloads</title>
  <style>
    :root{color-scheme:light;--ink:#112e3c;--muted:#4d626a;--paper:#faf9f5;--tint:#eff2ed;--rule:#d8dedb;--gold:#906522;--link:#235e70}
    *{box-sizing:border-box}body{margin:0;background:var(--paper);color:var(--ink);font:1rem/1.7 system-ui,-apple-system,"Segoe UI",sans-serif}
    a{color:var(--link);text-underline-offset:.23em}a:hover{text-decoration-thickness:2px}a,summary{-webkit-tap-highlight-color:transparent}
    :is(a,summary):focus-visible{outline:3px solid #2584a1;outline-offset:5px}h1,h2,h3,p{margin:0}h1,h2{font-family:Georgia,"Times New Roman",serif;font-weight:400;letter-spacing:-.035em;line-height:1.15}
    h1{font-size:clamp(2.8rem,6vw,4.8rem);max-width:10ch}h2{font-size:clamp(1.8rem,3vw,2.3rem)}h3{font-size:1.15rem;font-weight:600;line-height:1.4}
    .wrap{width:min(1120px,calc(100% - 3rem));margin-inline:auto}.skip{position:absolute;left:1rem;top:-8rem;padding:.65rem 1rem;background:var(--paper);color:var(--ink);z-index:5}.skip:focus{top:1rem}
    .site-header{background:var(--ink);border-bottom:1px solid #35505a}.navigation{min-height:78px;display:flex;align-items:center;justify-content:space-between;gap:1.5rem}
    .brand{font-family:Georgia,"Times New Roman",serif;font-size:1.5rem;letter-spacing:-.035em;text-decoration:none;color:var(--paper)}.navigation ul{display:flex;flex-wrap:wrap;gap:.4rem 1.7rem;list-style:none;margin:0;padding:0}.navigation li a{display:inline-block;padding:.65rem 0;color:#dce4e2;font-size:.85rem;text-decoration:none}.navigation li a:hover{text-decoration:underline}
    .eyebrow{text-transform:uppercase;letter-spacing:.14em;font-size:.72rem;font-weight:700;color:var(--gold);margin-bottom:1.2rem}.hero{display:grid;grid-template-columns:1.05fr 1fr;gap:clamp(2rem,6vw,5rem);align-items:start;padding-block:clamp(3rem,7vw,5.5rem) 3rem}
    .lead{font-size:1.12rem;color:var(--muted);max-width:38ch;margin-top:1.5rem}.release-label{font-size:.85rem;color:var(--muted);margin-top:1.7rem}.release-label strong{color:var(--ink);font-weight:600}.release-label span{display:block}
    .installer{background:var(--ink);color:var(--paper);padding:clamp(1.5rem,4vw,2.5rem);border-top:3px solid #dfc48b}.installer .eyebrow{color:#dfc48b;margin-bottom:.7rem}.installer h2{font-size:2rem;letter-spacing:-.025em}.installer p{color:#dce4e2;margin-top:.85rem}.installer .meta{font-size:.84rem;margin-top:.45rem}
    .button{display:inline-flex;justify-content:center;align-items:center;gap:.85rem;min-height:48px;padding:.8rem 1.2rem;border:1px solid var(--ink);font-size:.94rem;line-height:1.4;font-weight:600;text-decoration:none;color:var(--paper);background:var(--ink)}.button:hover{background:#235265;color:#fff;text-decoration:none}.installer .button{display:flex;margin-top:1.6rem;background:var(--paper);border-color:var(--paper);color:var(--ink)}.installer .button:hover{background:#e9e9df;border-color:#e9e9df}.arrow{font-size:1.35em;font-weight:400}.installer a:focus-visible,.installer summary:focus-visible{outline-color:#dfc48b}
    .file-details{font-size:.8rem;margin-top:1.15rem;border-top:1px solid var(--rule)}summary{cursor:pointer;padding-block:.85rem;min-height:44px}summary:hover{text-decoration:underline;text-underline-offset:.23em}dl{margin:0 0 .8rem}dt{font-weight:600;margin-top:.65rem}dd{margin:0;color:var(--muted)}code{font:.8rem/1.65 ui-monospace,SFMono-Regular,Consolas,monospace;overflow-wrap:anywhere}.hash{user-select:all}.installer .file-details{border-color:#35505a}.installer dd{color:#dce4e2}
    .notice{border-left:3px solid var(--gold);background:var(--tint);padding:1.1rem 1.4rem;font-size:.93rem;color:var(--muted);margin-bottom:3rem}.notice strong{color:var(--ink)}.notice a{white-space:normal}
    .section{padding-block:2.5rem;border-top:1px solid var(--rule)}.section-intro{color:var(--muted);margin-top:.8rem;max-width:62ch}.alternatives{display:grid;grid-template-columns:1fr 1fr;gap:3rem;margin-top:1.7rem}.alternative{min-width:0}.alternative p{color:var(--muted);margin-top:.6rem}.alternative .meta{font-size:.8rem}.alternative .button{margin-top:1.2rem;background:transparent;color:var(--ink)}.alternative .button:hover{background:var(--tint)}
    .getting-started{display:grid;grid-template-columns:1fr 1.35fr;gap:3rem}.requirements{list-style:none;padding:0;margin:1.2rem 0 0}.requirements li{padding:.5rem 0;border-bottom:1px solid var(--rule);font-size:.9rem;color:var(--muted)}.steps{margin:0;padding-left:1.3rem}.steps li{padding-left:.4rem;margin-bottom:1.1rem;color:var(--muted)}.steps li::marker{color:var(--gold);font-weight:600}.steps strong{color:var(--ink)}.steps p{font-size:.92rem;margin-top:.2rem}
    .records{background:var(--tint);border-top:1px solid var(--rule);padding-block:2.5rem}.records-grid{display:grid;grid-template-columns:1fr 1.35fr;gap:3rem}.record-links{display:flex;flex-wrap:wrap;gap:.4rem 1.3rem;margin-top:1.1rem}.identity{min-width:0}.identity dt{font-size:.76rem;text-transform:uppercase;letter-spacing:.08em}.identity dd{margin-top:.25rem}.records p{color:var(--muted);font-size:.9rem;margin-top:.8rem}
    .footer{padding-block:1.4rem;background:var(--ink);color:#c5d2d5;font-size:.8rem}.footer .wrap{display:flex;flex-wrap:wrap;justify-content:space-between;gap:.5rem 2rem}.footer a{color:#dce4e2}
    @media(max-width:760px){.hero,.getting-started,.records-grid{grid-template-columns:1fr;gap:2rem}.hero{padding-top:2.5rem}h1{max-width:none}.lead{max-width:45ch}.alternatives{gap:1.7rem}.navigation{gap:.6rem 1.5rem;flex-wrap:wrap;padding-block:.7rem;min-height:72px}.navigation ul{gap:1.2rem}.release-label{margin-top:1rem}.records-grid{gap:1rem}}
    @media(max-width:480px){.wrap{width:calc(100% - 2rem)}.alternatives{grid-template-columns:1fr;gap:2rem}.navigation{align-items:flex-start}.navigation li a{font-size:.78rem}.installer{padding:1.35rem}.notice{padding:1rem}.button{width:100%}}
    @media(forced-colors:active){.installer,.notice,.button{border:1px solid CanvasText}}
  </style>
</head>
<body>
  <a class="skip" href="#main">Skip to downloads</a>
  <header class="site-header">
    <nav class="wrap navigation" aria-label="Main navigation">
      <a class="brand" href="https://ioka.io/" aria-label="Ioka home">Ioka.io</a>
      <ul><li><a href="https://ioka.io/platform">Munarium</a></li><li><a href="https://ioka.io/about">About</a></li><li><a href="https://ioka.io/contact">Contact</a></li></ul>
    </nav>
  </header>
  <main id="main" tabindex="-1">
    <div class="wrap">
      <section class="hero" aria-labelledby="page-title">
        <div>
          <p class="eyebrow">Vibe Code Pro · Windows x64</p>
          <h1 id="page-title">VCP beta downloads</h1>
          <p class="lead">A coding assistant for your local workspace, with explicit permissions and retained task history. Start with a small workspace and a reviewed task.</p>
          <p class="release-label"><strong>Native ${escape(release.version)}</strong> · VSIX ${escape(release.vsixVersion)}<span>Candidate date <time datetime="${escape(release.candidateAt)}">${escape(candidateDate)}</time></span></p>
        </div>
        <article class="installer" aria-labelledby="installer-title">
          <p class="eyebrow">Start here</p>
          <h2 id="installer-title">Windows installer</h2>
          <p>Per-user setup with a stable launcher and a separate directory for your data.</p>
          <p class="meta">Windows x64 · EXE · ${escape(size(setup.bytes))}</p>
          <a class="button" href="${link(setup.href)}" download="${escape(setup.name)}">Download Windows installer <span class="arrow" aria-hidden="true">↓</span></a>
          ${fileDetails(setup)}
        </article>
      </section>
      <aside class="notice" aria-label="Beta status">
        <strong>Unsigned beta for manual testing.</strong> Qualification is incomplete. These downloads do not establish clean-host or upgrade support. Use a nonsensitive sample workspace and follow your organization’s policy for unsigned software.
        <a href="${link(`${source}/docs/usage/beta-known-issues.md`)}">Read the known limitations</a>.
      </aside>
      <section class="section" aria-labelledby="alternatives-title">
        <h2 id="alternatives-title">The matching downloads</h2>
        <p class="section-intro">Use the files from this release together. The VS Code extension connects to the native engine; it does not install it.</p>
        <div class="alternatives">
          <article class="alternative" aria-labelledby="zip-title">
            <h3 id="zip-title">Portable ZIP</h3>
            <p>Extract to a new directory and launch the native executable directly. Keep data outside the extracted payload.</p>
            <p class="meta">Native ${escape(release.version)} · ${escape(size(zip.bytes))}</p>
            <a class="button" href="${link(zip.href)}" download="${escape(zip.name)}">Download portable ZIP <span aria-hidden="true">↓</span></a>
            ${fileDetails(zip)}
          </article>
          <article class="alternative" aria-labelledby="vsix-title">
            <h3 id="vsix-title">VS Code extension</h3>
            <p>Install the VSIX in VS Code, then select your installed engine and data directory explicitly in User settings.</p>
            <p class="meta">VSIX ${escape(release.vsixVersion)} · ${escape(size(vsix.bytes))}</p>
            <a class="button" href="${link(vsix.href)}" download="${escape(vsix.name)}">Download VS Code extension <span aria-hidden="true">↓</span></a>
            ${fileDetails(vsix)}
          </article>
        </div>
      </section>
      <section class="section getting-started" aria-labelledby="start-title">
        <div>
          <h2 id="start-title">Before your first task</h2>
          <ul class="requirements"><li>Native Windows x64 · local NTFS paths</li><li>PowerShell 7 · standard Program Files location</li><li>VS Code 1.138.0 · for the extension</li></ul>
        </div>
        <ol class="steps">
          <li><strong>Install and confirm the selected engine.</strong><p>Choose separate program and private data directories. <a href="${link(`${source}/docs/usage/beta-installation.md`)}">Follow the installation guide</a>.</p></li>
          <li><strong>Set your policy and budget.</strong><p>Provider setup and live tasks require your credential and explicit spending caps. Never put secrets in settings or shared logs. <a href="${link(`${source}/docs/usage/beta-onboarding.md`)}">Follow the onboarding guide</a>.</p></li>
          <li><strong>Connect VS Code deliberately.</strong><p>Select the resolved native executable and data directory in User settings. Reconnect observes history; it does not resume work. <a href="${link(`${source}/src/packages/vscode/SETUP.md`)}">Set up the extension</a>.</p></li>
        </ol>
      </section>
    </div>
    <section class="records" aria-labelledby="records-title">
      <div class="wrap records-grid">
        <div><h2 id="records-title">Verify your download</h2><p>Compare each file’s SHA-256 with the checksum record. Matching hashes identify the bytes; they do not authenticate an unsigned publisher.</p>
          <div class="record-links"><a href="${link(release.checksumHref)}">SHA-256 checksums</a><a href="${link(release.manifestHref)}">Release manifest</a><a href="${link(release.runUrl)}">Build record</a></div>
        </div>
        <dl class="identity"><dt>Artifact pair</dt><dd><code>${escape(release.pairId)}</code></dd><dt>Source commit</dt><dd><a href="${link(`https://github.com/iokaio/vcp/commit/${encodeURIComponent(release.commit)}`)}"><code>${escape(release.commit)}</code></a></dd></dl>
      </div>
    </section>
  </main>
  <footer class="footer"><div class="wrap"><p>VCP · Built by <a href="https://ioka.io/">Ioka</a></p><a href="${link(`${source}/docs/usage/beta-recovery.md`)}">Removal and data preservation</a></div></footer>
</body>
</html>
`;
}

module.exports = {renderDownloadPage};
