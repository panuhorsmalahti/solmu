param(
    [string]$Version = $env:SOLMU_VERSION,
    [string]$InstallDir = $env:SOLMU_INSTALL_DIR,
    [switch]$NoPath,
    [switch]$NoService,
    [string]$ReleaseApi = $(if ($env:SOLMU_RELEASE_API) { $env:SOLMU_RELEASE_API } else { 'https://api.github.com/repos/panuhorsmalahti/solmu/releases' }),
    [string]$DownloadBase = $(if ($env:SOLMU_RELEASE_BASE_URL) { $env:SOLMU_RELEASE_BASE_URL } else { 'https://github.com/panuhorsmalahti/solmu/releases/download' })
)
$ErrorActionPreference = 'Stop'
$solmuInstallerBase = if ($env:SOLMU_INSTALLER_BASE_URL) { $env:SOLMU_INSTALLER_BASE_URL.TrimEnd('/') } else { 'https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts' }
$solmuInstaller = Invoke-RestMethod "$solmuInstallerBase/install.ps1"
& ([scriptblock]::Create([string]$solmuInstaller)) -Component 'boxer' -Version $Version -InstallDir $InstallDir -NoPath:$NoPath -NoService:$NoService -ReleaseApi $ReleaseApi -DownloadBase $DownloadBase
