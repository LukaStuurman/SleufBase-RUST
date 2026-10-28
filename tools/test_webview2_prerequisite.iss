#define WebView2ReadRegistry "TestReadRegistry"
#define WebView2Extract "TestExtract"
#define WebView2Execute "TestExecute"
[Setup]
AppName=SleufBase prerequisite contract
AppVersion=1
DefaultDirName={tmp}\sleufbase-prerequisite-contract
PrivilegesRequired=lowest
Uninstallable=no
CreateAppDir=no
OutputDir=..\rust\dist-installer
OutputBaseFilename=prerequisite-contract
[Code]
var
  AvailableRoot, Calls, RuntimeExit: Integer;
  AvailableVersion: String;
  LaunchWorks, InstallWorks: Boolean;
function TestReadRegistry(Root: Integer; Key, ValueName: String; var Value: String): Boolean;
begin
  if (ValueName <> 'pv') or (Pos('{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', Key) = 0) then RaiseException('Wrong runtime identity');
  Result := Root = AvailableRoot;
  Value := AvailableVersion;
end;
procedure TestExtract(Name: String);
begin
  if Name <> 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe' then RaiseException('Wrong prerequisite');
end;
function TestExecute(Filename, Params, WorkingDir: String; ShowCmd: Integer; Wait: TExecWait; var ResultCode: Integer): Boolean;
begin
  if (Params <> '/silent /install') or (ShowCmd <> SW_HIDE) or (Wait <> ewWaitUntilTerminated) then RaiseException('Wrong prerequisite invocation');
  Calls := Calls + 1;
  Result := LaunchWorks;
  ResultCode := RuntimeExit;
  if InstallWorks then begin AvailableRoot := HKCU32; AvailableVersion := '155.0.0.1'; end;
end;
#include "..\rust\installer\webview2.iss"
procedure Check(Condition: Boolean; Message: String);
begin
  if not Condition then RaiseException(Message);
end;
procedure InitializeWizard;
var NeedsRestart: Boolean; Message: String;
begin
  Check(not ValidWebView2Version(''), 'Empty version accepted');
  Check(not ValidWebView2Version('0.0.0.0'), 'Zero version accepted');
  Check(not ValidWebView2Version('broken'), 'Malformed version accepted');
  Check(ValidWebView2Version('155.0.0.1'), 'Valid version rejected');
  Calls := 0; AvailableRoot := HKLM32; AvailableVersion := '155.0.0.1';
  NeedsRestart := False; Message := PrepareToInstall(NeedsRestart);
  Check((Message = '') and (Calls = 0), 'Existing machine runtime was replaced');
  AvailableRoot := HKCU32;
  Check(WebView2Installed, 'Per-user runtime not recognized');
  AvailableVersion := '0.0.0.0';
  Check(not WebView2Installed, 'Zero registry version was accepted');
  AvailableRoot := -1; LaunchWorks := False; InstallWorks := False; RuntimeExit := 2;
  Check(PrepareToInstall(NeedsRestart) <> '', 'Launch failure was ignored');
  LaunchWorks := True; RuntimeExit := 1603;
  Check(PrepareToInstall(NeedsRestart) <> '', 'Installation failure was ignored');
  RuntimeExit := 0;
  Check(PrepareToInstall(NeedsRestart) <> '', 'Success without installed runtime was accepted');
  RuntimeExit := 3010;
  Message := PrepareToInstall(NeedsRestart);
  Check((Message <> '') and NeedsRestart, 'Restart requirement was ignored');
  RuntimeExit := 0; InstallWorks := True; NeedsRestart := False;
  Message := PrepareToInstall(NeedsRestart);
  Check((Message = '') and not NeedsRestart and WebView2Installed, 'Successful runtime installation was rejected');
  Log('WebView2 prerequisite contracts passed');
end;
