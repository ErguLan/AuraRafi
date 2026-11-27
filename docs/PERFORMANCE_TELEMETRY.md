# Performance Telemetry

AuraRafi exposes one performance contract to the native Agent, the `raf` CLI
and MCP. Every surface reads the same bounded frame samples, computes the same
window aggregates and uses the same benchmark JSON files. The result is
measurement evidence, not a sampling profiler.

This surface answers three practical questions:

- Is the editor viewport hitting its presentation target?
- Where is the frame cost going: CPU, GPU, draw submission or uploads?
- Did a change improve or regress frame performance under the same conditions?

## What is measured

Each frame sample contains:

| Field | Unit | Meaning |
| --- | --- | --- |
| `fps` | frames/s | Presented FPS from the editor scheduler. |
| `frame_ms` | ms | Frame duration. Derived from FPS, with the scheduler frame time as fallback. |
| `cpu_ms` | ms | CPU frame time reported by the editor frame loop. |
| `gpu_ms` | ms | GPU frame time when the backend exposes timestamp queries; otherwise 0. |
| `draw_calls` | count | Total draw calls, with mesh, line, overlay and shadow breakdowns. |
| `upload_bytes` | bytes | Bytes uploaded during the window (summed). |
| `upload_rejected` | count | Latest cumulative upload-rejection counter. |
| `resident_mb` | MiB | Latest resident graphics-memory ledger value. |
| `hitches` | count | Delta of the scheduler hitch counter inside the window. |
| `mesh_cache_misses` | count | Latest mesh cache misses. |
| `shadow_map_rebuilds` | count | Latest shadow map rebuilds. |
| `shadow_prepare_cpu_ms` | ms | Latest shadow-preparation CPU time. |

The samples come from the editor's own frame loop: `finish_frame` forwards the
renderer scheduler metrics and the last-frame metrics into the shared
telemetry ring. The aggregated window also carries a stable environment:

| Environment field | Meaning |
| --- | --- |
| `backend` | Graphics backend used by the attached editor. |
| `gpu_hardware` | Whether the backend is running on GPU hardware. |
| `gpu_timing_supported` | Whether GPU timestamps are available. |
| `target_fps` | Presentation target used for the diagnosis. |
| `requested_target_fps` | Target requested by settings before backend clamping. |
| `width`, `height` | Viewport resolution at sampling time. |
| `revision` | Document revision when the window was requested. |
| `scene_fingerprint` | Stable scene identity used for benchmark comparability. |

The live ring keeps at most 2400 samples and records at most one sample every
250 ms, which covers roughly ten minutes of editor history. Benchmarks are
different: while a benchmark is active, every editor frame is collected.

### Editor viewport, not a standalone runtime

These counters describe the **editor viewport**. A live editor window is
required for `performance.stats` and `performance.benchmark.start`; the Hub and
headless hosts have no frame loop to sample. This surface does not attach to a
standalone `raf_player` session, and telemetry for that process is not part of
this contract yet. Nothing in this surface starts Play or the runtime.

## Live windows

The live window is available through every adapter:

| Surface | Example |
| --- | --- |
| Editor command | `/performance.stats window_seconds=10` |
| CLI | `raf perf --window 10` |
| MCP tool | `raf_performance` with `{"window_seconds":10}` |
| MCP resource | `raf://performance` |

The default window is 5 seconds. Values are clamped to 0.5-600 seconds. A
window ends at the newest recorded sample, so a short window immediately after
opening the editor may contain only a few samples. With no samples at all, the
window returns zeroed metrics and `diagnosis.verdict = "no_data"`.

`MetricStats` exposes `avg`, `min`, `max`, `p50` and `p95` for FPS, frame time,
CPU time, GPU time and draw calls:

- `p50` is the median.
- `p95` is the 95th percentile.
- For `frame_ms`, `cpu_ms` and `gpu_ms`, a `p95` well above `avg` indicates
  spikes or hitches.
- For `fps`, `min` is the worst presented rate in the window, while `p95`
  describes the upper end of the distribution.

