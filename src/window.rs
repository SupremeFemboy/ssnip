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
            && id_a == id_b {
                return true;
            }

        if !self.name.is_empty() && !other.name.is_empty() && self.name == other.name {
            return true;
        }

        false
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
    #[serde(rename = "stableId", default)]
    stable_id: Option<String>,
}

fn default_mapped() -> bool {
    true
}

fn query_hyprland_socket(sock_path: &PathBuf, request: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut stream = UnixStream::connect(sock_path)?;
    stream.write_all(request.as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

fn query_hyprland(sock_path: &PathBuf, request: &str, cli_arg: &str) -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(res) = query_hyprland_socket(sock_path, request) {
        return Ok(res);
    }
    let output = Command::new("hyprctl").args(["-j", cli_arg]).output()?;
    Ok(String::from_utf8(output.stdout)?)
}

fn get_hyprland_windows(monitor_name: &str, scale: i32) -> Result<Vec<WindowInfo>, Box<dyn std::error::Error>> {
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

    // Sort: tiled windows first, floating windows last
    // This ensures reverse search hits floating windows first when they overlap tiled windows!
    clients.sort_by_key(|c| c.floating);

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

fn get_sway_windows(monitor_name: &str, scale: i32) -> Result<Vec<WindowInfo>, Box<dyn std::error::Error>> {
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
                let mon_x = output.get("rect").and_then(|r| r.get("x")).and_then(|v| v.as_i64()).unwrap_or(0);
                let mon_y = output.get("rect").and_then(|r| r.get("y")).and_then(|v| v.as_i64()).unwrap_or(0);

                collect_sway_windows(output, mon_x, mon_y, scale, &mut windows);
                break;
            }
        }
    }

    Ok(windows)
}

fn collect_sway_windows(node: &serde_json::Value, mon_x: i64, mon_y: i64, scale: i32, out: &mut Vec<WindowInfo>) {
    let is_floating = node.get("type").and_then(|t| t.as_str()) == Some("floating_con");
    if let Some(rect) = node.get("rect")
        && (node.get("type").and_then(|t| t.as_str()) == Some("con") || is_floating)
        && node.get("nodes").and_then(|n| n.as_array()).is_none_or(|a| a.is_empty())
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

#[allow(dead_code)]
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

/// Captures a single toplevel window in memory using grim
pub fn capture_toplevel_to_memory(
    identifier: &str,
    capture_cursor: bool,
) -> Option<image::RgbaImage> {
    let mut cmd = Command::new("grim");
    cmd.arg("-T").arg(identifier);

    if capture_cursor {
        cmd.arg("-c");
    }

    cmd.arg("-t").arg("png").arg("-");

    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let dyn_img = image::load_from_memory(&output.stdout).ok()?;
    Some(dyn_img.to_rgba8())
}

/// Captures a selected window live from the compositor.
/// If the window has translucent pixels (e.g. transparent terminal),
/// it is blended over the desktop background from `cap` so the real background shows through.
pub fn capture_window_live(
    win: &WindowInfo,
    output_x: i32,
    output_y: i32,
    scale: i32,
    capture_cursor: bool,
    cap: &crate::CapturedFrameData,
) -> Option<image::RgbaImage> {
    if let Some(ref stable_id) = win.stable_id
        && let Some(top_img) = capture_toplevel_to_memory(stable_id, capture_cursor)
    {
        let has_transparency = top_img.pixels().any(|p| p[3] < 255);
        if has_transparency {
            let mut composite = crate::render::crop_to_image(
                cap.shm_buffer.as_slice(),
                cap.width,
                cap.height,
                cap.stride,
                cap.format,
                cap.y_invert,
                win.rect.x,
                win.rect.y,
                top_img.width(),
                top_img.height(),
            );

            for py in 0..top_img.height() {
                for px in 0..top_img.width() {
                    let top_px = top_img.get_pixel(px, py);
                    if top_px[3] == 255 {
                        composite.put_pixel(px, py, *top_px);
                    } else if top_px[3] > 0 {
                        let bg_px = composite.get_pixel(px, py);
                        let a = top_px[3] as u32;
                        let inv_a = 255 - a;
                        let r = ((top_px[0] as u32 * a + bg_px[0] as u32 * inv_a) / 255) as u8;
                        let g = ((top_px[1] as u32 * a + bg_px[1] as u32 * inv_a) / 255) as u8;
                        let b = ((top_px[2] as u32 * a + bg_px[2] as u32 * inv_a) / 255) as u8;
                        composite.put_pixel(px, py, image::Rgba([r, g, b, 255]));
                    }
                }
            }
            return Some(composite);
        } else {
            return Some(top_img);
        }
    }

    capture_region_live(win.rect, output_x, output_y, scale, capture_cursor)
}

/// Captures a region live from the display via grim after the overlay has been closed.
pub fn capture_region_live(
    rect: Rect,
    output_x: i32,
    output_y: i32,
    scale: i32,
    capture_cursor: bool,
) -> Option<image::RgbaImage> {
    let scale_f = scale.max(1) as f64;
    let geom_x = output_x as f64 + (rect.x as f64 / scale_f);
    let geom_y = output_y as f64 + (rect.y as f64 / scale_f);
    let geom_w = rect.width as f64 / scale_f;
    let geom_h = rect.height as f64 / scale_f;

    let geom_str = format!(
        "{},{} {}x{}",
        geom_x.round() as i32,
        geom_y.round() as i32,
        geom_w.round().max(1.0) as u32,
        geom_h.round().max(1.0) as u32
    );

    let mut cmd = Command::new("grim");
    cmd.arg("-g").arg(geom_str);
    cmd.arg("-s").arg(scale.max(1).to_string());

    if capture_cursor {
        cmd.arg("-c");
    }

    cmd.arg("-t").arg("png").arg("-");

    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let dyn_img = image::load_from_memory(&output.stdout).ok()?;
    Some(dyn_img.to_rgba8())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hyprland_query() {
        if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
            let res = get_hyprland_windows("eDP-1", 1);
            assert!(res.is_ok(), "Query to Hyprland should succeed: {:?}", res.err());
        }
    }

    #[test]
    fn test_get_overlapping_windows() {
        let tiled = WindowInfo {
            rect: Rect { x: 0, y: 0, width: 1000, height: 1000 },
            stable_id: Some("1".into()),
            floating: false,
            z_index: 0,
        };

        let floating_on_top = WindowInfo {
            rect: Rect { x: 200, y: 200, width: 400, height: 400 },
            stable_id: Some("2".into()),
            floating: true,
            z_index: 1,
        };

        let floating_outside = WindowInfo {
            rect: Rect { x: 2000, y: 2000, width: 400, height: 400 },
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
}
