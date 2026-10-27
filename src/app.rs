use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Color32, Context, Pos2, Rect, Vec2};
use egui_wgpu::{wgpu, Renderer, ScreenDescriptor};
use egui_winit::State as EguiState;
use image::RgbaImage;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{CursorIcon, Window, WindowId, WindowLevel},
};

use crate::{
    annotate::compose::compose,
    annotate::object::{Annotation, Kind, Tool},
    annotate::tool::Draft,
    capture::screen::CapturedFrame,
    config::{AfterCapture, Config, SettingsDraft},
    event::{TrayAction, UserEvent},
    hotkey::{HotkeyAction, HotkeyManager},
    overlay::selection::{BORDER_HIT, HANDLE_HIT},
    overlay::OverlayState,
    pin::{self, PinState},
    tray::Tray,
};

pub struct App {
    proxy: EventLoopProxy<UserEvent>,
    config: Config,
    egui_ctx: Context,
    tray: Option<Tray>,
    hotkey: Option<HotkeyManager>,
    gpu: Gpu,
    /// 所有窗口共享一个 egui Renderer，避免新窗口缺少字体/纹理（egui Context 是共享的）。
    renderer: Option<Arc<Mutex<Renderer>>>,
    renderer_format: Option<wgpu::TextureFormat>,
    windows: HashMap<WindowId, WindowCtx>,
}

struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

struct WindowCtx {
    window: Arc<Window>,
    egui_state: EguiState,
    renderer: Arc<Mutex<Renderer>>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    kind: WindowKind,
    modifiers: ModifiersState,
    /// 上一次鼠标按下的时间/位置，用于识别双击
    last_click: Option<(Instant, Pos2)>,
    /// egui 请求的下一次重绘时刻
    next_repaint: Option<Instant>,
}

enum WindowKind {
    Settings(SettingsState),
    Overlay(OverlayState),
    Pin(PinState),
}

struct SettingsState {
    draft: SettingsDraft,
    proxy: EventLoopProxy<UserEvent>,
    error: Option<String>,
    status: Option<String>,
}

impl App {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("no suitable wgpu adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
                .expect("request_device failed");

        let egui_ctx = Context::default();
        install_cjk_font(&egui_ctx);

        Self {
            proxy,
            config: Config::load(),
            egui_ctx,
            tray: None,
            hotkey: None,
            gpu: Gpu {
                instance,
                adapter,
                device,
                queue,
            },
            renderer: None,
            renderer_format: None,
            windows: HashMap::new(),
        }
    }

