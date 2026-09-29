# Loaded by the installer after all release files have been verified.
function Stop-SolmuService {
    if (Get-ScheduledTask -TaskName 'Solmu Backend' -ErrorAction SilentlyContinue) {
        Stop-ScheduledTask -TaskName 'Solmu Backend'
        # Allow the supervisor's job handle to close and release the executable.
        for ($attempt = 0; $attempt -lt 50; $attempt++) {
            if ((Get-ScheduledTask -TaskName 'Solmu Backend').State -ne 'Running') { break }
            Start-Sleep -Milliseconds 100
        }
    }
}

function Register-SolmuModule([string]$Name, [string]$Destination, [string]$Version, [string]$Directory) {
    $manifest = Join-Path $Directory 'update-components'
    New-Item -ItemType Directory -Path $manifest -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $manifest $Name), $Destination, (New-Object System.Text.UTF8Encoding($false)))
    [IO.File]::WriteAllText((Join-Path $Directory ".solmu-version-$Name"), $Version, (New-Object System.Text.UTF8Encoding($false)))
}

function Register-SolmuAutoUpdate([string]$Directory) {
    $configuration = Join-Path $Directory '.env'
    if ($env:SOLMU_AUTO_UPDATE -match '^(?i:false|no|off|0)$') { return }
    if (Test-Path -LiteralPath $configuration) {
        foreach ($line in Get-Content -LiteralPath $configuration) {
            if ($line -match '^\s*SOLMU_AUTO_UPDATE\s*=\s*(.*?)\s*(?:#.*)?$' -and $Matches[1].Trim([char[]]@(' ', [char]9, [char]34, [char]39)) -match '^(?i:false|no|off|0)$') { return }
        }
    }
    New-Item -ItemType Directory -Path $Directory -Force | Out-Null
    $updater = Join-Path $Directory 'auto-update-v2.ps1'
    if (-not (Test-Path -LiteralPath $updater)) {
        $updaterScript = @'
$ErrorActionPreference = 'Stop'
$Directory = Split-Path -Parent $PSCommandPath
$enabled = 'true'
$configuration = Join-Path $Directory '.env'
if (Test-Path -LiteralPath $configuration) {
    foreach ($line in Get-Content -LiteralPath $configuration) {
        if ($line -match '^\s*SOLMU_AUTO_UPDATE\s*=\s*(.*?)\s*(?:#.*)?$') { $enabled = $Matches[1].Trim([char[]]@(' ', [char]9, [char]34, [char]39)); break }
    }
}
if ($enabled -match '^(?i:false|no|off|0)$') { exit 0 }
$releaseApi = if ($env:SOLMU_RELEASE_API) { $env:SOLMU_RELEASE_API.TrimEnd('/') } else { 'https://api.github.com/repos/panuhorsmalahti/solmu/releases' }
$latest = (Invoke-RestMethod "$releaseApi/latest").tag_name
if ($latest -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') { exit 0 }
$base = if ($env:SOLMU_INSTALLER_BASE_URL) { $env:SOLMU_INSTALLER_BASE_URL.TrimEnd('/') } else { 'https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts' }
$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ('solmu-update-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
$installer = Join-Path $temporaryDirectory 'install.ps1'
$cache = Join-Path $temporaryDirectory 'cache'
$failed = $false
try {
    Invoke-WebRequest -UseBasicParsing "$base/install.ps1" -OutFile $installer
    foreach ($component in @('web', 'cli', 'desktop', 'boxer', 'muxer', 'backend')) {
        $manifest = Join-Path (Join-Path $Directory 'update-components') $component
        if (-not (Test-Path -LiteralPath $manifest)) { continue }
        $destination = (Get-Content -LiteralPath $manifest -Raw).Trim()
        if ($component -eq 'web') { $marker = Join-Path $destination '.solmu-web' }
        else { $marker = Join-Path $Directory ".solmu-version-$component" }
        if ((Test-Path -LiteralPath $marker) -and (Get-Content -LiteralPath $marker -Raw).Trim() -eq $latest) { continue }
        $processNames = switch ($component) {
            'cli' { @('solmu') }
            'desktop' { @('solmu-desktop') }
            'boxer' { @('boxer') }
            'muxer' { @('muxer') }
            default { @() }
        }
        $busy = $false
        foreach ($processName in $processNames) { if (Get-Process -Name $processName -ErrorAction SilentlyContinue) { $busy = $true } }
        if ($busy) { continue }
        $env:SOLMU_COMPONENT = $component
        $env:SOLMU_INSTALL_DIR = $destination
        $env:SOLMU_SERVICE_DIR = $Directory
        $env:SOLMU_WEB_INSTALL_DIR = Join-Path $Directory 'web'
        $env:SOLMU_INSTALL_CACHE_DIR = $cache
        $env:SOLMU_NO_PATH = '1'
        $env:SOLMU_NO_SERVICE = if ($component -eq 'backend') { '0' } else { '1' }
        try { & $installer -Component $component -Version $latest -NoPath }
        catch { [Console]::Error.WriteLine("Could not update $component`: $_"); $failed = $true }
    }
} finally { Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue }
if ($failed) { exit 1 }
'@
        [IO.File]::WriteAllText($updater, $updaterScript, (New-Object System.Text.UTF8Encoding($false)))
    }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    $action = New-ScheduledTaskAction -Execute (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -Argument ('-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "' + $updater + '"') -WorkingDirectory $Directory
    $trigger = New-ScheduledTaskTrigger -Daily -At '4:17 AM'
    $principal = New-ScheduledTaskPrincipal -UserId $identity -LogonType Interactive -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable -MultipleInstances IgnoreNew
    $existing = Get-ScheduledTask -TaskName 'Solmu Auto Update' -ErrorAction SilentlyContinue
    if (-not $existing -or $existing.Actions.Execute -notmatch 'powershell' -or $existing.Actions.Arguments -notmatch 'auto-update-v2\.ps1') {
        Register-ScheduledTask -TaskName 'Solmu Auto Update' -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
    }
}

function Start-SolmuService([string]$Binary, [string]$Directory) {
    New-Item -ItemType Directory -Path $Directory -Force | Out-Null
    $utf8 = New-Object System.Text.UTF8Encoding($false)
    $configuration = Join-Path $Directory '.env'
    if (-not (Test-Path -LiteralPath $configuration)) {
        [IO.File]::WriteAllText($configuration, "LLM_PROVIDER=openai`nOPENAI_API_KEY=`nLLM_MODEL=gpt-6-sol`nSOLMU_WEB_DIR=web`nSOLMU_AUTO_UPDATE=true`n", $utf8)
    }
    $runner = Join-Path $Directory 'backend-service.ps1'
    $supervisor = @'
param([string]$Binary, [string]$Directory)
$ErrorActionPreference = 'Stop'
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class SolmuJob {
    [StructLayout(LayoutKind.Sequential)] public struct Basic {
        public long ProcessTime, JobTime; public uint Flags;
        public UIntPtr Minimum, Maximum; public uint Active;
        public UIntPtr Affinity; public uint Priority, Scheduling;
    }
    [StructLayout(LayoutKind.Sequential)] public struct Counters {
        public ulong ReadOps, WriteOps, OtherOps, ReadBytes, WriteBytes, OtherBytes;
    }
    [StructLayout(LayoutKind.Sequential)] public struct Extended {
        public Basic Basic; public Counters IO;
        public UIntPtr ProcessMemory, JobMemory, PeakProcessMemory, PeakJobMemory;
    }
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern IntPtr CreateJobObject(IntPtr attributes, string name);
    [DllImport("kernel32.dll", SetLastError=true)] public static extern bool SetInformationJobObject(IntPtr job, int type, ref Extended info, uint size);
    [DllImport("kernel32.dll", SetLastError=true)] public static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
    [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle);
}
"@
$job = [SolmuJob]::CreateJobObject([IntPtr]::Zero, $null)
if ($job -eq [IntPtr]::Zero) { throw 'Cannot create backend job' }
$limits = New-Object SolmuJob+Extended
$basic = New-Object SolmuJob+Basic
$basic.Flags = 8192 # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
$limits.Basic = $basic
if (-not [SolmuJob]::SetInformationJobObject($job, 9, [ref]$limits, [Runtime.InteropServices.Marshal]::SizeOf($limits))) { throw 'Cannot configure backend job' }
# Assign the supervisor before spawning, so every backend child inherits the job.
if (-not [SolmuJob]::AssignProcessToJobObject($job, [Diagnostics.Process]::GetCurrentProcess().Handle)) { throw 'Cannot enter backend job' }
try {
    while ($true) {
        $start = New-Object Diagnostics.ProcessStartInfo
        $start.FileName = $Binary
        $start.WorkingDirectory = $Directory
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        $child = [Diagnostics.Process]::Start($start)
        # Drain both pipes concurrently to avoid blocking the backend.
        $output = New-Object IO.FileStream((Join-Path $Directory 'backend.log'), [IO.FileMode]::Append, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite, 1, [IO.FileOptions]::WriteThrough)
        $errors = New-Object IO.FileStream((Join-Path $Directory 'backend-error.log'), [IO.FileMode]::Append, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite, 1, [IO.FileOptions]::WriteThrough)
        try {
            $outCopy = $child.StandardOutput.BaseStream.CopyToAsync($output)
            $errCopy = $child.StandardError.BaseStream.CopyToAsync($errors)
            $child.WaitForExit()
            $outCopy.GetAwaiter().GetResult()
            $errCopy.GetAwaiter().GetResult()
        } finally { $output.Dispose(); $errors.Dispose(); $child.Dispose() }
        Start-Sleep -Seconds 3
    }
} finally { [void][SolmuJob]::CloseHandle($job) }
'@
    [IO.File]::WriteAllText($runner, $supervisor, $utf8)
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    $arguments = '-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "' + $runner + '" -Binary "' + $Binary + '" -Directory "' + $Directory + '"'
    $action = New-ScheduledTaskAction -Execute (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -Argument $arguments -WorkingDirectory $Directory
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $identity
    $principal = New-ScheduledTaskPrincipal -UserId $identity -LogonType Interactive -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1)
    Register-ScheduledTask -TaskName 'Solmu Backend' -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
    Start-ScheduledTask -TaskName 'Solmu Backend'
    Write-Host "Backend runs in the background at login. Set your provider key in $configuration and restart the task."
}
