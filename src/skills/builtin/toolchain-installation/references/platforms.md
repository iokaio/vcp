# Platform installation procedures

Original VCP guidance. Official documentation consulted on 2026-10-05. Package
availability and installer switches change: inspect the selected package and its
current vendor instructions before execution. Examples are templates, not a
catalog of approved versions. Replace variables with values established by
repository requirements and verified package metadata, not model guesses.

## Execution and scope

Use the execution tools already available to the current VCP task. Skill
activation grants no tool, network, filesystem, process, elevation, or profile
permission. Continue installs covered by the user's task and existing authority;
do not ask for the same permission again. If execution is denied, preserve the
denial and request only the missing authority through the host's normal flow.
Never edit VCP profiles, process allowlists, sandbox settings, or execution
policies to make an installer run.

Inspect the host OS and architecture, executable discovery, project pins and
existing managers first. Resolve compatible existing installations before
downloading anything. A missing PATH entry is different from a missing tool.
Use a workspace or user versioned directory for a portable tool where supported.
Do not overwrite an existing tool directory, upgrade all packages, remove older
SDKs, change default toolchain selections, or permanently edit PATH as a default.

Installation may execute vendor code, build hooks, or package lifecycle scripts.
Retrieve metadata through approved sources, retain configured mirrors/proxies
and certificates, and verify authenticity/integrity before execution. Never
disable TLS validation, signature/hash checks, or malware checks. Do not pipe
downloads into a shell or evaluate command strings from metadata. Record source,
selected version, architecture, verification evidence, destination, command,
exit status and actual executable path/version, with credentials redacted.

System installations, privileged helpers, new services, drivers, hypervisors,
firewall changes, reboots and new commercial license obligations are separate
effects. Check existing authorization for those effects. If missing, complete
independent user/workspace setup and identify the specific remaining action.
Installing a CLI alone does not authorize a service, cloud account or deployment.

## Windows: inspect, select, install, verify

Run discovery through PowerShell using separate command invocations:

```powershell
Get-Command java, javac, mvn, node, python, dotnet, git -ErrorAction SilentlyContinue
Get-Command winget -ErrorAction SilentlyContinue
[System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
```

An application alias is not proof that a runtime is installed. Run a bounded
version probe on the resolved executable. Prefer explicit absolute paths for
subsequent commands. `.cmd`/`.bat` launchers require an authorized Windows shell;
do not assume a native executable API will execute them directly. In PowerShell,
invoke a path with `&` and pass each argument separately:

```powershell
& $mavenCmd '--version'
if ($LASTEXITCODE -ne 0) { throw 'Maven version probe failed.' }
```

For WinGet, select an existing authorized source first. Search by product name,
inspect the exact returned identifier and versions, then inspect the selected
version's publisher, installer URL, scope, architecture, dependencies and terms.
Do not invent package IDs or automatically install the first search match.

```powershell
winget source list
winget search --query $productName --source $selectedSource
winget show --id $selectedId --exact --source $selectedSource --versions
winget show --id $selectedId --exact --source $selectedSource --version $selectedVersion
winget install --id $selectedId --exact --source $selectedSource --version $selectedVersion --scope user --no-upgrade --disable-interactivity
```

