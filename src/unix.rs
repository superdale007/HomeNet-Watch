//! Linux and macOS versions of the low-level network probes. Each uses what the
//! system already has: the `ping`, `route` and `arp` tools, /proc on Linux,
//! and the C library's name lookup (which also covers mDNS ".local" names on macOS).

use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[allow(dead_code)]
fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// The router address.
pub fn default_gateway() -> Option<Ipv4Addr> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/net/route").ok()?;
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() > 2 && f[1] == "00000000" {
                let v = u32::from_str_radix(f[2], 16).ok()?;
                if v != 0 {
                    return Some(Ipv4Addr::from(v.to_le_bytes()));
                }
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        // macOS / BSD: "route -n get default" prints "gateway: 192.168.1.1"
        let text = run("route", &["-n", "get", "default"])?;
        text.lines()
            .find_map(|l| l.trim().strip_prefix("gateway:"))
            .and_then(|g| g.trim().parse().ok())
    }
}

/// One ping using the system's ping tool. Returns the round trip in ms.
pub fn ping(ip: Ipv4Addr, timeout_ms: u32) -> Option<f32> {
    let ip_s = ip.to_string();
    // Linux ping waits in whole seconds; macOS ping waits in milliseconds.
    #[cfg(target_os = "linux")]
    let wait = format!("{}", ((timeout_ms + 999) / 1000).max(1));
    #[cfg(not(target_os = "linux"))]
    let wait = format!("{}", timeout_ms.max(100));
    let start = Instant::now();
    let out = Command::new("ping")
        .args(["-n", "-c", "1", "-W", &wait, &ip_s])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if let Some(i) = text.find("time=") {
        let num: String = text[i + 5..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        if let Ok(v) = num.parse::<f32>() {
            return Some(v.max(0.1));
        }
    }
    Some(start.elapsed().as_secs_f32() * 1000.0)
}

/// Nudge the system into asking for a device's hardware address: sending any
/// packet to it makes the system resolve it on the home network first.
fn poke(ip: Ipv4Addr) {
    if let Ok(s) = UdpSocket::bind("0.0.0.0:0") {
        let _ = s.send_to(&[0u8], SocketAddrV4::new(ip, 9));
    }
}

fn normalize_mac(m: &str) -> Option<String> {
    let parts: Vec<u8> = m.split([':', '-']).filter_map(|p| u8::from_str_radix(p, 16).ok()).collect();
    if parts.len() == 6 && parts.iter().any(|b| *b != 0) {
        Some(crate::net::format_mac(&parts))
    } else {
        None
    }
}

fn lookup_mac(ip: Ipv4Addr) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/net/arp").ok()?;
        let s = ip.to_string();
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            // columns: IP, HW type, Flags (0x0 = not resolved), HW address, ...
            if f.len() > 3 && f[0] == s && f[2] != "0x0" {
                return normalize_mac(f[3]);
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        // "? (192.168.1.20) at 0:11:32:a:b:c on en0 ifscope [ethernet]"
        let text = run("arp", &["-n", &ip.to_string()])?;
        let at = text.split(" at ").nth(1)?;
        let mac = at.split_whitespace().next()?;
        normalize_mac(mac)
    }
}

/// Hardware (MAC) address of a device on the home network.
pub fn arp_mac(ip: Ipv4Addr) -> Option<String> {
    if let Some(m) = lookup_mac(ip) {
        return Some(m);
    }
    poke(ip);
    for _ in 0..4 {
        std::thread::sleep(Duration::from_millis(150));
        if let Some(m) = lookup_mac(ip) {
            return Some(m);
        }
    }
    None
}

/// Reverse name lookup through the C library.
pub fn reverse_name(ip: Ipv4Addr) -> Option<String> {
    unsafe {
        let mut sa: libc::sockaddr_in = std::mem::zeroed();
        sa.sin_family = libc::AF_INET as _;
        sa.sin_addr.s_addr = u32::from_ne_bytes(ip.octets());
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
        {
            sa.sin_len = std::mem::size_of::<libc::sockaddr_in>() as u8;
        }
        let mut host = [0 as libc::c_char; 256];
        let r = libc::getnameinfo(
            &sa as *const libc::sockaddr_in as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
            host.as_mut_ptr(),
            host.len() as _,
            std::ptr::null_mut(),
            0,
            libc::NI_NAMEREQD,
        );
        if r != 0 {
            return None;
        }
        let name = std::ffi::CStr::from_ptr(host.as_ptr()).to_string_lossy().to_string();
        Some(name)
    }
}

/// Play a short warning sound with whatever the system offers.
pub fn beep() {
    std::thread::spawn(|| {
        #[cfg(target_os = "macos")]
        let tries: &[(&str, &[&str])] = &[("afplay", &["/System/Library/Sounds/Funk.aiff"])];
        #[cfg(not(target_os = "macos"))]
        let tries: &[(&str, &[&str])] = &[
            ("canberra-gtk-play", &["-i", "dialog-warning"]),
            ("paplay", &["/usr/share/sounds/freedesktop/stereo/dialog-warning.oga"]),
            ("aplay", &["-q", "/usr/share/sounds/alsa/Front_Center.wav"]),
        ];
        for (cmd, args) in tries {
            let ok = Command::new(cmd).args(*args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
            if ok {
                return;
            }
        }
    });
}

/// Web request through the `curl` tool (present on macOS and nearly every Linux).
pub fn curl(url: &str, o: crate::web::Opts) -> Result<crate::web::Response, String> {
    use std::io::{Read, Write};
    let mut o = o;
    let secs = format!("{}", ((if o.timeout_ms > 0 { o.timeout_ms } else { 15000 }) as f32 / 1000.0).max(1.0));
    let method = if o.method.is_empty() { "GET" } else { o.method };
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "-L", "--max-time", &secs, "-A", "HomeNetWatch/1.0", "-X", method, "-o", "-", "-w", "%{stderr}HNWSTATUS:%{http_code}"]);
    if let Some(ct) = o.content_type {
        cmd.args(["-H", &format!("Content-Type: {}", ct)]);
    }
    if o.body.is_some() {
        cmd.args(["--data-binary", "@-"]);
    }
    cmd.arg(url).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|_| "the curl tool isn't installed".to_string())?;
    if let Some(body) = o.body {
        let mut stdin = child.stdin.take().ok_or("curl input")?;
        let body = body.to_vec();
        std::thread::spawn(move || {
            let _ = stdin.write_all(&body);
        });
    } else {
        drop(child.stdin.take());
    }
    let mut out = child.stdout.take().ok_or("curl output")?;
    let mut body = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    let mut stopped = false;
    loop {
        let n = out.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if !o.discard {
            body.extend_from_slice(&buf[..n]);
        }
        if let Some(cb) = o.on_bytes.as_mut() {
            if !cb(total) {
                stopped = true;
                let _ = child.kill();
                break;
            }
        }
    }
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut err);
    }
    let _ = child.wait();
    let status = err.rsplit("HNWSTATUS:").next().and_then(|s| s.trim().parse::<u16>().ok()).unwrap_or(0);
    if status == 0 && !stopped {
        let msg = err.split("HNWSTATUS:").next().unwrap_or("").trim().to_string();
        return Err(if msg.is_empty() { "no answer".into() } else { msg });
    }
    Ok(crate::web::Response { status, body, bytes: total })
}
