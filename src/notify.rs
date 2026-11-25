//! Sending alerts out of the app: desktop notifications (when the window isn't
//! in front), phone push through ntfy, and email through your mail provider.

use crate::model::{alert_info, Event, Settings, Severity, Shared};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub fn severity_rank(s: Severity) -> u8 {
    match s {
        Severity::Info => 0,
        Severity::Warning => 1,
        Severity::Critical => 2,
    }
}

/// Watch for new alerts and pass them on. Runs on its own thread.
pub fn start(shared: Arc<Mutex<Shared>>) {
    std::thread::spawn(move || {
        // Don't resend anything that happened before this start.
        let mut last_id = shared.lock().unwrap().events.iter().map(|e| e.id).max().unwrap_or(0);
        let mut sent_times: Vec<Instant> = vec![];
        loop {
            std::thread::sleep(Duration::from_secs(5));
            let (new, settings, window_active) = {
                let s = shared.lock().unwrap();
                if !s.persist {
                    continue; // test/screenshot runs never send anything
                }
                let new: Vec<Event> = s.events.iter().filter(|e| e.id > last_id && !e.muted && e.kind != "scan").cloned().collect();
                (new, s.settings.clone(), s.window_active)
            };
            if let Some(m) = new.iter().map(|e| e.id).max() {
                last_id = m;
            }
            if new.is_empty() {
                continue;
            }
            let quiet = settings.quiet_now();
            // Desktop: warnings and critical alerts, when the window isn't in front
            let desk: Vec<&Event> = new.iter().filter(|e| e.severity != Severity::Info).collect();
            if settings.notify_desktop && !window_active && !quiet && !desk.is_empty() {
                let (title, body) = summary(&desk);
                crate::background::desktop_notify(&title, &body);
            }
            // Phone / email: alerts at or above the chosen level
            if quiet && settings.quiet_remote {
                continue;
            }
            let min = settings.remote_min;
            let remote: Vec<&Event> = new.iter().filter(|e| severity_rank(e.severity) >= min).collect();
            if remote.is_empty() || !(settings.ntfy_enabled || settings.email_enabled) {
                continue;
            }
            // at most 20 messages an hour, so a bad night can't flood your phone
            sent_times.retain(|t| t.elapsed() < Duration::from_secs(3600));
            if sent_times.len() >= 20 {
                continue;
            }
            sent_times.push(Instant::now());
            let (title, body) = summary(&remote);
            let worst = remote.iter().map(|e| e.severity).max_by_key(|s| severity_rank(*s)).unwrap_or(Severity::Warning);
            if settings.ntfy_enabled {
                let _ = send_ntfy(&settings, &title, &body, worst);
            }
            if settings.email_enabled {
                let _ = send_email(&settings, &title, &format!("{}\n\n— HomeNet Watch by DDID", body));
            }
        }
    });
}

/// Title and text for one or more alerts.
fn summary(events: &[&Event]) -> (String, String) {
    if events.len() == 1 {
        let e = events[0];
        let info = alert_info(&e.kind);
        let title = if e.device_name == "System" { info.title.to_string() } else { format!("{}: {}", e.device_name, info.title) };
        let body = format!("{}\n{}", e.message, info.action);
        (title, body)
    } else {
        let title = format!("{} alerts from HomeNet Watch", events.len());
        let body = events.iter().take(10).map(|e| format!("• {} — {}", e.device_name, e.message)).collect::<Vec<_>>().join("\n");
        (title, body)
    }
}

