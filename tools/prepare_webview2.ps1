# Download the complete Microsoft Evergreen x64 installer for offline setup.
# Never execute the prerequisite while preparing a package.
param([string]$OutputDirectory = "$PSScriptRoot/../rust/dist/prerequisites")
$ErrorActionPreference = 'Stop'
$destination = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$runtimePath = Join-Path $destination 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe'
$downloadUrl = 'https://go.microsoft.com/fwlink/?linkid=2124701'
Invoke-WebRequest -Uri $downloadUrl -OutFile $runtimePath -MaximumRedirection 10
$runtimeFile = Get-Item -LiteralPath $runtimePath
if ($runtimeFile.Length -lt 50MB -or $runtimeFile.Length -gt 512MB) { throw 'WebView2 standalone download has an unexpected size' }
$signature = Get-AuthenticodeSignature -LiteralPath $runtimePath
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch '(^|,\s*)O=Microsoft Corporation(,|$)') {
    throw 'WebView2 installer does not have a valid Microsoft Corporation signature'
}
$receipt = [ordered]@{
    source = $downloadUrl
    sha256 = (Get-FileHash -LiteralPath $runtimePath -Algorithm SHA256).Hash.ToLowerInvariant()
    bytes = $runtimeFile.Length
    version = $runtimeFile.VersionInfo.ProductVersion
    publisher = $signature.SignerCertificate.Subject
    signature = $signature.Status.ToString()
}
$receipt | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $destination 'webview2-runtime.json') -Encoding utf8
$receipt | ConvertTo-Json
