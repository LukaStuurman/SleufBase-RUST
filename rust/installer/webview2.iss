// Hooks are compile-time only, for the isolated installer contract fixture.
#ifndef WebView2ReadRegistry
  #define WebView2ReadRegistry "RegQueryStringValue"
  #define WebView2Extract "ExtractTemporaryFile"
  #define WebView2Execute "Exec"
#endif
// Microsoft distribution guidance: https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution
function ValidWebView2Version(const Value: String): Boolean;
var RuntimeVersion: Int64;
begin
  Result := False;
  if StrToVersion(Value, RuntimeVersion) then Result := RuntimeVersion > 0;
end;

function WebView2InRegistry(Root: Integer): Boolean;
var Version: String;
begin
  Result := {#WebView2ReadRegistry}(Root,
    'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version)
    and ValidWebView2Version(Version);
end;

function WebView2Installed: Boolean;
begin
  // Microsoft documents the 32-bit HKLM view and the per-user key on x64.
  Result := WebView2InRegistry(HKLM32) or WebView2InRegistry(HKCU32);
  if not Result and IsWin64 then
    Result := WebView2InRegistry(HKCU64) or WebView2InRegistry(HKLM64);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var ExitCode: Integer;
begin
  Result := '';
  if WebView2Installed then begin
    Log('WebView2 Runtime is already installed; preserving the shared runtime.');
    Exit;
  end;
  WizardForm.PreparingLabel.Caption := 'Microsoft WebView2 Runtime installeren voor het ingebouwde inlogvenster...';
  {#WebView2Extract}('MicrosoftEdgeWebView2RuntimeInstallerX64.exe');
  if not {#WebView2Execute}(ExpandConstant('{tmp}\MicrosoftEdgeWebView2RuntimeInstallerX64.exe'),
    '/silent /install', '', SW_HIDE, ewWaitUntilTerminated, ExitCode) then begin
    Result := 'WebView2 kon niet worden gestart: ' + SysErrorMessage(ExitCode);
    Exit;
  end;
  Log(Format('WebView2 installer exit code: %d', [ExitCode]));
  if (ExitCode = 3010) or (ExitCode = 1641) then begin
    NeedsRestart := True;
    Result := 'WebView2 vereist een herstart. Start Windows opnieuw en voer deze installer nogmaals uit.';
    Exit;
  end;
  if not WebView2Installed then
    Result := Format('WebView2 is nog niet beschikbaar (code %d). Controleer bedrijfsbeleid of vraag de systeembeheerder de runtime te installeren. Voer daarna deze installer opnieuw uit.', [ExitCode]);
end;
