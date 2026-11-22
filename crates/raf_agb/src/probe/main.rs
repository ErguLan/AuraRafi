mod metrics;
mod soak;
mod startup;
mod window;

use std::{env, fs::File, io::BufReader, path::PathBuf, process::ExitCode, time::Instant};

use raf_agb::{
    backends::native::{NativeBackend, NativeRequest},
    capabilities::GraphicsMemoryBudget,
    fixtures, Backend, Device, Output, Workload,
};

const DEFAULT_EXTENT: [u32; 2] = [1280, 720];
const DEFAULT_FRAMES: usize = 240;
const INITIAL_ORBIT: f32 = 0.82;
const BACKEND_ENV: &str = "RAF_GRAPHICS_BACKEND";

type ProbeResult<T> = Result<T, Box<dyn std::error::Error>>;

struct Options {
    backend: String,
    extent: [u32; 2],
    frames: usize,
    warmup: usize,
    soak_seconds: u32,
    fixture: String,
    window: bool,
    startup: bool,
    scale: f64,
    smoke: bool,
    shadows: bool,
    workload: Option<PathBuf>,
    capture: Option<PathBuf>,
}

impl Options {
    fn from_args() -> ProbeResult<Option<Self>> {
        let mut options = Self {
            backend: env::var(BACKEND_ENV).unwrap_or_default(),
            extent: DEFAULT_EXTENT,
            frames: DEFAULT_FRAMES,
            warmup: 24,
            soak_seconds: 0,
            fixture: "static".into(),
            window: false,
            startup: false,
            scale: 1.,
            smoke: false,
            shadows: false,
            workload: None,
            capture: None,
        };
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            let value = |args: &mut std::iter::Skip<env::Args>| {
                args.next()
                    .ok_or_else(|| format!("missing value for {arg}"))
            };
            match arg.as_str() {
                "--window" => options.window = true,
                "--startup" => options.startup = true,
                "--smoke" => {
                    options.smoke = true;
                    options.window = true;
                }
                "--shadows" => options.shadows = true,
                "--backend" => options.backend = value(&mut args)?,
                "--scale" => options.scale = value(&mut args)?.parse()?,
                "--size" => {
                    let input = value(&mut args)?;
                    let (width, height) =
                        input.split_once('x').ok_or("size must be WIDTHxHEIGHT")?;
                    options.extent = [width.parse()?, height.parse()?];
                }
                "--frames" => options.frames = value(&mut args)?.parse()?,
                "--warmup" => options.warmup = value(&mut args)?.parse()?,
                "--soak-seconds" => options.soak_seconds = value(&mut args)?.parse()?,
                "--fixture" => options.fixture = value(&mut args)?,
                "--workload" => options.workload = Some(value(&mut args)?.into()),
                "--capture" => options.capture = Some(value(&mut args)?.into()),
                "--help" | "-h" => {
                    println!("raf-agb-probe [--backend auto|vulkan|dx12|metal] [--startup] [--window|--smoke] [--size WIDTHxHEIGHT] [--scale N] [--frames N] [--warmup N] [--soak-seconds N] [--fixture static|orbit|assets] [--shadows] [--workload FILE] [--capture PNG]");
                    return Ok(None);
                }
                _ => return Err(format!("unknown option: {arg}").into()),
            }
        }
        if !(1..=1_000_000).contains(&options.frames) || options.warmup > 100_000 {
            return Err("invalid sample count".into());
        }
        if !matches!(options.fixture.as_str(), "static" | "orbit" | "assets") {
            return Err("fixture must be static, orbit or assets".into());
        }
        if options.window && options.capture.is_some() {
            return Err("use offscreen mode to save a capture".into());
        }
        if options.soak_seconds > 3600 || options.window && options.soak_seconds > 0 {
            return Err("soak requires offscreen mode and a duration of up to one hour".into());
        }
        if !options.scale.is_finite() || options.scale <= 0. || options.extent.contains(&0) {
            return Err("nonzero extent and positive scale required".into());
        }
        if options.startup
            && (options.workload.is_some() || options.shadows || options.soak_seconds > 0)
        {
            return Err("startup uses its own fixture and does not run soak benchmarks".into());
        }
        if options.backend.is_empty() {
            options.backend = if options.startup { "auto" } else { "vulkan" }.into();
        }
        Ok(Some(options))
    }

    fn workload(&self) -> ProbeResult<Workload> {
        match &self.workload {
            Some(path) => Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?),
            None => Ok(fixtures::scene(
                self.shadows,
                if self.fixture == "assets" { 23 } else { 3 },
            )),
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ApiGraphicBasic probe: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> ProbeResult<()> {
    let Some(options) = Options::from_args()? else {
        return Ok(());
    };
    if options.window {
        let request = options.backend.parse::<NativeRequest>()?;
        let workload = (!options.startup).then(|| options.workload()).transpose()?;
        return window::run(options, workload, request);
    }
    if options.startup {
        return startup::headless(&options);
    }
    let workload = options.workload()?;
    let init = Instant::now();
    benchmark(
        NativeBackend::headless(options.backend.parse()?)?,
        &options,
        workload,
        init,
    )
}

fn benchmark<B: Backend>(
    backend: B,
    options: &Options,
    workload: Workload,
    init: Instant,
) -> ProbeResult<()> {
    let mut device = Device::new(backend, GraphicsMemoryBudget::potato())?;
    device.configure(options.extent, Output::Capture)?;
    let workload_bytes = workload.bytes();
    let handle = device.upload(&workload)?;
    drop(workload);
    let frame = fixtures::camera(options.extent, INITIAL_ORBIT);
    device.draw(handle, &frame)?;
    device.wait()?;
    let first_frame_ms = metrics::ms(init.elapsed());
    for _ in 0..options.warmup {
        device.draw(handle, &frame)?;
        device.wait()?;
    }
    let mut summary = metrics::Benchmark::new(options.frames);
    let start = Instant::now();
    for index in 0..options.frames {
        let frame = if options.fixture == "orbit" {
            fixtures::camera(options.extent, INITIAL_ORBIT + index as f32 * 0.008)
        } else {
            frame
        };
        let begin = Instant::now();
        let observation = device.draw(handle, &frame)?;
        let submit_ms = metrics::ms(begin.elapsed());
        device.wait()?;
        summary.record(submit_ms, metrics::ms(begin.elapsed()), observation);
    }
    let elapsed_ms = metrics::ms(start.elapsed());
    let memory = metrics::process_memory();
    let (gpu_shared, gpu_dedicated) = metrics::gpu_memory();
    let soak = if options.soak_seconds > 0 {
        Some(soak::run(
            &mut device,
            handle,
            options.extent,
            options.soak_seconds,
        )?)
    } else {
        None
    };
    if let Some(path) = &options.capture {
        let pixels = device.capture()?;
        save_png(path, options.extent, &pixels)?;
    }
    let ledger = device.ledger();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "adapter": device.info(), "size": options.extent, "fixture": options.fixture,
            "retained_workload": options.workload, "frames": options.frames, "shadows": options.shadows,
            "frame_submission_ms": summary.submit.finish(), "frame_complete_ms": summary.complete.finish(),
            "gpu_ms_previous_frame": summary.gpu.finish(), "cpu_peak_working_set_bytes": memory.peak,
            "cpu_working_set_bytes": memory.resident, "cpu_private_bytes": memory.private,
        "gpu_shared_bytes": gpu_shared, "gpu_dedicated_bytes": gpu_dedicated,
            "logical_workload_bytes": workload_bytes,
            "logical_resident_bytes": ledger.resident_bytes, "logical_peak_bytes": ledger.peak_bytes,
            "first_frame_ms": first_frame_ms, "throughput_fps": options.frames as f64 * 1000.0 / elapsed_ms,
        "soak": soak,
            "draws": summary.draws, "submits": summary.submits, "upload_bytes_during_samples": summary.uploads,
        }))?
    );
    Ok(())
}

fn save_png(path: &std::path::Path, extent: [u32; 2], rgba: &[u8]) -> ProbeResult<()> {
    let mut encoder = png::Encoder::new(File::create(path)?, extent[0], extent[1]);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}