Top-level counters are not all percentiles. `upload_bytes` is the sum for the
window, `hitches` is a counter delta, and the draw-call breakdown, resident
memory, upload rejections, cache misses and shadow counters describe the latest
sample.

`raf perf --json` prints the full window object instead of the human summary.
The human summary keeps the headline FPS, frame time, CPU/GPU time and
diagnosis verdict; pass `--json` when a script or agent needs every counter.

## Benchmarks

A benchmark is a warmup period followed by a measured period. Warmup samples are
discarded; measured samples are aggregated into one report.

### Lifecycle

1. **Start** with `/performance.benchmark.start` or `raf_benchmark_start`.
   The command requires an open project and a live editor viewport. Defaults
   are 10 measured seconds and 2 warmup seconds. The measured duration is
   clamped to 1-600 seconds and warmup to 0-60 seconds. Only one benchmark may
   be active at a time.
2. **Warmup** collects frames but discards them. When warmup elapses, the phase
   becomes `measuring` and the sample buffer is cleared.
3. **Measuring** collects every editor frame for the requested duration. The
   run moves to the `completed` phase when the duration is reached, but its
   report is persisted on the next status or stop.
4. **Status** reports phase, elapsed time, measured time, remaining time,
   sample count, average FPS, p95 frame time and average GPU time. Passing an
   `id` while no run is active loads a saved report with that id instead.
5. **Stop** finalizes the active run early. A report needs at least 8 measured
   samples and at least one measured second; otherwise the command returns an
   error and the run remains available.
6. **Cancel** discards the active run without writing a report.
7. **List** returns saved reports for the project, newest first, up to 64.
8. **Compare** reads two saved reports and returns deltas, warnings and a
   verdict.

The report is only written when a benchmark is stopped, or when status observes
that it has finished. Starting or cancelling a benchmark does not create a file.
If another benchmark is started before the completed run is polled or stopped,
the unsaved run is replaced.

### Report location and format

Reports are stored per project:

```text
<project>/.aura_rafi/benchmarks/<id>.json
```

The id is generated from a UTC timestamp and a label slug, for example
`20261125-143012-night-lights`. The file is pretty-printed JSON and written
atomically.

Top-level fields:

| Field | Meaning |
| --- | --- |
| `id` | Benchmark id and file name. |
| `label` | Human-readable label passed at start. |
| `created_at` | RFC 3339 UTC timestamp. |
| `warmup_seconds` | Discarded warmup. |
| `measured_seconds` | Actual measured duration. |
| `sample_count` | Measured frame samples. |
| `project` | `id`, `path` and `engine_version`. |
| `window` | Full `PerformanceWindow` aggregation. |

A trimmed report looks like this:

```json
{
  "id": "20261125-143012-night-lights",
  "label": "night-lights",
  "created_at": "2026-11-25T14:30:22.123456+00:00",
  "warmup_seconds": 2.0,
  "measured_seconds": 10.0,
  "sample_count": 598,
  "project": {
    "id": "3f4c...",
    "path": "D:\\Games\\HorseDemo",
    "engine_version": "0.12.0"
  },
  "window": {
    "window_seconds": 10.0,
    "sample_count": 598,
    "environment": {
      "backend": "Dx12",
      "gpu_hardware": true,
      "gpu_timing_supported": true,
      "target_fps": 60,
      "requested_target_fps": 60,
      "width": 1280,
      "height": 720,
      "revision": 42,
      "scene_fingerprint": 123456789
    },
    "fps": {"avg": 59.8, "min": 51.2, "max": 60.1, "p50": 60.0, "p95": 60.0},
    "frame_ms": {"avg": 16.72, "min": 16.63, "max": 19.53, "p50": 16.67, "p95": 17.91},
    "diagnosis": {
      "verdict": "at_target",
      "evidence": ["Presented 59.8 FPS against a 60 FPS target."],
      "suggestions": ["The engine is already at its presentation target."]
    }
  }
}
```

The example is trimmed; the real `window` also contains CPU, GPU and draw-call
statistics, upload and memory counters, hitches, cache and shadow counters.