/// Phone push via ntfy (free app for Android and iPhone, ntfy.sh).
pub fn send_ntfy(s: &Settings, title: &str, body: &str, sev: Severity) -> Result<(), String> {
    let topic = s.ntfy_topic.trim();
    if topic.is_empty() {
        return Err("Choose a topic name first.".into());
    }
    let server = s.ntfy_server.trim().trim_end_matches('/');
    let server = if server.is_empty() { "https://ntfy.sh" } else { server };
    let msg = serde_json::json!({
        "topic": topic,
        "title": title,
        "message": body,
        "priority": match sev { Severity::Critical => 5, Severity::Warning => 4, Severity::Info => 3 },
        "tags": [match sev { Severity::Critical => "rotating_light", Severity::Warning => "warning", Severity::Info => "information_source" }],
    });
    let bytes = serde_json::to_vec(&msg).unwrap_or_default();
    let r = crate::web::request(
        &format!("{}/", server),
        crate::web::Opts { method: "POST", body: Some(&bytes), content_type: Some("application/json"), timeout_ms: 15000, ..Default::default() },
    )?;
    if (200..300).contains(&r.status) {
        Ok(())
    } else {
        Err(format!("ntfy answered with status {}: {}", r.status, String::from_utf8_lossy(&r.body).chars().take(200).collect::<String>()))
    }
}

/// Email through your mail provider (SMTP with encryption), using the system's curl tool.
pub fn send_email(s: &Settings, subject: &str, body: &str) -> Result<(), String> {
    use std::io::Write;
    if s.smtp_host.trim().is_empty() || s.email_to.trim().is_empty() {
        return Err("Fill in the mail server and who to send to.".into());
    }
    let from = if s.email_from.trim().is_empty() { s.smtp_user.trim() } else { s.email_from.trim() };
    if from.is_empty() {
        return Err("Fill in the sending address.".into());
    }
    let host = s.smtp_host.trim();
    // port 465: encrypted from the start; others (587): upgraded with STARTTLS
    let url = if s.smtp_port == 465 { format!("smtps://{}:465", host) } else { format!("smtp://{}:{}", host, s.smtp_port) };
    let date = chrono::Local::now().to_rfc2822();
    let clean = |v: &str| v.replace(['\r', '\n'], " ");
    let msg = format!(
        "From: HomeNet Watch <{}>\r\nTo: {}\r\nSubject: {}\r\nDate: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{}\r\n",
        clean(from),
        clean(s.email_to.trim()),
        clean(subject),
        date,
        body.replace('\n', "\r\n")
    );
    let mut cmd = std::process::Command::new("curl");
    cmd.args(["-sS", "--ssl-reqd", "--max-time", "40", "--url", &url, "--mail-from", from]);
    for to in s.email_to.split([',', ';']).map(str::trim).filter(|t| !t.is_empty()) {
        cmd.args(["--mail-rcpt", to]);
    }
    if !s.smtp_user.trim().is_empty() {
        cmd.args(["--user", &format!("{}:{}", s.smtp_user.trim(), s.smtp_pass)]);
    }
    cmd.args(["-T", "-"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|_| "The curl tool wasn't found (it comes with Windows 10/11, macOS and most Linux).".to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(msg.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.contains("67") || err.to_lowercase().contains("login denied") || err.contains("535") {
            "The mail server refused the user name or password. Gmail, iCloud and Yahoo need an \"app password\", not your normal one.".into()
        } else if err.is_empty() {
            format!("Sending failed (curl exit {})", out.status.code().unwrap_or(-1))
        } else {
            format!("Sending failed: {}", err)
        })
    }
}

/// A fresh, hard-to-guess ntfy topic name (anyone who knows it can read your alerts).
pub fn new_topic() -> String {
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(7);
    let h = crate::sha256::hex(format!("{}{:?}", seed, std::thread::current().id()).as_bytes());
    format!("homenet-{}", &h[..14])
}

#[cfg(test)]
mod tests {
    use super::*;
    // Sends one generic test message to a random throwaway topic: cargo test -- --ignored
    #[test]
    #[ignore]
    fn ntfy_accepts_a_message() {
        let mut s = Settings::default();
        s.ntfy_topic = new_topic();
        println!("topic {}", s.ntfy_topic);
        let r = send_ntfy(&s, "HomeNet Watch test", "Test message from the HomeNet Watch build checks.", Severity::Info);
        println!("{:?}", r);
        assert!(r.is_ok());
    }

    #[test]
    fn topic_names_are_unguessable_looking() {
        let a = new_topic();
        assert!(a.starts_with("homenet-") && a.len() == 22);
    }

    #[test]
    fn email_needs_settings() {
        let s = Settings::default();
        assert!(send_email(&s, "x", "y").is_err());
    }
}
