; BGWM — Better Windows Workspaces Manager
; Requires Inno Setup 6: https://jrsoftware.org/isinfo.php

#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif

#define MyAppName "BGWM"
#define MyAppPublisher "belizariogr"
#define MyAppURL "https://github.com/belizariogr/bgwm"
#define MyAppExeName "bgwm.exe"
#define MyAppDescription "Better Windows Workspaces Manager"
#define ReleaseBinary AddBackslash(SourcePath) + "..\target\release\" + MyAppExeName

#if FileExists(ReleaseBinary)
#else
  #error "Release binary not found. Run: cargo build --release (or installer\build-installer.ps1)"
#endif

[Setup]
AppId={{A7E4C9B2-3F18-4D6A-9C0E-1B5F8D2A6E43}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
AppCopyright=Copyright (C) {#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE
OutputDir=..\dist
OutputBaseFilename=bgwm-setup-{#MyAppVersion}
SetupIconFile=..\assets\icon\bgwm.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=force
CloseApplicationsFilter={#MyAppExeName}
MinVersion=10.0

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "launchafterinstall"; Description: "Iniciar o {#MyAppName} após a instalação"; GroupDescription: "Após a instalação:"; Flags: checkedonce

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\tray\ref\*"; DestDir: "{app}\assets\tray\ref"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Comment: "{#MyAppDescription}"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon; Comment: "{#MyAppDescription}"

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent; Tasks: launchafterinstall

[UninstallDelete]
Type: filesandordirs; Name: "{app}\assets"

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  LegacyStartupApprovedKey = 'Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run';
  StartupValueName = 'BGWM';

procedure RemoveStartupRegistrationIfInstalled();
var
  Command: String;
begin
  if RegQueryStringValue(HKCU, RunKey, StartupValueName, Command) then
  begin
    if Pos(LowerCase(ExpandConstant('{app}')), LowerCase(Command)) > 0 then
      RegDeleteValue(HKCU, RunKey, StartupValueName);
  end;

  if RegValueExists(HKCU, LegacyStartupApprovedKey, StartupValueName) then
    RegDeleteValue(HKCU, LegacyStartupApprovedKey, StartupValueName);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveStartupRegistrationIfInstalled();
end;