### Comparability rules

Two reports are comparable only when all of these match:

- project id;
- scene fingerprint;
- viewport width and height;
- target FPS;
- graphics backend.

Any mismatch sets `comparable: false`, adds a warning that explains the
mismatch, and makes the verdict `incomparable`. This is intentional: a report
captured on another scene, resolution, target or backend is not evidence of a
performance change.

### Comparison verdicts

Deltas are calculated as `b - a`, where `a` is the baseline and `b` is the
candidate. Percentages are relative to the baseline.

| Verdict | Rule |
| --- | --- |
| `improved` | FPS delta is at least +3% and p95 frame time is no worse than +5%. |
| `regressed` | FPS delta is at most -3%, or p95 frame time is at least +10%. |
| `unchanged` | Comparable, but neither threshold is crossed. |
| `incomparable` | At least one comparability check failed. |

A positive frame-time delta means the candidate is slower. The comparison also
returns deltas for average CPU time, average GPU time and average draw calls,
plus the diagnosis verdict of each report.

## Diagnosis

Every window ends with a diagnosis. The verdicts are evaluated in this order:

| Verdict | Condition | Suggested action |
| --- | --- | --- |
| `no_data` | No frame samples in the window. | Keep the viewport active while sampling. |
| `at_target` | Average FPS is at least 97% of `target_fps`. | Raise the FPS target to measure headroom, or compare draw calls if cost is still too high. |
| `gpu_bound` | GPU timing is available and average GPU time is at least 70% of the frame. | Reduce GPU work: shadow resolution, overdraw, triangle count and draw calls. |
| `cpu_bound` | Average CPU time is at least 70% of the frame. | Reduce CPU work: scene updates, UI layout, script ticks and per-frame allocations. |
| `hitch_prone` | Hitches were recorded and p95 frame time is above 1.8x the average. | Inspect shadow rebuilds, mesh cache misses and upload rejections around the hitch frames. |
| `upload_pressure` | The latest sample reports rejected uploads. | Reduce texture churn or raise the graphics memory budget. |
| `balanced` | None of the above dominates. | Compare a benchmark before and after one focused change. |

`at_target` is checked first, so a machine that is stable at its target is
reported as `at_target` even if one subsystem dominates the frame. Raise the
target when the goal is to expose headroom.

`gpu_bound` requires `gpu_timing_supported` and a non-zero GPU sample. Without
timestamp queries, the diagnosis can still report `cpu_bound`, `hitch_prone`,
`upload_pressure` or `balanced`, but it never guesses a GPU bottleneck.

Each diagnosis includes `evidence` and `suggestions`. Evidence normally covers
FPS against the target, frame time with p95, CPU and GPU time, draw calls and
hitch count. Additional lines appear for mesh cache misses and shadow
preparation cost. Suggestions are short, actionable strings; they do not change
any setting automatically.

## Doctor hardware

`raf doctor --json` includes a machine-level snapshot under `hardware`:

```json
{
  "hardware": {
    "os": "windows",
    "os_version": "10.0.22631",
    "kernel_version": "10.0.22631",
    "arch": "x86_64",
    "cpu_brand": "AMD Ryzen 7 5800X 8-Core Processor",
    "cpu_cores": 16,
    "ram_total_mb": 32768,
    "ram_available_mb": 18420
  }
}
```

| Field | Meaning |
| --- | --- |
| `os` | Lowercase platform constant, for example `windows`, `macos` or `linux`. |
| `os_version` | Human-readable OS version when available. |
| `kernel_version` | Kernel version when available. |
| `arch` | Target architecture. |
| `cpu_brand` | CPU model string when the platform exposes it. |
| `cpu_cores` | Usable logical cores. |
| `ram_total_mb` | Total physical memory in MiB. |
| `ram_available_mb` | Available physical memory in MiB at the time of the call. |

Optional values may be `null`. The hardware snapshot is cheap and intended for
an explicit doctor call, not per-frame use. It does not replace the performance
environment: backend, resolution and target FPS come from the attached editor
that owns the live graphics device.

