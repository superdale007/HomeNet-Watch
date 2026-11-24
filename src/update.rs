//! In-app updates from GitHub Releases.
//!
//! Published builds are made by the GitHub workflow in `.github/workflows/release.yml`,
//! which stamps the repository name in at build time (HNW_UPDATE_REPO). The app asks
//! GitHub for the latest release, downloads the file for this system, checks its
//! SHA-256 fingerprint against the release's SHA256SUMS.txt, swaps it in for the
//! running program and restarts.

use std::sync::{Arc, Mutex};

/// "owner/repo" of the GitHub repository this build came from (empty for local builds).
pub fn repo() -> &'static str {
    option_env!("HNW_UPDATE_REPO").unwrap_or("")
}

/// The release file for this operating system.
pub fn asset_name() -> &'static str {
    if cfg!(windows) {
        "homenet-watch-windows-x86_64.exe"
    } else if cfg!(target_os = "macos") {
        "homenet-watch-macos-universal"
    } else if cfg!(target_arch = "aarch64") {
        "homenet-watch-linux-aarch64"
    } else {
        "homenet-watch-linux-x86_64"
    }
}

#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    pub version: (u32, u32, u32),
    pub notes: String,
    pub page: String,
    pub asset_url: Option<String>,
    pub asset_size: u64,
    pub sums_url: Option<String>,
}

#[derive(Clone, Default)]
pub struct UpdateState {
    pub checking: bool,
    pub checked: bool,
    pub latest: Option<Release>,
    pub newer: bool,
    pub error: Option<String>,
    pub installing: bool,
    pub progress: f32,
    pub installed: bool,
}

pub type UpdateRef = Arc<Mutex<UpdateState>>;

pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let core = s.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0)))
}

pub fn current() -> (u32, u32, u32) {
    parse_version(env!("CARGO_PKG_VERSION")).unwrap_or((0, 0, 0))
}

fn fetch_latest() -> Result<Release, String> {
    if repo().is_empty() {
        return Err("This copy was built locally, so it isn't linked to a GitHub repository yet.".into());
    }
    fetch_latest_from(repo())
}

fn fetch_latest_from(repo: &str) -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let r = crate::web::request(&url, crate::web::Opts { method: "GET", timeout_ms: 15000, ..Default::default() })?;
    if r.status == 404 {
        return Err("No published versions yet.".into());
    }
    if r.status != 200 {
        return Err(format!("GitHub answered with status {}", r.status));
    }
    let v: serde_json::Value = serde_json::from_slice(&r.body).map_err(|_| "GitHub's answer couldn't be read".to_string())?;
    let tag = v.get("tag_name").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let version = parse_version(&tag).ok_or_else(|| format!("The latest release tag \"{}\" isn't a version number", tag))?;
    let assets = v.get("assets").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let find = |name: &str| {
        assets
            .iter()
            .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(name))
            .map(|a| (a.get("browser_download_url").and_then(|u| u.as_str()).unwrap_or("").to_string(), a.get("size").and_then(|s| s.as_u64()).unwrap_or(0)))
    };
    let asset = find(asset_name());
    Ok(Release {
        tag,
        version,
        notes: v.get("body").and_then(|x| x.as_str()).unwrap_or("").trim().to_string(),
        page: v.get("html_url").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        asset_url: asset.as_ref().map(|a| a.0.clone()),
        asset_size: asset.map(|a| a.1).unwrap_or(0),
        sums_url: find("SHA256SUMS.txt").map(|a| a.0),
    })
}

/// Ask GitHub for the newest version (in the background).
pub fn check(state: UpdateRef, ctx: eframe::egui::Context) {
    {
        let mut s = state.lock().unwrap();
        if s.checking || s.installing {
            return;
        }
        s.checking = true;
        s.error = None;
    }
    std::thread::spawn(move || {
        let r = fetch_latest();
        let mut s = state.lock().unwrap();
        s.checking = false;
        s.checked = true;
        match r {
            Ok(rel) => {
                s.newer = rel.version > current();
                s.latest = Some(rel);
            }
            Err(e) => s.error = Some(e),
        }
        drop(s);
        ctx.request_repaint();
    });
}

/// Download, verify and swap in the new version (in the background).
pub fn install(state: UpdateRef, ctx: eframe::egui::Context) {
    let rel = {
        let mut s = state.lock().unwrap();
        if s.installing {
            return;
        }
        let Some(rel) = s.latest.clone() else { return };
        s.installing = true;
        s.progress = 0.0;
        s.error = None;
        rel
    };
    std::thread::spawn(move || {
        let r = download_and_replace(&rel, &state, &ctx);
        let mut s = state.lock().unwrap();
        s.installing = false;
        match r {
            Ok(()) => s.installed = true,
            Err(e) => s.error = Some(e),
        }
        drop(s);
        ctx.request_repaint();
    });
}

