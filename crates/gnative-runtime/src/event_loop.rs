//! Event loop winit 0.30 (`ApplicationHandler`) + fungsi `run_app`.
//!
//! Binary hasil kompiler `.gn` memanggil intrinsik `gn_window_run` yang
//! menuju ke sini: buat window, pilih backend (wgpu → cuda), jalankan loop.

use std::sync::Arc;

use gnative_gfx::scene::{Node, Quad, Scene, TextItem};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::Window;

use crate::backend_select::select_backend;

/// Scene default saat intrinsik tidak mengirim scene sendiri
/// (window kosong + teks statis dari `gn_window_create`).
fn default_scene(size: gnative_core::Size, title: &str) -> Scene {
    Scene {
        size,
        clear: gnative_core::Color::new(0.13, 0.17, 0.23, 1.0),
        nodes: vec![
            Node::Quad(
                Quad::new(
                    gnative_core::Rect::new(0.0, 0.0, 96.0, 32.0),
                    gnative_core::Color::new(0.2, 0.6, 0.9, 1.0),
                )
                .with_radius(8.0),
            ),
            Node::Text(TextItem {
                rect: gnative_core::Rect::new(8.0, 8.0, 300.0, 24.0),
                text: title.to_string(),
                font_size: 14.0,
                color: gnative_core::Color::new(1.0, 1.0, 1.0, 1.0),
            }),
        ],
    }
}

/// Aplikasi native: window + backend render.
struct App {
    window: Option<Arc<Window>>,
    backend: Option<Box<dyn gnative_gfx::backend::RenderBackend>>,
    title: String,
    size: gnative_core::Size,
    /// Scene dari binary `.gn` (None → `default_scene` placeholder).
    scene: Option<Scene>,
    /// Aksi klik `(id, komponen, method)` dari build terakhir.
    actions: Vec<(usize, String, String)>,
    /// Posisi kursor terakhir (piksel logis).
    cursor: (f32, f32),
    initialized: bool,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.initialized {
            return;
        }
        self.initialized = true;

        let attrs = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.size.width as f64,
                self.size.height as f64,
            ));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("[gnative] gagal membuat window: {e}");
                event_loop.exit();
                return;
            }
        };

        // Pilih backend: wgpu → cuda.
        match select_backend(&window) {
            Ok(backend) => {
                self.backend = Some(backend);
            }
            Err(e) => {
                eprintln!("[gnative] {e}");
                event_loop.exit();
                return;
            }
        }
        self.window = Some(window.clone());
        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(backend) = &mut self.backend {
                    let _ = backend.resize(gnative_core::Size::new(
                        size.width as f32,
                        size.height as f32,
                    ));
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let Some(window) = &self.window else { return };
                let Some(backend) = &mut self.backend else {
                    return;
                };
                let size = window.inner_size();
                // Pakai scene dari binary `.gn` bila ada, else placeholder.
                let built_default;
                let scene = match &self.scene {
                    Some(s) => s,
                    None => {
                        built_default = default_scene(
                            gnative_core::Size::new(size.width as f32, size.height as f32),
                            &self.title,
                        );
                        &built_default
                    }
                };
                if let Err(e) = backend.render_scene(scene) {
                    eprintln!("[gnative] render gagal: {e}");
                }
                window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Simpan posisi logis untuk hit-test klik.
                let scale = self
                    .window
                    .as_ref()
                    .map(|w| w.scale_factor())
                    .unwrap_or(1.0);
                let logical = position.to_logical::<f64>(scale);
                self.cursor = (logical.x as f32, logical.y as f32);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                use winit::event::{ElementState, MouseButton};
                if state == ElementState::Pressed && button == MouseButton::Left {
                    self.handle_click();
                }
            }
            _ => {}
        }
    }
}

impl App {
    /// Klik kiri: hit-test → panggil method → rebuild scene → redraw.
    fn handle_click(&mut self) {
        let (x, y) = self.cursor;
        let action = self
            .scene
            .as_ref()
            .and_then(|s| s.action_at(&self.actions, x, y));
        let Some((comp, method)) = action else {
            return;
        };
        if !crate::intrinsics::call_method(&comp, &method) {
            eprintln!("[gnative] method `{comp}::{method}` tidak terregistrasi");
            return;
        }
        // Ukuran ikut window saat ini agar layout rebuild tepat.
        let size = match &self.window {
            Some(w) => {
                let s = w.inner_size();
                gnative_core::Size::new(s.width as f32, s.height as f32)
            }
            None => return,
        };
        let (scene, actions) = crate::intrinsics::rebuild_scene(size);
        self.scene = Some(scene);
        self.actions = actions;
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Jalankan aplikasi native sampai window ditutup.
///
/// `scene` dihitung layout-nya sekali sebelum event loop. `None` →
/// `default_scene` (placeholder) ketika binary `.gn` belum membangun scene.
pub fn run_app(title: &str, width: u32, height: u32, scene: Option<Scene>) -> Result<(), String> {
    run_app_inner(title, width, height, scene, Vec::new())
}

/// Jalankan aplikasi dinamis: scene + aksi klik dari build pertama.
/// Klik tombol memicu method → rebuild → redraw (event-driven).
pub fn run_app_dynamic(
    title: &str,
    width: u32,
    height: u32,
    scene: Scene,
    actions: Vec<(usize, String, String)>,
) -> Result<(), String> {
    run_app_inner(title, width, height, Some(scene), actions)
}

fn run_app_inner(
    title: &str,
    width: u32,
    height: u32,
    scene: Option<Scene>,
    actions: Vec<(usize, String, String)>,
) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| format!("gagal membuat event loop: {e}"))?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App {
        window: None,
        backend: None,
        title: title.to_string(),
        size: gnative_core::Size::new(width as f32, height as f32),
        scene,
        actions,
        cursor: (0.0, 0.0),
        initialized: false,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop gagal: {e}"))
}
