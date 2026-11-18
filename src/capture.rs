use crate::render::Rect;
use crate::shm::ShmBuffer;
use wayland_client::{
    protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool},
    Connection, Dispatch, QueueHandle, WEnum,
};
use wayland_protocols::ext::{
    foreign_toplevel_list::v1::client::{
        ext_foreign_toplevel_handle_v1, ext_foreign_toplevel_list_v1,
    },
    image_capture_source::v1::client::{
        ext_foreign_toplevel_image_capture_source_manager_v1, ext_image_capture_source_v1,
    },
    image_copy_capture::v1::client::{
        ext_image_copy_capture_frame_v1, ext_image_copy_capture_manager_v1,
        ext_image_copy_capture_session_v1,
    },
};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1, zwlr_screencopy_manager_v1,
};

// ---------------------------------------------------------------------------
// Pixel conversion & transform helpers
// ---------------------------------------------------------------------------

#[inline(always)]
fn unpremultiply(r: u8, g: u8, b: u8, a: u8) -> [u8; 4] {
    if a == 0 {
        [0, 0, 0, 0]
    } else if a == 255 {
        [r, g, b, 255]
    } else {
        let a32 = a as u32;
        let half = a32 / 2;
        [
            ((r as u32 * 255 + half) / a32).min(255) as u8,
            ((g as u32 * 255 + half) / a32).min(255) as u8,
            ((b as u32 * 255 + half) / a32).min(255) as u8,
            a,
        ]
    }
}

/// Converts raw Wayland SHM pixels into a straight-alpha `image::RgbaImage`,
/// honoring pixel format, Wayland premultiplied alpha for ARGB/ABGR formats,
/// and `wl_output::Transform`.
pub fn shm_to_rgba_image(
    data: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    format: wl_shm::Format,
    transform: wl_output::Transform,
    preserve_alpha: bool,
) -> image::RgbaImage {
    let stride = stride as usize;
    let mut raw = image::RgbaImage::new(width, height);

    for y in 0..height {
        let row_offset = (y as usize) * stride;
        for x in 0..width {
            let px_offset = row_offset + (x as usize) * 4;
            if px_offset + 4 > data.len() {
                continue;
            }
            let p = match format {
                wl_shm::Format::Argb8888 => {
                    let b = data[px_offset];
                    let g = data[px_offset + 1];
                    let r = data[px_offset + 2];
                    let a = data[px_offset + 3];
                    if preserve_alpha {
                        unpremultiply(r, g, b, a)
                    } else {
                        [r, g, b, 255]
                    }
                }
                wl_shm::Format::Xrgb8888 => {
                    let b = data[px_offset];
                    let g = data[px_offset + 1];
                    let r = data[px_offset + 2];
                    [r, g, b, 255]
                }
                wl_shm::Format::Abgr8888 => {
                    let r = data[px_offset];
                    let g = data[px_offset + 1];
                    let b = data[px_offset + 2];
                    let a = data[px_offset + 3];
                    if preserve_alpha {
                        unpremultiply(r, g, b, a)
                    } else {
                        [r, g, b, 255]
                    }
                }
                wl_shm::Format::Xbgr8888 => {
                    let r = data[px_offset];
                    let g = data[px_offset + 1];
                    let b = data[px_offset + 2];
                    [r, g, b, 255]
                }
                _ => {
                    let b = data[px_offset];
                    let g = data[px_offset + 1];
                    let r = data[px_offset + 2];
                    let a = data[px_offset + 3];
                    if preserve_alpha {
                        unpremultiply(r, g, b, a)
                    } else {
                        [r, g, b, 255]
                    }
                }
            };
            raw.put_pixel(x, y, image::Rgba(p));
        }
    }

    apply_transform(raw, transform)
}

fn apply_transform(img: image::RgbaImage, transform: wl_output::Transform) -> image::RgbaImage {
    match transform {
        wl_output::Transform::Normal => img,
        wl_output::Transform::_90 => image::imageops::rotate270(&img),
        wl_output::Transform::_180 => image::imageops::rotate180(&img),
        wl_output::Transform::_270 => image::imageops::rotate90(&img),
        wl_output::Transform::Flipped => image::imageops::flip_horizontal(&img),
        wl_output::Transform::Flipped90 => {
            image::imageops::rotate270(&image::imageops::flip_horizontal(&img))
        }
        wl_output::Transform::Flipped180 => image::imageops::flip_vertical(&img),
        wl_output::Transform::Flipped270 => {
            image::imageops::rotate90(&image::imageops::flip_horizontal(&img))
        }
        _ => img,
    }
}

