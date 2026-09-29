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

function Start-SolmuService([string]$Binary, [string]$Directory) {
    New-Item -ItemType Directory -Path $Directory -Force | Out-Null
    $utf8 = New-Object System.Text.UTF8Encoding($false)
    $configuration = Join-Path $Directory '.env'
    if (-not (Test-Path -LiteralPath $configuration)) {
        [IO.File]::WriteAllText($configuration, "LLM_PROVIDER=openai`nOPENAI_API_KEY=`nLLM_MODEL=gpt-6-sol`nSOLMU_WEB_DIR=web`nSOLMU_AUTO_UPDATE=true`n", $utf8)
    }
    $updater = Join-Path $Directory 'auto-update.ps1'
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
$versionFile = Join-Path $Directory '.solmu-backend-version'
if ((Test-Path -LiteralPath $versionFile) -and (Get-Content -LiteralPath $versionFile -Raw).Trim() -eq $latest) { exit 0 }
$base = if ($env:SOLMU_INSTALLER_BASE_URL) { $env:SOLMU_INSTALLER_BASE_URL.TrimEnd('/') } else { 'https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts' }
$installer = Join-Path ([IO.Path]::GetTempPath()) ('solmu-update-' + [guid]::NewGuid().ToString('N') + '.ps1')
try {
    Invoke-WebRequest -UseBasicParsing "$base/install.ps1" -OutFile $installer
    $env:SOLMU_NO_PATH = '1'
    & $installer -Component backend -Version $latest -NoPath
    if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) { throw "Update installer exited with $LASTEXITCODE" }
} finally { Remove-Item -LiteralPath $installer -Force -ErrorAction SilentlyContinue }
'@
    [IO.File]::WriteAllText($updater, $updaterScript, $utf8)
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
    if (-not (Get-ScheduledTask -TaskName 'Solmu Auto Update' -ErrorAction SilentlyContinue)) {
        $updateAction = New-ScheduledTaskAction -Execute (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -Argument ('-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "' + $updater + '"') -WorkingDirectory $Directory
        $updateTrigger = New-ScheduledTaskTrigger -Daily -At '4:17 AM'
        $updateSettings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew
        Register-ScheduledTask -TaskName 'Solmu Auto Update' -Action $updateAction -Trigger $updateTrigger -Principal $principal -Settings $updateSettings -Force | Out-Null
    }
    Write-Host "Backend runs in the background at login. Set your provider key in $configuration and restart the task."
}
