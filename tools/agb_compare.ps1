[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Probe,
    [Parameter(Mandatory)][string]$Output,
    [string]$Workload,
    [ValidateSet('vulkan', 'dx12', 'metal', 'auto')][string]$Left = 'vulkan',
    [ValidateSet('vulkan', 'dx12', 'metal', 'auto')][string]$Right = 'dx12',
    [ValidatePattern('^[0-9]+x[0-9]+$')][string]$Size = '1280x720',
    [ValidateRange(1, 10)][int]$Trials = 3,
    [ValidateRange(1, 1000000)][int]$Frames = 480
)

$ErrorActionPreference = 'Stop'
if ($Left -eq $Right) { throw 'Left and Right must be different native backends.' }
$exe = (Resolve-Path -LiteralPath $Probe).Path
$outputDir = (New-Item -Path $Output -ItemType Directory -Force).FullName
[string[]]$workloadArgs = if ($Workload) { @('--workload', (Resolve-Path -LiteralPath $Workload).Path) } else { @() }

foreach ($case in 'static', 'orbit', 'assets') {
    foreach ($trial in 1..$Trials) {
        $order = if ($trial % 2 -eq 0) { @($Right, $Left) } else { @($Left, $Right) }
        foreach ($lane in $order) {
            [string[]]$capture = if ($trial -eq 1) { @('--capture', (Join-Path $outputDir "$case-$lane.png")) } else { @() }
            [string[]]$source = if ($case -eq 'assets') { @('--shadows') } else { $workloadArgs }
            $report = & $exe --backend $lane --fixture $case --size $Size --frames $Frames --warmup 120 @source @capture
            if ($LASTEXITCODE -ne 0) { throw "Probe failed for $case, $lane, trial $trial" }
            $report | Set-Content -LiteralPath (Join-Path $outputDir "$case-$lane-$trial.json") -Encoding utf8
            $data = ($report -join [Environment]::NewLine) | ConvertFrom-Json
            [pscustomobject]@{
                Fixture = $case; Backend = $data.adapter.backend; Trial = $trial
                P95ms = [math]::Round($data.frame_complete_ms.p95, 3)
                RAMMiB = [math]::Round($data.cpu_working_set_bytes / 1MB, 2)
                SharedGpuMiB = if ($null -ne $data.gpu_shared_bytes) { [math]::Round($data.gpu_shared_bytes / 1MB, 2) }
                DedicatedGpuMiB = if ($null -ne $data.gpu_dedicated_bytes) { [math]::Round($data.gpu_dedicated_bytes / 1MB, 2) }
                UploadBytes = $data.upload_bytes_during_samples
            }
        }
    }
}
