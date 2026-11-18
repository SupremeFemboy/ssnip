use crate::render::Rect;
use serde::Deserialize;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub rect: Rect,
    pub stable_id: Option<String>,
    pub floating: bool,
    pub z_index: usize,
}

#[derive(Debug, Deserialize, Default, Clone, PartialEq, Eq)]
struct HyprWorkspace {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    name: String,
}

impl HyprWorkspace {
    fn matches(&self, other: &HyprWorkspace) -> bool {
        if self.name.is_empty() && self.id.is_none() {
            return false;
        }
        if other.name.is_empty() && other.id.is_none() {
            return false;
        }

        if let (Some(id_a), Some(id_b)) = (self.id, other.id)
            && id_a == id_b
        {
            return true;
        }

        !self.name.is_empty() && !other.name.is_empty() && self.name == other.name
    }
}

#[derive(Debug, Deserialize)]
struct HyprMonitor {
    id: i64,
    name: String,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(rename = "activeWorkspace", default)]
    active_workspace: HyprWorkspace,
    #[serde(rename = "specialWorkspace", default)]
    special_workspace: Option<HyprWorkspace>,
}

#[derive(Debug, Deserialize)]
struct HyprClient {
    at: [i32; 2],
    size: [i32; 2],
    #[serde(default)]
    workspace: HyprWorkspace,
    monitor: i64,
    #[serde(default = "default_mapped")]
    mapped: bool,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    floating: bool,
    #[serde(default)]
    pinned: bool,
    #[serde(rename = "focusHistoryID", default)]
    focus_history_id: i64,
    #[serde(rename = "stableId", default)]
    stable_id: Option<String>,
}

fn default_mapped() -> bool {
    true
}

fn query_hyprland_socket(
    sock_path: &PathBuf,
    request: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut stream = UnixStream::connect(sock_path)?;
    stream.write_all(request.as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

fn query_hyprland(
    sock_path: &PathBuf,
    request: &str,
    cli_arg: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(res) = query_hyprland_socket(sock_path, request) {
        return Ok(res);
    }
    let output = Command::new("hyprctl").args(["-j", cli_arg]).output()?;
    Ok(String::from_utf8(output.stdout)?)
}

fn get_hyprland_windows(
    monitor_name: &str,
    scale: i32,
) -> Result<Vec<WindowInfo>, Box<dyn std::error::Error>> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")?;
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| {
        let uid = rustix::process::getuid().as_raw();
        format!("/run/user/{}", uid)
    });

    let sock_path = PathBuf::from(&runtime_dir)
        .join("hypr")
        .join(&signature)
        .join(".socket.sock");

    let sock_path = if sock_path.exists() {
        sock_path
    } else {
        PathBuf::from("/tmp")
            .join("hypr")
            .join(&signature)
            .join(".socket.sock")
    };

    let monitors_json = query_hyprland(&sock_path, "j/monitors", "monitors")?;
    let monitors: Vec<HyprMonitor> = serde_json::from_str(&monitors_json)?;

    let target_mon = monitors
        .iter()
        .find(|m| m.name == monitor_name)
        .or_else(|| monitors.first())
        .ok_or("Monitor not found in Hyprland")?;

    let clients_json = query_hyprland(&sock_path, "j/clients", "clients")?;
    let mut clients: Vec<HyprClient> = serde_json::from_str(&clients_json)?;

    // Sort bottom-to-top: tiled windows first, then floating windows from oldest-focused
    // to most-recently-focused, with pinned floating windows on very top.
    clients.sort_by_key(|c| (c.floating, c.pinned, std::cmp::Reverse(c.focus_history_id)));

    let scale = scale.max(1);
    let mut windows = Vec::new();

    for (z_index, client) in clients.into_iter().enumerate() {
        if !client.mapped || client.hidden {
            continue;
        }

        let is_visible = client.workspace.matches(&target_mon.active_workspace)
            || target_mon
                .special_workspace
                .as_ref()
                .is_some_and(|s| client.workspace.matches(s));

        if !is_visible || client.monitor != target_mon.id {
            continue;
        }

        let rel_x = client.at[0] - target_mon.x;
        let rel_y = client.at[1] - target_mon.y;
        let w = client.size[0];
        let h = client.size[1];

        if w > 0 && h > 0 && rel_x + w > 0 && rel_y + h > 0 {
            let win_x = rel_x.max(0);
            let win_y = rel_y.max(0);
            let win_w = (rel_x + w - win_x).max(1);
            let win_h = (rel_y + h - win_y).max(1);

            windows.push(WindowInfo {
                rect: Rect {
                    x: (win_x * scale) as u32,
                    y: (win_y * scale) as u32,
                    width: (win_w * scale) as u32,
                    height: (win_h * scale) as u32,
                },
                stable_id: client.stable_id,
                floating: client.floating,
                z_index,
            });
        }
    }

    Ok(windows)
}

