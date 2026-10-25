; Task Reaper (formerly Process Killer) installer - built by tools/release.py:  ISCC /DAppVersion=2.1.0 /DSourceDir=... ProcessKiller.iss
;
; MIGRATION FROM 1.x (Python/Tkinter):  1.x installs are upgraded IN PLACE because this script keeps
;   - the same AppId (below)            -> Windows/Inno treat it as the same product
;   - the same exe name ProcessKiller.exe and the /RELAUNCH=1 relaunch entry used by the 1.x self-updater
;   - [InstallDelete] below              -> removes the old Python runtime ({app}\_internal, ~40 MB, ~960 files)
; Never change AppId.
;
; RENAME (Process Killer -> Task Reaper, 2.1.0): the exe file name stays ProcessKiller.exe on purpose, so taskbar pins and the
; shortcuts of existing installs keep working through the update. Only the product name changes: the installer removes the old
; "Process Killer" shortcuts / Start menu folder and creates new ones. %APPDATA%\ProcessKiller (settings) is kept.
#ifndef AppVersion
  #define AppVersion "2.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\src-tauri\target\release"
#endif
#ifndef OutDir
  #define OutDir "..\dist"
#endif
#ifndef AppIdGuid
  ; production id (doubled '{' is Inno's escape). tools/test_upgrade.py overrides it with a throw-away id.
  #define AppIdGuid "{{8F2C6E1B-5B1E-4E0C-9C55-3D7A6B1E9A42}"
#endif
#define AppName "Task Reaper"
#define AppExe "ProcessKiller.exe"
#define WebView2Guid "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

[Setup]
AppId={#AppIdGuid}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=mba-x86
AppCopyright=Copyright (c) 2026 mba-x86
AppPublisherURL=https://github.com/mba-x86/Process-killer
LicenseFile=..\LICENSE
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
SetupIconFile=..\src-tauri\icons\icon.ico
WizardSmallImageFile=wizard_small.bmp
WizardImageFile=wizard_large.bmp
OutputDir={#OutDir}
OutputBaseFilename=TaskReaper-Setup-{#AppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
RestartApplications=no
UsePreviousAppDir=yes
; new shortcuts go to "Task Reaper", not back into the old "Process Killer" Start menu folder
UsePreviousGroup=no
UsePreviousTasks=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"

[InstallDelete]
; leftovers of the 1.x Python build (PyInstaller one-dir bundle)
Type: filesandordirs; Name: "{app}\_internal"
Type: files; Name: "{app}\unins000.msg"
; shortcuts and Start menu folder under the pre-2.1 name (per-user and all-users locations)
Type: files; Name: "{autodesktop}\Process Killer.lnk"
Type: files; Name: "{userdesktop}\Process Killer.lnk"
Type: filesandordirs; Name: "{autoprograms}\Process Killer"
Type: filesandordirs; Name: "{userprograms}\Process Killer"

[Files]
Source: "{#SourceDir}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
#ifexist "MicrosoftEdgeWebview2Setup.exe"
Source: "MicrosoftEdgeWebview2Setup.exe"; DestDir: "{tmp}"; Flags: deleteafterinstall; Check: NeedsWebView2
#endif

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
#ifexist "MicrosoftEdgeWebview2Setup.exe"
; Task Reaper draws its UI with the Microsoft Edge WebView2 Runtime (present on Windows 11 and most Windows 10 PCs).
Filename: "{tmp}\MicrosoftEdgeWebview2Setup.exe"; Parameters: "/silent /install"; StatusMsg: "Installing Microsoft Edge WebView2 Runtime..."; Flags: waituntilterminated; Check: NeedsWebView2
#endif
; Normal install: offer to launch at the end.
Filename: "{app}\{#AppExe}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent shellexec
; Silent self-update (1.x and 2.x updaters pass /RELAUNCH=1): the new version opens by itself.
Filename: "{app}\{#AppExe}"; Flags: nowait shellexec; Check: ShouldRelaunch

[UninstallDelete]
Type: filesandordirs; Name: "{userappdata}\ProcessKiller"

[Code]
function ShouldRelaunch: Boolean;
begin
  Result := WizardSilent and (ExpandConstant('{param:RELAUNCH|0}') = '1');
end;

function WebView2Version(const Root: Integer; const Key: String): String;
begin
  if not RegQueryStringValue(Root, Key, 'pv', Result) then Result := '';
end;

function NeedsWebView2: Boolean;
var v: String;
begin
  v := WebView2Version(HKLM32, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{#WebView2Guid}');
  if (v = '') or (v = '0.0.0.0') then
    v := WebView2Version(HKCU, 'Software\Microsoft\EdgeUpdate\Clients\{#WebView2Guid}');
  Result := (v = '') or (v = '0.0.0.0');
end;
