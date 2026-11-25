//! Running in the background: one copy at a time (a second start just brings
//! the window back), hiding/showing the window, starting with the computer,
//! and desktop notifications.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::time::Duration;

const PORT: u16 = 47613;

/// If HomeNet Watch is already running, ask it to show its window. Returns true if it was.
pub fn signal_existing() -> bool {
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, PORT);
    let Ok(mut s) = TcpStream::connect_timeout(&addr.into(), Duration::from_millis(400)) else { return false };
    let _ = s.set_read_timeout(Some(Duration::from_millis(800)));
    if s.write_all(b"HNW-SHOW\n").is_err() {
        return false;
    }
    let mut buf = [0u8; 8];
    matches!(s.read(&mut buf), Ok(n) if n >= 2 && &buf[..2] == b"OK")
}

/// Listen for "show yourself" requests from a second start.
pub fn listen(on_show: impl Fn() + Send + 'static) {
    let Ok(l) = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, PORT)) else { return };
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            let mut c = c;
            let _ = c.set_read_timeout(Some(Duration::from_millis(500)));
            let mut buf = [0u8; 16];
            if let Ok(n) = c.read(&mut buf) {
                if buf[..n].starts_with(b"HNW-SHOW") {
                    let _ = c.write_all(b"OK");
                    on_show();
                }
            }
        }
    });
}

// ───────────────────────────── window ─────────────────────────────

#[cfg(windows)]
static HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Remember the main window (Windows), so it can be shown from the tray at any time.
pub fn remember_window(cc: &eframe::CreationContext<'_>) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(h) = cc.window_handle() {
            if let RawWindowHandle::Win32(w) = h.as_raw() {
                HWND.store(w.hwnd.get(), std::sync::atomic::Ordering::SeqCst);
            }
        }
    }
    #[cfg(not(windows))]
    let _ = cc;
}

/// Hide the window but keep running. Windows: fully hidden (the tray icon brings it back).
/// Mac and Linux: minimised to the Dock / taskbar.
pub fn hide_window(ctx: &eframe::egui::Context) {
    #[cfg(windows)]
    {
        let h = HWND.load(std::sync::atomic::Ordering::SeqCst);
        if h != 0 {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(h as _, windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE);
            }
            return;
        }
    }
    ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Minimized(true));
}

pub fn show_window(ctx: &eframe::egui::Context) {
    #[cfg(windows)]
    {
        let h = HWND.load(std::sync::atomic::Ordering::SeqCst);
        if h != 0 {
            use windows_sys::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW};
            unsafe {
                ShowWindow(h as _, SW_SHOW);
                ShowWindow(h as _, SW_RESTORE);
                SetForegroundWindow(h as _);
            }
        }
    }
    ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

// ───────────────────────────── start with the computer ─────────────────────────────

fn exe() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

#[cfg(windows)]
mod autostart {
    use windows_sys::Win32::System::Registry::*;
    const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const NAME: &str = "HomeNet Watch";
    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
    pub fn set(on: bool) -> Result<(), String> {
        unsafe {
            let (k, n) = (w(KEY), w(NAME));
            let rc = if on {
                let v = w(&format!("\"{}\" --background", super::exe()));
                RegSetKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr(), REG_SZ, v.as_ptr() as *const _, (v.len() * 2) as u32)
            } else {
                let rc = RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr());
                if rc == 2 {
                    0 // already gone
                } else {
                    rc
                }
            };
            if rc == 0 {
                Ok(())
            } else {
                Err(format!("Windows refused (error {})", rc))
            }
        }
    }
    /// The command saved for sign-in, if any.
    pub fn command() -> Option<String> {
        unsafe {
            let (k, n) = (w(KEY), w(NAME));
            let mut buf = vec![0u16; 2048];
            let mut size: u32 = (buf.len() * 2) as u32;
            let rc = RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr() as *mut _, &mut size);
            if rc != 0 {
                return None;
            }
            let len = (size as usize / 2).saturating_sub(1);
            Some(String::from_utf16_lossy(&buf[..len]))
        }
    }
    pub fn get() -> bool {
        unsafe {
            let (k, n) = (w(KEY), w(NAME));
            let mut size: u32 = 0;
            RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), &mut size) == 0
        }
    }
}

