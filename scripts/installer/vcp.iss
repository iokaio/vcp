; SPDX-License-Identifier: Apache-2.0
; Build only through build-setup.ps1 for a release-bound artifact.
; build-local-setup.ps1 selects the separate, unsigned local identity contract.
#ifdef VcpLocalCandidate
  #ifdef VcpSigned
    #error Local candidates cannot be signed releases
  #endif
  #define CandidateKind "UnsignedLocal"
#else
  #define CandidateKind "Release"
#endif
#if Ver != 0x06070300
  #error Release setup requires exactly Inno Setup 6.7.3
#endif
#ifndef NativeArchive
  #error NativeArchive is required
#endif
#ifndef SetupFiles
  #error Staged SetupFiles is required
#endif
#ifndef MaxAppRootLength
  #error Computed MaxAppRootLength is required
#endif
#if Int(MaxAppRootLength) < 4 || Int(MaxAppRootLength) > 240
  #error Invalid computed MaxAppRootLength
#endif
#ifndef ProductVersion
  #error ProductVersion is required
#endif
#ifndef NativeSha256
  #error NativeSha256 is required
#endif
#ifndef CandidateId
  #error CandidateId is required
#endif
#ifndef ShellSha256
  #error ShellSha256 is required
#endif
#ifndef EngineScriptSha256
  #error EngineScriptSha256 is required
#endif
#ifndef NoticesSha256
  #error NoticesSha256 is required
#endif
#ifndef VcpAppId
  #define VcpAppId "VCP.InternalBeta.1"
#endif
#ifndef ProductName
  #define ProductName "VCP Coding Agent Engine"
#endif

