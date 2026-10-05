# Common development stacks

Original VCP recipes; official references consulted on 2026-10-05. Read only the
relevant stack. Apply [platform installation procedures](platforms.md) to every
download/installer. Install prerequisites in dependency order, preserving project
pins, existing toolchains, mirrors and lockfiles. These recipes cover common
stacks without restricting the skill to a fixed allowlist. They do not certify
that every listed stack has been executed on the current host.

Commands below are templates. Variables represent validated exact versions,
resolved executable paths and authorized destinations established during
discovery; do not execute unresolved placeholders. Use the host's existing
process interface with distinct argv values. In PowerShell, a path stored in a
variable needs `&`. Run steps sequentially and check each exit status. A build,
restore or wrapper can execute repository code and remains subject to normal
runtime authorization.

## JVM: Java, Maven, Gradle, Kotlin and Scala

Detect `java -version`, `javac -version`, `mvn --version`, and the repository's
`mvnw.cmd`/`mvnw` or `gradlew.bat`/`gradlew`. Read `pom.xml`, `.mvn/`, Gradle
wrapper properties, Java toolchain declarations, `.java-version`, `.tool-versions`
and CI configuration. A JRE alone cannot compile Java. Select a compatible JDK
first, then the build tool. Preserve Maven `settings.xml` and Gradle repository
configuration without printing credentials.

Use a verified JDK distribution for the required vendor/major/architecture.
For Maven without a wrapper, select the required Apache binary release, verify
its published SHA-512/signature and extract it into a fresh versioned directory.
Set `JAVA_HOME` only in the task process to the JDK root. The example assumes
both paths have already been resolved and verified:

```powershell
$env:JAVA_HOME = $jdkRoot
$env:PATH = (Join-Path $jdkRoot 'bin') + [IO.Path]::PathSeparator + $env:PATH
& $mavenCmd '--version'
if ($LASTEXITCODE -ne 0) { throw 'Maven is not usable with this JDK.' }
& $mavenCmd '-B' 'test'
if ($LASTEXITCODE -ne 0) { throw 'Project Maven test failed.' }
```

The version output must report the selected Maven and JDK. Prefer an existing
reviewed wrapper and its configured distribution/checksum; never rewrite it to
the latest version. Gradle wrapper verification must cover its executable
wrapper JAR as well as its downloaded distribution. Check the pinned Gradle/JDK
compatibility before running `& '.\gradlew.bat' '--version'` and the project's
test target. A wrapper still requires a suitable Java runtime.

If the approved VCP profile launches `java.exe` directly and has no shell, a
standard Maven binary distribution can use its Java launcher. Locate exactly
one `boot/plexus-classworlds-*.jar` and `bin/m2.conf` in the verified Maven
directory; refuse an unsupported layout. Pass these separate arguments to the
selected Java profile, with absolute paths and the actual project directory:

```text
-classpath
<maven-directory>/boot/<actual-plexus-classworlds-jar>
-Dclassworlds.conf=<maven-directory>/bin/m2.conf
-Dmaven.home=<maven-directory>
-Dmaven.multiModuleProjectDirectory=<project-directory>
org.codehaus.plexus.classworlds.launcher.Launcher
-B
--version
```

After checking the Maven/JDK result, substitute the project's requested goal for
`--version`. Do not send `mvn.cmd` as a native executable or assume that installing
Maven created a new VCP process profile. The Windows Maven distribution smoke in
TI-01 exercises this launcher independently of the scenario harness.

Kotlin normally uses the project's Gradle or Maven plugin and needs no global
compiler. If a standalone compiler is required, select the official Kotlin
release and verify `kotlinc -version`. Scala may use sbt, Scala CLI or a project
wrapper; read `project/build.properties`, `build.sbt` or Scala CLI directives,
select a compatible JDK, and install the selected launcher through its official
distribution/existing manager. Verify the actual launcher's documented version
command, then the requested compile/test. Do not install every JVM tool.

