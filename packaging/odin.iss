; Per-user CLI installer. Does not alter PATH, client config, permissions or user state.
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
Name: "{group}\Odin documentation"; Filename: "{app}\README.md"
; No UninstallDelete: personal config/state and files added by the user are retained.
[Code]
function ShouldRegister: Boolean;
begin
  Result := ExpandConstant('{param:ODIN_QA|0}') <> '1';
end;

function InstallationId(Param: String): String;
begin
  Result := '{BA80AB81-DC70-4A33-833F-B10BFD86C09D}';
  if not ShouldRegister then Result := Result + '.isolated-qa';
end;