Run each step only after reviewing the preceding result. Use `--scope user` only
when the package supports it. `--no-upgrade` can skip an existing incompatible
version; confirm the resulting executable instead of equating exit success with
installation success. If the required version cannot coexist through WinGet,
prefer a vendor portable distribution in a new directory. Do not silently fall
back to machine scope. Agreement flags may be added only for terms already
covered by user authority; they are not a way to bypass license decisions.
Do not enable local manifests or add third-party package sources just to install
an otherwise unverified package. [WinGet install reference](https://learn.microsoft.com/en-us/windows/package-manager/winget/install)

If WinGet is absent or does not offer the required version, use a vendor archive
or a verified installer with its documented options. Inspect local `--help` or
vendor documentation; MSI switches do not apply to arbitrary EXE installers.
Wait for completion and check exit codes. A reboot-required result is pending,
not a reason to reboot automatically. Do not install another package manager
when the approved vendor distribution already satisfies the task.

## Portable archives and scripts

1. Select the exact supported release, OS/architecture and binary archive from
   the vendor or configured mirror. Resolve a checksum/signature from trusted
   release metadata before downloading. A digest computed only from the download
   proves no authenticity. Verify signed checksum manifests where provided.
2. Download to a fresh staging location. Verify the expected digest/signature;
   a mismatch stops installation. Record the verification source, not secrets.
3. Inspect archive members before extracting. Reject traversal, absolute paths,
   links/reparse points escaping the destination, duplicate/colliding names and
   unreasonable expansion. Use the bundled verified ZIP helper only for its
   documented ZIP subset. It is not an MSI, EXE, tar, DMG or package installer.
4. Extract into a fresh versioned directory under an authorized root. Never
   merge into an unrelated existing installation. Keep extraction and execution
   separate. Verify the intended executable and version before adding it to a
   task's environment.
5. Use explicit paths or a process-local PATH. Environment changes in one child
   process do not persist into the next VCP execution call. Set them in each
   authorized invocation or use an existing task environment mechanism. Report
   this limitation instead of claiming the parent VCP process was reconfigured.

For a vendor script, download it to a file, verify provenance and inspect its
effects before running a separate interpreter process. Do not bypass script
execution policy. Preserve configured downloads, caches and mirrors. The script
may download additional binaries; verify those through its supported integrity
mechanism or select verified archives instead.

## macOS

Only use these routes when the execution host is actually macOS. A Windows VCP
session does not become a macOS host by downloading an Apple SDK.

Prefer an existing project manager or verified user archive when an exact pin
matters. With an already installed Homebrew, inspect before installation:

```sh
brew search "$formula_name"
brew info "$selected_formula"
brew list --versions "$selected_formula"
brew install "$selected_formula"
```

The last command is appropriate only when that formula supplies the required
version. Homebrew is not an arbitrary historical-version installer; a versioned
formula must actually exist. A cask may run a system installer. Inspect those
effects and avoid blanket `brew upgrade` or changes to shared links. Honor
existing Brew environment settings; installing a new manager is its own scoped
prerequisite. [Homebrew command reference](https://docs.brew.sh/Manpage)

For Apple development, inspect `xcode-select -p`, `xcrun --find clang` and
`xcodebuild -version`. Command Line Tools and full Xcode serve different needs.
Use the official selected Xcode/CLT installation route when required; account
access, license acceptance and privileged installation may require host/user
action. Prefer a task-local `DEVELOPER_DIR` over changing a shared selection.

## Linux and WSL

Identify distribution, release, architecture and libc before choosing binaries.
An archive built for glibc need not work on musl. Do not replace the system
Python, package-manager runtime, libc or compiler symlinks. Existing project
managers and verified user directories can avoid a privileged installation.

For an authorized Debian/Ubuntu system package installation, inspect the
configured repositories and available versions first:

```sh
apt-cache policy "$package_name"
apt-cache show "$package_name"
apt-get --simulate install "$package_name=$selected_version"
apt-get install "$package_name=$selected_version"
```

Only execute the final command through already authorized privileges after
reviewing the dependency/removal plan. Update repository indexes only when
needed and authorized; do not run broad upgrades or add arbitrary repositories.
If the pin is unavailable, choose a verified vendor/user distribution rather
than changing the project's version. Other distributions follow the same
inspect/version/dependency-review pattern with their documented manager; do not
translate APT switches mechanically. [APT reference](https://manpages.debian.org/bookworm/apt/apt-get.8.en.html)

WSL is a separate Linux environment with its own tools and paths. Use an already
authorized WSL environment only when it matches the task's target. Installing a
Windows tool does not install it inside WSL, and installing one in WSL does not
fix a native Windows process's PATH. Enabling WSL/virtualization or choosing a
different target environment must not be an implicit workaround.

## Unlisted tools

Apply the same procedure to any additional common stack: read project pins,
identify the official maintainer and supported host, resolve an exact artifact
or existing-manager package, inspect scope/dependencies, verify integrity,
install within authority, and run both a version probe and the original blocked
command. If no safe compatible distribution exists, record the exact missing
capability and finish independent work. Never invent a package ID, checksum,
installation flag, host capability or successful verification.
