use crate::{PlayerSession, PlayerSource};
use raf_render::api_graphic_basic::ui_surface::{
    NativeUiInputBridge, NativeUiWindowConfig, NativeUiWindowHost,
};
use raf_render::api_graphic_basic::{GraphicsAdapterPreference, NativeEditorCompositor};
use raf_runtime::manifest::RuntimeHostSettings;
use raf_runtime::wire::{RuntimePacket, RuntimePeer};
use raf_runtime::RuntimeControl;
use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct PlayerLaunch {
    pub manifest: PathBuf,
    pub settings: RuntimeHostSettings,
    pub project_name: String,
    pub instance: Uuid,
    pub control_address: Option<SocketAddr>,
    pub control_token: Uuid,
}
pub fn run(launch: PlayerLaunch) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = PlayerApplication {
        launch,
        window: None,
        host: None,
        compositor: None,
        session: None,
        input: NativeUiInputBridge::default(),
        peer: None,
        next_frame: Instant::now(),
        last_status: Instant::now(),
        last_render: Instant::now(),
        minimized: false,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("runtime event loop: {e}"))
}
struct PlayerApplication {
    launch: PlayerLaunch,
    window: Option<Arc<Window>>,
    host: Option<NativeUiWindowHost>,
    compositor: Option<NativeEditorCompositor>,
    session: Option<PlayerSession>,
    input: NativeUiInputBridge,
    peer: Option<RuntimePeer>,
    next_frame: Instant,
    last_status: Instant,
    last_render: Instant,
    minimized: bool,
}
impl PlayerApplication {
    fn stop(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(session) = &mut self.session {
            session.control(RuntimeControl::Stop);
        }
        if let Some(peer) = &mut self.peer {
            if let Some(session) = &self.session {
                let _ = peer.send(RuntimePacket::Status(session.status().clone()));
            }
            let _ = peer.poll();
        }
        event_loop.exit();
    }
    fn poll_control(&mut self, event_loop: &ActiveEventLoop) {
        let packets = match self.peer.as_mut().map(|peer| peer.poll()) {
            None => return,
            Some(Ok(packets)) => packets,
            Some(Err(error)) => {
                tracing::warn!(%error, "runtime parent disconnected");
                self.peer = None;
                self.stop(event_loop);
                return;
            }
        };
        for packet in packets {
            if let RuntimePacket::Control(control) = packet {
                if let Some(session) = &mut self.session {
                    session.control(control);
                }
                self.next_frame = Instant::now();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }
    }
}
impl ApplicationHandler for PlayerApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Some(address) = self.launch.control_address {
            if !address.ip().is_loopback() {
                event_loop.exit();
                return;
            }
            let result = TcpStream::connect_timeout(&address, Duration::from_millis(250))
                .map_err(|e| e.to_string())
                .and_then(RuntimePeer::new);
            match result {
                Ok(mut peer) => {
                    let _ = peer.send(RuntimePacket::Hello {
                        token: self.launch.control_token,
                        instance: self.launch.instance,
                    });
                    let _ = peer.poll();
                    self.peer = Some(peer);
                }
                Err(error) => {
                    tracing::error!(%error, "runtime control connection failed");
                    event_loop.exit();
                    return;
                }
            }
        }
        let attributes = Window::default_attributes()
            .with_title(format!("{} — Runtime", self.launch.project_name))
            .with_inner_size(LogicalSize::new(620.0, 500.0))
            .with_decorations(false)
            .with_resizable(false);
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                tracing::error!(%error);
                event_loop.exit();
                return;
            }
        };
        if let Some(monitor) = window.current_monitor() {
            let position = monitor.position();
            let size = monitor.size();
            let win = window.outer_size();
            window.set_outer_position(PhysicalPosition::new(
                position.x + (size.width as i32 - win.width as i32) / 2,
                position.y + (size.height as i32 - win.height as i32) / 2,
            ));
        }
        let config = NativeUiWindowConfig {
            adapter_preference: if self.launch.settings.render_execution_policy
                == raf_core::config::RenderExecutionPolicy::GpuPreferred
            {
                GraphicsAdapterPreference::HighPerformance
            } else {
                GraphicsAdapterPreference::LowPower
            },
            ..NativeUiWindowConfig::default()
        };
        let mut host = match pollster::block_on(NativeUiWindowHost::create_with_config(
            window.clone(),
            config,
        )) {
            Ok(host) => host,
            Err(error) => {
                tracing::error!(%error);
                event_loop.exit();
                return;
            }
        };
        host.set_vsync(self.launch.settings.vsync);
        self.compositor = Some(NativeEditorCompositor::new(
            &host.graphics_context(),
            [8, 11, 15, 255],
        ));
        self.session = Some(PlayerSession::new(
            &host,
            PlayerSource::File(self.launch.manifest.clone()),
            self.launch.settings.clone(),
            self.launch.instance,
        ));
        self.input.set_scale_factor(window.scale_factor());
        self.window = Some(window.clone());
        self.host = Some(host);
        window.request_redraw();
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match &event {
            WindowEvent::CloseRequested => {
                self.stop(event_loop);
                return;
            }
            WindowEvent::Resized(size) => {
                self.minimized = size.width == 0 || size.height == 0;
                if !self.minimized {
                    if let Some(host) = &mut self.host {
                        host.resize(size.width, size.height);
                    }
                }
            }
            WindowEvent::Focused(focused) => {
                if let Some(session) = &mut self.session {
                    session.set_focused(*focused);
                }
            }
            WindowEvent::RedrawRequested => {
                self.poll_control(event_loop);
                if !self.minimized {
                    if let (Some(session), Some(host), Some(compositor)) =
                        (&mut self.session, &mut self.host, &mut self.compositor)
                    {
                        if let Err(error) = session.draw(host, compositor, &self.input) {
                            tracing::error!(%error, "runtime draw failed");
                            self.stop(event_loop);
                            return;
                        }
                        if session.entered_game() {
                            let size = session.settings.runtime.window_size;
                            if let Some(window) = &self.window {
                                window.set_decorations(true);
                                window.set_resizable(true);
                                window.set_min_inner_size(Some(LogicalSize::new(640.0, 360.0)));
                                let _ = window.request_inner_size(LogicalSize::new(
                                    size[0] as f64,
                                    size[1] as f64,
                                ));
                            }
                        }
                        for message in session.drain_diagnostics().into_iter().take(16) {
                            tracing::info!(%message, "runtime");
                            if let Some(peer) = &mut self.peer {
                                let _ = peer.send(RuntimePacket::Diagnostic(message));
                            }
                        }
                        if self.last_status.elapsed() >= Duration::from_millis(250) {
                            if let Some(peer) = &mut self.peer {
                                let _ = peer.send(RuntimePacket::Status(session.status().clone()));
                                let _ = peer.poll();
                            }
                            self.last_status = Instant::now();
                        }
                    }
                }
                self.input.begin_frame();
                self.last_render = Instant::now();
                if self.session.as_ref().is_some_and(|s| s.should_close()) {
                    self.stop(event_loop);
                    return;
                }
                self.next_frame = Instant::now()
                    + self
                        .session
                        .as_ref()
                        .map_or(Duration::from_millis(250), |s| s.frame_interval());
                return;
            }
            _ => {}
        }
        if self.input.ingest(&event) {
            let input_deadline = self.last_render
                + Duration::from_secs_f64(
                    1.0 / self.launch.settings.runtime.fps_limit.clamp(15, 120) as f64,
                );
            self.next_frame = self.next_frame.min(input_deadline);
            if Instant::now() >= input_deadline {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.poll_control(event_loop);
        if self.session.as_ref().is_some_and(|s| s.should_close()) {
            self.stop(event_loop);
            return;
        }
        let now = Instant::now();
        if now >= self.next_frame {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            self.next_frame.max(now + Duration::from_millis(1)),
        ));
    }
}