    fn build_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        attrs: winit::window::WindowAttributes,
        kind: WindowKind,
        transparent: bool,
    ) -> Option<WindowId> {
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("create_window failed: {e}");
                return None;
            }
        };
        let surface = match self.gpu.instance.create_surface(window.clone()) {
            Ok(s) => s,
            Err(e) => {
                log::error!("create_surface failed: {e}");
                return None;
            }
        };
        let caps = surface.get_capabilities(&self.gpu.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let alpha_mode = if transparent {
            if caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
            {
                wgpu::CompositeAlphaMode::PreMultiplied
            } else if caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
            {
                wgpu::CompositeAlphaMode::PostMultiplied
            } else {
                caps.alpha_modes[0]
            }
        } else {
            caps.alpha_modes[0]
        };

        let size = window.inner_size();
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&self.gpu.device, &surface_config);

        let mut egui_state = EguiState::new(
            self.egui_ctx.clone(),
            egui::ViewportId::from_hash_of(window.id()),
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let max_tex = self.gpu.device.limits().max_texture_dimension_2d as usize;
        egui_state.set_max_texture_side(max_tex);

        let renderer = match &self.renderer {
            Some(existing) => {
                if self.renderer_format != Some(format) {
                    log::error!(
                        "surface format {:?} differs from existing renderer {:?}",
                        format,
                        self.renderer_format
                    );
                    return None;
                }
                existing.clone()
            }
            None => {
                let created = Arc::new(Mutex::new(Renderer::new(
                    &self.gpu.device,
                    format,
                    None,
                    1,
                    false,
                )));
                self.renderer = Some(created.clone());
                self.renderer_format = Some(format);
                created
            }
        };

        let id = window.id();
        self.windows.insert(
            id,
            WindowCtx {
                window: window.clone(),
                egui_state,
                renderer,
                surface,
                surface_config,
                kind,
                modifiers: ModifiersState::empty(),
                last_click: None,
                next_repaint: None,
            },
        );
        window.request_redraw();
        window.focus_window();
        Some(id)
    }

    fn spawn_overlay(&mut self, event_loop: &ActiveEventLoop) {
        if self.has_overlay() {
            return;
        }
        let frames = match crate::capture::screen::capture_all() {
            Ok(f) => f,
            Err(e) => {
                log::error!("capture_all failed: {e}");
                return;
            }
        };
        self.spawn_overlay_frames(event_loop, frames, false);
    }

    /// 截取当前前台窗口，并打开一个只覆盖该窗口的标注层。
    fn spawn_window_capture(&mut self, event_loop: &ActiveEventLoop) {
        if self.has_overlay() {
            return;
        }
        match crate::capture::window::capture_focused() {
            Ok(frame) => self.spawn_overlay_frames(event_loop, vec![frame], true),
            Err(e) => log::error!("window capture failed: {e}"),
        }
    }

    fn spawn_overlay_frames(
        &mut self,
        event_loop: &ActiveEventLoop,
        frames: Vec<CapturedFrame>,
        full_selection: bool,
    ) {
        let prefs = self.config.tool_prefs.clone();
        log::info!("spawning {} overlay windows", frames.len());
        for frame in frames {
            let attrs = Window::default_attributes()
                .with_title("fshot overlay")
                .with_decorations(false)
                .with_transparent(true)
                .with_window_level(WindowLevel::AlwaysOnTop)
                .with_resizable(false)
                .with_position(PhysicalPosition::new(frame.monitor_x, frame.monitor_y))
                .with_inner_size(PhysicalSize::new(frame.width, frame.height));
            let state = if full_selection {
                OverlayState::new_with_full_selection(frame, &prefs)
            } else {
                OverlayState::new(frame, &prefs)
            };
            if let Some(id) = self.build_window(event_loop, attrs, WindowKind::Overlay(state), true)
            {
                if let Some(wc) = self.windows.get(&id) {
                    wc.window.set_cursor(CursorIcon::Crosshair);
                }
            }
        }
    }

    fn has_overlay(&self) -> bool {
        self.windows
            .values()
            .any(|wc| matches!(wc.kind, WindowKind::Overlay(_)))
    }

    fn spawn_settings(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(id) = self
            .windows
            .iter()
            .find(|(_, wc)| matches!(wc.kind, WindowKind::Settings(_)))
            .map(|(id, _)| *id)
        {
            if let Some(wc) = self.windows.get(&id) {
                wc.window.request_redraw();
                wc.window.focus_window();
            }
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("fshot 设置")
            .with_inner_size(LogicalSize::new(560.0, 480.0))
            .with_resizable(true);
        let state = SettingsState {
            draft: SettingsDraft::from_config(&self.config),
            proxy: self.proxy.clone(),
            error: None,
            status: None,
        };
        self.build_window(event_loop, attrs, WindowKind::Settings(state), false);
    }

    /// 把合成后的图像作为置顶贴图窗口显示。
    fn spawn_pin(&mut self, event_loop: &ActiveEventLoop, image: RgbaImage, x: i32, y: i32) {
        let size = PhysicalSize::new(image.width().max(1), image.height().max(1));
        let attrs = Window::default_attributes()
            .with_title("fshot pin")
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_position(PhysicalPosition::new(x, y))
            .with_inner_size(size);
        log::info!("pin {}x{} at ({x}, {y})", image.width(), image.height());
        self.build_window(
            event_loop,
            attrs,
            WindowKind::Pin(PinState::new(image)),
            false,
        );
    }

    fn teardown_overlay(&mut self) {
        let ids: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|(_, wc)| matches!(wc.kind, WindowKind::Overlay(_)))
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if let Some(wc) = self.windows.remove(&id) {
                if let WindowKind::Overlay(state) = wc.kind {
                    self.config.tool_prefs = state.tool.to_prefs();
                }
            }
        }
        let _ = self.config.save();
    }

    /// 保存当前 overlay 的合成结果为 PNG（不关闭覆盖层）
    fn save_overlay_png(&mut self, id: WindowId) {
        let prefs = self.config.tool_prefs.clone();
        let save_dir = self.config.save_dir.clone();
        let Some(wc) = self.windows.get(&id) else {
            return;
        };
        let WindowKind::Overlay(state) = &wc.kind else {
            return;
        };
        let Some(sel) = state.selection.rect else {
            log::info!("Ctrl+S: no selection, skipped");
            return;
        };
        if state.screen_size.x <= 0.0 || state.screen_size.y <= 0.0 {
            return;
        }
        let img = compose(
            &state.frame,
            sel,
            state.screen_size,
            &state.history.items,
            state.tool.mosaic_block,
        );
        match crate::output::save_png(&img, &save_dir) {
            Ok(path) => {
                log::info!(
                    "saved {}x{} (with {} annotations) → {}",
                    img.width(),
                    img.height(),
                    state.history.items.len(),
                    path.display()
                );
                // 记录本次工具偏好（含 tool.active 等）
                let mut p = prefs;
                p.active = state.tool.to_prefs().active;
                p.stroke = state.tool.to_prefs().stroke;
                p.stroke_width = state.tool.to_prefs().stroke_width;
                p.fill = state.tool.to_prefs().fill;
                p.text_size = state.tool.to_prefs().text_size;
                p.mosaic_block = state.tool.to_prefs().mosaic_block;
                self.config.tool_prefs = p;
                let _ = self.config.save();
            }
            Err(e) => log::error!("save png: {e}"),
        }
    }

    fn redraw(&mut self, id: WindowId, event_loop: &ActiveEventLoop) {
        let egui_ctx = self.egui_ctx.clone();
        let max_tex = self.gpu.device.limits().max_texture_dimension_2d as usize;
        let Some(wc) = self.windows.get_mut(&id) else {
            return;
        };

        let mut raw_input = wc.egui_state.take_egui_input(&wc.window);
        raw_input.max_texture_side = Some(max_tex);

        let surf_w = wc.surface_config.width;
        let surf_h = wc.surface_config.height;
        let scale = wc.window.scale_factor() as f32;
        let window = wc.window.clone();
        let kind = &mut wc.kind;

        let mut request_close = false;
        let full_output = egui_ctx.run(raw_input, |c| match kind {
            WindowKind::Settings(state) => {
                draw_settings(c, state, &mut request_close);
            }
            WindowKind::Pin(state) => {
                let resp = pin::draw(c, state, id);
                if resp.drag {
                    let _ = window.drag_window();
                }
                if resp.close {
                    request_close = true;
                }
            }
            WindowKind::Overlay(state) => {
                crate::overlay::draw(c, state, surf_w as f32 / scale, surf_h as f32 / scale);
            }
        });

        // 按 egui 的 repaint_delay 调度下一帧（动画、文本光标、点击反馈等）
        let repaint_after = full_output
            .viewport_output
            .values()
            .map(|v| v.repaint_delay)
            .min()
            .unwrap_or(Duration::MAX);
        if repaint_after.is_zero() {
            wc.next_repaint = None;
            wc.window.request_redraw();
        } else if repaint_after >= Duration::MAX {
            wc.next_repaint = None;
        } else {
            wc.next_repaint = Some(Instant::now() + repaint_after);
        }

        wc.egui_state
            .handle_platform_output(&wc.window, full_output.platform_output);

        let clipped = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

        let frame = match wc.surface.get_current_texture() {
            Ok(f) => f,
            Err(e) => {
                log::warn!("get_current_texture: {e}");
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("egui"),
            });

        let renderer = wc.renderer.clone();
        let mut renderer = renderer.lock().unwrap();

        for (tid, delta) in &full_output.textures_delta.set {
            renderer.update_texture(&self.gpu.device, &self.gpu.queue, *tid, delta);
        }

        let screen = ScreenDescriptor {
            size_in_pixels: [surf_w, surf_h],
            pixels_per_point: full_output.pixels_per_point,
        };
        let extra_cmds = renderer.update_buffers(
            &self.gpu.device,
            &self.gpu.queue,
            &mut encoder,
            &clipped,
            &screen,
        );

        {
            let is_overlay = matches!(kind, WindowKind::Overlay(_));
            let clear = if is_overlay {
                wgpu::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                }
            } else {
                wgpu::Color {
                    r: 0.10,
                    g: 0.10,
                    b: 0.12,
                    a: 1.0,
                }
            };
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            renderer.render(&mut pass, &clipped, &screen);
        }

        self.gpu.queue.submit(
            extra_cmds
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        frame.present();

        for tid in &full_output.textures_delta.free {
            renderer.free_texture(tid);
        }

        if request_close {
            self.windows.remove(&id);
        }

        self.update_repaint_schedule(event_loop);
    }

    /// 汇总各窗口的 `next_repaint`，让事件循环在最早的时刻醒来。
    fn update_repaint_schedule(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let mut earliest: Option<Instant> = None;
        for wc in self.windows.values_mut() {
            if let Some(t) = wc.next_repaint {
                if t <= now {
                    wc.next_repaint = None;
                    wc.window.request_redraw();
                } else {
                    earliest = Some(earliest.map_or(t, |e| e.min(t)));
                }
            }
        }
        match earliest {
            Some(t) => event_loop.set_control_flow(ControlFlow::WaitUntil(t)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }

    fn on_tray(&mut self, action: TrayAction, event_loop: &ActiveEventLoop) {
        match action {
            TrayAction::Capture => self.spawn_overlay(event_loop),
            TrayAction::CaptureWindow => self.spawn_window_capture(event_loop),
            TrayAction::Settings => self.spawn_settings(event_loop),
            TrayAction::Quit => {
                // 退出前也保存一次工具偏好
                for wc in self.windows.values() {
                    if let WindowKind::Overlay(state) = &wc.kind {
                        self.config.tool_prefs = state.tool.to_prefs();
                    }
                }
                let _ = self.config.save();
                event_loop.exit();
            }
        }
    }

    /// 设置窗口点击「保存并应用」后：校验目录、重绑热键、落盘配置。
    fn apply_settings(&mut self, draft: SettingsDraft) {
        let save_dir = draft.save_dir.trim().to_string();
        if save_dir.is_empty() {
            self.set_settings_message(Some("保存目录不能为空".into()), None);
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&save_dir) {
            self.set_settings_message(Some(format!("保存目录不可用：{e}")), None);
            return;
        }

        let capture_hotkey = draft.hotkey.trim().to_string();
        let window_hotkey = draft.window_hotkey.trim().to_string();
        let hotkey_result = match self.hotkey.as_mut() {
            Some(manager) => manager.rebind(&capture_hotkey, &window_hotkey),
            None => match HotkeyManager::new(&capture_hotkey, &window_hotkey, self.proxy.clone()) {
                Ok(manager) => {
                    self.hotkey = Some(manager);
                    Ok(())
                }
                Err(e) => Err(e),
            },
        };
        if let Err(e) = hotkey_result {
            self.set_settings_message(Some(format!("热键应用失败：{e}")), None);
            return;
        }

        self.config.hotkey = capture_hotkey;
        self.config.window_hotkey = window_hotkey;
        self.config.after_capture = draft.after_capture;
        self.config.save_dir = save_dir;
        if let Err(e) = self.config.save() {
            self.set_settings_message(Some(format!("保存配置失败：{e}")), None);
            return;
        }
        self.set_settings_message(None, Some("已保存并应用".into()));
        log::info!(
            "settings applied: hotkey={}, window_hotkey={}, after_capture={:?}, save_dir={}",
            self.config.hotkey,
            self.config.window_hotkey,
            self.config.after_capture,
            self.config.save_dir
        );
    }

    fn set_settings_message(&mut self, error: Option<String>, status: Option<String>) {
        for wc in self.windows.values_mut() {
            if let WindowKind::Settings(state) = &mut wc.kind {
                state.error = error.clone();
                state.status = status.clone();
                wc.window.request_redraw();
            }
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.tray.is_none() {
            match Tray::new(self.proxy.clone()) {
                Ok(t) => self.tray = Some(t),
                Err(e) => log::error!("tray init failed: {e}"),
            }
            match HotkeyManager::new(
                &self.config.hotkey,
                &self.config.window_hotkey,
                self.proxy.clone(),
            ) {
                Ok(h) => self.hotkey = Some(h),
                Err(e) => log::error!("hotkey init failed: {e}"),
            }
            let _ = self.config.save();
            log::info!(
                "fshot ready (hotkey = {}, window_hotkey = {})",
                self.config.hotkey,
                self.config.window_hotkey
            );
        }
        let _ = event_loop;
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.update_repaint_schedule(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let scale = match self.windows.get(&id) {
            Some(wc) => wc.window.scale_factor() as f32,
            None => return,
        };

        if let WindowEvent::ModifiersChanged(m) = &event {
            if let Some(wc) = self.windows.get_mut(&id) {
                wc.modifiers = m.state();
            }
        }

        let in_text_edit = self
            .windows
            .get(&id)
            .and_then(|wc| {
                if let WindowKind::Overlay(s) = &wc.kind {
                    Some(s.tool.text_edit.is_some())
                } else {
                    None
                }
            })
            .unwrap_or(false);

        if let Some(wc) = self.windows.get_mut(&id) {
            let resp = wc.egui_state.on_window_event(&wc.window, &event);
            if resp.repaint {
                wc.window.request_redraw();
            }
        }

        let proxy = self.proxy.clone();
        let mut action: Option<UserEvent> = None;
        let mut save_request = false;
        let popup_open = self.egui_ctx.memory(|m| m.any_popup_open());

        if let Some(wc) = self.windows.get_mut(&id) {
            if matches!(wc.kind, WindowKind::Overlay(_)) {
                let ctrl = wc.modifiers.control_key();

                if let WindowKind::Overlay(state) = &mut wc.kind {
                    match &event {
                        WindowEvent::CursorMoved { position, .. } => {
                            let pos =
                                Pos2::new(position.x as f32 / scale, position.y as f32 / scale);
                            state.hovering = Some(pos);
                            if state.selection.is_dragging() {
                                state.selection.drag_to(pos);
                            } else if let Some((aid, last)) = state.drag_annotation.as_mut() {
                                let delta = pos - *last;
                                if delta != Vec2::ZERO {
                                    // 只有真正拖动后才记录撤销快照，单击选中不产生空撤销步
                                    if !state.drag_dirty {
                                        state.history.snapshot();
                                        state.drag_dirty = true;
                                    }
                                    if let Some(a) = state.history.find_mut(*aid) {
                                        a.translate(delta);
                                    }
                                    *last = pos;
                                }
                            } else if let Some(draft) = &mut state.tool.draft {
                                match draft {
                                    Draft::Shape { current, .. } => *current = pos,
                                    Draft::Pen(points) => points.push(pos),
                                    Draft::Mosaic { current, .. } => *current = pos,
                                }
                            }
                            wc.window.request_redraw();
                        }
                        WindowEvent::MouseInput {
                            state: btn_state,
                            button: MouseButton::Left,
                            ..
                        } => match btn_state {
                            ElementState::Pressed => {
                                if let Some(pos) = state.hovering {
                                    // 工具栏、文本输入框、取色器等弹出层上的点击不参与选区/标注
                                    let on_ui = state.toolbar_rect.contains(pos)
                                        || state.tool.text_edit.is_some()
                                        || popup_open;
                                    if on_ui {
                                        wc.last_click = None;
                                    } else {
                                        let now = Instant::now();
                                        let double_click = wc.last_click.is_some_and(|(t, p)| {
                                            now.duration_since(t) <= Duration::from_millis(450)
                                                && p.distance(pos) <= 8.0
                                        });
                                        wc.last_click = Some((now, pos));

                                        if state.selection.rect.is_none() {
                                            state.selection.start_new(pos);
                                        } else if !state.selection.is_dragging() {
                                            if let Some(handle) =
                                                state.selection.hit_handle(pos, HANDLE_HIT)
                                            {
                                                state.selection.start_resize(handle);
                                            } else if state.selection.hit_border(pos, BORDER_HIT) {
                                                state.selection.start_move(pos);
                                            } else if state.tool.active == Tool::Pointer {
                                                // 双击文本标注 → 二次编辑
                                                if !(double_click && begin_text_edit(state, pos)) {
                                                    let hit = state
                                                        .history
                                                        .items
                                                        .iter()
                                                        .rev()
                                                        .find(|a| a.hit_test(pos))
                                                        .map(|a| a.id);
                                                    state.selected_id = hit;
                                                    if let Some(aid) = hit {
                                                        state.drag_annotation = Some((aid, pos));
                                                        state.drag_dirty = false;
                                                    }
                                                }
                                            } else if state.tool.active == Tool::Text {
                                                state.tool.text_edit =
                                                    Some((0, String::new(), pos));
                                            } else if state.tool.active == Tool::Step {
                                                let n = state.tool.next_step;
                                                state.tool.next_step += 1;
                                                let ann = Annotation {
                                                    id: 0,
                                                    kind: Kind::Step {
                                                        center: pos,
                                                        radius: 14.0,
                                                        number: n,
                                                    },
                                                    style: state.tool.style.clone(),
                                                };
                                                state.history.push(ann);
                                            } else {
                                                start_drawing(state, pos);
                                            }
                                        }
                                    }
                                }
                                wc.window.request_redraw();
                            }
                            ElementState::Released => {
                                if state.selection.is_dragging() {
                                    state.selection.end_drag();
                                } else if state.drag_annotation.is_some() {
                                    state.drag_annotation = None;
                                    state.drag_dirty = false;
                                } else if state.tool.draft.is_some() {
                                    finish_draft(state);
                                }
                                wc.window.request_redraw();
                            }
                        },
                        WindowEvent::KeyboardInput { event: kev, .. } => {
                            if kev.state != ElementState::Pressed || kev.repeat {
                            } else if in_text_edit || state.tool.text_edit.is_some() {
                                match &kev.logical_key {
                                    Key::Named(NamedKey::Escape) => {
                                        state.tool.text_edit = None;
                                    }
                                    Key::Named(NamedKey::Enter) => {
                                        if let Some((aid, buf, pos)) = state.tool.text_edit.take() {
                                            let trimmed = buf.trim().to_string();
                                            if !trimmed.is_empty() {
                                                if aid == 0 {
                                                    let ann = Annotation {
                                                        id: 0,
                                                        kind: Kind::Text {
                                                            pos,
                                                            text: trimmed,
                                                            size: state.tool.text_size,
                                                        },
                                                        style: state.tool.style.clone(),
                                                    };
                                                    state.history.push(ann);
                                                } else {
                                                    state.history.set_text(aid, trimmed);
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                                wc.window.request_redraw();
                            } else if popup_open {
                                // 取色器等弹出层打开时，键盘交给 egui（Esc 只关闭弹出层）
                            } else if ctrl {
                                match &kev.logical_key {
                                    Key::Character(s) if s.eq_ignore_ascii_case("z") => {
                                        state.history.undo();
                                        state.tool.next_step = state.history.max_step_number() + 1;
                                    }
                                    Key::Character(s) if s.eq_ignore_ascii_case("y") => {
                                        state.history.redo();
                                        state.tool.next_step = state.history.max_step_number() + 1;
                                    }
                                    Key::Character(s) if s.eq_ignore_ascii_case("s") => {
                                        save_request = true;
                                    }
                                    _ => {}
                                }
                                wc.window.request_redraw();
                            } else {
                                match &kev.logical_key {
                                    Key::Named(NamedKey::Escape) => {
                                        action = Some(UserEvent::CaptureCancelled);
                                    }
                                    Key::Named(NamedKey::Enter) => {
                                        action = Some(UserEvent::CaptureConfirmed { window: id });
                                    }
                                    Key::Named(NamedKey::Delete)
                                    | Key::Named(NamedKey::Backspace) => {
                                        if let Some(id) = state.selected_id.take() {
                                            state.history.remove(id);
                                            state.tool.next_step =
                                                state.history.max_step_number() + 1;
                                        }
                                    }
                                    Key::Character(s) => match s.as_str() {
                                        "r" | "R" => state.tool.active = Tool::Rect,
                                        "e" | "E" => state.tool.active = Tool::Ellipse,
                                        "a" | "A" => state.tool.active = Tool::Arrow,
                                        "l" | "L" => state.tool.active = Tool::Line,
                                        "p" | "P" => state.tool.active = Tool::Pen,
                                        "t" | "T" => state.tool.active = Tool::Text,
                                        "n" | "N" => state.tool.active = Tool::Step,
                                        "m" | "M" => state.tool.active = Tool::Mosaic,
                                        "v" | "V" => state.tool.active = Tool::Pointer,
                                        _ => {}
                                    },
                                    _ => {}
                                }
                                wc.window.request_redraw();
                            }
                        }
                        WindowEvent::Focused(f) => {
                            if *f {
                                state.focused_once = true;
                            } else {
                                // 失焦时关闭取色器等弹出层，避免弹层状态残留在其他窗口
                                self.egui_ctx.memory_mut(|m| m.close_popup());
                            }
                            // 多屏场景下不再因失焦取消
                        }
                        _ => {}
                    }
                }
            }
        }

        if save_request {
            self.save_overlay_png(id);
        }

        match event {
            WindowEvent::CloseRequested => {
                if matches!(
                    self.windows.get(&id).map(|w| &w.kind),
                    Some(WindowKind::Overlay(_))
                ) {
                    action = Some(UserEvent::CaptureCancelled);
                } else {
                    self.windows.remove(&id);
                }
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    if let Some(wc) = self.windows.get_mut(&id) {
                        wc.surface_config.width = size.width;
                        wc.surface_config.height = size.height;
                        wc.surface.configure(&self.gpu.device, &wc.surface_config);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.redraw(id, event_loop);
            }
            _ => {}
        }

        if let Some(ev) = action {
            let _ = proxy.send_event(ev);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Hotkey(HotkeyAction::Capture) => self.spawn_overlay(event_loop),
            UserEvent::Hotkey(HotkeyAction::WindowCapture) => self.spawn_window_capture(event_loop),
            UserEvent::TrayAction(a) => self.on_tray(a, event_loop),
            UserEvent::ApplySettings(draft) => self.apply_settings(draft),
            UserEvent::CaptureCancelled => self.teardown_overlay(),
            UserEvent::CaptureConfirmed { window } => {
                let overlay_ids: Vec<WindowId> = self
                    .windows
                    .iter()
                    .filter(|(_, wc)| matches!(wc.kind, WindowKind::Overlay(_)))
                    .map(|(id, _)| *id)
                    .collect();

                let mut parts: Vec<(i32, i32, RgbaImage)> = Vec::new();
                let mut selected_screens = 0usize;
                let mut total = 0usize;
                let mut annotation_count = 0usize;
                let mut last_prefs = None;
                for id in overlay_ids {
                    if let Some(wc) = self.windows.remove(&id) {
                        if let WindowKind::Overlay(state) = wc.kind {
                            total += 1;
                            let prefs = state.tool.to_prefs();
                            if id == window || last_prefs.is_none() {
                                last_prefs = Some(prefs);
                            }
                            if let Some(sel) = state.selection.rect {
                                if state.screen_size.x > 0.0 && state.screen_size.y > 0.0 {
                                    selected_screens += 1;
                                    let items = state.history.items.clone();
                                    annotation_count += items.len();
                                    let img = compose(
                                        &state.frame,
                                        sel,
                                        state.screen_size,
                                        &items,
                                        state.tool.mosaic_block,
                                    );
                                    let (ox, oy) =
                                        selection_origin(&state.frame, sel, state.screen_size);
                                    parts.push((ox, oy, img));
                                }
                            }
                        }
                    }
                }

                log::info!(
                    "CaptureConfirmed: {} overlay(s) closed, {} selected",
                    total,
                    selected_screens
                );
                if let Some(p) = last_prefs {
                    self.config.tool_prefs = p;
                    let _ = self.config.save();
                }
                if parts.is_empty() {
                    log::info!("no non-empty selection; nothing captured");
                    return;
                }

                let origin_x = parts.iter().map(|(x, _, _)| *x).min().unwrap_or(0);
                let origin_y = parts.iter().map(|(_, y, _)| *y).min().unwrap_or(0);
                let part_count = parts.len();
                let img = crate::output::merge_images(&parts);
                if part_count > 1 {
                    log::info!(
                        "merged {} selections into {}x{}",
                        part_count,
                        img.width(),
                        img.height()
                    );
                }

                let after = self.config.after_capture;
                let save_dir = self.config.save_dir.clone();
                match after {
                    AfterCapture::Clipboard => copy_image(&img, annotation_count),
                    AfterCapture::Save => match crate::output::save_png(&img, &save_dir) {
                        Ok(path) => log::info!(
                            "saved {}x{} (with {} annotations) → {}",
                            img.width(),
                            img.height(),
                            annotation_count,
                            path.display()
                        ),
                        Err(e) => log::error!("save png: {e}"),
                    },
                    AfterCapture::None => {
                        log::info!("after_capture = none; image dropped");
                    }
                    AfterCapture::Pin => self.spawn_pin(event_loop, img, origin_x, origin_y),
                }
            }
            UserEvent::PinClosed(id) => {
                self.windows.remove(&id);
            }
        }
    }
}

// ---- 辅助函数 ----

fn draw_settings(ctx: &Context, state: &mut SettingsState, request_close: &mut bool) {
    let mut apply = false;
    let mut open_dir = false;

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("fshot 设置");
        ui.add_space(6.0);

        ui.label("截图热键（格式如 Ctrl+Shift+S）");
        ui.add(
            egui::TextEdit::singleline(&mut state.draft.hotkey)
                .desired_width(280.0)
                .hint_text("Ctrl+Shift+S"),
        );
        ui.add_space(4.0);

        ui.label("窗口截图热键（截取当前前台窗口）");
        ui.add(
            egui::TextEdit::singleline(&mut state.draft.window_hotkey)
                .desired_width(280.0)
                .hint_text("Ctrl+Shift+W"),
        );

        ui.add_space(8.0);
        ui.separator();
        ui.label("截图完成后");
        ui.horizontal_wrapped(|ui| {
            for action in [
                AfterCapture::Clipboard,
                AfterCapture::Save,
                AfterCapture::Pin,
                AfterCapture::None,
            ] {
                ui.radio_value(&mut state.draft.after_capture, action, action.label());
            }
        });

        ui.add_space(8.0);
        ui.label("保存目录");
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.draft.save_dir)
                    .desired_width(360.0)
                    .hint_text("PNG 保存位置"),
            );
            if ui.button("打开").clicked() {
                open_dir = true;
            }
        });

        ui.add_space(8.0);
        if let Some(err) = &state.error {
            ui.colored_label(Color32::from_rgb(235, 90, 90), err);
        }
        if let Some(status) = &state.status {
            ui.colored_label(Color32::from_rgb(90, 190, 110), status);
        }

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("保存并应用").clicked() {
                apply = true;
            }
            if ui.button("关闭").clicked() {
                *request_close = true;
            }
        });

        ui.add_space(8.0);
        ui.label(format!("配置文件：{}", Config::path().display()));
        ui.label("提示：热键被其他程序占用时会保留原热键；窗口截图默认 Ctrl+Shift+W。");
    });

    if open_dir {
        let dir = state.draft.save_dir.trim().to_string();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            state.error = Some(format!("创建目录失败：{e}"));
        } else if let Err(e) = std::process::Command::new("explorer").arg(&dir).spawn() {
            state.error = Some(format!("打开目录失败：{e}"));
        }
    }

    if apply {
        state.error = None;
        state.status = Some("正在应用…".into());
        if state
            .proxy
            .send_event(UserEvent::ApplySettings(state.draft.clone()))
            .is_err()
        {
            state.error = Some("应用失败：事件循环已关闭".into());
            state.status = None;
        }
    }
}

