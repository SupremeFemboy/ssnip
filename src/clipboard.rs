use std::fs::File;
use std::path::Path;
use std::process::{Command, Stdio};
use wl_clipboard_rs::copy::{MimeType, Options, Source};

/// Copies PNG image to the Wayland clipboard strictly with MIME type `image/png`.
pub fn copy_png_to_clipboard(png_bytes: &[u8], path: &Path) {
    let is_png = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("png"))
        .unwrap_or(false);

    let temp_png_path = if is_png {
        None
    } else {
        let temp_path = std::env::temp_dir().join("ssnip_clipboard.png");
        if std::fs::write(&temp_path, png_bytes).is_ok() {
            Some(temp_path)
        } else {
            None
        }
    };

    let copy_target = temp_png_path.as_deref().unwrap_or(path);

    // 1. Try standard wl-copy if available
    if let Ok(file) = File::open(copy_target)
        && let Ok(mut child) = Command::new("wl-copy")
            .arg("-t")
            .arg("image/png")
            .stdin(Stdio::from(file))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    {
        let _ = child.wait();
        return;
    }

    // 2. Fallback: native worker using wl-clipboard-rs offering strictly image/png
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe)
            .arg("--serve-clipboard")
            .arg(copy_target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

/// Worker loop serving the clipboard in foreground with strictly `image/png`.
pub fn serve_clipboard_loop(path: &Path) {
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };

    let mut opts = Options::new();
    opts.foreground(true);

    if let Ok(prepared) = opts.prepare_copy(
        Source::Bytes(bytes.into()),
        MimeType::Specific("image/png".to_string()),
    ) {
        let _ = prepared.serve();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_options() {
        let mut opts = Options::new();
        opts.foreground(true);
    }
}
