mod capture;
mod clipboard;
mod color;
mod config;
mod lock;
mod notification;
mod render;
mod shm;
mod window;

use config::Config;
use render::Rect;
use shm::ShmBuffer;

use wayland_client::{
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_registry,
        wl_seat, wl_shm, wl_shm_pool, wl_surface,
    },
    Connection, Dispatch, QueueHandle, WEnum,
};
use wayland_cursor::CursorTheme;
use wayland_protocols_wlr::{
    layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1},
    screencopy::v1::client::{zwlr_screencopy_frame_v1, zwlr_screencopy_manager_v1},
};

pub struct OutputData {
    pub output: wl_output::WlOutput,
    pub name: String,
    pub scale: i32,
    pub screencopy_frame: Option<zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1>,
    pub captured_frame: Option<CapturedFrameData>,
    pub surface: Option<wl_surface::WlSurface>,
    pub layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    pub overlay_buffers: Option<[ShmBuffer; 2]>,
    pub active_buffer_idx: usize,
    pub configured: bool,
    pub windows: Vec<window::WindowInfo>,
}

pub struct CapturedFrameData {
    pub shm_buffer: ShmBuffer,
    pub dimmed_pixels: Vec<u8>,
    pub format: wl_shm::Format,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub y_invert: bool,
    pub ready: bool,
    pub failed: bool,
}

pub struct AppState {
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub seat: Option<wl_seat::WlSeat>,
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    pub screencopy_manager: Option<zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1>,
    pub outputs: Vec<OutputData>,

    pub config: Config,

    pub pointer: Option<wl_pointer::WlPointer>,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,
    pub cursor_surface: Option<wl_surface::WlSurface>,
    #[allow(dead_code)]
    pub cursor_buffer: Option<ShmBuffer>,
    pub cursor_hotspot: (u32, u32),

    pub active_output_idx: Option<usize>,
    pub pointer_pos: (f64, f64),
    pub drag_start: Option<(f64, f64)>,
    pub is_dragging: bool,
    pub selection: Option<Rect>,
    pub selected_window: Option<window::WindowInfo>,

    pub exit: bool,
    pub should_save: bool,
    pub clear_frame_done: bool,
}

impl AppState {
    pub fn redraw_output(&mut self, output_idx: usize) {
        let output = &mut self.outputs[output_idx];
        let (Some(buffers), Some(surface), Some(cap)) = (
            &mut output.overlay_buffers,
            &output.surface,
            &output.captured_frame,
        ) else {
            return;
        };

        // Switch double-buffer
        output.active_buffer_idx = 1 - output.active_buffer_idx;
        let current_buf = &mut buffers[output.active_buffer_idx];

        let selection_rect = if self.active_output_idx == Some(output_idx) {
            self.selection
        } else {
            None
        };

        render::render_overlay(
            current_buf.as_slice_mut(),
            cap.shm_buffer.as_slice(),
            &cap.dimmed_pixels,
            cap.width,
            cap.height,
            cap.stride,
            cap.format,
            selection_rect,
            self.is_dragging,
            self.config.border_color(),
            self.config.border_width(),
        );

        surface.attach(Some(&current_buf.buffer), 0, 0);
        surface.damage_buffer(0, 0, cap.width as i32, cap.height as i32);
        surface.commit();
    }
}

// ---------------------------------------------------------------------------
// Wayland Protocol Dispatches
// ---------------------------------------------------------------------------

impl Dispatch<wl_registry::WlRegistry, ()> for AppState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), qh, ()));
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind(name, version.min(1), qh, ()));
                }
                "wl_seat" => {
                    state.seat = Some(registry.bind(name, version.min(7), qh, ()));
                }
                "wl_output" => {
                    let output_idx = state.outputs.len();
                    let output = registry.bind(name, version.min(4), qh, output_idx);
                    state.outputs.push(OutputData {
                        output,
                        name: format!("Output-{}", output_idx),
                        scale: 1,
                        screencopy_frame: None,
                        captured_frame: None,
                        surface: None,
                        layer_surface: None,
                        overlay_buffers: None,
                        active_buffer_idx: 0,
                        configured: false,
                        windows: Vec::new(),
                    });
                }
                "zwlr_screencopy_manager_v1" => {
                    state.screencopy_manager = Some(registry.bind(name, version.min(3), qh, ()));
                }
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, usize> for AppState {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        &output_idx: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(output) = state.outputs.get_mut(output_idx) {
            match event {
                wl_output::Event::Scale { factor } => {
                    output.scale = factor;
                }
                wl_output::Event::Name { name } => {
                    output.name = name;
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for AppState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event
            && let WEnum::Value(caps) = capabilities {
                if caps.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                    state.pointer = Some(seat.get_pointer(qh, ()));
                }
                if caps.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
                    state.keyboard = Some(seat.get_keyboard(qh, ()));
                }
            }
    }
}

