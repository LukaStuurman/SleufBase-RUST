# Isolated CI-only smoke test: never install/uninstall on the user's machine.
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or -not $env:RUNNER_TEMP) { throw 'Installer smoke test must run on an isolated GitHub Actions runner' }
$workspace = [IO.Path]::GetFullPath("$PSScriptRoot/..")
$installDirectory = Join-Path $env:RUNNER_TEMP ('sleufbase-installer-' + [guid]::NewGuid().ToString('N'))
$installerPath = Join-Path $workspace 'rust/dist-installer/SleufBase-Rust-preview-Setup.exe'
$manifest = Get-Content -LiteralPath (Join-Path $workspace 'rust/dist/portable-build/SleufBase-Rust-portable-preview/build-info.json') -Raw | ConvertFrom-Json
for ($iteration = 1; $iteration -le 2; $iteration++) {
    $logPath = Join-Path $env:RUNNER_TEMP "sleufbase-install-$iteration.log"
    $arguments = @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-',('/DIR="' + $installDirectory + '"'),('/LOG="' + $logPath + '"'))
    $process = Start-Process -FilePath $installerPath -ArgumentList $arguments -Wait -PassThru -WindowStyle Hidden
    if ($process.ExitCode -ne 0) { Get-Content -LiteralPath $logPath -Tail 40; throw "Installer returned $($process.ExitCode)" }
    foreach ($record in $manifest.files) {
        $installedPath = Join-Path $installDirectory $record.path
        if (-not (Test-Path -LiteralPath $installedPath)) { throw "Installed payload missing: $($record.path)" }
        if ((Get-FileHash -LiteralPath $installedPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $record.sha256) { throw "Installed payload differs: $($record.path)" }
    }
    & (Join-Path $installDirectory 'sleufbase-rust.exe') --help
    if ($LASTEXITCODE -ne 0) { throw 'Installed app could not start' }
}
$uninstaller = Join-Path $installDirectory 'unins000.exe'
$process = Start-Process -FilePath $uninstaller -ArgumentList '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART' -Wait -PassThru -WindowStyle Hidden
if ($process.ExitCode -ne 0 -or (Test-Path -LiteralPath (Join-Path $installDirectory 'sleufbase-rust.exe'))) { throw 'Uninstall did not remove the app' }
Write-Output 'Installer: install, upgrade, 14 payload hashes, CLI startup and uninstall passed'
