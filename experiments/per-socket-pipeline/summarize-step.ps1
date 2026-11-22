# Summarise one or more step-metrics run files: per mode, the mean over the
# repeats (and min..max) of the metrics the strategy's §9 compares.
#   .\summarize-step.ps1 ..\..\atlas\runs\ryzen\2026-11-22-step1-baseline.json [more files]
param([Parameter(Mandatory, ValueFromRemainingArguments)] [string[]] $Files)

function Stat($xs) {
    $v = @($xs | Where-Object { $null -ne $_ } | ForEach-Object { [double]$_ })
    if ($v.Count -eq 0) { return 'n/a' }
    $m = ($v | Measure-Object -Average -Minimum -Maximum)
    '{0:N1} [{1:N1}..{2:N1}]' -f $m.Average, $m.Minimum, $m.Maximum
}

foreach ($f in $Files) {
    $j = Get-Content $f -Raw | ConvertFrom-Json
    "== $(Split-Path -Leaf $f): $($j.kind)"
    foreach ($mode in 'realtime', 'unpaced') {
        $rs = @($j.results | Where-Object mode -eq $mode | ForEach-Object result)
        if ($rs.Count -eq 0) { continue }
        "  $mode ($($rs.Count) runs)"
        "    interrupts/s per pipeline core : " + (Stat ($rs | ForEach-Object { $_.interrupts_per_cpu_per_s.pipeline_cores_all }))
        "    interrupts/s per other core    : " + (Stat ($rs | ForEach-Object { $_.interrupts_per_cpu_per_s.other_cores_all }))
        if ($rs[0].interrupts_per_cpu_per_s.pipeline_cores_local_timer) {
            "    local timer/s, pipeline core   : " + (Stat ($rs | ForEach-Object { $_.interrupts_per_cpu_per_s.pipeline_cores_local_timer }))
        }
        if ($rs[0].dpcs_queued_per_cpu_per_s) {
            "    DPCs/s per pipeline core       : " + (Stat ($rs | ForEach-Object { $_.dpcs_queued_per_cpu_per_s.pipeline_cores }))
            "    % DPC + % interrupt, pipeline  : " + (Stat ($rs | ForEach-Object { $_.dpc_time_pct.pipeline_cores + $_.interrupt_time_pct.pipeline_cores }))
        }
        foreach ($k in 'window_compute', 'ring_transit_ingest_to_store', 'ingest_to_ack') {
            $l = $rs | ForEach-Object { $_.latency_us.$k }
            "    {0,-31}: p50 {1}  p99 {2}  p99.9 {3}  max {4}" -f "$k us", (Stat ($l | % p50)), (Stat ($l | % p99)), (Stat ($l | % p99_9)), (Stat ($l | % max))
        }
        "    elapsed s                      : " + (Stat ($rs | ForEach-Object elapsed_s))
    }
}
