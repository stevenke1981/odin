; Per-user Odin plus offline bundled LSPs. Host profiles use explicit roots and CAS backups.
#ifndef AppVersion
  #error AppVersion must be provided by scripts/package.ps1
#endif
#ifndef StageDir
  #error StageDir must name the inspected portable staging directory
#endif
#ifndef OutputPath
  #error OutputPath is required
#endif
[Setup]
AppId={code:InstallationId}
AppName=Odin
AppVersion={#AppVersion}
AppPublisher=Odin contributors
DefaultDirName={localappdata}\Programs\Odin
DefaultGroupName=Odin
PrivilegesRequired=lowest
; The native progress helper uses explicit Win32 record layouts.
SetupArchitecture=x86
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputPath}
OutputBaseFilename=odin-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes
UninstallDisplayName=Odin
CreateUninstallRegKey=ShouldRegister
UsePreviousAppDir=no
UsePreviousLanguage=no
UsePreviousGroup=no
UsePreviousTasks=no
UsePreviousSetupType=no
UsePreviousUserInfo=no
CloseApplications=yes
RestartApplications=no
LicenseFile={#StageDir}\LICENSE
[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
[Icons]
Name: "{group}\Odin documentation"; Filename: "{app}\README.md"; Check: ShouldRegister
; No UninstallDelete: personal config/state and files added by the user are retained.
[Code]
var
  HostPage: TInputQueryWizardPage;
  WorkspacePage: TInputQueryWizardPage;
  ReplaceProfiles: TNewCheckBox;
  LspSetupExitCode: Integer;
  WorkspaceDefaultHost, WorkspaceDefaultValue: String;

function HostConfigPath: String;
begin
  Result := Trim(HostPage.Values[0]);
end;

function WorkspaceRoot: String;
begin
  Result := Trim(WorkspacePage.Values[0]);
end;

function ValidAbsolutePath(Value: String): Boolean;
begin
  Result := (Pos('"', Value) = 0) and (Pos(#13, Value) = 0) and
    (Pos(#10, Value) = 0) and
    (((Length(Value) >= 3) and (Value[2] = ':') and
      ((Value[3] = '\') or (Value[3] = '/'))) or
     ((Length(Value) >= 3) and (Copy(Value, 1, 2) = '\\')));
end;

function QuoteWindowsArg(Value: String): String;
var
  I, J, Backslashes: Integer;
begin
  { Windows argv parsing doubles backslashes before a quote, including the final
    quote. AddQuotes alone corrupts a path such as C:\project space\. }
  Result := '"';
  Backslashes := 0;
  for I := 1 to Length(Value) do begin
    if Value[I] = '\' then
      Backslashes := Backslashes + 1
    else begin
      if Value[I] = '"' then begin
        for J := 1 to (Backslashes * 2 + 1) do Result := Result + '\';
        Result := Result + '"';
      end else begin
        for J := 1 to Backslashes do Result := Result + '\';
        Result := Result + Value[I];
      end;
      Backslashes := 0;
    end;
  end;
  for J := 1 to (Backslashes * 2) do Result := Result + '\';
  Result := Result + '"';
end;

function SetupReceiptPath: String;
begin
  Result := Trim(ExpandConstant('{param:ODIN_SETUP_RECEIPT|}'));
end;

#include "installer-progress.iss"

procedure InitializeWizard;
begin
  HostPage := CreateInputQueryPage(wpSelectDir,
    'Odin host configuration', 'Choose the trusted host configuration outside projects',
    'Existing permissions, workspace roots and unrelated settings are retained. ' +
    'Bundled LSP profiles are added with backups; custom profiles require an explicit replacement decision.');
  HostPage.Add('Host configuration (absolute path):', False);
  HostPage.Values[0] := ExpandConstant('{param:ODIN_HOST_CONFIG|}');
  if HostPage.Values[0] = '' then
    HostPage.Values[0] := ExpandConstant('{%USERPROFILE}') + '\.config\odin\host.toml';
  WorkspacePage := CreateInputQueryPage(HostPage.ID,
    'Odin workspace access', 'Choose the folders Odin may analyze',
    'Your home folder is preselected for a new host. Odin analyzes the project you use inside it. ' +
    'Access stays read-only; source writes, shell and debugging stay disabled. ' +
    'Leave this empty to retain the roots of an existing host configuration.');
  WorkspacePage.Add('Workspace root (required for a new host configuration):', False);
  WorkspacePage.Values[0] := ExpandConstant('{param:ODIN_WORKSPACE_ROOT|}');
  if (WorkspacePage.Values[0] = '') and (not FileExists(HostConfigPath)) then
    WorkspacePage.Values[0] := ExpandConstant('{%USERPROFILE}');
  WorkspaceDefaultHost := HostConfigPath;
  WorkspaceDefaultValue := WorkspacePage.Values[0];
  ReplaceProfiles := TNewCheckBox.Create(WorkspacePage);
  ReplaceProfiles.Parent := WorkspacePage.Surface;
  ReplaceProfiles.Left := 0;
  ReplaceProfiles.Top := WorkspacePage.Edits[0].Top + WorkspacePage.Edits[0].Height + ScaleY(18);
  ReplaceProfiles.Width := WorkspacePage.SurfaceWidth;
  ReplaceProfiles.Height := ScaleY(48);
  ReplaceProfiles.Caption := 'Replace custom profiles for all bundled languages except Rust (custom Rust settings are kept)';
  ReplaceProfiles.Checked := ExpandConstant('{param:ODIN_REPLACE_LANGUAGE_PROFILES|0}') = '1';
  InitializeLspProgress;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if (LspSetupTimeout < 1) or (LspSetupTimeout > 1800) then
    Result := 'Setup timeout must be between 1 and 1800 seconds.'
  else if not ValidAbsolutePath(HostConfigPath) then
    Result := 'Choose an absolute host configuration path outside projects.'
  else if (ExpandConstant('{param:ODIN_QA|0}') = '1') and
      (ExpandConstant('{param:ODIN_HOST_CONFIG|}') = '') then
    Result := 'Isolated QA requires /ODIN_HOST_CONFIG= with a private host path.'
  else if (SetupReceiptPath <> '') and (not ValidAbsolutePath(SetupReceiptPath)) then
    Result := 'Choose an absolute setup receipt path without quotes or line breaks.'
  else if (WorkspaceRoot <> '') and
      ((not ValidAbsolutePath(WorkspaceRoot)) or (not DirExists(WorkspaceRoot))) then
    Result := 'Choose an existing absolute workspace directory.'
  else if (not FileExists(HostConfigPath)) and (WorkspaceRoot = '') then
    Result := 'A new host configuration requires a workspace root. For silent install, pass /ODIN_WORKSPACE_ROOT=.';
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Parameters: String;
  ExitCode: Integer;
  Receipt: String;
begin
  if CurStep = ssPostInstall then begin
    Parameters := QuoteWindowsArg(ExpandConstant('{app}\setup-lsps.mjs')) +
      ' --install --host-config ' + QuoteWindowsArg(HostConfigPath);
    if WorkspaceRoot <> '' then Parameters := Parameters +
      ' --workspace-root ' + QuoteWindowsArg(WorkspaceRoot);
    if ReplaceProfiles.Checked then Parameters := Parameters + ' --replace-language-profiles';
    Receipt := SetupReceiptPath;
    if Receipt <> '' then Parameters := Parameters + ' --receipt ' + QuoteWindowsArg(Receipt);
    LspSetupExitCode := 1;
    ExitCode := RunLspWorker(Parameters);
    LspSetupExitCode := ExitCode;
    if ExitCode <> 0 then Log('Odin setup needs attention: ' + LspFailureMessage);
  end;
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if (CurPageID = WorkspacePage.ID) and
      (ExpandConstant('{param:ODIN_WORKSPACE_ROOT|}') = '') and
      (WorkspaceDefaultHost <> HostConfigPath) then begin
    { Refresh automatic roots after a host-path decision, preserving an edited root. }
    if Trim(WorkspacePage.Values[0]) = WorkspaceDefaultValue then begin
      if FileExists(HostConfigPath) then WorkspaceDefaultValue := ''
      else WorkspaceDefaultValue := ExpandConstant('{%USERPROFILE}');
      WorkspacePage.Values[0] := WorkspaceDefaultValue;
    end;
    WorkspaceDefaultHost := HostConfigPath;
  end;
  if (CurPageID = wpFinished) and (LspSetupExitCode <> 0) then begin
    WizardForm.FinishedHeadingLabel.Caption := 'Odin language server setup needs attention';
    WizardForm.FinishedLabel.Caption := LspFailureMessage + #13#10#13#10 +
      'Odin files were copied, but language server setup did not complete. ' +
      'Checks stop before host changes; a configuration commit already in progress is allowed to finish. ' +
      'Inspect the setup receipt and host backup before retrying setup-lsps.mjs --install.';
  end;
end;

function GetCustomSetupExitCode: Integer;
begin
  { A post-install exception alone can still produce exit 0 in silent mode. }
  Result := LspSetupExitCode;
end;

function ShouldRegister: Boolean;
begin
  Result := ExpandConstant('{param:ODIN_QA|0}') <> '1';
end;

function InstallationId(Param: String): String;
begin
  Result := '{BA80AB81-DC70-4A33-833F-B10BFD86C09D}';
  if not ShouldRegister then Result := Result + '.isolated-qa';
end;
