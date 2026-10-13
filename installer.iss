; Inno Setup script - built by build.py (ISCC /DAppVersion=x.y.z installer.iss)
#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "dist\ProcessKiller"
#endif
#ifndef OutDir
  #define OutDir "dist"
#endif
#define AppName "Process Killer"
#define AppExe "ProcessKiller.exe"

[Setup]
; Keep AppId constant forever: it is how Windows (and updates) recognise this app.
AppId={{8F2C6E1B-5B1E-4E0C-9C55-3D7A6B1E9A42}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Process Killer
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
SetupIconFile=assets\app.ico
WizardSmallImageFile=assets\wizard_small.bmp
WizardImageFile=assets\wizard_large.bmp
OutputDir={#OutDir}
OutputBaseFilename=ProcessKiller-Setup-{#AppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; Installs for the current user by default (no admin prompt) - the user can choose "all users" in the first dialog.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Closes a running copy during install/update.
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
; Normal install: offer to launch at the end.
Filename: "{app}\{#AppExe}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent shellexec
; Silent self-update: the app passes /RELAUNCH=1 so the new version opens by itself.
Filename: "{app}\{#AppExe}"; Flags: nowait shellexec; Check: ShouldRelaunch

[UninstallDelete]
Type: filesandordirs; Name: "{userappdata}\ProcessKiller"

[Code]
function ShouldRelaunch: Boolean;
begin
  Result := WizardSilent and (ExpandConstant('{param:RELAUNCH|0}') = '1');
end;