## Limitations

- `performance.stats` and `performance.benchmark.start` require a live editor
  viewport with an open project.
- The Hub and headless hosts have no frame loop. They cannot answer live
  telemetry, even though headless `doctor` still reports hardware.
- `performance.benchmark.list`, `performance.benchmark.compare` and loading a
  saved report through `performance.benchmark.status` only need the project
  files; they do not need a running viewport.
- Standalone `raf_player` telemetry is not part of this surface yet.
- These are renderer and scheduler counters, not a sampling profiler. There is
  no per-function CPU breakdown.
- Live windows sample at most every 250 ms. Very short windows can be coarse;
  use a benchmark when every frame matters.
- GPU attribution depends on backend timestamp support. Without it, `gpu_ms`
  can be 0 and `gpu_bound` is never selected.
- Telemetry is per editor process. Restarting the editor clears the live ring;
  benchmark reports persist on disk.
- Benchmark comparisons are local to the project. There is no cross-machine
  aggregation.
- Diagnosis is advisory. It never changes graphics settings by itself.

## CLI examples

Run these while the project is open in an editor window. The benchmark
examples pass their arguments with the CLI's standard `--params` JSON envelope.

```text
raf perf
raf perf --window 10
raf perf --window 30 --json

raf benchmark start --params '{"seconds":10,"warmup_seconds":2,"label":"night-lights"}'
raf benchmark status
raf benchmark status --params '{"id":"20261125-143012-night-lights"}'
raf benchmark stop --json
raf benchmark cancel
raf benchmark list --json
raf benchmark compare --params '{"a":"20261125-143012-baseline","b":"20261125-150000-night-lights"}' --json
```

The corresponding editor commands are:

```text
/performance.stats window_seconds=10
/performance.benchmark.start seconds=10 warmup_seconds=2 label=night-lights
/performance.benchmark.status
/performance.benchmark.status id=20261125-143012-night-lights
/performance.benchmark.stop
/performance.benchmark.cancel
/performance.benchmark.list
/performance.benchmark.compare a=20261125-143012-baseline b=20261125-150000-night-lights
```

## MCP examples

Start the attached server:

```text
raf mcp serve --attach D:\Games\HorseDemo
```

The typed tools are:

| Tool | Arguments |
| --- | --- |
| `raf_performance` | `window_seconds` (optional, default 5) |
| `raf_benchmark_start` | `seconds` (default 10), `warmup_seconds` (default 2), `label` (optional) |
| `raf_benchmark_status` | `id` (optional active or saved report id) |
| `raf_benchmark_stop` | `id` (optional active benchmark id) |
| `raf_benchmark_cancel` | none |
| `raf_benchmark_list` | none |
| `raf_benchmark_compare` | `a` and `b` (required benchmark ids) |

Raw JSON-RPC requests over the MCP stdio session:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"raf_performance","arguments":{"window_seconds":10}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"raf_benchmark_start","arguments":{"seconds":10,"warmup_seconds":2,"label":"night-lights"}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"raf_benchmark_status","arguments":{}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"raf_benchmark_stop","arguments":{}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"raf_benchmark_cancel","arguments":{}}}
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"raf_benchmark_list","arguments":{}}}
{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"raf_benchmark_compare","arguments":{"a":"20261125-143012-baseline","b":"20261125-150000-night-lights"}}}
{"jsonrpc":"2.0","id":8,"method":"resources/read","params":{"uri":"raf://performance"}}
```

The `raf://performance` resource returns the same window as `raf_performance`
using a 10-second window. MCP advertises `raf_performance` and the
start/status/stop/cancel lifecycle only when attached to a live editor.
`raf_benchmark_list` and `raf_benchmark_compare` also work through a headless
server with a mounted project because they read project files.

## Related documentation

- [CLI and MCP quickstart](CLI_MCP_QUICKSTART.md)
- [Optimization use cases](Optimization_USECASES.md)
- [Renderer](RENDERER.md)
- [Local runtime](LOCAL_RUNTIME.md)
