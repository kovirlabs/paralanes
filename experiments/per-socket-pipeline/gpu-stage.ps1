# Item 1 of docs/cpu-throughput-strategy.md §11: the GPU as the feature stage
# of the step-metrics pipeline, against the CPU reference, across window
# batch sizes. Writes one run file.
#
#   .\gpu-stage.ps1 -StoreDir D:\psp -Out ..\..\atlas\runs\ryzen\2026-11-22-gpu-feature-stage.json -Mode unpaced
#   .\gpu-stage.ps1 ... -Mode realtime -Configs "cpu:4","gpu:1:64"
param(
    [Parameter(Mandatory)] [string] $StoreDir,
    [Parameter(Mandatory)] [string] $Out,
    [ValidateSet('unpaced', 'realtime')] [string] $Mode = 'unpaced',
    [string[]] $Configs = @('cpu:4', 'cpu:1', 'gpu:1:1', 'gpu:1:4', 'gpu:1:16', 'gpu:1:64', 'gpu:1:256', 'gpu:1:1024'),
    [int] $Seconds = 60,
    [int] $Repeats = 3
)
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Join-Path $here 'target\release\per-socket-pipeline.exe'
if (Get-Process Runner.Listener -ErrorAction SilentlyContinue) { throw "stop the GitHub Actions runner first" }
if (Test-Path $Out) { throw "$Out exists; run files are never overwritten" }
New-Item -ItemType Directory -Force $StoreDir | Out-Null
$results = @(); $commands = @()
foreach ($c in $Configs) {
    $p = $c.Split(':')
    $a = @('--layout', 'step-metrics', '--store-dir', $StoreDir, '--seconds', "$Seconds", '--workers', $p[1], '--features', $p[0])
    if ($p[0] -eq 'gpu') { $a += @('--batch', $p[2]) }
    if ($Mode -eq 'realtime') { $a += '--realtime' }
    $commands += "per-socket-pipeline.exe " + ($a -join ' ')
    for ($rep = 1; $rep -le $Repeats; $rep++) {
        Write-Host "$Mode $c rep $rep"
        $r = (& $exe @a | Select-Object -Last 1 | ConvertFrom-Json)
        $results += [pscustomobject]@{ config = $c; rep = $rep; mode = $Mode; result = $r.result }
    }
}
$cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim()
$os = Get-CimInstance Win32_OperatingSystem
$gpuinfo = (Get-CimInstance Win32_VideoController | Where-Object Name -like '*Radeon*' | Select-Object -First 1)
$root = Resolve-Path (Join-Path $here '..\..\..')
$gitc = { param($d) (git -C $d rev-parse HEAD).Trim() + $(if (git -C $d status --porcelain --untracked-files=no) { ' (uncommitted changes)' } else { '' }) }
[ordered]@{
    kind        = "GPU feature stage in the step-metrics pipeline (item 1, docs/cpu-throughput-strategy.md §11): $Mode"
    date        = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd')
    machine     = "ryzen: $cpu, 32 GB DDR4-3200; one socket; bare metal"
    os          = "$($os.Caption) $($os.Version) build $($os.BuildNumber)"
    power_plan  = (powercfg /getactivescheme).Trim()
    rust        = (rustc -V).Trim()
    gpu         = "$($gpuinfo.Name), driver $($gpuinfo.DriverVersion); first hardware adapter wgpu lists (WGPU_BACKEND=$($env:WGPU_BACKEND))"
    gpu_host_link = 'PCIe 3.0 x16 (card supports 4.0 x16), Resizable BAR on'
    network     = 'loopback; sender and ingest on the same machine'
    store       = "$StoreDir on the NVMe (WD_BLACK SN770)"
    configs     = 'cpu:W = the CPU reference (features::reference) on W worker cores; gpu:1:B = one feature thread batching B windows per diagsteno Engine::compute call on the GPU (partial batches flushed after 10 ms idle or at the end)'
    metrics     = 'as step-metrics, plus process_cpu_s (all threads of the process over the run, including the sender), gpu_feature_thread_cpu_in_compute_s (CPU time the GPU feature thread spent inside Engine::compute, which spins on poll while the GPU works), latency_us.ingest_to_feature_row (per window), and every GPU row compared with features::reference after the run'
    commits     = [ordered]@{ paralanes = & $gitc (Join-Path $root 'paralanes'); diagsteno = & $gitc (Join-Path $root 'diagsteno'); bytelanes = & $gitc (Join-Path $root 'bytelanes'); stagelanes = & $gitc (Join-Path $root 'stagelanes') }
    commands    = $commands
    results     = $results
} | ConvertTo-Json -Depth 30 | Set-Content -Encoding utf8 $Out
Write-Host "wrote $Out"
