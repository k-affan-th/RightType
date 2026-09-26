; RightType — Inno Setup script (optional richer installer).
; Build:  ISCC packaging\RightType.iss   (requires Inno Setup 6+)
; Produces dist\RightType-<version>-setup.exe. build_release.ps1 passes the
; version from Cargo.toml as /DMyAppVersion=...; the fallback below is only
; for a manual ISCC run and must match Cargo.toml.

#define MyAppName "RightType"
#ifndef MyAppVersion
  #define MyAppVersion "1.1.0"
#endif
#define MyAppExe "righttype.exe"

[Setup]
AppId={{8C6B9A2E-52C1-4E63-9B7A-7C1F4A2B9D33}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
DefaultDirName={autopf}\RightType
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=RightType-{#MyAppVersion}-setup
Compression=lzma2
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\{#MyAppExe}
SetupIconFile=..\assets\icon.ico

[Files]
Source: "..\target\release\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\RightType"; Filename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\RightType"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"
Name: "autostart"; Description: "Start RightType when I log in"; GroupDescription: "Startup:"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; \
    ValueName: "RightType"; ValueData: "{app}\{#MyAppExe}"; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "Launch RightType"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{cmd}"; Parameters: "/C taskkill /IM righttype.exe /F"; Flags: runhidden; RunOnceId: "KillApp"