Sources: [Maven installation](https://maven.apache.org/install.html),
[Gradle wrapper](https://docs.gradle.org/current/userguide/gradle_wrapper.html),
[Kotlin command line](https://kotlinlang.org/docs/command-line.html),
[Scala installation](https://www.scala-lang.org/download/).

## JavaScript and TypeScript: Node, npm, pnpm, Yarn, Deno and Bun

Read `package.json` (`engines`, `packageManager`, existing manager configuration),
`.nvmrc`, `.node-version` and lockfiles. Detect `node --version`, `npm --version`
and the selected package manager. Install a matching Node binary distribution
using the official signed checksum manifest or an existing authorized manager.
Do not assume Corepack is bundled with every Node version.

If the project uses pnpm/Yarn, reuse an existing Corepack when compatible with
the pin; an explicit `corepack pnpm --version` or `corepack yarn --version` avoids
changing global shim defaults. Otherwise install the exact manager release in
a task/user prefix using verified manager/vendor guidance. Do not replace a
lockfile or switch package managers to make installation easier. After runtime
and manager verification, use the repository's established restore command:
`npm ci` for `package-lock.json`, `pnpm install --frozen-lockfile` for pnpm, or
`yarn install --immutable` for modern Yarn (`--frozen-lockfile` for Yarn Classic).
These commands can modify dependency directories and run lifecycle hooks; use
only the intended project directory and existing authorization.

Deno (`deno.json`, `deno.lock`) and Bun (`bun.lock`/`bun.lockb`) are separate
runtime choices. Select their official exact OS/CPU release; verify
`deno --version` or `bun --version`, then the project-defined task/test. Preserve
the selected runtime's lockfile enforcement and script permissions. Do not
substitute Bun for Node or grant Deno broad execution permissions as a setup fix.

Sources: [Node downloads and verification](https://nodejs.org/en/download),
[Corepack](https://github.com/nodejs/corepack),
[npm ci](https://docs.npmjs.com/cli/v11/commands/npm-ci/),
[pnpm install](https://pnpm.io/cli/install),
[Yarn install](https://yarnpkg.com/cli/install),
[Deno installation](https://docs.deno.com/runtime/getting_started/installation/),
[Bun installation](https://bun.sh/docs/installation).

## Python, pip, venv and uv

Read `.python-version`, `pyproject.toml`, requirements/constraints, `uv.lock` and
existing environment configuration. Resolve `python`/`py`/`python3` to an actual
interpreter and inspect its version; Windows Store aliases can be misleading.
Install the required interpreter via an existing approved manager or verified
vendor distribution. For an existing uv, `uv python install $pythonVersion`
selects a concrete interpreter version; uv uses its own managed Python
distributions, so preserve configured download mirrors and record that source.

Create a new project environment only if the intended path is unused. Do not
overwrite an existing unrelated `.venv`. Activation is optional:

```powershell
& $pythonExe '-m' 'venv' $venvPath
if ($LASTEXITCODE -ne 0) { throw 'Virtual environment creation failed.' }
& $venvPython '-m' 'pip' '--version'
```

Use the environment's interpreter for restore/test. For a fully hashed pip
requirements lock, use `-m pip install --require-hashes -r requirements.txt`;
do not pretend an ordinary unpinned requirements file is a lock. With a uv
project, use `uv sync --locked` only in its intended environment and inspect
existing environment ownership first: sync can remove undeclared packages.
Do not repair `ensurepip`/venv support by upgrading system pip or modifying an
externally managed system Python. Verify the actual interpreter path/version
and the original test command.

Sources: [Python venv](https://docs.python.org/3/library/venv.html),
[uv managed Python](https://docs.astral.sh/uv/guides/install-python/),
[uv locking and syncing](https://docs.astral.sh/uv/concepts/projects/sync/).

## .NET

Read `global.json` (including roll-forward policy), target frameworks,
`NuGet.config` and tool manifests. Inspect `dotnet --info` and
`dotnet --list-sdks`; a runtime alone cannot build. Prefer the exact SDK already
installed. Otherwise use a verified official SDK installer/archive. Microsoft's
downloaded and inspected install script is also suitable for an isolated task
SDK; execute it as a separate process with an exact version and fresh directory:

```powershell
& $pwshExe '-NoProfile' '-File' $verifiedDotnetInstallScript '-Version' $sdkVersion '-InstallDir' $sdkDirectory '-NoPath'
if ($LASTEXITCODE -ne 0) { throw 'SDK installation failed.' }
& $dotnetExe '--info'
```

Use the resolved `dotnet` path and task-local `DOTNET_ROOT` where required.
Do not alter shared SDK selection or replace `global.json`. Restore repository
tools with `dotnet tool restore` and use locked restore when the repository
already maintains a NuGet lock. Workloads/native targeting prerequisites are
additional components; install only those required for the selected target.
Source: [.NET installation script](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-install-script).

## Rust and Go

Rust: read `rust-toolchain.toml`/`rust-toolchain`, Cargo `rust-version`, target and
component requirements. Inspect `rustup show` and `rustc --version`. Reuse rustup;
install the pin with `rustup toolchain install $toolchain --profile minimal` and
only required components/targets. Verify with `rustup run $toolchain rustc --version`
and run the project's locked Cargo test. Avoid `rustup default`/blanket updates.
An MSVC Rust target needs matching C++ Build Tools/Windows SDK; a GNU target has
different linker requirements. Install rustup itself only through a verified
official bootstrap, preserving task/user scope and existing defaults.

Go: read `go.mod` (`go`, `toolchain`), `go.work` and CI pins. Inspect `go version`
and task-relevant `go env` fields without dumping unrelated environment values.
Use the verified official exact SDK for the host in a new versioned directory;
do not unpack over an existing Go tree. Account for configured automatic
toolchain downloads and mirrors. Verify the selected executable and run the
project's `go test` target without rewriting `go.mod` or `go.sum` unnecessarily.

Sources: [rustup toolchains](https://rust-lang.github.io/rustup/concepts/toolchains.html),
[Go installation](https://go.dev/doc/install).

## C, C++, CMake and Ninja

Read `CMakePresets.json`, `CMakeLists.txt`, Meson/build files and compiler/SDK
requirements. Probe `cmake --version`, `ninja --version`, `clang --version` or
`gcc --version`. MSVC `cl` needs a developer environment and has different probe
behavior; a missing command in a normal shell does not prove missing Build Tools.
Discover an existing Visual Studio installation and use its documented developer
shell setup in the same authorized process that builds. Do not treat copying
`cl.exe` or changing PATH alone as a complete MSVC installation.

Install only required MSVC/Windows SDK components through a verified Microsoft
installer when authorized. On macOS select CLT/Xcode as required; on Linux use
the distro's compiler packages or a verified compatible LLVM distribution.
Respect the target ABI; MSVC, MinGW and Linux compilers are not interchangeable.
CMake and Ninja have standalone vendor binaries; select and verify pins using
the platform archive procedure. Verify by configuring a fresh build directory
with the project's preset and compiling its smallest target. A version banner
alone does not prove the linker or platform SDK works.

Sources: [MSVC command-line environment](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line),
[CMake distributions](https://cmake.org/download/), [Ninja](https://ninja-build.org/).

## Ruby and PHP

Ruby: read `.ruby-version`, `Gemfile`, `Gemfile.lock` (Ruby and Bundler pins).
Select an existing version manager or official platform installation route.
Windows native gems may need the matching RubyInstaller Devkit/MSYS2 components;
Unix builds may need compiler and native libraries. Verify `ruby --version` and
`gem --version`, install only the pinned Bundler into the intended user/tool
environment, then use the repository's frozen/deployment bundle procedure.
Do not update the gem lock or system Ruby to resolve a setup mismatch.

PHP: read `composer.json`, `composer.lock`, PHP version and extension requirements.
Select the correct architecture and Windows thread-safety/compiler build when
applicable. Install PHP first, then a verified pinned Composer PHAR or vendor
installer. Check `php --version`, `php --ini` and `php -m` without printing secret
configuration values. Invoke the resolved PHP binary with the Composer PHAR,
run `install` against the existing lock, and `check-platform-reqs`. Do not use
`--ignore-platform-reqs` to hide missing extensions or run `composer update` as
an installation fallback. Restore scripts/plugins remain executable code.

Sources: [Ruby installation](https://www.ruby-lang.org/en/documentation/installation/),
[Composer installation](https://getcomposer.org/doc/00-intro.md),
[PHP installation](https://www.php.net/manual/en/install.php).

## Dart, Flutter, Swift/Xcode and Android

Dart/Flutter: read `pubspec.yaml`, `pubspec.lock`, project Flutter manager pins
and CI configuration. Select the exact official Flutter SDK archive when the
project uses Flutter; it includes its matching Dart SDK. Avoid installing a
second arbitrary Dart version. Verify `flutter --version`, `dart --version` and
`flutter doctor -v`; doctor reports missing platform components, not completed
installation. Install only the target's prerequisites, then restore/test using
the repository's lock policy. Standalone Dart projects can use an exact official
Dart SDK. [Flutter installation](https://docs.flutter.dev/install)

Swift: inspect `Package.swift`, Swift tools version and target platforms. Select
the official supported host toolchain and required native prerequisites. Verify
`swift --version` and a project build. Windows/Linux Swift does not supply Apple
SDKs, iOS signing or Xcode. Apple app builds require an authorized macOS/Xcode
host. Do not claim iOS validation from a Windows Swift install.
[Swift installation](https://www.swift.org/install/)

Android: read Gradle/JDK compatibility, compile SDK, build-tools/NDK/CMake pins
and existing SDK paths. Obtain verified Android command-line tools if absent,
then inspect `sdkmanager --list`. Install exact package identifiers reported by
the official manager, using the intended SDK root:

```powershell
& $sdkmanagerBat "--sdk_root=$androidSdkRoot" '--install' $platformPackage $buildToolsPackage
if ($LASTEXITCODE -ne 0) { throw 'Android SDK component installation failed.' }
```

Quoted arguments matter because SDK package IDs contain semicolons. Install
only selected components; do not run blanket SDK updates or auto-accept licenses
without authority. Set SDK paths in the task environment. Emulators,
virtualization, drivers, device access and signing credentials require their
own available capabilities. Verify installed package IDs and the requested
Gradle build; an emulator test remains unrun without a functioning emulator.
[Android sdkmanager](https://developer.android.com/tools/sdkmanager)

## Git, shells and command-line utilities

Detect Git, PowerShell, Bash and required utilities by resolved path and version.
Use official Git/PowerShell releases or an existing authorized system manager.
Select the required version and architecture; portable installations can avoid
system defaults. A shell installation does not authorize arbitrary shell
execution. Do not change Git credentials, global configuration, PowerShell
profiles or execution policy while installing tools. Install Bash/WSL only when
the project actually requires that environment. Verify `git --version` and, for
PowerShell, invoke `pwsh -NoProfile -Command '$PSVersionTable.PSVersion.ToString()'`
through appropriate literal argv handling for the host.
Sources: [Git downloads](https://git-scm.com/downloads/),
[PowerShell installation](https://learn.microsoft.com/en-us/powershell/scripting/install/installing-powershell).

## Database clients and development servers

Identify the required engine/version, project connection target and whether the
missing dependency is only a client. PostgreSQL (`psql --version`), MySQL/MariaDB
(`mysql --version`), SQLite (`sqlite3 --version`), SQL Server (`sqlcmd` help/version
for the installed variant), Redis and MongoDB each have different platform
distributions. Use the official vendor's platform package/client-only component
where available; do not install a server to obtain a client unnecessarily.

If a local server is required, select the project's engine major/version, use a
fresh task-owned data directory and loopback binding, and preserve existing
instances/data/ports. An already authorized container runtime can host a pinned
official image, but do not create a daemon/VM implicitly. Service registration,
database initialization/migration and credentials require appropriate authority.
Verify client startup separately from a read-only connection/health query to
the intended development server. Never use a production endpoint to smoke-test
installation or put passwords into command arguments/logs. Redis on Windows
may require a supported WSL/container route; do not pick an unofficial port as
an invisible substitute. Client success is not server readiness.

Sources: [PostgreSQL](https://www.postgresql.org/download/),
[MySQL](https://dev.mysql.com/doc/refman/8.4/en/installing.html),
[SQLite](https://www.sqlite.org/download.html),
[sqlcmd](https://learn.microsoft.com/en-us/sql/tools/sqlcmd/sqlcmd-download-install),
[Redis](https://redis.io/docs/latest/operate/oss_and_stack/install/),
[MongoDB](https://www.mongodb.com/docs/manual/installation/).

## Containers, Kubernetes and infrastructure tools

Read project Docker/Compose, Kubernetes and Terraform/OpenTofu requirements.
Inspect `docker --version`, `docker compose version`, `kubectl version --client`
and `terraform version` as applicable. Install the exact official CLI/plugin
release needed for the target, verifying checksums/signatures. Preserve existing
Docker contexts, kubeconfig, cloud credentials and Terraform version/provider
locks. CLI setup does not authorize a cluster, cloud login or infrastructure
changes. Never use `terraform apply` or `kubectl apply` as an installation test.

Docker Desktop/Engine or Podman may require services, virtualization, drivers,
group membership, host changes and commercial terms. Reuse a compatible existing
runtime when authorized; otherwise identify those effects before installing.
`docker version` can test an already selected authorized daemon, but record
client-only success if no daemon is available. Do not automatically download/run
an image merely to prove the CLI banner. Any test container must have an approved
image/version and owned resources.
Sources: [Docker installation](https://docs.docker.com/engine/install/),
[Kubernetes tools](https://kubernetes.io/docs/tasks/tools/),
[Terraform installation](https://developer.hashicorp.com/terraform/install).

## R, Julia, Elixir/Erlang and Haskell

R: read project R/`renv.lock` requirements, select the official CRAN platform
distribution and inspect `R --version`. Native packages may need matching
Rtools on Windows or compiler/system libraries elsewhere. Restore only the
project environment through its existing renv workflow; do not update all
user packages. [CRAN](https://cran.r-project.org/)

Julia: read `Project.toml` compatibility and `Manifest.toml`, then select the
exact official runtime through an existing Juliaup or verified distribution.
Use a version-specific invocation instead of changing the shared default.
Verify `julia --version` and the project's environment/test; keep manifest and
depot ownership intact. [Julia installation](https://julialang.org/install/)

Elixir: read `.tool-versions`, `mix.exs` and `mix.lock`; select a compatible
Erlang/OTP version before Elixir. Use the official host route or existing manager,
verify `elixir --version` reports both, and run the project's Mix restore/test.
Do not update Hex/Rebar or all dependencies merely because a tool is missing.
[Elixir installation](https://elixir-lang.org/install/)

Haskell: read `stack.yaml` resolver or `cabal.project` compiler constraints.
Use an existing Stack/GHCup or verified official installation to obtain the
selected GHC and Cabal versions, preserving shared defaults. Windows may require
matching MSYS2 components. Verify `ghc --version` and `cabal --version` or
`stack --version`, then the selected project build. Do not execute web bootstrap
one-liners that bypass local script policy. [GHCup](https://www.haskell.org/ghcup/)
