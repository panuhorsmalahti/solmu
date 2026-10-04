param(
    [string]$Version = $env:SOLMU_VERSION,
    [string]$InstallDir = $env:SOLMU_INSTALL_DIR,
    [ValidateSet('all', 'backend', 'cli', 'desktop', 'muxer', 'muxer-gui', 'boxer', 'web')]
    [string]$Component = $(if ($env:SOLMU_COMPONENT) { $env:SOLMU_COMPONENT } else { 'all' }),
    [switch]$NoPath,
    [switch]$NoService,
    [string]$ReleaseApi = $(if ($env:SOLMU_RELEASE_API) { $env:SOLMU_RELEASE_API } else { 'https://api.github.com/repos/panuhorsmalahti/solmu/releases' }),
    [string]$DownloadBase = $(if ($env:SOLMU_RELEASE_BASE_URL) { $env:SOLMU_RELEASE_BASE_URL } else { 'https://github.com/panuhorsmalahti/solmu/releases/download' })
)
$ErrorActionPreference = 'Stop'
$serviceDir = if ($env:SOLMU_SERVICE_DIR) { $env:SOLMU_SERVICE_DIR } else { Join-Path $env:USERPROFILE '.solmu' }
$webDestination = if ($env:SOLMU_WEB_INSTALL_DIR) { $env:SOLMU_WEB_INSTALL_DIR } else { Join-Path $serviceDir 'web' }
$NoService = $NoService -or $env:SOLMU_NO_SERVICE -eq '1'
function Test-WebDestination([string]$Directory) {
    if (Test-Path -LiteralPath $Directory) {
        if ((Get-Item -LiteralPath $Directory -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Web destination is a link' }
        if (-not (Test-Path -LiteralPath (Join-Path $Directory '.solmu-web') -PathType Leaf)) { throw 'Web destination already exists and is not managed by Solmu' }
        if (Get-ChildItem -LiteralPath $Directory -Recurse -Force | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) { throw 'Web destination contains links' }
    }
}
function Install-Web([string]$Directory, [string]$Source) {
    New-Item -ItemType Directory -Path $Directory -Force | Out-Null
    if (Test-Path -LiteralPath (Join-Path $Source 'assets')) {
        New-Item -ItemType Directory -Path (Join-Path $Directory 'assets') -Force | Out-Null
        Get-ChildItem -LiteralPath (Join-Path $Source 'assets') -Force | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $Directory 'assets') -Recurse -Force }
    }
    Copy-Item -LiteralPath (Join-Path $Source 'index.html') -Destination (Join-Path $Directory '.index.html.new') -Force
    Move-Item -LiteralPath (Join-Path $Directory '.index.html.new') -Destination (Join-Path $Directory 'index.html') -Force
    [IO.File]::WriteAllText((Join-Path $Directory '.solmu-web'), $Version)
    Write-Host "Installed Solmu web $Version to $Directory"
    Write-Host 'Open http://127.0.0.1:3000 with the backend running.'
}
if (-not $InstallDir) {
    $InstallDir = if ($Component -eq 'web') { $webDestination } else { Join-Path $env:LOCALAPPDATA 'Solmu\bin' }
}
$InstallDir = [IO.Path]::GetFullPath($InstallDir)
$serviceDir = [IO.Path]::GetFullPath($serviceDir)
if ($Component -eq 'web') { Test-WebDestination $InstallDir }
if ($Component -eq 'all') { Test-WebDestination $webDestination }
if (-not $Version) {
    try { $Version = (Invoke-RestMethod "$ReleaseApi/latest").tag_name }
    catch { throw 'No Solmu release is available. See the GitHub Releases page.' }
}
if (-not $Version.StartsWith('v')) { $Version = "v$Version" }
if ($Version -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') { throw 'Invalid Solmu release version' }
if ($Component -ne 'web' -and [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') { throw 'Windows releases currently support x64. Build Solmu from source for another architecture.' }
$asset = if ($Component -eq 'web') { "solmu-$Version-web.zip" } else { "solmu-$Version-windows-x86_64.zip" }
$solmuTemporary = Join-Path ([System.IO.Path]::GetTempPath()) ("solmu-install-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $solmuTemporary | Out-Null
try {
    function Expand-Release([string]$asset, [string]$Output, [bool]$Web) {
        $archive = Join-Path $solmuTemporary $asset
        $cacheFile = $null
        if ($env:SOLMU_INSTALL_CACHE_DIR) {
            New-Item -ItemType Directory -Path $env:SOLMU_INSTALL_CACHE_DIR -Force | Out-Null
            $cacheFile = Join-Path $env:SOLMU_INSTALL_CACHE_DIR $asset
        }
        if ($cacheFile -and (Test-Path -LiteralPath $cacheFile -PathType Leaf)) { Copy-Item -LiteralPath $cacheFile -Destination $archive }
        else { Invoke-WebRequest -UseBasicParsing "$DownloadBase/$Version/$asset" -OutFile $archive }
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
        if ($actual -ne $expected -and $cacheFile -and (Test-Path -LiteralPath $cacheFile -PathType Leaf)) {
            Remove-Item -LiteralPath $cacheFile -Force
            Invoke-WebRequest -UseBasicParsing "$DownloadBase/$Version/$asset" -OutFile $archive
            $hasher = [System.Security.Cryptography.SHA256]::Create()
            $inputStream = [System.IO.File]::OpenRead($archive)
            try { $actual = [BitConverter]::ToString($hasher.ComputeHash($inputStream)).Replace('-', '').ToLowerInvariant() }
            finally { $inputStream.Dispose(); $hasher.Dispose() }
        }
        if ($actual -ne $expected) { throw 'Release checksum mismatch' }
        if ($cacheFile -and -not (Test-Path -LiteralPath $cacheFile -PathType Leaf)) { Copy-Item -LiteralPath $archive -Destination $cacheFile }
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $zip = [System.IO.Compression.ZipFile]::OpenRead($archive)
        $binaries = @('solmu-backend.exe', 'solmu.exe', 'solmu-desktop.exe', 'boxer.exe', 'muxer.exe', 'muxer-gui.exe')
        $entries = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
        try {
            foreach ($entry in $zip.Entries) {
                if (-not $entries.Add($entry.FullName)) { throw 'Duplicate release archive entry' }
                $fileType = ($entry.ExternalAttributes -shr 16) -band 61440
                if ($fileType -notin @(0, 32768, 16384)) { throw 'Links and special files are not allowed in release archives' }
                if ($Web) {
                    if ($entry.FullName -notmatch '^[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*/?$' -or ($entry.FullName.Split('/') | Where-Object { $_ -in @('.', '..') })) { throw 'Unsafe path in web archive' }
                } elseif ($entry.FullName -notin ($binaries + @('solmu-cli.exe'))) { throw "Unexpected file in release archive: $($entry.FullName)" }
            }
        } finally { $zip.Dispose() }
        [System.IO.Compression.ZipFile]::ExtractToDirectory($archive, $Output)
        if ($Web) {
            if (-not (Test-Path -LiteralPath (Join-Path $Output 'index.html') -PathType Leaf)) { throw 'Missing web index.html' }
        } else {
            if (Test-Path -LiteralPath (Join-Path $Output 'solmu-cli.exe')) {
                if (Test-Path -LiteralPath (Join-Path $Output 'solmu.exe')) { throw 'Ambiguous CLI binaries in archive' }
                Copy-Item -LiteralPath (Join-Path $Output 'solmu-cli.exe') -Destination (Join-Path $Output 'solmu.exe')
            }
            foreach ($binary in $binaries) { if (-not (Test-Path -LiteralPath (Join-Path $Output $binary) -PathType Leaf)) { throw "Missing binary: $binary" } }
        }
    }
    $binaries = @('solmu-backend.exe', 'solmu.exe', 'solmu-desktop.exe', 'boxer.exe', 'muxer.exe', 'muxer-gui.exe')
    $needsService = $Component -in @('backend', 'all') -and -not $NoService
    $installerBase = if ($env:SOLMU_INSTALLER_BASE_URL) { $env:SOLMU_INSTALLER_BASE_URL.TrimEnd('/') } else { 'https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts' }
    $helper = Join-Path $solmuTemporary 'service.ps1'
    Invoke-WebRequest -UseBasicParsing "$installerBase/service.ps1" -OutFile $helper
    . $helper
    if ($Component -eq 'web') {
        Expand-Release $asset (Join-Path $solmuTemporary 'web') $true
        Install-Web $InstallDir (Join-Path $solmuTemporary 'web')
        Register-SolmuModule 'web' $InstallDir $Version $serviceDir
        if ($env:SOLMU_NO_AUTO_UPDATE -ne '1') { Register-SolmuAutoUpdate $serviceDir }
        return
    }
    Expand-Release $asset (Join-Path $solmuTemporary 'files') $false
    if ($Component -eq 'all') { Expand-Release "solmu-$Version-web.zip" (Join-Path $solmuTemporary 'web') $true }
    if ($needsService) { Stop-SolmuService }
    $selected = switch ($Component) {
        'all' { $binaries }
        'backend' { 'solmu-backend.exe' }
        'cli' { 'solmu.exe' }
        'desktop' { 'solmu-desktop.exe' }
        'muxer' { 'solmu.exe'; 'muxer.exe' }
        'muxer-gui' { 'solmu.exe'; 'muxer.exe'; 'muxer-gui.exe' }
        'boxer' { 'boxer.exe' }
    }
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    foreach ($binary in $selected) { Copy-Item -LiteralPath (Join-Path $solmuTemporary "files\$binary") -Destination (Join-Path $InstallDir $binary) -Force }
    if ($Component -in @('backend', 'all')) {
        New-Item -ItemType Directory -Path $serviceDir -Force | Out-Null
        if (-not $NoService) { [IO.File]::WriteAllText((Join-Path $serviceDir '.solmu-backend-version'), $Version, (New-Object System.Text.UTF8Encoding($false))) }
    }
    # Older Boxer/Muxer releases still locate the legacy runtime name.
    if ('solmu.exe' -in $selected -and (Test-Path -LiteralPath (Join-Path $solmuTemporary 'files/solmu-cli.exe'))) {
        Copy-Item -LiteralPath (Join-Path $solmuTemporary 'files/solmu-cli.exe') -Destination (Join-Path $InstallDir 'solmu-cli.exe') -Force
    }
    if (-not $NoPath) {
        [string]$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        if (($userPath -split ';') -notcontains $InstallDir) { [Environment]::SetEnvironmentVariable('Path', (($userPath.TrimEnd(';') + ';' + $InstallDir).TrimStart(';')), 'User') }
        if (($env:Path -split ';') -notcontains $InstallDir) { $env:Path = "$InstallDir;$env:Path" }
    }
    $moduleNames = switch ($Component) {
        'all' { @('backend', 'cli', 'desktop', 'boxer', 'muxer', 'muxer-gui') }
        'muxer' { @('cli', 'muxer') }
        'muxer-gui' { @('cli', 'muxer', 'muxer-gui') }
        default { @($Component) }
    }
    if ($NoService -and $Component -eq 'backend') { $moduleNames = @() }
    if ($NoService -and $Component -eq 'all') { $moduleNames = @('cli', 'desktop', 'boxer', 'muxer', 'muxer-gui') }
    foreach ($module in $moduleNames) { Register-SolmuModule $module $InstallDir $Version $serviceDir }
    if ($Component -eq 'all') {
        Install-Web $webDestination (Join-Path $solmuTemporary 'web')
        Register-SolmuModule 'web' $webDestination $Version $serviceDir
    }
    if ($Component -in @('backend', 'all') -and -not $NoService) { Start-SolmuService (Join-Path $InstallDir 'solmu-backend.exe') $serviceDir }
    if ($env:SOLMU_NO_AUTO_UPDATE -ne '1' -and (Get-Command Register-SolmuAutoUpdate -ErrorAction SilentlyContinue)) { Register-SolmuAutoUpdate $serviceDir }
    Write-Host "Installed Solmu $Version ($Component) to $InstallDir"
    Write-Host 'Clients connect to a separately running solmu-backend. New terminals will use the updated PATH.'
} finally {
    $resolvedTemporary = [System.IO.Path]::GetFullPath($solmuTemporary)
    $temporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolvedTemporary.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Invalid installer temporary directory' }
    Remove-Item -LiteralPath $resolvedTemporary -Recurse -Force
}
