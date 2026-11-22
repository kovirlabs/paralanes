# Step 2c of the CPU throughput strategy on Windows: steer device interrupts
# to the housekeeping core, so pipeline cores take as few as possible.
#
# For every present PCI device, sets the documented interrupt affinity policy
# (HKLM\SYSTEM\CurrentControlSet\Enum\<device>\Device Parameters\
#  Interrupt Management\Affinity Policy):
#   DevicePolicy          = 4  (IrqPolicySpecifiedProcessors)
#   AssignmentSetOverride = processor mask (default: CPUs 0 and 1, core 0)
# Any existing values are saved first, and -Revert puts them back exactly.
# The change takes effect after a reboot (or a device restart).
#
#   .\windows-irq-affinity.ps1 -Show
#   .\windows-irq-affinity.ps1 -Apply  [-Mask 0x3]    # elevated; then reboot
#   .\windows-irq-affinity.ps1 -Revert                # elevated; then reboot
param([switch] $Apply, [switch] $Revert, [switch] $Show, [UInt64] $Mask = 0x3)
$ErrorActionPreference = 'Stop'
$backup = Join-Path $PSScriptRoot 'irq-affinity-backup.json'
$enum = 'HKLM:\SYSTEM\CurrentControlSet\Enum'

function Get-Devices {
    Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -like 'PCI\*' -and $_.Status -eq 'OK' }
}
function Policy-Key($id) { Join-Path $enum "$id\Device Parameters\Interrupt Management\Affinity Policy" }
function Read-Policy($id) {
    $k = Policy-Key $id
    if (-not (Test-Path $k)) { return [ordered]@{ key_existed = $false } }
    $p = Get-ItemProperty $k
    [ordered]@{
        key_existed           = $true
        DevicePolicy          = $p.DevicePolicy
        AssignmentSetOverride = if ($null -ne $p.AssignmentSetOverride) { [Convert]::ToBase64String([byte[]]$p.AssignmentSetOverride) } else { $null }
    }
}

if ($Show -or (-not $Apply -and -not $Revert)) {
    foreach ($d in Get-Devices) {
        $p = Read-Policy $d.InstanceId
        '{0,-12} {1,-55} policy={2} mask={3}' -f $d.Class, $d.FriendlyName, $p.DevicePolicy, $p.AssignmentSetOverride
    }
    return
}
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { throw 'run elevated (Administrator)' }

if ($Apply) {
    if (Test-Path $backup) { throw "$backup exists: revert first, so the original settings are not lost" }
    $saved = [ordered]@{}
    foreach ($d in Get-Devices) { $saved[$d.InstanceId] = Read-Policy $d.InstanceId }
    $saved | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $backup
    $bytes = [BitConverter]::GetBytes([UInt64]$Mask)
    foreach ($d in Get-Devices) {
        $k = Policy-Key $d.InstanceId
        New-Item -Force $k | Out-Null
        New-ItemProperty -Force -Path $k -Name DevicePolicy -PropertyType DWord -Value 4 | Out-Null
        New-ItemProperty -Force -Path $k -Name AssignmentSetOverride -PropertyType Binary -Value $bytes | Out-Null
        "set   {0,-12} {1}" -f $d.Class, $d.FriendlyName
    }
    "Saved the previous settings to $backup. Reboot to apply."
}

if ($Revert) {
    if (-not (Test-Path $backup)) { throw "no backup at $backup" }
    $saved = Get-Content $backup -Raw | ConvertFrom-Json
    foreach ($prop in $saved.PSObject.Properties) {
        $id = $prop.Name; $p = $prop.Value; $k = Policy-Key $id
        if (-not $p.key_existed) {
            if (Test-Path $k) { Remove-Item -Recurse -Force $k }
        } else {
            foreach ($name in 'DevicePolicy', 'AssignmentSetOverride') { Remove-ItemProperty -Path $k -Name $name -ErrorAction SilentlyContinue }
            if ($null -ne $p.DevicePolicy) { New-ItemProperty -Force -Path $k -Name DevicePolicy -PropertyType DWord -Value $p.DevicePolicy | Out-Null }
            if ($null -ne $p.AssignmentSetOverride) { New-ItemProperty -Force -Path $k -Name AssignmentSetOverride -PropertyType Binary -Value ([Convert]::FromBase64String($p.AssignmentSetOverride)) | Out-Null }
        }
        "restored $id"
    }
    Remove-Item $backup
    'Reboot to apply.'
}
