# BETA-06 registered Windows setup

`scripts/build-setup.ps1` assembles the unsigned per-user Windows x64 setup from
one strict native release result and its reviewed production build receipt. The
receipt must bind both `vcp.exe` and the `vcp-launch.exe` launcher. The builder
rejects dirty source, qualification features, changed archives, changed setup
sources, or a different compiler. It performs no network downloads.

Provide the exact Inno Setup installer pinned by `release/internal-beta.json`.
The builder checks its SHA-256, extracts it in portable mode beneath a fresh
output directory, compiles with the compiler's own exact 6.7.3 version check,
and records the extracted compiler file hashes. Portable extraction does not
register the compiler as an installed application. This build prerequisite is
separate from the product setup's explicit PowerShell 7 prerequisite.

```powershell
./scripts/build-setup.ps1 `
  -NativeResult 'D:\candidate\native\result.json' `
  -BuildReceipt 'D:\candidate\build\build-receipt.json' `
  -Launcher 'D:\candidate\build\vcp-launch.exe' `
  -CompilerInstaller 'D:\tools\innosetup-6.7.3.exe' `
  -ReviewedCommit '<reviewed 40-character commit>'
```

The emitted `vcp-setup-result/1` identifies the final setup SHA-256, native ZIP
SHA-256, candidate ID, build receipt, launcher, compiler and source. Feed that
result into release-pair construction with the matching native and VSIX results.
Its `qualification-required` status deliberately does not attest to an installed
product run. Signing or rebuilding changes final bytes and requires a new pair.

The setup carries its separate `release/installer-notices` inventory into
`{app}/setup-notices`, including the pinned Inno license, RemObjects attribution,
MPL/CPL license selections and corresponding source. It records these hashes in
the result and refuses modified notice files during lifecycle operations. The
native ZIP's dependency inventory remains specific to the engine payload.

## Ownership and lifecycle

Inno owns the registered application root, launcher and maintenance scripts. The
existing validated package lifecycle owns only its `engine` child directory.
An absent application root receives a setup ownership marker before staging;
an existing unowned directory is refused even if it is empty. The chosen data
directory must be disjoint from the entire application root. Existing data stays
in place on uninstall, including user files that are not part of VCP's program.
Before running a maintenance script, the compiled setup/uninstaller checks its
SHA-256 against the bytes used at build time. Changed scripts fail closed.

The registered shell holds a per-user mutex from initialization through its
integration/registration phase. The engine lifecycle holds its own per-user
mutex while validating and mutating engine state. These are separate lock
scopes: direct expert engine commands do not modify registered integration.
Engine upgrade/rollback also inspects retained Files/SQLite format markers and
holds existing owner/selection locks before activation. A nonempty SQLite WAL
requires recovery and clean closure by the existing engine; setup never infers
effective compatibility from only the main-file header or edits the WAL.
After Inno copies integration files it verifies that the selected candidate,
archive and data directory still match the active engine and validated launcher.
It refuses to report successful activation if a concurrent expert operation
changed that selection.

Uninstall calls ownership checks and validated engine removal from Inno's
`usUninstall` event. At the pinned source revision this callback propagates a
failure before `PerformUninstall`, preserving registration and integration when
engine removal fails. The regression below exercises that behavior using an
actual compiled installer and deliberately changed engine bytes.

## Focused regression

On Windows with PowerShell 7, an extracted pinned compiler and the existing .NET
Framework fixture compiler:

```powershell
./scripts/installer/shell.test.ps1 -CompilerRoot 'D:\tools\inno-6.7.3'
```

This test uses a unique temporary application ID and HKCU registration. It checks
unowned-root refusal, application/data overlap refusal, installation with spaces
and Unicode paths, competing setup refusal, failed-uninstall preservation, and
successful uninstall preserving protected and unrelated data. It also refuses
changed maintenance scripts and preserves modified runtime notices. It leaves logs
under the emitted evidence path. Its engine and launcher are synthetic fixtures;
passing this regression is not BETA-08 or BETA-09 installed-product acceptance.

The exact installer source behavior used here is documented in Inno's
[portable-mode notes](https://jrsoftware.org/ishelp/topic_technotes.htm),
[compiler version variable](https://jrsoftware.org/ishelp/topic_predefinedvars.htm),
and pinned
[uninstall implementation](https://github.com/jrsoftware/issrc/blob/4adf37ed7f3fd2bd11c6836ba056e3de170fbabf/Projects/Src/Setup.Uninstall.pas).
