//! Small helpers that differ per operating system.

use std::path::PathBuf;

/// Is this program file HomeNet Watch itself?
pub fn is_self(path: &str) -> bool {
    static SELF: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let me = SELF.get_or_init(|| std::env::current_exe().map(|p| norm(&p.to_string_lossy())).unwrap_or_default());
    if path.is_empty() || me.is_empty() {
        return false;
    }
    let p = norm(path);
    if p == *me {
        return true;
    }
    // Another build of HomeNet Watch from the same project (e.g. a test build next to this one).
    let file = |s: &str| s.rsplit('/').next().unwrap_or("").to_string();
    let project = |s: &str| s.find("/target/").map(|i| s[..i].to_string());
    file(&p) == file(me) && project(&p).is_some() && project(&p) == project(me)
}

fn norm(p: &str) -> String {
    let p = p.replace('\\', "/");
    let p = p.strip_prefix("//?/").unwrap_or(&p).to_string();
    if cfg!(windows) || cfg!(target_os = "macos") {
        p.to_lowercase()
    } else {
        p
    }
}

/// The person's Documents folder (falls back to the app's data folder).
pub fn documents_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
    let docs = home.map(|h| h.join("Documents"));
    match docs {
        Some(d) if d.exists() => d,
        _ => crate::model::data_dir(),
    }
}

/// Open a file or web address with the system's default program.
pub fn open(target: &str) {
    #[cfg(windows)]
    let r = std::process::Command::new("explorer").arg(target).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(target).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(target).spawn();
    let _ = r;
}

/// Restart this app with administrator rights (Windows asks for permission first).
#[cfg(windows)]
pub fn restart_as_admin() -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    let Ok(exe) = std::env::current_exe() else { return false };
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let exe = w(&exe.to_string_lossy());
    let verb = w("runas");
    let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), exe.as_ptr(), std::ptr::null(), std::ptr::null(), 1) };
    r as isize > 32
}
