param(
    [string]$Version = $env:SOLMU_VERSION,
    [string]$InstallDir = $(if ($env:SOLMU_INSTALL_DIR) { $env:SOLMU_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Solmu\bin' }),
    [switch]$NoPath,
    [string]$ReleaseApi = 'https://api.github.com/repos/panuhorsmalahti/solmu/releases',
    [string]$DownloadBase = 'https://github.com/panuhorsmalahti/solmu/releases/download'
)
$ErrorActionPreference = 'Stop'
if (-not $Version) {
    try { $Version = (Invoke-RestMethod "$ReleaseApi/latest").tag_name }
    catch { throw 'No Solmu release is available. See the GitHub Releases page.' }
}
if (-not $Version.StartsWith('v')) { $Version = "v$Version" }
if ($Version -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') { throw 'Invalid Solmu release version' }
if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') { throw 'Windows releases currently support x64. Build Solmu from source for another architecture.' }
$asset = "solmu-$Version-windows-x86_64.zip"
$solmuTemporary = Join-Path ([System.IO.Path]::GetTempPath()) ("solmu-install-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $solmuTemporary | Out-Null
try {
    $archive = Join-Path $solmuTemporary $asset
    Invoke-WebRequest -UseBasicParsing "$DownloadBase/$Version/$asset" -OutFile $archive
    $checksumFile = Join-Path $solmuTemporary 'SHA256SUMS'
    Invoke-WebRequest -UseBasicParsing "$DownloadBase/$Version/SHA256SUMS" -OutFile $checksumFile
    $checksums = Get-Content -LiteralPath $checksumFile -Raw -Encoding UTF8
    $checksum = ($checksums -split "`n" | Where-Object { $_.Trim() -match ("^[a-f0-9]{64}\s+" + [regex]::Escape($asset) + '$') })
    if (-not $checksum) { throw 'Missing release checksum' }
    $expected = ($checksum.Trim() -split '\s+')[0]
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    $inputStream = [System.IO.File]::OpenRead($archive)
    try { $actual = [BitConverter]::ToString($hasher.ComputeHash($inputStream)).Replace('-', '').ToLowerInvariant() }
    finally { $inputStream.Dispose(); $hasher.Dispose() }
    if ($actual -ne $expected) { throw 'Release checksum mismatch' }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($archive)
    $binaries = @('solmu-backend.exe', 'solmu-cli.exe', 'solmu-desktop.exe', 'sandbox.exe')
    try {
        foreach ($entry in $zip.Entries) { if ($entry.FullName -notin $binaries) { throw 'Unexpected file in release archive' } }
    } finally { $zip.Dispose() }
    [System.IO.Compression.ZipFile]::ExtractToDirectory($archive, (Join-Path $solmuTemporary 'files'))
    foreach ($binary in $binaries) { if (-not (Test-Path -LiteralPath (Join-Path $solmuTemporary "files\$binary") -PathType Leaf)) { throw "Missing binary: $binary" } }
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    foreach ($binary in $binaries) { Copy-Item -LiteralPath (Join-Path $solmuTemporary "files\$binary") -Destination (Join-Path $InstallDir $binary) -Force }
    if (-not $NoPath) {
        [string]$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        if (($userPath -split ';') -notcontains $InstallDir) { [Environment]::SetEnvironmentVariable('Path', (($userPath.TrimEnd(';') + ';' + $InstallDir).TrimStart(';')), 'User') }
        if (($env:Path -split ';') -notcontains $InstallDir) { $env:Path = "$InstallDir;$env:Path" }
    }
    Write-Host "Installed Solmu $Version to $InstallDir"
    Write-Host 'Run solmu-backend and solmu-cli in separate terminals. New terminals will use the updated PATH.'
} finally {
    $resolvedTemporary = [System.IO.Path]::GetFullPath($solmuTemporary)
    $temporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolvedTemporary.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Invalid installer temporary directory' }
    Remove-Item -LiteralPath $resolvedTemporary -Recurse -Force
}
