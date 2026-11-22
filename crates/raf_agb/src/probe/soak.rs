use std::time::{Duration, Instant};

use raf_agb::{fixtures, handles::MeshHandle, Backend, Device, Result};

use super::{metrics, INITIAL_ORBIT};

const PRESENT_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 30);
const SAMPLE_INTERVAL: Duration = Duration::from_secs(10);

#[derive(serde::Serialize)]
pub struct Soak {
    duration_seconds: f64,
    frames: u32,
    samples: Vec<Sample>,
}

#[derive(serde::Serialize)]
struct Sample {
    seconds: f64,
    working_set_bytes: Option<u64>,
    private_bytes: Option<u64>,
    shared_gpu_bytes: Option<u64>,
    dedicated_gpu_bytes: Option<u64>,
}

pub fn run<B: Backend>(
    device: &mut Device<B>,
    handle: MeshHandle,
    extent: [u32; 2],
    seconds: u32,
) -> Result<Soak> {
    let start = Instant::now();
    let end = start + Duration::from_secs(seconds.into());
    let mut next_sample = start + SAMPLE_INTERVAL;
    let mut next_frame = start;
    let mut frames = 0;
    let mut samples = vec![sample(start)];
    while Instant::now() < end {
        let orbit = INITIAL_ORBIT + start.elapsed().as_secs_f32() * 0.24;
        device.draw(handle, &fixtures::camera(extent, orbit))?;
        device.wait()?;
        frames += 1;
        let now = Instant::now();
        if now >= next_sample {
            samples.push(sample(start));
            next_sample = now + SAMPLE_INTERVAL;
        }
        next_frame = (next_frame + PRESENT_INTERVAL).max(now);
        std::thread::sleep(
            next_frame
                .min(end)
                .saturating_duration_since(Instant::now()),
        );
    }
    samples.push(sample(start));
    Ok(Soak {
        duration_seconds: start.elapsed().as_secs_f64(),
        frames,
        samples,
    })
}

fn sample(start: Instant) -> Sample {
    let cpu = metrics::process_memory();
    let (shared_gpu_bytes, dedicated_gpu_bytes) = metrics::gpu_memory();
    Sample {
        seconds: start.elapsed().as_secs_f64(),
        working_set_bytes: cpu.resident,
        private_bytes: cpu.private,
        shared_gpu_bytes,
        dedicated_gpu_bytes,
    }
}