#[cfg(target_os = "macos")]
mod autostart {
    fn path() -> Option<std::path::PathBuf> {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Library/LaunchAgents/com.daledumbsitdown.homenetwatch.plist"))
    }
    pub fn set(on: bool) -> Result<(), String> {
        let p = path().ok_or("no home folder")?;
        if !on {
            let _ = std::fs::remove_file(&p);
            return Ok(());
        }
        let esc = super::exe().replace('&', "&amp;").replace('<', "&lt;");
        let plist = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>com.daledumbsitdown.homenetwatch</string>\n<key>ProgramArguments</key><array><string>{}</string><string>--background</string></array>\n<key>RunAtLoad</key><true/>\n</dict></plist>\n",
            esc
        );
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, plist).map_err(|e| e.to_string())
    }
    pub fn command() -> Option<String> {
        path().and_then(|p| std::fs::read_to_string(p).ok())
    }
    pub fn get() -> bool {
        path().map(|p| p.exists()).unwrap_or(false)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod autostart {
    fn path() -> Option<std::path::PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
        Some(base.join("autostart/homenet-watch.desktop"))
    }
    pub fn set(on: bool) -> Result<(), String> {
        let p = path().ok_or("no home folder")?;
        if !on {
            let _ = std::fs::remove_file(&p);
            return Ok(());
        }
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=HomeNet Watch\nComment=Home network monitor by DDID\nExec=\"{}\" --background\nX-GNOME-Autostart-enabled=true\n",
            super::exe()
        );
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, entry).map_err(|e| e.to_string())
    }
    pub fn command() -> Option<String> {
        path().and_then(|p| std::fs::read_to_string(p).ok())
    }
    pub fn get() -> bool {
        path().map(|p| p.exists()).unwrap_or(false)
    }
}

pub fn set_autostart(on: bool) -> Result<(), String> {
    autostart::set(on)
}

pub fn autostart_enabled() -> bool {
    autostart::get()
}

// ───────────────────────────── desktop notifications ─────────────────────────────

/// Show a notification from the system (works while the window is hidden).
pub fn desktop_notify(title: &str, body: &str) {
    let (title, body) = (title.to_string(), body.to_string());
    std::thread::spawn(move || {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Windows' own notification system, through the built-in PowerShell (text passed safely via environment).
            let script = "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; \
                $x = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
                $t = $x.GetElementsByTagName('text'); \
                $t.Item(0).AppendChild($x.CreateTextNode($env:HNW_TITLE)) | Out-Null; \
                $t.Item(1).AppendChild($x.CreateTextNode($env:HNW_BODY)) | Out-Null; \
                $n = [Windows.UI.Notifications.ToastNotification]::new($x); \
                [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe').Show($n)";
            let _ = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
                .env("HNW_TITLE", &title)
                .env("HNW_BODY", &body)
                .creation_flags(0x0800_0000) // no console window
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("osascript")
                .args(["-e", "on run argv", "-e", "display notification (item 2 of argv) with title (item 1 of argv)", "-e", "end run", &title, &body])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let _ = std::process::Command::new("notify-send")
                .args(["--app-name=HomeNet Watch", &title, &body])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    });
}

/// 32×32 picture for the icon by the clock: the gold tile with the pulse line.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub fn icon_rgba() -> (Vec<u8>, u32, u32) {
    let (w, h) = (32u32, 32u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    let gold = [0xE9, 0xB6, 0x3B, 0xFF];
    let dark = [0x11, 0x19, 0x26, 0xFF];
    let r = 6.0f32;
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // rounded square
            let cx = fx.clamp(r, w as f32 - r);
            let cy = fy.clamp(r, h as f32 - r);
            let inside = (fx - cx).powi(2) + (fy - cy).powi(2) <= r * r;
            if inside {
                let i = ((y * w + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&gold);
            }
        }
    }
    // roof and pulse line
    let pts_roof = [(6.0, 15.0), (16.0, 6.0), (26.0, 15.0)];
    let pts_pulse = [(5.0, 21.0), (11.0, 21.0), (14.0, 16.0), (18.0, 27.0), (21.0, 21.0), (27.0, 21.0)];
    for line in [&pts_roof[..], &pts_pulse[..]] {
        for seg in line.windows(2) {
            let ((x0, y0), (x1, y1)): ((f32, f32), (f32, f32)) = (seg[0], seg[1]);
            let steps = 60;
            for k in 0..=steps {
                let t = k as f32 / steps as f32;
                let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (px_, py_) = (x as i32 + dx, y as i32 + dy);
                        if px_ >= 0 && py_ >= 0 && (px_ as u32) < w && (py_ as u32) < h && (dx * dx + dy * dy) <= 1 {
                            let i = ((py_ as u32 * w + px_ as u32) * 4) as usize;
                            px[i..i + 4].copy_from_slice(&dark);
                        }
                    }
                }
            }
        }
    }
    (px, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn second_start_reaches_the_first() {
        let shown = Arc::new(AtomicBool::new(false));
        let s2 = shown.clone();
        listen(move || s2.store(true, Ordering::SeqCst));
        std::thread::sleep(Duration::from_millis(200));
        assert!(signal_existing(), "the running copy should answer");
        std::thread::sleep(Duration::from_millis(200));
        assert!(shown.load(Ordering::SeqCst), "the running copy should be asked to show its window");
    }

    #[test]
    fn tray_picture_is_32_square() {
        let (px, w, h) = icon_rgba();
        assert_eq!((w, h), (32, 32));
        assert_eq!(px.len(), 32 * 32 * 4);
    }
}

/// Start-with-computer status: None = off; Some(true) = starts this copy; Some(false) = points at another (old) copy.
pub fn autostart_points_here() -> Option<bool> {
    let cmd = autostart::command()?;
    let me = exe();
    Some(!me.is_empty() && cmd.to_lowercase().contains(&me.to_lowercase()))
}
