use std::time::Instant;

use raf_agb::{
    backends::native::NativeBackend,
    capabilities::GraphicsMemoryBudget,
    product::{startup, ProductDevice},
    Output,
};

use super::{metrics, save_png, Options, ProbeResult};

pub fn headless(options: &Options) -> ProbeResult<()> {
    let begin = Instant::now();
    let backend = NativeBackend::headless(options.backend.parse()?)?;
    let mut device = ProductDevice::new(backend, GraphicsMemoryBudget::potato())?;
    let initialization_ms = metrics::ms(begin.elapsed());
    device.configure(options.extent, Output::Capture)?;
    let frame = startup::draw(&mut device, options.extent, options.scale, 1)?;
    device.wait()?;
    let first_frame_ms = metrics::ms(begin.elapsed());
    let memory = metrics::process_memory();
    let (gpu_shared, gpu_dedicated) = metrics::gpu_memory();
    if let Some(path) = &options.capture {
        save_png(path, options.extent, &device.capture()?)?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "adapter": device.info(),
            "initialization_ms": initialization_ms,
            "first_frame_ms": first_frame_ms,
            "resident_bytes": device.ledger().resident_bytes,
            "cpu_working_set_bytes": memory.resident,
            "gpu_shared_bytes": gpu_shared,
            "gpu_dedicated_bytes": gpu_dedicated,
            "first_frame": frame
        }))?
    );
    Ok(())
}
