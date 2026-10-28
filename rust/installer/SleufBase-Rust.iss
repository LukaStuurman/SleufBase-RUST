#define MyAppName "SleufBase Rust preview"
#define MyAppVersion "0.1.0"
#define MyAppExeName "sleufbase-rust.exe"

[Setup]
AppId={{B1B60FE1-34DA-4C80-A808-3F0E9A4245E6}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=Techbase
DefaultDirName={localappdata}\Programs\Techbase\SleufBase-Rust-preview
DefaultGroupName=Techbase\SleufBase Rust preview
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist-installer
OutputBaseFilename=SleufBase-Rust-preview-Setup
SetupIconFile=..\..\assets\sleufbase_icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2
SolidCompression=yes
WizardStyle=classic
WizardResizable=no
CloseApplications=yes
RestartApplications=yes
UsePreviousAppDir=yes
VersionInfoVersion=0.1.0.0
VersionInfoDescription=SleufBase native Rust preview installer
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppVersion}

[Languages]
Name: "dutch"; MessagesFile: "compiler:Languages\Dutch.isl"

[Tasks]
Name: "desktopicon"; Description: "Bureaubladsnelkoppeling maken"; GroupDescription: "Extra snelkoppelingen:"; Flags: unchecked

[Files]
Source: "..\dist\prerequisites\MicrosoftEdgeWebView2RuntimeInstallerX64.exe"; Flags: dontcopy noencryption nocompression
Source: "..\dist\portable-build\SleufBase-Rust-portable-preview\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\SleufBase Rust preview"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"
Name: "{autodesktop}\SleufBase Rust preview"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "SleufBase Rust preview starten"; Flags: nowait postinstall skipifsilent

[Code]
#include "webview2.iss"