/// 选区左上角在虚拟桌面物理坐标中的位置。
fn selection_origin(frame: &CapturedFrame, sel: Rect, screen_size: Vec2) -> (i32, i32) {
    let sx = frame.width as f32 / screen_size.x.max(1.0);
    let sy = frame.height as f32 / screen_size.y.max(1.0);
    (
        frame.monitor_x + (sel.min.x * sx).round() as i32,
        frame.monitor_y + (sel.min.y * sy).round() as i32,
    )
}

fn copy_image(img: &RgbaImage, annotations: usize) {
    match crate::output::copy_to_clipboard(img) {
        Ok(()) => log::info!(
            "copied {}x{} (with {} annotations) to clipboard",
            img.width(),
            img.height(),
            annotations
        ),
        Err(e) => log::error!("clipboard: {e}"),
    }
}

fn install_cjk_font(ctx: &Context) {
    let candidates: &[&str] = &[
        "C:/Windows/Fonts/msyh.ttc",
        "C:/Windows/Fonts/seguisym.ttf",
        "C:/Windows/Fonts/simhei.ttf",
        "C:/Windows/Fonts/Deng.ttf",
        "C:/Windows/Fonts/simsun.ttc",
    ];
    let mut fonts = egui::FontDefinitions::default();
    let mut loaded = 0u32;
    for (i, path) in candidates.iter().enumerate() {
        if let Ok(bytes) = std::fs::read(path) {
            let name = format!("fb_{i}");
            let mut data = egui::FontData::from_owned(bytes);
            data.index = 0;
            fonts.font_data.insert(name.clone(), data);
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push(name.clone());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push(name);
            log::info!("Loaded fallback font: {path}");
            loaded += 1;
        }
    }
    if loaded > 0 {
        ctx.set_fonts(fonts);
    } else {
        log::warn!("No fallback fonts found; Chinese/symbols may render as boxes");
    }
}

