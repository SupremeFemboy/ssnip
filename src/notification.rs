use notify_rust::{Hint, Notification, Timeout};
use std::path::Path;

/// Sends a native desktop notification via D-Bus (org.freedesktop.Notifications).
pub fn send_screenshot_notification(path: &Path) {
    let filename = path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("Screenshot");

    let _ = Notification::new()
        .appname("ssnip")
        .summary("Screenshot saved")
        .body(filename)
        .icon(path.to_str().unwrap_or(""))
        .hint(Hint::Transient(true))
        .timeout(Timeout::Milliseconds(1000))
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_notification() {
        let path = Path::new("/tmp/test_shot.png");
        send_screenshot_notification(path);
    }
}