fn get_sway_windows(
    monitor_name: &str,
    scale: i32,
) -> Result<Vec<WindowInfo>, Box<dyn std::error::Error>> {
    let swaysock = std::env::var("SWAYSOCK")?;
    let mut stream = UnixStream::connect(swaysock)?;

    let magic = b"i3-ipc";
    let len: u32 = 0;
    let msg_type: u32 = 4; // GET_TREE

    let mut header = Vec::with_capacity(14);
    header.extend_from_slice(magic);
    header.extend_from_slice(&len.to_le_bytes());
    header.extend_from_slice(&msg_type.to_le_bytes());
    stream.write_all(&header)?;

    let mut resp_header = [0u8; 14];
    stream.read_exact(&mut resp_header)?;
    let resp_len = u32::from_le_bytes(resp_header[6..10].try_into()?) as usize;

    let mut body = vec![0u8; resp_len];
    stream.read_exact(&mut body)?;

    let root: serde_json::Value = serde_json::from_slice(&body)?;
    let scale = scale.max(1);
    let mut windows = Vec::new();

    if let Some(nodes) = root.get("nodes").and_then(|n| n.as_array()) {
        for output in nodes {
            if output.get("name").and_then(|n| n.as_str()) == Some(monitor_name) {
                let mon_x = output
                    .get("rect")
                    .and_then(|r| r.get("x"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let mon_y = output
                    .get("rect")
                    .and_then(|r| r.get("y"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                collect_sway_windows(output, mon_x, mon_y, scale, &mut windows);
                break;
            }
        }
    }

    Ok(windows)
}

fn collect_sway_windows(
    node: &serde_json::Value,
    mon_x: i64,
    mon_y: i64,
    scale: i32,
    out: &mut Vec<WindowInfo>,
) {
    let is_floating = node.get("type").and_then(|t| t.as_str()) == Some("floating_con");
    if let Some(rect) = node.get("rect")
        && (node.get("type").and_then(|t| t.as_str()) == Some("con") || is_floating)
        && node
            .get("nodes")
            .and_then(|n| n.as_array())
            .is_none_or(|a| a.is_empty())
    {
        let x = rect.get("x").and_then(|v| v.as_i64()).unwrap_or(0) - mon_x;
        let y = rect.get("y").and_then(|v| v.as_i64()).unwrap_or(0) - mon_y;
        let w = rect.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
        let h = rect.get("height").and_then(|v| v.as_i64()).unwrap_or(0);

        if w > 0 && h > 0 && x >= 0 && y >= 0 {
            let z_index = out.len();
            out.push(WindowInfo {
                rect: Rect {
                    x: (x as i32 * scale) as u32,
                    y: (y as i32 * scale) as u32,
                    width: (w as i32 * scale) as u32,
                    height: (h as i32 * scale) as u32,
                },
                stable_id: None,
                floating: is_floating,
                z_index,
            });
        }
    }

    if let Some(nodes) = node.get("nodes").and_then(|n| n.as_array()) {
        for child in nodes {
            collect_sway_windows(child, mon_x, mon_y, scale, out);
        }
    }
    if let Some(floating) = node.get("floating_nodes").and_then(|n| n.as_array()) {
        for child in floating {
            collect_sway_windows(child, mon_x, mon_y, scale, out);
        }
    }
}

pub fn get_windows_for_monitor(monitor_name: &str, scale: i32) -> Vec<WindowInfo> {
    if let Ok(windows) = get_hyprland_windows(monitor_name, scale)
        && !windows.is_empty()
    {
        return windows;
    }
    if let Ok(windows) = get_sway_windows(monitor_name, scale)
        && !windows.is_empty()
    {
        return windows;
    }
    Vec::new()
}

pub fn get_overlapping_windows(target: &WindowInfo, all_windows: &[WindowInfo]) -> Vec<Rect> {
    let mut overlaps = Vec::new();

    for other in all_windows {
        if other.rect == target.rect && other.stable_id == target.stable_id {
            continue;
        }

        let is_on_top = (!target.floating && other.floating)
            || (target.floating && other.floating && other.z_index > target.z_index);

        if is_on_top && target.rect.intersects(&other.rect) {
            overlaps.push(other.rect);
        }
    }

    overlaps
}

/// Builds a pixel occlusion mask in `win_rect`-local coordinates for `overlaps`
/// expanded by `margin` pixels to also exclude compositor borders and drop shadows.
fn build_occlusion_mask(
    w: u32,
    h: u32,
    scr_w: u32,
    scr_h: u32,
    win_rect: Rect,
    overlaps: &[Rect],
    margin: u32,
) -> Vec<bool> {
    let mut occluded = vec![false; (w * h) as usize];

    // Pixels outside the captured screen reference are treated as occluded
    if scr_w < w || scr_h < h {
        for y in 0..h {
            let row = (y * w) as usize;
            if y >= scr_h {
                occluded[row..row + w as usize].fill(true);
            } else if scr_w < w {
                occluded[row + scr_w as usize..row + w as usize].fill(true);
            }
        }
    }

    for ov in overlaps {
        let ex0 = ov.x.saturating_sub(margin);
        let ey0 = ov.y.saturating_sub(margin);
        let ex1 = ov.x.saturating_add(ov.width).saturating_add(margin);
        let ey1 = ov.y.saturating_add(ov.height).saturating_add(margin);

        let lx0 = ex0.saturating_sub(win_rect.x).min(w);
        let ly0 = ey0.saturating_sub(win_rect.y).min(h);
        let lx1 = ex1.saturating_sub(win_rect.x).min(w);
        let ly1 = ey1.saturating_sub(win_rect.y).min(h);

        if lx1 > lx0 && ly1 > ly0 {
            for y in ly0..ly1 {
                let row = (y * w) as usize;
                occluded[row + lx0 as usize..row + lx1 as usize].fill(true);
            }
        }
    }

    occluded
}

/// Composites a live isolated toplevel capture (`top_img`, straight RGBA) cleanly:
/// - Opaque windows return `top_img` directly with zero overlap bleed.
/// - Translucent windows without overlaps return the live desktop region directly
///   so native wallpaper blur/shadows remain intact.
/// - Translucent windows WITH overlapping floating windows recover the real underlying
///   desktop background from un-occluded translucent pixels and harmonically interpolate it
///   across the occluded area (plus border/shadow margin), eliminating overlapping windows
///   and their shadows completely while preserving a seamless translucent background.
pub fn composite_window_capture(
    top_img: image::RgbaImage,
    screen_ref: Option<&image::RgbaImage>,
    win_rect: Rect,
    overlaps: &[Rect],
    scale: i32,
) -> image::RgbaImage {
    let has_transparency = top_img.pixels().any(|p| p[3] < 255);
    if !has_transparency {
        return top_img;
    }

    let Some(scr) = screen_ref else {
        // No desktop reference available; flatten straight RGB to opaque so JPEG/BMP never turn black
        let mut out = top_img;
        for px in out.pixels_mut() {
            px[3] = 255;
        }
        return out;
    };

    let (w, h) = top_img.dimensions();
    let (scr_w, scr_h) = scr.dimensions();

    // Fast path: translucent window with NO overlapping windows and matching on-screen size
    if overlaps.is_empty() && scr_w == w && scr_h == h {
        return scr.clone();
    }

    // Choose largest shadow/border margin that still leaves un-occluded translucent samples
    let base_margin = (28 * scale.max(1)) as u32;
    let margins = [base_margin, (4 * scale.max(1)) as u32, 0];
    let mut occluded = Vec::new();

    for &m in &margins {
        let candidate = build_occlusion_mask(w, h, scr_w, scr_h, win_rect, overlaps, m);
        let has_sample = (0..h).any(|y| {
            let row = (y * w) as usize;
            (0..w).any(|x| !candidate[row + x as usize] && top_img.get_pixel(x, y)[3] < 250)
        });
        occluded = candidate;
        if has_sample {
            break;
        }
    }

    // Coarse grid for recovering & harmonically interpolating the real underlying wallpaper
    const CELL: u32 = 16;
    let gw = (w.div_ceil(CELL)).max(1) as usize;
    let gh = (h.div_ceil(CELL)).max(1) as usize;
    let mut sum_r = vec![0u64; gw * gh];
    let mut sum_g = vec![0u64; gw * gh];
    let mut sum_b = vec![0u64; gw * gh];
    let mut count = vec![0u32; gw * gh];

    let mut tot_r: u64 = 0;
    let mut tot_g: u64 = 0;
    let mut tot_b: u64 = 0;
    let mut tot_cnt: u64 = 0;

    for y in 0..h.min(scr_h) {
        let row = (y * w) as usize;
        let cy = (y / CELL) as usize;
        for x in 0..w.min(scr_w) {
            if occluded[row + x as usize] {
                continue;
            }
            let top_px = top_img.get_pixel(x, y);
            let a = top_px[3];
            if a < 250 {
                let scr_px = scr.get_pixel(x, y);
                let a32 = a as i32;
                let inv_a = 255 - a32;
                let wr = ((scr_px[0] as i32 * 255 - top_px[0] as i32 * a32) / inv_a).clamp(0, 255)
                    as u64;
                let wg = ((scr_px[1] as i32 * 255 - top_px[1] as i32 * a32) / inv_a).clamp(0, 255)
                    as u64;
                let wb = ((scr_px[2] as i32 * 255 - top_px[2] as i32 * a32) / inv_a).clamp(0, 255)
                    as u64;

                let cell_idx = cy * gw + (x / CELL) as usize;
                sum_r[cell_idx] += wr;
                sum_g[cell_idx] += wg;
                sum_b[cell_idx] += wb;
                count[cell_idx] += 1;

                tot_r += wr;
                tot_g += wg;
                tot_b += wb;
                tot_cnt += 1;
            }
        }
    }

    let has_bg_field = tot_cnt > 0;
    let mut grid = vec![[0.0f32; 3]; gw * gh];
    let mut known = vec![false; gw * gh];

    if has_bg_field {
        let fallback = [
            (tot_r as f32) / (tot_cnt as f32),
            (tot_g as f32) / (tot_cnt as f32),
            (tot_b as f32) / (tot_cnt as f32),
        ];

        for i in 0..(gw * gh) {
            if count[i] > 0 {
                let c = count[i] as f32;
                grid[i] = [sum_r[i] as f32 / c, sum_g[i] as f32 / c, sum_b[i] as f32 / c];
                known[i] = true;
            } else {
                grid[i] = fallback;
            }
        }

        // Smooth harmonic (Laplace) relaxation over unknown/occluded cells
        for _ in 0..48 {
            for cy in 0..gh {
                for cx in 0..gw {
                    let idx = cy * gw + cx;
                    if known[idx] {
                        continue;
                    }
                    let mut nr = 0.0f32;
                    let mut ng = 0.0f32;
                    let mut nb = 0.0f32;
                    let mut n = 0.0f32;

                    if cx > 0 {
                        let p = grid[idx - 1];
                        nr += p[0];
                        ng += p[1];
                        nb += p[2];
                        n += 1.0;
                    }
                    if cx + 1 < gw {
                        let p = grid[idx + 1];
                        nr += p[0];
                        ng += p[1];
                        nb += p[2];
                        n += 1.0;
                    }
                    if cy > 0 {
                        let p = grid[idx - gw];
                        nr += p[0];
                        ng += p[1];
                        nb += p[2];
                        n += 1.0;
                    }
                    if cy + 1 < gh {
                        let p = grid[idx + gw];
                        nr += p[0];
                        ng += p[1];
                        nb += p[2];
                        n += 1.0;
                    }

                    if n > 0.0 {
                        grid[idx] = [nr / n, ng / n, nb / n];
                    }
                }
            }
        }
    }

    let sample_bg = |x: u32, y: u32| -> [u8; 3] {
        let fx = ((x as f32 + 0.5) / CELL as f32 - 0.5).clamp(0.0, (gw - 1) as f32);
        let fy = ((y as f32 + 0.5) / CELL as f32 - 0.5).clamp(0.0, (gh - 1) as f32);
        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let x1 = (x0 + 1).min(gw - 1);
        let y1 = (y0 + 1).min(gh - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;

        let c00 = grid[y0 * gw + x0];
        let c10 = grid[y0 * gw + x1];
        let c01 = grid[y1 * gw + x0];
        let c11 = grid[y1 * gw + x1];

        let mut out = [0u8; 3];
        for ch in 0..3 {
            let top = c00[ch] * (1.0 - tx) + c10[ch] * tx;
            let bot = c01[ch] * (1.0 - tx) + c11[ch] * tx;
            out[ch] = (top * (1.0 - ty) + bot * ty).round().clamp(0.0, 255.0) as u8;
        }
        out
    };

    let mut out = image::RgbaImage::new(w, h);
    for y in 0..h {
        let row = (y * w) as usize;
        for x in 0..w {
            let top_px = *top_img.get_pixel(x, y);
            let a = top_px[3];
            if a == 255 {
                // Opaque pixel from live isolated toplevel -> never affected by overlapping windows or shadows
                out.put_pixel(x, y, top_px);
            } else if !occluded[row + x as usize] && x < scr_w && y < scr_h {
                // Un-occluded translucent pixel -> keep exact real compositor blur & wallpaper
                out.put_pixel(x, y, *scr.get_pixel(x, y));
            } else if has_bg_field {
                // Occluded translucent pixel -> composite live window over smoothly reconstructed wallpaper
                let bg = sample_bg(x, y);
                let a32 = a as u32;
                let inv_a = 255 - a32;
                let r = ((top_px[0] as u32 * a32 + bg[0] as u32 * inv_a + 127) / 255) as u8;
                let g = ((top_px[1] as u32 * a32 + bg[1] as u32 * inv_a + 127) / 255) as u8;
                let b = ((top_px[2] as u32 * a32 + bg[2] as u32 * inv_a + 127) / 255) as u8;
                out.put_pixel(x, y, image::Rgba([r, g, b, 255]));
            } else {
                // Fully covered window with no background samples -> keep window's own straight RGB
                out.put_pixel(x, y, image::Rgba([top_px[0], top_px[1], top_px[2], 255]));
            }
        }
    }

    out
}

/// Captures a selected window live via native Wayland protocols (`ext_image_copy_capture_v1`
/// for isolated toplevels + `zwlr_screencopy_manager_v1` for live desktop background when needed).
pub fn capture_window_live(
    win: &WindowInfo,
    output_name: &str,
    scale: i32,
    capture_cursor: bool,
    all_windows: &[WindowInfo],
    cap: &crate::CapturedFrameData,
) -> Option<image::RgbaImage> {
    let overlaps = get_overlapping_windows(win, all_windows);

    if let Some(ref stable_id) = win.stable_id
        && let Some(top_img) = crate::capture::capture_toplevel_live(stable_id, capture_cursor)
    {
        let has_transparency = top_img.pixels().any(|p| p[3] < 255);
        if !has_transparency {
            return Some(top_img);
        }

        // Translucent window: capture live desktop region (falling back to startup frame)
        let screen_ref = crate::capture::capture_region_live(
            output_name,
            win.rect,
            scale,
            capture_cursor,
        )
        .unwrap_or_else(|| {
            crate::render::crop_to_image(
                cap.shm_buffer.as_slice(),
                cap.width,
                cap.height,
                cap.stride,
                cap.format,
                cap.y_invert,
                win.rect.x,
                win.rect.y,
                win.rect.width,
                win.rect.height,
            )
        });

        return Some(composite_window_capture(
            top_img,
            Some(&screen_ref),
            win.rect,
            &overlaps,
            scale,
        ));
    }

    crate::capture::capture_region_live(output_name, win.rect, scale, capture_cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hyprland_query() {
        if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
            let res = get_hyprland_windows("eDP-1", 1);
            assert!(
                res.is_ok(),
                "Query to Hyprland should succeed: {:?}",
                res.err()
            );
        }
    }

    #[test]
    fn test_get_overlapping_windows() {
        let tiled = WindowInfo {
            rect: Rect {
                x: 0,
                y: 0,
                width: 1000,
                height: 1000,
            },
            stable_id: Some("1".into()),
            floating: false,
            z_index: 0,
        };

        let floating_on_top = WindowInfo {
            rect: Rect {
                x: 200,
                y: 200,
                width: 400,
                height: 400,
            },
            stable_id: Some("2".into()),
            floating: true,
            z_index: 1,
        };

        let floating_outside = WindowInfo {
            rect: Rect {
                x: 2000,
                y: 2000,
                width: 400,
                height: 400,
            },
            stable_id: Some("3".into()),
            floating: true,
            z_index: 2,
        };

        let all = vec![tiled.clone(), floating_on_top.clone(), floating_outside];

        let overlaps_tiled = get_overlapping_windows(&tiled, &all);
        assert_eq!(overlaps_tiled.len(), 1);
        assert_eq!(overlaps_tiled[0], floating_on_top.rect);

        let overlaps_floating = get_overlapping_windows(&floating_on_top, &all);
        assert!(overlaps_floating.is_empty());
    }

    #[test]
    fn test_composite_translucent_removes_floating_window_and_shadow() {
        let w = 200u32;
        let h = 200u32;
        let win_rect = Rect {
            x: 10,
            y: 10,
            width: w,
            height: h,
        };

        // Wallpaper color: blue-purple (80, 100, 200)
        // Translucent terminal background: (24, 24, 24), alpha = 217
        let wall = [80u8, 100, 200];
        let term = [24u8, 24, 24];
        let alpha = 217u8;
        let expected_bg = [
            ((term[0] as u32 * alpha as u32 + wall[0] as u32 * (255 - alpha as u32) + 127) / 255)
                as u8,
            ((term[1] as u32 * alpha as u32 + wall[1] as u32 * (255 - alpha as u32) + 127) / 255)
                as u8,
            ((term[2] as u32 * alpha as u32 + wall[2] as u32 * (255 - alpha as u32) + 127) / 255)
                as u8,
            255,
        ];

        // Toplevel capture: translucent terminal + white opaque text at (100, 100)
        let mut top_img = image::RgbaImage::from_pixel(w, h, image::Rgba([term[0], term[1], term[2], alpha]));
        top_img.put_pixel(100, 100, image::Rgba([255, 255, 255, 255]));

        // Screen capture: real composited terminal, EXCEPT a bright red floating window + dark shadow
        let mut scr_img = image::RgbaImage::from_pixel(w, h, image::Rgba(expected_bg));
        // Shadow ring around [70..130, 70..130] (margin 10px -> [60..140])
        for y in 60..140 {
            for x in 60..140 {
                scr_img.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
            }
        }
        // Bright red floating window at local [70..130, 70..130] => output [80..140, 80..140]
        for y in 70..130 {
            for x in 70..130 {
                scr_img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }

        let overlaps = vec![Rect {
            x: 80,
            y: 80,
            width: 60,
            height: 60,
        }];

        let result = composite_window_capture(top_img, Some(&scr_img), win_rect, &overlaps, 1);

        // Opaque text pixel inside the occluded area must be pure white (no red floating window!)
        assert_eq!(*result.get_pixel(100, 100), image::Rgba([255, 255, 255, 255]));

        // Translucent terminal pixel inside the occluded area (where red floating window was)
        // must match expected_bg (terminal over reconstructed blue-purple wallpaper), ZERO red!
        let inner = *result.get_pixel(90, 90);
        for ch in 0..3 {
            assert!(
                inner[ch].abs_diff(expected_bg[ch]) <= 2,
                "Occluded pixel channel {} mismatch: got {:?}, expected {:?}",
                ch,
                inner,
                expected_bg
            );
        }

        // Shadow ring pixel at (65, 65) must also match expected_bg, ZERO black shadow!
        let shadow_px = *result.get_pixel(65, 65);
        for ch in 0..3 {
            assert!(
                shadow_px[ch].abs_diff(expected_bg[ch]) <= 2,
                "Shadow pixel channel {} mismatch: got {:?}, expected {:?}",
                ch,
                shadow_px,
                expected_bg
            );
        }
    }

    #[test]
    fn test_composite_opaque_window_ignores_overlaps() {
        let top_img = image::RgbaImage::from_pixel(100, 100, image::Rgba([40, 80, 120, 255]));
        let scr_img = image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 0, 0, 255]));
        let win_rect = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        let overlaps = vec![win_rect];

        let result = composite_window_capture(top_img, Some(&scr_img), win_rect, &overlaps, 1);
        assert_eq!(*result.get_pixel(50, 50), image::Rgba([40, 80, 120, 255]));
    }
}
