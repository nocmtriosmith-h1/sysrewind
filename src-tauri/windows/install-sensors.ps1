# Run elevated by the installer (see hooks.nsh). Sets up temperature reading:
#  1. installs the signed PawnIO driver if it is not already present,
#  2. creates %ProgramData%\Sysrewind, writable only by SYSTEM/Administrators,
#  3. registers the "Sysrewind Sensors" task that runs `sysrewind.exe --sensors` as SYSTEM at boot.
param([Parameter(Mandatory = $true)][string]$InstallDir)
$ErrorActionPreference = "Continue"

$taskName = "Sysrewind Sensors"
$exe = Join-Path $InstallDir "sysrewind.exe"
# The NSIS installer is 32-bit; if this script ever runs under WOW64, $env:ProgramFiles is
# "Program Files (x86)". ProgramW6432 always names the 64-bit Program Files.
$pf64 = if ($env:ProgramW6432) { $env:ProgramW6432 } else { $env:ProgramFiles }

# The task runs as SYSTEM, so its exe must live where ordinary users cannot replace it
# (or rename its folder). Only Program Files guarantees that; any other location gets no sensors.
$programFiles = [IO.Path]::GetFullPath($pf64).TrimEnd('\') + '\'
$fullInstall = [IO.Path]::GetFullPath($InstallDir).TrimEnd('\') + '\'
if (-not $fullInstall.StartsWith($programFiles, [StringComparison]::OrdinalIgnoreCase)) {
    Write-Output "Sysrewind is not installed under $programFiles; temperature sensors are disabled for safety."
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
    exit 0
}

# 1. PawnIO driver (shared with other hardware tools, so it is kept on uninstall).
$pawnio = Join-Path $pf64 "PawnIO\PawnIOLib.dll"
if (-not (Test-Path $pawnio)) {
    $setup = Join-Path $InstallDir "PawnIO_setup.exe"
    if (Test-Path $setup) {
        Start-Process -FilePath $setup -ArgumentList "-install", "-silent" -Wait
    }
}

# 2. The helper runs as SYSTEM and writes here, so ordinary users must not be able to
#    plant files or links in this folder. Inheritance is removed and Users get read-only.
$data = Join-Path $env:ProgramData "Sysrewind"
New-Item -ItemType Directory -Force $data | Out-Null
& icacls $data /inheritance:r /grant:r "*S-1-5-18:(OI)(CI)F" "*S-1-5-32-544:(OI)(CI)F" "*S-1-5-32-545:(OI)(CI)RX" | Out-Null

# 3. Scheduled task: start at boot, never time out, keep running on battery, restart if it crashes.
$action = New-ScheduledTaskAction -Execute $exe -Argument "--sensors"
$trigger = New-ScheduledTaskTrigger -AtStartup
$principal = New-ScheduledTaskPrincipal -UserId "SYSTEM" -LogonType ServiceAccount -RunLevel Highest
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) -MultipleInstances IgnoreNew
Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Principal $principal `
    -Settings $settings -Description "Reads CPU and disk temperatures for Sysrewind." -Force | Out-Null
Start-ScheduledTask -TaskName $taskName
