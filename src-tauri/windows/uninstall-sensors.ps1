# Run elevated by the installer before files are replaced or removed (see hooks.nsh).
# Stops the sensor helper so sysrewind.exe is not locked. With -Remove, also deletes the task and its data.
param([switch]$Remove)
$ErrorActionPreference = "SilentlyContinue"

$taskName = "Sysrewind Sensors"
Stop-ScheduledTask -TaskName $taskName
# The helper is the only sysrewind.exe running as SYSTEM.
Get-CimInstance Win32_Process -Filter "Name = 'sysrewind.exe'" |
    Where-Object { $_.CommandLine -match "--sensors" } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force }

if ($Remove) {
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    Remove-Item -Recurse -Force (Join-Path $env:ProgramData "Sysrewind")
}
