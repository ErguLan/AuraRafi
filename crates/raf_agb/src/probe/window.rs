use raf_agb::{
    backends::native::{NativeApi, NativeBackend, NativeRequest},
    capabilities::GraphicsMemoryBudget,
    fixtures,
    frame_scheduler::{FrameInvalidation, FramePacingProfile, FramePermit, FrameScheduler},
    handles::MeshHandle,
    product::{startup, ProductDevice},
    Device, Error, FrameMetrics, Output, Workload,
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use super::{Options, ProbeResult, INITIAL_ORBIT};

const SMOKE_TIMEOUT: Duration = Duration::from_secs(12);
const RECOVERY_DELAY: Duration = Duration::from_millis(80);
const PRESENT_RETRY_DELAY: Duration = Duration::from_millis(8);

pub fn run(
    options: Options,
    workload: Option<Workload>,
    request: NativeRequest,
) -> ProbeResult<()> {
    let event_loop = EventLoop::new()?;
    let mut probe = Probe {
        device: None,
        window: None,
        configured: None,
        workload,
        options,
        request,
        api: None,
        startup_layout: None,
        revision: 0,
        orbit: INITIAL_ORBIT,
        clock: Instant::now(),
        cursor_x: None,
        dragging: false,
        scheduler: FrameScheduler::new(FramePacingProfile::Eco),
        permit: None,
        error: None,
        submitted: 0,
        smoke_step: 0,
        wake: None,
        retry: None,
        consecutive_failures: 0,
    };
    event_loop.run_app(&mut probe)?;
    match probe.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct Probe {
    device: Option<Presenter>,
    window: Option<Window>,
    configured: Option<[u32; 2]>,
    workload: Option<Workload>,
    options: Options,
    request: NativeRequest,
    api: Option<NativeApi>,
    startup_layout: Option<([u32; 2], f64)>,
    revision: u64,
    orbit: f32,
    clock: Instant,
    cursor_x: Option<f64>,
    dragging: bool,
    scheduler: FrameScheduler,
    permit: Option<FramePermit>,
    error: Option<Box<dyn std::error::Error>>,
    submitted: u32,
    smoke_step: u8,
    wake: Option<Instant>,
    retry: Option<Instant>,
    consecutive_failures: u8,
}

enum Presenter {
    Fixture(Device<NativeBackend>, Option<MeshHandle>),
    Startup(ProductDevice<NativeBackend>),
}

impl Presenter {
    fn configure(&mut self, extent: [u32; 2]) -> raf_agb::Result<()> {
        match self {
            Self::Fixture(device, _) => device.configure(extent, Output::Present),
            Self::Startup(device) => device.configure(extent, Output::Present),
        }
    }

    fn draw(
        &mut self,
        workload: Option<&Workload>,
        extent: [u32; 2],
        scale: f64,
        revision: u64,
        orbit: f32,
    ) -> raf_agb::Result<FrameMetrics> {
        match self {
            Self::Fixture(device, mesh) => {
                let handle = if let Some(handle) = *mesh {
                    handle
                } else {
                    let workload = workload.ok_or(Error::Invalid("missing probe fixture"))?;
                    let handle = device.upload(workload)?;
                    *mesh = Some(handle);
                    handle
                };
                device.draw(handle, &fixtures::camera(extent, orbit))
            }
            Self::Startup(device) => startup::draw(device, extent, scale, revision),
        }
    }
}

impl Probe {
    fn open(&mut self, event_loop: &ActiveEventLoop) -> ProbeResult<()> {
        let [w, h] = self.options.extent;
        let window = event_loop.create_window(
            Window::default_attributes()
                .with_title("ApiGraphicBasic")
                .with_inner_size(PhysicalSize::new(w, h)),
        )?;
        self.window = Some(window);
        self.reconnect()?;
        self.scheduler.request(FrameInvalidation::EXPLICIT);
        Ok(())
    }

    fn reconnect(&mut self) -> ProbeResult<()> {
        self.device = None;
        self.configured = None;
        let window = self.window.as_ref().ok_or("missing probe window")?;
        let begin = Instant::now();
        // Probe keeps the window alive until after the device drops.
        let backend = unsafe {
            NativeBackend::window(
                self.api.map_or(self.request, NativeRequest::Exact),
                window.display_handle()?.as_raw(),
                window.window_handle()?.as_raw(),
            )
        }?;
        self.api = Some(backend.api());
        let info = raf_agb::Backend::info(&backend);
        eprintln!(
            "{} | {} | device_ms={:.2}",
            info.backend,
            info.name,
            super::metrics::ms(begin.elapsed())
        );
        let budget = GraphicsMemoryBudget::potato();
        self.device = Some(if self.options.startup {
            Presenter::Startup(ProductDevice::new(backend, budget)?)
        } else {
            Presenter::Fixture(Device::new(backend, budget)?, None)
        });
        Ok(())
    }

    fn redraw(&mut self, permit: FramePermit) -> ProbeResult<()> {
        let window = self.window.as_ref().unwrap();
        let size = window.inner_size();
        let extent = [size.width, size.height];
        if extent.contains(&0) || window.is_minimized() == Some(true) {
            return Ok(());
        }
        let scale = window.scale_factor();
        if self.options.startup && self.startup_layout != Some((extent, scale)) {
            self.startup_layout = Some((extent, scale));
            self.revision += 1;
        }
        let device = self.device.as_mut().ok_or("missing presentation device")?;
        if self.configured != Some(extent) {
            device.configure(extent)?;
            self.configured = Some(extent);
        }
        let start = Instant::now();
        let stats = device.draw(
            self.workload.as_ref(),
            extent,
            scale,
            self.revision,
            self.orbit,
        )?;
        let now = self.clock.elapsed().as_secs_f64();
        let cpu_ms = start.elapsed().as_secs_f32() * 1000.;
        self.scheduler.finish_frame(
            permit,
            now,
            cpu_ms,
            stats.gpu_ms.unwrap_or_default() as f32,
            cpu_ms,
        );
        self.submitted += 1;
        self.consecutive_failures = 0;
        if self.submitted == 1 {
            eprintln!(
                "presented {}x{} | scale={scale:.2} | first_present_ms={:.2}",
                extent[0],
                extent[1],
                now * 1000.
            );
        }
        Ok(())
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: Box<dyn std::error::Error>) {
        self.error = Some(error);
        event_loop.exit();
    }

    fn smoke(&mut self, event_loop: &ActiveEventLoop) {
        if !self.options.smoke {
            return;
        }
        if self.clock.elapsed() > SMOKE_TIMEOUT {
            self.fail(event_loop, "window smoke did not complete".into());
            return;
        }
        let Some(window) = self.window.as_ref() else {
            return;
        };
        match self.smoke_step {
            0 if self.submitted == 1 => {
                let [w, h] = self.options.extent;
                let _ = window.request_inner_size(PhysicalSize::new(
                    w.saturating_sub(72).max(96),
                    h.saturating_sub(64).max(96),
                ));
                self.scheduler.request(FrameInvalidation::EXPLICIT);
                self.smoke_step = 1;
            }
            1 if self.submitted == 2 => {
                window.set_minimized(true);
                self.wake = Some(Instant::now() + Duration::from_millis(250));
                self.smoke_step = 2;
            }
            2 if self.wake.is_some_and(|time| Instant::now() >= time) => {
                window.set_minimized(false);
                self.scheduler.set_suspended(false);
                self.scheduler.request(FrameInvalidation::EXPLICIT);
                self.wake = None;
                self.smoke_step = 3;
            }
            3 if self.submitted >= 3 => {
                if let Err(error) = self.reconnect() {
                    self.fail(event_loop, error);
                    return;
                }
                self.scheduler.request(FrameInvalidation::EXPLICIT);
                self.smoke_step = 4;
            }
            4 if self.submitted >= 4 => {
                self.smoke_step = 5;
                eprintln!("window smoke passed, {} presented frames", self.submitted);
                event_loop.exit();
            }
            _ => {}
        }
    }
}

impl ApplicationHandler for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.scheduler.set_suspended(false);
        if self.window.is_none() {
            if let Err(error) = self.open(event_loop) {
                self.fail(event_loop, error);
            }
        }
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.scheduler.set_suspended(true);
        self.device = None;
        self.window = None;
        self.configured = None;
        self.permit = None;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().is_none_or(|window| window.id() != id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.scheduler
                    .set_suspended(size.width == 0 || size.height == 0);
                self.scheduler.request(FrameInvalidation::EXPLICIT);
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.configured = None;
                self.scheduler.request(FrameInvalidation::EXPLICIT)
            }
            WindowEvent::Focused(focused) => self.scheduler.set_window_focused(focused),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => self.dragging = state == ElementState::Pressed,
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(last) = self
                    .cursor_x
                    .replace(position.x)
                    .filter(|_| self.dragging && !self.options.startup)
                {
                    self.orbit += (position.x - last) as f32 * 0.007;
                    self.scheduler.request(FrameInvalidation::EXPLICIT);
                }
            }
            WindowEvent::RedrawRequested => {
                let permit = self.permit.take().or_else(|| {
                    self.scheduler.request(FrameInvalidation::WINDOW);
                    self.scheduler
                        .request_frame(self.clock.elapsed().as_secs_f64())
                });
                let Some(permit) = permit else {
                    return;
                };
                if let Err(error) = self.redraw(permit) {
                    if matches!(error.downcast_ref::<Error>(), Some(Error::Busy)) {
                        self.retry = Some(Instant::now() + PRESENT_RETRY_DELAY);
                        return;
                    }
                    self.consecutive_failures += 1;
                    if self.consecutive_failures > 4 {
                        self.fail(event_loop, error);
                        return;
                    }
                    match error.downcast_ref::<Error>() {
                        Some(Error::SurfaceOutdated) => {
                            self.configured = None;
                            self.retry = Some(Instant::now() + RECOVERY_DELAY);
                        }
                        Some(Error::SurfaceLost | Error::DeviceLost) => {
                            if let Err(recovery) = self.reconnect() {
                                self.fail(event_loop, recovery);
                                return;
                            }
                            self.retry = Some(Instant::now() + RECOVERY_DELAY);
                        }
                        _ => self.fail(event_loop, error),
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.smoke(event_loop);
        if self.retry.is_some_and(|time| Instant::now() >= time) {
            self.retry = None;
            self.scheduler.request(FrameInvalidation::EXPLICIT);
        }
        let Some(window) = &self.window else {
            return;
        };
        let now = self.clock.elapsed().as_secs_f64();
        if self.permit.is_none() {
            self.permit = self.scheduler.request_frame(now);
            if self.permit.is_some() {
                window.request_redraw();
            }
        }
        let deadline = self
            .scheduler
            .seconds_until_next_frame(now)
            .map(|seconds| Instant::now() + Duration::from_secs_f64(seconds.max(0.001)));
        let timeout = self.options.smoke.then_some(self.clock + SMOKE_TIMEOUT);
        let deadline = deadline
            .into_iter()
            .chain(self.wake)
            .chain(self.retry)
            .chain(timeout)
            .min();
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}
