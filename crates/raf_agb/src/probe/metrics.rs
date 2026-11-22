use std::time::Duration;

use raf_agb::FrameMetrics;

#[cfg(windows)]
mod gpu_memory;

pub fn ms(elapsed: Duration) -> f64 {
    elapsed.as_secs_f64() * 1000.0
}

pub struct Benchmark {
    pub submit: Samples,
    pub complete: Samples,
    pub gpu: Samples,
    pub draws: u64,
    pub submits: u64,
    pub uploads: u64,
}

impl Benchmark {
    pub fn new(count: usize) -> Self {
        let samples = || Samples(Vec::with_capacity(count));
        Self {
            submit: samples(),
            complete: samples(),
            gpu: samples(),
            draws: 0,
            submits: 0,
            uploads: 0,
        }
    }

    pub fn record(&mut self, submit: f64, completion: f64, metrics: FrameMetrics) {
        self.submit.0.push(submit);
        self.complete.0.push(completion);
        self.gpu.0.extend(metrics.gpu_ms);
        self.draws += u64::from(metrics.draw_calls);
        self.submits += u64::from(metrics.submits);
        self.uploads += metrics.upload_bytes;
    }
}

pub struct Samples(Vec<f64>);

impl Samples {
    pub fn finish(mut self) -> Option<serde_json::Value> {
        if self.0.is_empty() {
            return None;
        }
        self.0.sort_unstable_by(f64::total_cmp);
        let at = |p: f64| self.0[(p * self.0.len() as f64).ceil() as usize - 1];
        Some(
            serde_json::json!({ "p50": at(0.50), "p95": at(0.95), "p99": at(0.99),
            "max": self.0.last(), "over_50ms": self.0.iter().filter(|&&ms| ms > 50.).count() }),
        )
    }
}

#[derive(Default)]
pub struct Memory {
    pub resident: Option<u64>,
    pub peak: Option<u64>,
    pub private: Option<u64>,
}

pub fn gpu_memory() -> (Option<u64>, Option<u64>) {
    #[cfg(windows)]
    return gpu_memory::sample();
    #[cfg(not(windows))]
    (None, None)
}

pub fn process_memory() -> Memory {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::{ProcessStatus::*, Threading::GetCurrentProcess};
        let size = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let mut metrics: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
        metrics.cb = size;
        if unsafe {
            K32GetProcessMemoryInfo(
                GetCurrentProcess(),
                (&mut metrics as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                size,
            )
        } != 0
        {
            return Memory {
                resident: Some(metrics.WorkingSetSize as u64),
                peak: Some(metrics.PeakWorkingSetSize as u64),
                private: Some(metrics.PrivateUsage as u64),
            };
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            let lookup = |prefix: &str| {
                status
                    .lines()
                    .find_map(|line| line.strip_prefix(prefix))
                    .and_then(|line| line.split_whitespace().next())
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(|kib| kib * 1024)
            };
            return Memory {
                resident: lookup("VmRSS:"),
                peak: lookup("VmHWM:"),
                private: None,
            };
        }
    }
    Memory::default()
}