fn download_and_replace(rel: &Release, state: &UpdateRef, ctx: &eframe::egui::Context) -> Result<(), String> {
    let url = rel.asset_url.clone().ok_or_else(|| format!("This release has no file for your system ({}).", asset_name()))?;
    let sums_url = rel.sums_url.clone().ok_or("This release has no SHA256SUMS.txt, so the download can't be checked. Not installing.")?;
    // Expected fingerprint
    let sums = crate::web::get(&sums_url, 20000)?;
    let sums = String::from_utf8_lossy(&sums.body).to_string();
    let expected = sums
        .lines()
        .find_map(|l| {
            let mut it = l.split_whitespace();
            let (h, n) = (it.next()?, it.next()?);
            (n.trim_start_matches('*') == asset_name()).then(|| h.to_lowercase())
        })
        .ok_or("The checksum file doesn't list this system's download.")?;
    // Download
    let total = rel.asset_size.max(1) as f32;
    let mut cb = |n: u64| {
        if let Ok(mut s) = state.try_lock() {
            s.progress = (n as f32 / total).min(0.99);
        }
        ctx.request_repaint();
        true
    };
    let r = crate::web::request(&url, crate::web::Opts { method: "GET", timeout_ms: 60000, on_bytes: Some(&mut cb), ..Default::default() })?;
    if r.status != 200 || r.body.is_empty() {
        return Err(format!("The download failed (status {}).", r.status));
    }
    if crate::sha256::hex(&r.body) != expected {
        return Err("The download's fingerprint doesn't match the published one. It may be damaged or tampered with, so it wasn't installed.".into());
    }
    replace_running(&r.body)
}

fn replace_running(bytes: &[u8]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    replace_file(&exe, bytes)
}

/// Put `bytes` in place of the program file at `exe`, keeping the old one aside (Windows).
fn replace_file(exe: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let exe = exe.to_path_buf();
    let dir = exe.parent().ok_or("can't find the program folder")?;
    let new = dir.join(if cfg!(windows) { "homenet-watch.new.exe" } else { ".homenet-watch.new" });
    let protected = |e: std::io::Error| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            "The program is in a protected folder, so it can't replace itself. Download the new version from the release page instead.".to_string()
        } else {
            e.to_string()
        }
    };
    std::fs::write(&new, bytes).map_err(protected)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755));
    }
    #[cfg(windows)]
    {
        // A running program can't be overwritten on Windows, but it can be renamed.
        let old = dir.join("homenet-watch.old.exe");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old).map_err(protected)?;
        if let Err(e) = std::fs::rename(&new, &exe) {
            let _ = std::fs::rename(&old, &exe);
            return Err(protected(e));
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(&new, &exe).map_err(protected)?;
    Ok(())
}

/// Start the new version and close this one.
pub fn restart() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
    std::process::exit(0);
}

/// Remove the previous version left behind by an update (Windows).
pub fn clean_up_old() {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let _ = std::fs::remove_file(dir.join("homenet-watch.old.exe"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.7.0"), Some((0, 7, 0)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("v2.0.1-beta"), Some((2, 0, 1)));
        assert_eq!(parse_version("latest"), None);
        assert!(parse_version("v0.10.0").unwrap() > parse_version("v0.9.9").unwrap());
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    /// Reads a real public release (needs the internet): cargo test -- --ignored
    #[test]
    #[ignore]
    fn reads_a_real_github_release() {
        let r = fetch_latest_from("BurntSushi/ripgrep").expect("release");
        println!("tag={} version={:?} page={} notes={}…", r.tag, r.version, r.page, r.notes.chars().take(60).collect::<String>());
        assert!(r.version.0 >= 14);
        assert!(r.page.starts_with("https://github.com/"));
    }

    #[test]
    fn swaps_a_program_file() {
        let dir = std::env::temp_dir().join(format!("hnw-update-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(if cfg!(windows) { "homenet-watch.exe" } else { "homenet-watch" });
        std::fs::write(&exe, b"old version").unwrap();
        replace_file(&exe, b"new version").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new version");
        #[cfg(windows)]
        assert_eq!(std::fs::read(dir.join("homenet-watch.old.exe")).unwrap(), b"old version");
        // a second update replaces the leftover old file too
        replace_file(&exe, b"newer version").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"newer version");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