impl Dispatch<zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1, usize> for AppState {
    fn event(
        state: &mut Self,
        frame: &zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        &output_idx: &usize,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let Some(output) = state.outputs.get_mut(output_idx) else {
            return;
        };

        match event {
            zwlr_screencopy_frame_v1::Event::Buffer { format, width, height, stride } => {
                let fmt = match format {
                    WEnum::Value(f) => f,
                    _ => wl_shm::Format::Xrgb8888,
                };
                let shm = state.shm.as_ref().expect("wl_shm is missing");
                let buf = ShmBuffer::new(shm, width, height, stride, fmt, qh)
                    .expect("Failed to create SHM buffer for screencopy");

                frame.copy(&buf.buffer);

                output.captured_frame = Some(CapturedFrameData {
                    shm_buffer: buf,
                    dimmed_pixels: Vec::new(),
                    format: fmt,
                    width,
                    height,
                    stride,
                    y_invert: false,
                    ready: false,
                    failed: false,
                });
            }
            zwlr_screencopy_frame_v1::Event::Flags { flags } => {
                if let Some(ref mut cap) = output.captured_frame
                    && let WEnum::Value(f) = flags
                    && f.contains(zwlr_screencopy_frame_v1::Flags::YInvert) {
                        cap.y_invert = true;
                    }
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => {
                if let Some(ref mut cap) = output.captured_frame {
                    cap.ready = true;
                }
            }
            zwlr_screencopy_frame_v1::Event::Failed => {
                if let Some(ref mut cap) = output.captured_frame {
                    cap.failed = true;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, usize> for AppState {
    fn event(
        state: &mut Self,
        layer_surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        &output_idx: &usize,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, .. } => {
                layer_surface.ack_configure(serial);

                let shm = state.shm.clone().expect("wl_shm missing");
                let output = &mut state.outputs[output_idx];
                output.configured = true;

                if output.overlay_buffers.is_none() {
                    let cap = output.captured_frame.as_ref().expect("captured frame missing");
                    let overlay_fmt = match cap.format {
                        wl_shm::Format::Xbgr8888 | wl_shm::Format::Abgr8888 => {
                            wl_shm::Format::Abgr8888
                        }
                        _ => wl_shm::Format::Argb8888,
                    };
                    let buf0 = ShmBuffer::new(&shm, cap.width, cap.height, cap.stride, overlay_fmt, qh)
                        .expect("Failed to create overlay buffer 0");
                    let buf1 = ShmBuffer::new(&shm, cap.width, cap.height, cap.stride, overlay_fmt, qh)
                        .expect("Failed to create overlay buffer 1");
                    output.overlay_buffers = Some([buf0, buf1]);
                    output.active_buffer_idx = 0;
                }

                // Initial state: border automatically appears around the entire screen
                if state.selection.is_none() {
                    let cap = output.captured_frame.as_ref().unwrap();
                    state.selection = Some(Rect {
                        x: 0,
                        y: 0,
                        width: cap.width,
                        height: cap.height,
                    });
                }

                state.redraw_output(output_idx);
            }
            zwlr_layer_surface_v1::Event::Closed => {
                state.exit = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for AppState {
    fn event(
        state: &mut Self,
        pointer: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if state.exit && !state.should_save {
            return;
        }

        match event {
            wl_pointer::Event::Enter { serial, surface, surface_x, surface_y } => {
                for (idx, out) in state.outputs.iter().enumerate() {
                    if let Some(s) = &out.surface
                        && s == &surface {
                            state.active_output_idx = Some(idx);
                            break;
                        }
                }

                state.pointer_pos = (surface_x, surface_y);

                if let Some(cursor_surface) = &state.cursor_surface {
                    pointer.set_cursor(
                        serial,
                        Some(cursor_surface),
                        state.cursor_hotspot.0 as i32,
                        state.cursor_hotspot.1 as i32,
                    );
                }

                // Initial hover check
                if !state.is_dragging
                    && let Some(idx) = state.active_output_idx {
                        let out = &state.outputs[idx];
                        let scale = out.scale.max(1) as f64;
                        let px = (surface_x * scale).round() as u32;
                        let py = (surface_y * scale).round() as u32;

                        let hovered_win = out.windows.iter().rev().find(|w| w.rect.contains(px, py)).cloned();
                        let target = hovered_win.as_ref().map(|w| w.rect).or_else(|| {
                            out.captured_frame.as_ref().map(|cap| Rect {
                                x: 0,
                                y: 0,
                                width: cap.width,
                                height: cap.height,
                            })
                        });

                        state.selected_window = hovered_win;
                        if state.selection != target {
                            state.selection = target;
                            state.redraw_output(idx);
                        }
                    }
            }
            wl_pointer::Event::Leave { .. } => {}
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                state.pointer_pos = (surface_x, surface_y);

                if let Some(idx) = state.active_output_idx {
                    let out = &state.outputs[idx];
                    let scale = out.scale.max(1) as f64;

                    if let Some(start) = state.drag_start {
                        let dist = ((surface_x - start.0) * scale).hypot((surface_y - start.1) * scale);
                        if dist >= 5.0 {
                            state.is_dragging = true;
                        }
                    }

                    if state.is_dragging {
                        if let Some(start) = state.drag_start {
                            let px0 = (start.0 * scale).round() as u32;
                            let py0 = (start.1 * scale).round() as u32;
                            let px1 = (surface_x * scale).round() as u32;
                            let py1 = (surface_y * scale).round() as u32;

                            state.selection = Some(Rect::from_points(px0, py0, px1, py1));
                            state.selected_window = None;
                            state.redraw_output(idx);
                        }
                    } else {
                        let px = (surface_x * scale).round() as u32;
                        let py = (surface_y * scale).round() as u32;

                        let hovered_win = out.windows.iter().rev().find(|w| w.rect.contains(px, py)).cloned();
                        let target = hovered_win.as_ref().map(|w| w.rect).or_else(|| {
                            out.captured_frame.as_ref().map(|cap| Rect {
                                x: 0,
                                y: 0,
                                width: cap.width,
                                height: cap.height,
                            })
                        });

                        state.selected_window = hovered_win;
                        if state.selection != target {
                            state.selection = target;
                            state.redraw_output(idx);
                        }
                    }
                }
            }
            wl_pointer::Event::Button { button, state: btn_state, .. } => {
                const BTN_LEFT: u32 = 0x110;
                if button == BTN_LEFT {
                    match btn_state {
                        WEnum::Value(wl_pointer::ButtonState::Pressed) => {
                            state.drag_start = Some(state.pointer_pos);
                            state.is_dragging = false;
                        }
                        WEnum::Value(wl_pointer::ButtonState::Released) => {
                            if let (Some(start), Some(idx)) = (state.drag_start, state.active_output_idx) {
                                let out = &state.outputs[idx];
                                let scale = out.scale.max(1) as f64;
                                let dist = ((state.pointer_pos.0 - start.0) * scale)
                                    .hypot((state.pointer_pos.1 - start.1) * scale);

                                if state.is_dragging && dist >= 5.0 {
                                    // Manual zone selection
                                    let px0 = (start.0 * scale).round() as u32;
                                    let py0 = (start.1 * scale).round() as u32;
                                    let px1 = (state.pointer_pos.0 * scale).round() as u32;
                                    let py1 = (state.pointer_pos.1 * scale).round() as u32;

                                    state.selection = Some(Rect::from_points(px0, py0, px1, py1));
                                    state.selected_window = None;
                                    state.should_save = true;
                                    state.exit = true;
                                } else {
                                    // Single click without dragging
                                    let px = (state.pointer_pos.0 * scale).round() as u32;
                                    let py = (state.pointer_pos.1 * scale).round() as u32;

                                    let clicked_win = out.windows.iter().rev().find(|w| w.rect.contains(px, py)).cloned();
                                    if let Some(w) = clicked_win {
                                        // Clicked on a window
                                        state.selection = Some(w.rect);
                                        state.selected_window = Some(w);
                                    } else {
                                        // Clicked outside any window -> screenshot of the entire screen
                                        state.selection = out.captured_frame.as_ref().map(|cap| Rect {
                                            x: 0,
                                            y: 0,
                                            width: cap.width,
                                            height: cap.height,
                                        });
                                        state.selected_window = None;
                                    }

                                    state.should_save = true;
                                    state.exit = true;
                                }
                            }

                            state.is_dragging = false;
                            state.drag_start = None;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for AppState {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key { key, state: key_state, .. } = event
            && let WEnum::Value(wl_keyboard::KeyState::Pressed) = key_state {
                const KEY_ESC: u32 = 1;
                const KEY_ENTER: u32 = 28;
                const KEY_KPENTER: u32 = 96;

                if key == KEY_ESC {
                    state.should_save = false;
                    state.exit = true;
                } else if key == KEY_ENTER || key == KEY_KPENTER {
                    state.should_save = true;
                    state.exit = true;
                }
            }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for AppState {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.clear_frame_done = true;
        }
    }
}

// Dummy dispatches for interfaces with no custom events
impl Dispatch<wl_shm::WlShm, ()> for AppState {
    fn event(
        _state: &mut Self,
        _shm: &wl_shm::WlShm,
        _event: wl_shm::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<wl_buffer::WlBuffer, ()> for AppState {
    fn event(
        _state: &mut Self,
        _buffer: &wl_buffer::WlBuffer,
        _event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<wl_surface::WlSurface, ()> for AppState {
    fn event(
        _state: &mut Self,
        _surface: &wl_surface::WlSurface,
        _event: wl_surface::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

wayland_client::delegate_noop!(AppState: wl_compositor::WlCompositor);
wayland_client::delegate_noop!(AppState: wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(AppState: zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1);
wayland_client::delegate_noop!(AppState: zwlr_layer_shell_v1::ZwlrLayerShellV1);

// ---------------------------------------------------------------------------
// Crosshair helper
// ---------------------------------------------------------------------------

fn create_crosshair_cursor(
    conn: &Connection,
    shm: &wl_shm::WlShm,
    compositor: &wl_compositor::WlCompositor,
    qh: &QueueHandle<AppState>,
) -> (wl_surface::WlSurface, Option<ShmBuffer>, (u32, u32)) {
    let surface = compositor.create_surface(qh, ());

    // 1. Try loading from system cursor theme
    if let Ok(mut theme) = CursorTheme::load(conn, shm.clone(), 24) {
        let cursor = if theme.get_cursor("crosshair").is_some() {
            theme.get_cursor("crosshair")
        } else {
            theme.get_cursor("default")
        };

        if let Some(cursor) = cursor {
            let img = &cursor[0];
            surface.attach(Some(img), 0, 0);
            surface.commit();
            return (surface, None, img.hotspot());
        }
    }

    // 2. Fallback: Draw custom 25x25 crosshair buffer via memmap2
    let size = 25u32;
    let stride = size * 4;
    let mut buf = ShmBuffer::new(shm, size, size, stride, wl_shm::Format::Argb8888, qh)
        .expect("Failed to create crosshair buffer");

    let slice = buf.as_slice_mut();
    slice.fill(0); // Transparent

    let center = 12i32;
    for y in 0..size as i32 {
        for x in 0..size as i32 {
            let is_cross = (x == center && (2..=22).contains(&y)) || (y == center && (2..=22).contains(&x));
            let is_outline = (x.abs_diff(center) == 1 && (1..=23).contains(&y))
                || (y.abs_diff(center) == 1 && (1..=23).contains(&x))
                || (x == center && (y == 1 || y == 23))
                || (y == center && (x == 1 || x == 23));

            let offset = (y as usize * size as usize + x as usize) * 4;
            if is_cross {
                slice[offset..offset + 4].copy_from_slice(&[255, 255, 255, 255]); // White
            } else if is_outline {
                slice[offset..offset + 4].copy_from_slice(&[0, 0, 0, 255]); // Black outline
            }
        }
    }

    surface.attach(Some(&buf.buffer), 0, 0);
    surface.commit();
    (surface, Some(buf), (center as u32, center as u32))
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--serve-clipboard" {
        let path = std::path::PathBuf::from(&args[2]);
        clipboard::serve_clipboard_loop(&path);
        return Ok(());
    }

    let _instance_lock = match lock::SingleInstanceLock::acquire() {
        Some(l) => l,
        None => {
            // Another instance of ssnip is already active; exit silently
            return Ok(());
        }
    };

    let (config, _) = Config::load()?;

    let conn = Connection::connect_to_env()?;
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();
    let display = conn.display();
    let _registry = display.get_registry(&qh, ());

    let mut state = AppState {
        compositor: None,
        shm: None,
        seat: None,
        layer_shell: None,
        screencopy_manager: None,
        outputs: Vec::new(),
        config,
        pointer: None,
        keyboard: None,
        cursor_surface: None,
        cursor_buffer: None,
        cursor_hotspot: (0, 0),
        active_output_idx: None,
        pointer_pos: (0.0, 0.0),
        drag_start: None,
        is_dragging: false,
        selection: None,
        selected_window: None,
        exit: false,
        should_save: false,
        clear_frame_done: false,
    };

    // 1. Initial roundtrip to discover globals
    event_queue.roundtrip(&mut state)?;

    let screencopy_mgr = state
        .screencopy_manager
        .clone()
        .ok_or("zwlr_screencopy_manager_v1 is not supported by compositor")?;
    let layer_shell = state
        .layer_shell
        .clone()
        .ok_or("zwlr_layer_shell_v1 is not supported by compositor")?;
    let compositor = state
        .compositor
        .clone()
        .ok_or("wl_compositor is not supported")?;
    let shm = state.shm.clone().ok_or("wl_shm is not supported")?;

    if state.outputs.is_empty() {
        return Err("No active outputs (wl_output) found".into());
    }

    // 2. Screencopy: capture all outputs before creating overlays
    let overlay_cursor = if state.config.capture_cursor() { 1 } else { 0 };
    for idx in 0..state.outputs.len() {
        let frame = screencopy_mgr.capture_output(overlay_cursor, &state.outputs[idx].output, &qh, idx);
        state.outputs[idx].screencopy_frame = Some(frame);
    }

    // Wait until screencopy captures are ready
    loop {
        let all_done = state.outputs.iter().all(|out| {
            out.captured_frame
                .as_ref()
                .is_some_and(|cap| cap.ready || cap.failed)
        });
        if all_done {
            break;
        }
        event_queue.blocking_dispatch(&mut state)?;
    }

    // 3. Check captures, precompute dimmed images, load windows
    for out in &mut state.outputs {
        let cap = out
            .captured_frame
            .as_mut()
            .ok_or("Failed to capture screen frame")?;
        if cap.failed {
            return Err("Screencopy frame capture failed".into());
        }
        cap.dimmed_pixels = render::precompute_dimmed(
            cap.shm_buffer.as_slice(),
            state.config.effective_dim_color(),
            cap.format,
        );
        out.windows = window::get_windows_for_monitor(&out.name, out.scale);
    }

    // 4. Setup cursor
    let (cursor_surface, cursor_buf, hotspot) =
        create_crosshair_cursor(&conn, &shm, &compositor, &qh);
    state.cursor_surface = Some(cursor_surface);
    state.cursor_buffer = cursor_buf;
    state.cursor_hotspot = hotspot;

    // 5. Initialize default selection to fullscreen
    if let Some(out) = state.outputs.first()
        && let Some(cap) = &out.captured_frame {
            state.selection = Some(Rect {
                x: 0,
                y: 0,
                width: cap.width,
                height: cap.height,
            });
        }

    // 6. Create layer surfaces for selection overlay
    for idx in 0..state.outputs.len() {
        let surface = compositor.create_surface(&qh, ());
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            Some(&state.outputs[idx].output),
            zwlr_layer_shell_v1::Layer::Overlay,
            "ssnip".to_string(),
            &qh,
            idx,
        );

        layer_surface.set_anchor(
            zwlr_layer_surface_v1::Anchor::Top
                | zwlr_layer_surface_v1::Anchor::Bottom
                | zwlr_layer_surface_v1::Anchor::Left
                | zwlr_layer_surface_v1::Anchor::Right,
        );
        layer_surface.set_exclusive_zone(-1);
        layer_surface
            .set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive);
        layer_surface.set_size(0, 0);

        surface.set_buffer_scale(state.outputs[idx].scale.max(1));
        surface.commit();

        state.outputs[idx].surface = Some(surface);
        state.outputs[idx].layer_surface = Some(layer_surface);
    }

    // Set default active output
    state.active_output_idx = Some(0);

    // 7. Main event loop: handle user interaction
    while !state.exit {
        event_queue.blocking_dispatch(&mut state)?;
    }

    let active_idx = state.active_output_idx.unwrap_or(0);
    let is_full_screen = state.selected_window.is_none()
        && state
            .outputs
            .get(active_idx)
            .and_then(|o| o.captured_frame.as_ref())
            .is_some_and(|cap| {
                state.selection.is_none_or(|r| {
                    r.x == 0 && r.y == 0 && r.width >= cap.width && r.height >= cap.height
                })
            });

    // When taking a live capture (window or dragged region), replace the overlay with a
    // 100% transparent frame and wait for one compositor frame callback before destroying
    // the layer surface. This prevents compositor fade-out animations (e.g. Hyprland layersOut)
    // from bleeding the dim overlay or selection border into the live screenshot.
    if state.should_save && !is_full_screen {
        let mut waiting_for_frame = false;
        for (idx, out) in state.outputs.iter_mut().enumerate() {
            if !out.configured {
                continue;
            }
            if let (Some(buffers), Some(surface), Some(cap)) = (
                &mut out.overlay_buffers,
                &out.surface,
                &out.captured_frame,
            ) {
                out.active_buffer_idx = 1 - out.active_buffer_idx;
                let clear_buf = &mut buffers[out.active_buffer_idx];
                clear_buf.as_slice_mut().fill(0);
                if idx == active_idx {
                    state.clear_frame_done = false;
                    let _cb = surface.frame(&qh, ());
                    waiting_for_frame = true;
                }
                surface.attach(Some(&clear_buf.buffer), 0, 0);
                surface.damage_buffer(0, 0, cap.width as i32, cap.height as i32);
                surface.commit();
            }
        }
        if waiting_for_frame {
            while !state.clear_frame_done {
                if event_queue.blocking_dispatch(&mut state).is_err() {
                    break;
                }
            }
        }
    }

    // Immediately destroy all overlay surfaces so the screen unfreezes instantly
    for out in &mut state.outputs {
        if let Some(layer_surface) = out.layer_surface.take() {
            layer_surface.destroy();
        }
        if let Some(surface) = out.surface.take() {
            surface.destroy();
        }
    }
    // Wait for compositor to process overlay destruction before performing any live capture
    let _ = event_queue.roundtrip(&mut state);

    // 8. Save or cancel
    if state.should_save {
        let output = &state.outputs[active_idx];
        let cap = output.captured_frame.as_ref().unwrap();
        let target_path = state.config.resolve_save_path()?;
        let format_options = state.config.format_options();

        let mut live_image: Option<image::RgbaImage> = None;

        if !is_full_screen {
            if let Some(ref win) = state.selected_window {
                live_image = window::capture_window_live(
                    win,
                    &output.name,
                    output.scale,
                    state.config.capture_cursor(),
                    &output.windows,
                    cap,
                );
            } else if let Some(rect) = state.selection {
                live_image = capture::capture_region_live(
                    &output.name,
                    rect,
                    output.scale,
                    state.config.capture_cursor(),
                );
            }
        }

        let png_bytes = if let Some(ref img) = live_image {
            render::save_image(img, &format_options, &target_path)?
        } else {
            render::save_screenshot(
                cap.shm_buffer.as_slice(),
                cap.width,
                cap.height,
                cap.stride,
                cap.format,
                cap.y_invert,
                state.selection,
                None,
                &format_options,
                &target_path,
            )?
        };

        if state.config.copy_to_clipboard() {
            clipboard::copy_png_to_clipboard(&png_bytes, &target_path);
        }

        notification::send_screenshot_notification(&target_path);
    }

    Ok(())
}
