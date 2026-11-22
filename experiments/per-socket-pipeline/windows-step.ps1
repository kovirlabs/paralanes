# Windows run of the CPU throughput strategy benchmark (docs/cpu-throughput-strategy.md §9)
# with the step-metrics layout: N repeats real time (60 s of data at the
# real rate) and N repeats unpaced (the same data as fast as possible).
# Writes one run file in the same shape as the Dell's step-1 run file.
#
#   .\windows-step.ps1 -Step 1 -Label baseline -StoreDir D:\psp -Out ..\..\atlas\runs\ryzen\2026-11-22-step1-baseline.json -Settings "stock"
#
# Stop the GitHub Actions runner first; nothing else heavy may run.

param(
    [Parameter(Mandatory)] [int] $Step,
    [Parameter(Mandatory)] [string] $Label,
    [Parameter(Mandatory)] [string] $StoreDir,
    [Parameter(Mandatory)] [string] $Out,
    [Parameter(Mandatory)] [string] $Settings,
    [int] $Seconds = 60,
    [int] $Repeats = 3,
    [int] $Workers = 4,
    [string] $ExtraArgs = ""
)
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Join-Path $here 'target\release\per-socket-pipeline.exe'
if (-not (Test-Path $exe)) { throw "build first: cargo build --release" }
if (Get-Process Runner.Listener -ErrorAction SilentlyContinue) { throw "stop the GitHub Actions runner first" }
New-Item -ItemType Directory -Force $StoreDir | Out-Null

$results = @()
$commands = @()
foreach ($mode in 'realtime', 'unpaced') {
    for ($rep = 1; $rep -le $Repeats; $rep++) {
        $args = @('--layout', 'step-metrics', '--store-dir', $StoreDir, '--seconds', "$Seconds", '--workers', "$Workers")
        if ($mode -eq 'realtime') { $args += '--realtime' }
        if ($ExtraArgs) { $args += $ExtraArgs.Split(' ') }
        $commands += "per-socket-pipeline.exe " + ($args -join ' ')
        Write-Host "step $Step $Label $mode rep $rep"
        $line = & $exe @args | Select-Object -Last 1
        $run = $line | ConvertFrom-Json
        $results += [pscustomobject]@{ rep = $rep; mode = $mode; layout = $run.layout; seconds_of_data = $run.seconds_of_data; store_dir = $run.store_dir; workers_per_shard = $run.workers_per_shard; result = $run.result }
    }
}

$cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim()
$os = Get-CimInstance Win32_OperatingSystem
$disk = Get-PhysicalDisk | Where-Object { $_.BusType -eq 'NVMe' } | Select-Object -First 1
$gitc = { param($d) (git -C $d rev-parse HEAD).Trim() + $(if (git -C $d status --porcelain --untracked-files=no) { ' (uncommitted changes)' } else { '' }) }
$root = Resolve-Path (Join-Path $here '..\..\..')
$file = [ordered]@{
    kind            = "CPU throughput strategy, benchmark step $Step ($Label), docs/cpu-throughput-strategy.md §9, Windows form"
    date            = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd')
    machine         = "ryzen: $cpu, 32 GB DDR4-3200, Gigabyte A520I AC; one socket, one NUMA node; bare metal (no VM)"
    os              = "$($os.Caption) $($os.Version) build $($os.BuildNumber)"
    power_plan      = (powercfg /getactivescheme).Trim()
    rust            = (rustc -V).Trim()
    gpu             = "not used by this run"
    network         = "loopback (127.0.0.1); sender and ingest on the same machine"
    store           = "$StoreDir on $($disk.FriendlyName) (NVMe, firmware $($disk.FirmwareVersion)); NTFS. The Dell's step-1 run used /dev/shm (memory), so store latency is not comparable; the interrupt and compute metrics are"
    os_settings     = $Settings
    core_plan       = "one shard (single socket), the full 64-channel layout; one logical CPU per physical core (first SMT thread): core 0 housekeeping (occupancy monitor), 1 ingest, 2 store, 3-6 feature workers, 7 sender. Pipeline CPUs are OS CPUs 2,4,6,8,10,12; SMT siblings unused by the pipeline"
    metrics         = "as the Dell step-1 run (latency_us, ring_occupancy), except interrupts: Windows Processor counters via typeperf sampled every 1 s during the run (Interrupts/sec includes the clock; there is no separate local-timer count), plus DPCs Queued/sec, % DPC Time and % Interrupt Time"
    commits         = [ordered]@{ paralanes = & $gitc (Join-Path $root 'paralanes'); stagelanes = & $gitc (Join-Path $root 'stagelanes'); diagsteno = & $gitc (Join-Path $root 'diagsteno'); bytelanes = & $gitc (Join-Path $root 'bytelanes') }
    commands        = $commands
    results         = $results
}
New-Item -ItemType Directory -Force (Split-Path -Parent $Out) | Out-Null
if (Test-Path $Out) { throw "$Out exists; run files are never overwritten" }
$file | ConvertTo-Json -Depth 30 | Set-Content -Encoding utf8 $Out
Write-Host "wrote $Out"