[Setup]
AppId={#VcpAppId}
AppName={#ProductName}
AppVersion={#ProductVersion}
AppVerName={#ProductName} {#ProductVersion}
AppPublisher=Ioka LLC
DefaultDirName={autopf}\VCP
DefaultGroupName={#ProductName}
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0
WizardStyle=modern
DisableProgramGroupPage=yes
DisableDirPage=no
UninstallDisplayIcon={app}\vcp.exe
UninstallDisplayName={#ProductName} {#ProductVersion}
#ifdef VcpSigned
SignTool=vcp
SignedUninstaller=yes
SignedUninstallerDir={#SignedUninstallerRoot}
SignToolRetryCount=0
SignToolRunMinimized=yes
#endif
CloseApplications=no
RestartApplications=no
ChangesEnvironment=yes
ArchiveExtraction=basic
Compression=lzma2
SolidCompression=yes
#ifdef VcpSigned
OutputBaseFilename=vcp-{#ProductVersion}-windows-x64-signed-setup
#else
OutputBaseFilename=vcp-{#ProductVersion}-windows-x64-unsigned-setup
#endif
SetupLogging=yes

[Tasks]
Name: startmenu; Description: "Create a Start menu shortcut"; Flags: unchecked

[Files]
Source: "{#NativeArchive}"; DestName: "native.zip"; Flags: dontcopy
; The builder derives the path bound from this exact staged destination tree.
Source: "{#SetupFiles}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\{#ProductName}"; Filename: "{app}\vcp.exe"; WorkingDir: "{autodocs}"; Tasks: startmenu

[UninstallDelete]
Type: files; Name: "{app}\.vcp-setup-owned.ini"
Type: dirifempty; Name: "{app}"

[Code]
var
  DataPage: TInputDirWizardPage;
  SetupLock: THandle;
  ChosenData: String;
  PostInstallVerificationFailed: Boolean;

function NativeCreateMutex(Attributes: LongWord; InitialOwner: Boolean; Name: String): THandle;
  external 'CreateMutexW@kernel32.dll stdcall';
function NativeWaitForSingleObject(Handle: THandle; Milliseconds: LongWord): LongWord;
  external 'WaitForSingleObject@kernel32.dll stdcall';
function NativeReleaseMutex(Handle: THandle): Boolean;
  external 'ReleaseMutex@kernel32.dll stdcall';
function NativeCloseHandle(Handle: THandle): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';

function AcquireSetupLock: Boolean;
var Name: String; WaitResult: LongWord;
begin
  { Shared setup serializes machine-wide; private setup uses an OS-derived
    identity rather than caller-controlled environment variables. }
  if IsAdminInstallMode then Name := 'Global\VCP.Setup.Shared'
  else Name := 'Global\VCP.Setup.' + Uppercase(GetComputerNameString) + '.' + Uppercase(GetUserNameString);
  SetupLock := NativeCreateMutex(0, False, Name);
  Result := False;
  if SetupLock <> 0 then begin
    WaitResult := NativeWaitForSingleObject(SetupLock, 0);
    { An abandoned owner grants this process ownership; ordinary validation
      still decides whether retained files can be recovered safely. }
    Result := (WaitResult = 0) or (WaitResult = $80);
  end;
  if not Result then begin
    if SetupLock <> 0 then NativeCloseHandle(SetupLock);
    SetupLock := 0;
    SuppressibleMsgBox('Another VCP registered setup or uninstall is active. Wait for it to finish and retry.', mbError, MB_OK, IDOK);
  end;
end;

procedure ReleaseSetupLock;
begin
  if SetupLock <> 0 then begin
    NativeReleaseMutex(SetupLock);
    NativeCloseHandle(SetupLock);
    SetupLock := 0;
  end;
end;

function PowerShellPath: String;
begin
  Result := ExpandConstant('{pf64}\PowerShell\7\pwsh.exe');
  if not FileExists(Result) then
    RaiseException('PowerShell 7 is required at Program Files\PowerShell\7\pwsh.exe. Install this prerequisite explicitly, then rerun setup. No prerequisite is downloaded automatically.');
end;

function Quoted(Value: String): String;
begin
  if Pos('"', Value) <> 0 then RaiseException('Invalid quoted path');
  Result := '"' + Value + '"';
end;

function ScopeArguments: String;
begin
  if IsAdminInstallMode then Result := ' -DataScope User'
  else Result := ' -DataRoot ' + Quoted(ChosenData);
end;

function RunScript(Script, Arguments: String): Boolean;
var Code: Integer; Expected: String;
begin
  if CompareText(ExtractFileDir(Script), ExpandConstant('{app}\maintenance')) = 0 then
    if CompareText(GetSHA256OfFile(ExpandConstant('{app}\setup-notices\inventory.json')), '{#NoticesSha256}') <> 0 then
      RaiseException('Setup runtime notice inventory changed. Restore the matching original notice files before retrying.');
  if CompareText(ExtractFileName(Script), 'shell.ps1') = 0 then Expected := '{#ShellSha256}'
  else if CompareText(ExtractFileName(Script), 'package-install.ps1') = 0 then Expected := '{#EngineScriptSha256}'
  else RaiseException('Unexpected maintenance script');
  if CompareText(GetSHA256OfFile(Script), Expected) <> 0 then
    RaiseException('Maintenance script changed. Registration and integration are preserved; restore the matching original script before retrying.');
  Result := ExecAndLogOutput(PowerShellPath, '-NoProfile -NonInteractive -File ' + Quoted(Script) + ' ' + Arguments,
    '', SW_HIDE, ewWaitUntilTerminated, Code, nil) and (Code = 0);
end;

function InitializeSetup: Boolean;
begin
  Result := AcquireSetupLock;
end;

procedure InitializeWizard;
begin
  DataPage := CreateInputDirPage(wpSelectDir, 'Choose your protected data directory',
    'History and profiles are preserved on uninstall.',
    'Choose a local, private directory outside the program directory, repositories and synchronized folders.', False, '');
  DataPage.Add('Data directory:');
  DataPage.Values[0] := ExpandConstant('{param:DATADIR|{localappdata}\VCP}');
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := IsAdminInstallMode and (PageID = DataPage.ID);
end;

function AppRootLengthError(const Root: String): String;
begin
  Result := '';
  if Length(Root) > {#MaxAppRootLength} then
    Result := 'The program directory is too long for this setup. Choose a shorter program directory (at most {#MaxAppRootLength} characters, including the drive and separators).';
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var Marker, Problem: String;
begin
  if CurPageID = wpSelectDir then begin
    Problem := AppRootLengthError(WizardDirValue);
    if Problem <> '' then begin
      SuppressibleMsgBox(Problem, mbError, MB_OK, IDOK);
      Result := False;
      Exit;
    end;
    Marker := AddBackslash(WizardDirValue) + '.vcp-setup-owned.ini';
    if (not IsAdminInstallMode) and FileExists(Marker) then
      DataPage.Values[0] := GetIniString('VCP', 'DataRoot', '', Marker);
  end;
  Result := True;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var Action, Arguments: String;
begin
  { Also applies to silent setup, before ownership markers or engine activation. }
  Result := AppRootLengthError(ExpandConstant('{app}'));
  if Result <> '' then Exit;
  try
    if IsAdminInstallMode then ChosenData := '' else ChosenData := DataPage.Values[0];
    ExtractTemporaryFile('shell.ps1');
    ExtractTemporaryFile('package-install.ps1');
    ExtractTemporaryFile('native.zip');
    Arguments := '-AppRoot ' + Quoted(ExpandConstant('{app}')) + ScopeArguments;
    if not RunScript(ExpandConstant('{tmp}\shell.ps1'), '-Action Prepare ' + Arguments) then
      RaiseException('Installation roots failed ownership, path or data-preservation checks. The program and data directories must be disjoint; existing unowned directories are refused.');
    if FileExists(ExpandConstant('{app}\engine\active.json')) then Action := 'Upgrade' else Action := 'Install';
    if not RunScript(ExpandConstant('{tmp}\package-install.ps1'), '-Action ' + Action +
      ' -PackageZip ' + Quoted(ExpandConstant('{tmp}\native.zip')) +
      ' -InstallRoot ' + Quoted(ExpandConstant('{app}\engine')) + ScopeArguments) then
      RaiseException('Engine installation failed. Existing data and retained releases were preserved; inspect the setup log and recover before retrying.');
  except
    Result := GetExceptionMessage;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then begin
    { Inno handles this event's exceptions without changing its success exit.
      Set the flag before any verification call, including one that raises. }
    PostInstallVerificationFailed := True;
    if not RunScript(ExpandConstant('{app}\maintenance\shell.ps1'),
      '-Action Verify -AppRoot ' + Quoted(ExpandConstant('{app}')) + ScopeArguments +
      ' -ExpectedArchive {#NativeSha256} -CandidateId {#CandidateId} -CandidateKind {#CandidateKind}') then
      RaiseException('The installed selection changed or failed validation. Setup cannot claim this candidate was activated. Preserve the retained engine for recovery.');
    if not RunScript(ExpandConstant('{app}\maintenance\shell.ps1'),
      '-Action AddPath -AppRoot ' + Quoted(ExpandConstant('{app}')) + ScopeArguments) then
      RaiseException('The installed launcher was verified, but the selected PATH could not be updated. Setup cannot claim command integration.');
    PostInstallVerificationFailed := False;
  end;
end;

function GetCustomSetupExitCode: Integer;
begin
  { Called only when Inno would otherwise return zero; keep its failure codes. }
  if PostInstallVerificationFailed then Result := 1001 else Result := 0;
end;

procedure DeinitializeSetup;
begin
  ReleaseSetupLock;
end;

function InitializeUninstall: Boolean;
begin
  Result := AcquireSetupLock;
end;

procedure CurUninstallStepChanged(CurStep: TUninstallStep);
var Marker, Arguments: String;
begin
  if CurStep = usUninstall then begin
    Marker := ExpandConstant('{app}\.vcp-setup-owned.ini');
    if IsAdminInstallMode then ChosenData := ''
    else ChosenData := GetIniString('VCP', 'DataRoot', '', Marker);
    Arguments := '-AppRoot ' + Quoted(ExpandConstant('{app}')) + ScopeArguments;
    if not RunScript(ExpandConstant('{app}\maintenance\shell.ps1'), '-Action Check ' + Arguments) then
      RaiseException('Uninstall ownership or path validation failed. Registration and program integration were preserved.');
    { In pinned Inno 6.7.3 this event is fatal on an exception and runs before
      PerformUninstall, so a failed engine removal cannot remove registration. }
    if not RunScript(ExpandConstant('{app}\maintenance\package-install.ps1'), '-Action Uninstall' +
      ' -InstallRoot ' + Quoted(ExpandConstant('{app}\engine')) + ScopeArguments) then
      RaiseException('Engine uninstall failed. Registration, launcher and protected data were preserved. Recover or close active processes before retrying.');
    if not RunScript(ExpandConstant('{app}\maintenance\shell.ps1'), '-Action RemovePath ' + Arguments) then
      RaiseException('Engine removal succeeded, but the selected PATH could not be updated. Retry uninstall to remove the registered launcher and PATH entry.');
  end;
end;

procedure DeinitializeUninstall;
begin
  ReleaseSetupLock;
end;