/// 双击文本标注进入二次编辑；返回 false 表示该位置没有文本标注。
fn begin_text_edit(state: &mut OverlayState, pos: Pos2) -> bool {
    let Some(a) = state.history.items.iter().rev().find(|a| a.hit_test(pos)) else {
        return false;
    };
    let Kind::Text {
        pos: text_pos,
        text,
        ..
    } = &a.kind
    else {
        return false;
    };
    let (id, text_pos, text) = (a.id, *text_pos, text.clone());
    state.selected_id = Some(id);
    state.tool.text_edit = Some((id, text, text_pos));
    true
}

fn start_drawing(state: &mut OverlayState, pos: Pos2) {
    match state.tool.active {
        Tool::Pointer | Tool::Text | Tool::Step => {}
        Tool::Rect | Tool::Ellipse | Tool::Arrow | Tool::Line => {
            state.tool.draft = Some(Draft::Shape {
                start: pos,
                current: pos,
            });
        }
        Tool::Pen => {
            state.tool.draft = Some(Draft::Pen(vec![pos]));
        }
        Tool::Mosaic => {
            state.tool.draft = Some(Draft::Mosaic {
                start: pos,
                current: pos,
            });
        }
    }
}

fn finish_draft(state: &mut OverlayState) {
    let Some(draft) = state.tool.draft.take() else {
        return;
    };
    let style = state.tool.style.clone();
    let fill = if state.tool.fill {
        Some(Color32::from_rgba_unmultiplied(
            style.stroke.r(),
            style.stroke.g(),
            style.stroke.b(),
            80,
        ))
    } else {
        None
    };

    let kind = match (state.tool.active, draft) {
        (Tool::Rect, Draft::Shape { start, current }) => Some(Kind::Rect {
            rect: Rect::from_two_pos(start, current),
            fill,
        }),
        (Tool::Ellipse, Draft::Shape { start, current }) => Some(Kind::Ellipse {
            rect: Rect::from_two_pos(start, current),
            fill,
        }),
        (Tool::Arrow, Draft::Shape { start, current }) => Some(Kind::Arrow {
            from: start,
            to: current,
        }),
        (Tool::Line, Draft::Shape { start, current }) => Some(Kind::Line {
            from: start,
            to: current,
        }),
        (Tool::Pen, Draft::Pen(points)) => {
            if points.len() >= 2 {
                Some(Kind::Pen { points })
            } else {
                None
            }
        }
        (Tool::Mosaic, Draft::Mosaic { start, current }) => Some(Kind::Mosaic {
            rect: Rect::from_two_pos(start, current),
        }),
        _ => None,
    };

    if let Some(k) = kind {
        let too_small = match &k {
            Kind::Rect { rect, .. } | Kind::Ellipse { rect, .. } | Kind::Mosaic { rect } => {
                rect.width() < 3.0 || rect.height() < 3.0
            }
            Kind::Arrow { from, to } | Kind::Line { from, to } => from.distance(*to) < 3.0,
            Kind::Pen { points } => points.len() < 2,
            _ => false,
        };
        if !too_small {
            state.history.push(Annotation {
                id: 0,
                kind: k,
                style,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_origin_uses_dpi_scale_and_monitor_offset() {
        let frame = CapturedFrame {
            width: 3840,
            height: 2160,
            pixels: RgbaImage::new(1, 1),
            monitor_x: -1920,
            monitor_y: 0,
            scale: 2.0,
        };
        let sel = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(10.0, 10.0));
        let origin = selection_origin(&frame, sel, Vec2::new(1920.0, 1080.0));
        assert_eq!(origin, (-1720, 100));
    }
}