fn pick_best_shm_format(formats: &[wl_shm::Format]) -> Option<wl_shm::Format> {
    let preferred = [
        wl_shm::Format::Argb8888,
        wl_shm::Format::Abgr8888,
        wl_shm::Format::Xrgb8888,
        wl_shm::Format::Xbgr8888,
    ];
    for p in preferred {
        if formats.contains(&p) {
            return Some(p);
        }
    }
    formats.first().copied()
}

// ---------------------------------------------------------------------------
// Native Toplevel Capture via ext_image_copy_capture_v1
// ---------------------------------------------------------------------------

struct ToplevelEntry {
    handle: ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
    identifier: Option<String>,
    closed: bool,
}

struct ToplevelCaptureState {
    shm: Option<wl_shm::WlShm>,
    toplevel_list: Option<ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1>,
    source_mgr: Option<
        ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
    >,
    capture_mgr: Option<ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1>,

    toplevels: Vec<ToplevelEntry>,

    // Session constraints
    width: u32,
    height: u32,
    shm_formats: Vec<wl_shm::Format>,
    session_done: bool,
    session_stopped: bool,

    // Frame state
    transform: wl_output::Transform,
    frame_ready: bool,
    frame_failed: bool,
    frame_retry_constraints: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for ToplevelCaptureState {
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
                "wl_shm" => {
                    state.shm = Some(registry.bind(name, version.min(1), qh, ()));
                }
                "ext_foreign_toplevel_list_v1" => {
                    state.toplevel_list = Some(registry.bind(name, version.min(1), qh, ()));
                }
                "ext_foreign_toplevel_image_capture_source_manager_v1" => {
                    state.source_mgr = Some(registry.bind(name, version.min(1), qh, ()));
                }
                "ext_image_copy_capture_manager_v1" => {
                    state.capture_mgr = Some(registry.bind(name, version.min(1), qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1, ()> for ToplevelCaptureState {
    fn event(
        state: &mut Self,
        _: &ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event {
            state.toplevels.push(ToplevelEntry {
                handle: toplevel,
                identifier: None,
                closed: false,
            });
        }
    }

    wayland_client::event_created_child!(ToplevelCaptureState, ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1, ())
    ]);
}

impl Dispatch<ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1, ()>
    for ToplevelCaptureState
{
    fn event(
        state: &mut Self,
        proxy: &ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(entry) = state.toplevels.iter_mut().find(|t| &t.handle == proxy) {
            match event {
                ext_foreign_toplevel_handle_v1::Event::Identifier { identifier } => {
                    entry.identifier = Some(identifier);
                }
                ext_foreign_toplevel_handle_v1::Event::Closed => {
                    entry.closed = true;
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1, ()>
    for ToplevelCaptureState
{
    fn event(
        state: &mut Self,
        _: &ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
        event: ext_image_copy_capture_session_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_image_copy_capture_session_v1::Event::BufferSize { width, height } => {
                state.width = width;
                state.height = height;
            }
            ext_image_copy_capture_session_v1::Event::ShmFormat {
                format: WEnum::Value(fmt),
            } => {
                state.shm_formats.push(fmt);
            }
            ext_image_copy_capture_session_v1::Event::Done => {
                state.session_done = true;
            }
            ext_image_copy_capture_session_v1::Event::Stopped => {
                state.session_stopped = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1, ()>
    for ToplevelCaptureState
{
    fn event(
        state: &mut Self,
        _: &ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1,
        event: ext_image_copy_capture_frame_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_image_copy_capture_frame_v1::Event::Transform {
                transform: WEnum::Value(t),
            } => {
                state.transform = t;
            }
            ext_image_copy_capture_frame_v1::Event::Ready => {
                state.frame_ready = true;
            }
            ext_image_copy_capture_frame_v1::Event::Failed { reason } => {
                state.frame_failed = true;
                if matches!(
                    reason,
                    WEnum::Value(
                        ext_image_copy_capture_frame_v1::FailureReason::BufferConstraints
                    )
                ) {
                    state.frame_retry_constraints = true;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_shm::WlShm, ()> for ToplevelCaptureState {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for ToplevelCaptureState {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

wayland_client::delegate_noop!(ToplevelCaptureState: wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(
    ToplevelCaptureState: ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1
);
wayland_client::delegate_noop!(
    ToplevelCaptureState: ext_image_capture_source_v1::ExtImageCaptureSourceV1
);
wayland_client::delegate_noop!(
    ToplevelCaptureState: ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1
);

/// Captures a single toplevel window live directly via native Wayland
/// `ext_foreign_toplevel_list_v1` + `ext_image_copy_capture_v1`.
pub fn capture_toplevel_live(identifier: &str, capture_cursor: bool) -> Option<image::RgbaImage> {
    let conn = Connection::connect_to_env().ok()?;
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();
    let display = conn.display();
    let _registry = display.get_registry(&qh, ());

    let mut state = ToplevelCaptureState {
        shm: None,
        toplevel_list: None,
        source_mgr: None,
        capture_mgr: None,
        toplevels: Vec::new(),
        width: 0,
        height: 0,
        shm_formats: Vec::new(),
        session_done: false,
        session_stopped: false,
        transform: wl_output::Transform::Normal,
        frame_ready: false,
        frame_failed: false,
        frame_retry_constraints: false,
    };

    // Roundtrip 1: discover and bind globals
    event_queue.roundtrip(&mut state).ok()?;

    let shm = state.shm.clone()?;
    let _toplevel_list = state.toplevel_list.clone()?;
    let source_mgr = state.source_mgr.clone()?;
    let capture_mgr = state.capture_mgr.clone()?;

    // Roundtrip 2: receive initial toplevel list & identifiers
    event_queue.roundtrip(&mut state).ok()?;

    let handle = state
        .toplevels
        .iter()
        .find(|t| !t.closed && t.identifier.as_deref() == Some(identifier))?
        .handle
        .clone();

    let source = source_mgr.create_source(&handle, &qh, ());
    let options = if capture_cursor {
        ext_image_copy_capture_manager_v1::Options::PaintCursors
    } else {
        ext_image_copy_capture_manager_v1::Options::empty()
    };
    let session = capture_mgr.create_session(&source, options, &qh, ());

    // Wait for initial session constraints
    while !state.session_done && !state.session_stopped {
        event_queue.blocking_dispatch(&mut state).ok()?;
    }

    if state.session_stopped || state.width == 0 || state.height == 0 {
        session.destroy();
        source.destroy();
        let _ = conn.flush();
        return None;
    }

    let mut result = None;

    for _attempt in 0..3 {
        let format = pick_best_shm_format(&state.shm_formats)?;
        let width = state.width;
        let height = state.height;
        let stride = width * 4;

        let buf = ShmBuffer::new(&shm, width, height, stride, format, &qh).ok()?;

        state.frame_ready = false;
        state.frame_failed = false;
        state.frame_retry_constraints = false;
        state.transform = wl_output::Transform::Normal;

        let frame = session.create_frame(&qh, ());
        frame.attach_buffer(&buf.buffer);
        frame.damage_buffer(0, 0, width as i32, height as i32);
        frame.capture();

        while !state.frame_ready && !state.frame_failed && !state.session_stopped {
            event_queue.blocking_dispatch(&mut state).ok()?;
        }

        frame.destroy();

        if state.frame_ready {
            result = Some(shm_to_rgba_image(
                buf.as_slice(),
                width,
                height,
                stride,
                format,
                state.transform,
                true,
            ));
            break;
        }

        if state.frame_retry_constraints && !state.session_stopped {
            state.session_done = false;
            state.shm_formats.clear();
            while !state.session_done && !state.session_stopped {
                event_queue.blocking_dispatch(&mut state).ok()?;
            }
            continue;
        }

        break;
    }

    session.destroy();
    source.destroy();
    for entry in state.toplevels.drain(..) {
        entry.handle.destroy();
    }
    if let Some(list) = state.toplevel_list.take() {
        list.stop();
        list.destroy();
    }
    let _ = conn.flush();

    result
}

// ---------------------------------------------------------------------------
// Native Live Region Capture via zwlr_screencopy_manager_v1
// ---------------------------------------------------------------------------

struct RegionOutputEntry {
    output: wl_output::WlOutput,
    name: String,
    scale: i32,
}

struct RegionCaptureState {
    shm: Option<wl_shm::WlShm>,
    screencopy_mgr: Option<zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1>,
    outputs: Vec<RegionOutputEntry>,

    shm_buffer: Option<ShmBuffer>,
    format: wl_shm::Format,
    width: u32,
    height: u32,
    stride: u32,
    y_invert: bool,
    ready: bool,
    failed: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for RegionCaptureState {
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
                "wl_shm" => {
                    state.shm = Some(registry.bind(name, version.min(1), qh, ()));
                }
                "zwlr_screencopy_manager_v1" => {
                    state.screencopy_mgr = Some(registry.bind(name, version.min(3), qh, ()));
                }
                "wl_output" => {
                    let idx = state.outputs.len();
                    let output = registry.bind(name, version.min(4), qh, idx);
                    state.outputs.push(RegionOutputEntry {
                        output,
                        name: format!("Output-{}", idx),
                        scale: 1,
                    });
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, usize> for RegionCaptureState {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        &idx: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(out) = state.outputs.get_mut(idx) {
            match event {
                wl_output::Event::Name { name } => out.name = name,
                wl_output::Event::Scale { factor } => out.scale = factor,
                _ => {}
            }
        }
    }
}

impl Dispatch<zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1, ()> for RegionCaptureState {
    fn event(
        state: &mut Self,
        frame: &zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => {
                let fmt = match format {
                    WEnum::Value(f) => f,
                    _ => wl_shm::Format::Xrgb8888,
                };
                if let Some(ref shm) = state.shm
                    && let Ok(buf) = ShmBuffer::new(shm, width, height, stride, fmt, qh)
                {
                    frame.copy(&buf.buffer);
                    state.shm_buffer = Some(buf);
                    state.format = fmt;
                    state.width = width;
                    state.height = height;
                    state.stride = stride;
                } else {
                    state.failed = true;
                }
            }
            zwlr_screencopy_frame_v1::Event::Flags { flags } => {
                if let WEnum::Value(f) = flags
                    && f.contains(zwlr_screencopy_frame_v1::Flags::YInvert)
                {
                    state.y_invert = true;
                }
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => {
                state.ready = true;
            }
            zwlr_screencopy_frame_v1::Event::Failed => {
                state.failed = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_shm::WlShm, ()> for RegionCaptureState {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for RegionCaptureState {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

wayland_client::delegate_noop!(RegionCaptureState: wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(
    RegionCaptureState: zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1
);

/// Captures an output region live via native `zwlr_screencopy_manager_v1::capture_output_region`.
/// `rect` is in physical output pixel coordinates (matching `AppState.selection`).
pub fn capture_region_live(
    output_name: &str,
    rect: Rect,
    scale: i32,
    capture_cursor: bool,
) -> Option<image::RgbaImage> {
    if rect.width == 0 || rect.height == 0 {
        return None;
    }

    let conn = Connection::connect_to_env().ok()?;
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();
    let display = conn.display();
    let _registry = display.get_registry(&qh, ());

    let mut state = RegionCaptureState {
        shm: None,
        screencopy_mgr: None,
        outputs: Vec::new(),
        shm_buffer: None,
        format: wl_shm::Format::Xrgb8888,
        width: 0,
        height: 0,
        stride: 0,
        y_invert: false,
        ready: false,
        failed: false,
    };

    // Roundtrip 1: discover globals
    event_queue.roundtrip(&mut state).ok()?;
    // Roundtrip 2: receive wl_output names and scales
    event_queue.roundtrip(&mut state).ok()?;

    let mgr = state.screencopy_mgr.clone()?;
    let target_output = state
        .outputs
        .iter()
        .find(|o| o.name == output_name)
        .or_else(|| state.outputs.first())?;

    let eff_scale = scale.max(target_output.scale).max(1) as u32;

    // Convert physical pixel rect -> output logical coordinates covering the full rect
    let log_x = rect.x / eff_scale;
    let log_y = rect.y / eff_scale;
    let log_x2 = (rect.x + rect.width).div_ceil(eff_scale);
    let log_y2 = (rect.y + rect.height).div_ceil(eff_scale);
    let log_w = log_x2.saturating_sub(log_x).max(1);
    let log_h = log_y2.saturating_sub(log_y).max(1);

    let overlay_cursor = if capture_cursor { 1 } else { 0 };
    let frame = mgr.capture_output_region(
        overlay_cursor,
        &target_output.output,
        log_x as i32,
        log_y as i32,
        log_w as i32,
        log_h as i32,
        &qh,
        (),
    );

    while !state.ready && !state.failed {
        event_queue.blocking_dispatch(&mut state).ok()?;
    }

    frame.destroy();
    let _ = conn.flush();

    if state.failed {
        return None;
    }

    let buf = state.shm_buffer.as_ref()?;
    let sub_x = rect.x.saturating_sub(log_x * eff_scale);
    let sub_y = rect.y.saturating_sub(log_y * eff_scale);
    let crop_w = rect.width.min(state.width.saturating_sub(sub_x));
    let crop_h = rect.height.min(state.height.saturating_sub(sub_y));

    if crop_w == 0 || crop_h == 0 {
        return None;
    }

    Some(crate::render::crop_to_image(
        buf.as_slice(),
        state.width,
        state.height,
        state.stride,
        state.format,
        state.y_invert,
        sub_x,
        sub_y,
        crop_w,
        crop_h,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unpremultiply_argb8888() {
        // Kitty #181818 (24,24,24) premultiplied at alpha 217 is (20,20,20,217)
        let data = [20u8, 20, 20, 217];
        let img = shm_to_rgba_image(
            &data,
            1,
            1,
            4,
            wl_shm::Format::Argb8888,
            wl_output::Transform::Normal,
            true,
        );
        assert_eq!(*img.get_pixel(0, 0), image::Rgba([24, 24, 24, 217]));
    }

    #[test]
    fn test_overlay_destroy_roundtrip_no_bleed() {
        if std::env::var("WAYLAND_DISPLAY").is_err() {
            return;
        }
        use wayland_client::protocol::{wl_callback, wl_compositor, wl_surface};
        use wayland_protocols_wlr::layer_shell::v1::client::{
            zwlr_layer_shell_v1, zwlr_layer_surface_v1,
        };

        struct OverlayProbe {
            compositor: Option<wl_compositor::WlCompositor>,
            shm: Option<wl_shm::WlShm>,
            layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
            configured: bool,
            frame_done: bool,
        }

        impl Dispatch<wl_registry::WlRegistry, ()> for OverlayProbe {
            fn event(
                state: &mut Self,
                registry: &wl_registry::WlRegistry,
                event: wl_registry::Event,
                _: &(),
                _: &Connection,
                qh: &QueueHandle<Self>,
            ) {
                if let wl_registry::Event::Global {
                    name,
                    interface,
                    version,
                } = event
                {
                    match interface.as_str() {
                        "wl_compositor" => {
                            state.compositor = Some(registry.bind(name, version.min(4), qh, ()));
                        }
                        "wl_shm" => {
                            state.shm = Some(registry.bind(name, version.min(1), qh, ()));
                        }
                        "zwlr_layer_shell_v1" => {
                            state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()));
                        }
                        _ => {}
                    }
                }
            }
        }
        impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for OverlayProbe {
            fn event(
                state: &mut Self,
                ls: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
                event: zwlr_layer_surface_v1::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
                if let zwlr_layer_surface_v1::Event::Configure { serial, .. } = event {
                    ls.ack_configure(serial);
                    state.configured = true;
                }
            }
        }
        impl Dispatch<wl_callback::WlCallback, ()> for OverlayProbe {
            fn event(
                state: &mut Self,
                _: &wl_callback::WlCallback,
                event: wl_callback::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
                if let wl_callback::Event::Done { .. } = event {
                    state.frame_done = true;
                }
            }
        }
        wayland_client::delegate_noop!(OverlayProbe: wl_compositor::WlCompositor);
        wayland_client::delegate_noop!(OverlayProbe: ignore wl_shm::WlShm);
        wayland_client::delegate_noop!(OverlayProbe: wl_shm_pool::WlShmPool);
        wayland_client::delegate_noop!(OverlayProbe: ignore wl_buffer::WlBuffer);
        wayland_client::delegate_noop!(OverlayProbe: ignore wl_surface::WlSurface);
        wayland_client::delegate_noop!(OverlayProbe: zwlr_layer_shell_v1::ZwlrLayerShellV1);

        let baseline = capture_region_live(
            "eDP-1",
            Rect {
                x: 0,
                y: 0,
                width: 8,
                height: 8,
            },
            1,
            false,
        )
        .unwrap();
        let px_baseline = *baseline.get_pixel(4, 4);

        let conn = Connection::connect_to_env().unwrap();
        let mut eq = conn.new_event_queue();
        let qh = eq.handle();
        let _reg = conn.display().get_registry(&qh, ());
        let mut st = OverlayProbe {
            compositor: None,
            shm: None,
            layer_shell: None,
            configured: false,
            frame_done: false,
        };
        eq.roundtrip(&mut st).unwrap();

        let (Some(comp), Some(shm), Some(ls_mgr)) =
            (st.compositor.clone(), st.shm.clone(), st.layer_shell.clone())
        else {
            return;
        };

        let surface = comp.create_surface(&qh, ());
        let ls = ls_mgr.get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Overlay,
            "ssnip".to_string(),
            &qh,
            (),
        );
        ls.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left);
        ls.set_exclusive_zone(-1);
        ls.set_size(8, 8);
        surface.commit();

        while !st.configured {
            eq.blocking_dispatch(&mut st).unwrap();
        }

        let mut buf = ShmBuffer::new(&shm, 8, 8, 32, wl_shm::Format::Argb8888, &qh).unwrap();
        for chunk in buf.as_slice_mut().as_chunks_mut::<4>().0 {
            chunk.copy_from_slice(&[255, 0, 255, 255]);
        }
        surface.attach(Some(&buf.buffer), 0, 0);
        surface.damage_buffer(0, 0, 8, 8);
        surface.commit();
        eq.roundtrip(&mut st).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(60));

        let during = capture_region_live(
            "eDP-1",
            Rect {
                x: 0,
                y: 0,
                width: 8,
                height: 8,
            },
            1,
            false,
        )
        .unwrap();
        let px_during = *during.get_pixel(4, 4);

        // Attach a 100% transparent Argb8888 buffer and wait for compositor frame callback
        // so Hyprland's fade-out animation fades out pure transparency!
        let mut clear_buf = ShmBuffer::new(&shm, 8, 8, 32, wl_shm::Format::Argb8888, &qh).unwrap();
        clear_buf.as_slice_mut().fill(0);
        st.frame_done = false;
        let _cb = surface.frame(&qh, ());
        surface.attach(Some(&clear_buf.buffer), 0, 0);
        surface.damage_buffer(0, 0, 8, 8);
        surface.commit();
        while !st.frame_done {
            eq.blocking_dispatch(&mut st).unwrap();
        }

        ls.destroy();
        surface.destroy();
        eq.roundtrip(&mut st).unwrap();

        let after = capture_region_live(
            "eDP-1",
            Rect {
                x: 0,
                y: 0,
                width: 8,
                height: 8,
            },
            1,
            false,
        )
        .unwrap();
        let px_after = *after.get_pixel(4, 4);
        eprintln!(
            "OVERLAY BLEED TEST: baseline={:?} during={:?} after_clear_frame={:?}",
            px_baseline, px_during, px_after
        );
        assert_ne!(
            px_during, px_baseline,
            "Overlay should have changed pixels while visible"
        );
        for ch in 0..3 {
            assert!(
                px_after[ch].abs_diff(px_baseline[ch]) <= 5,
                "Overlay leaked into live region capture on channel {}: baseline={:?} after={:?}",
                ch,
                px_baseline,
                px_after
            );
        }
    }

    #[test]
    fn test_live_toplevel_and_region_capture() {
        if std::env::var("WAYLAND_DISPLAY").is_err() {
            return;
        }
        let reg = capture_region_live(
            "eDP-1",
            Rect {
                x: 50,
                y: 50,
                width: 200,
                height: 150,
            },
            1,
            false,
        );
        assert!(reg.is_some(), "Native capture_region_live should succeed");
        assert_eq!(reg.unwrap().dimensions(), (200, 150));

        if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
            let wins = crate::window::get_windows_for_monitor("eDP-1", 1);
            for w in wins {
                if let Some(ref sid) = w.stable_id {
                    let top = capture_toplevel_live(sid, false);
                    assert!(
                        top.is_some(),
                        "Native capture_toplevel_live failed for stableId {}",
                        sid
                    );
                    let img = top.unwrap();
                    assert!(img.width() > 0 && img.height() > 0);
                }
            }
        }
    }
}
