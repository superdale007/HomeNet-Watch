//! Settings → Checkup: one place to try the things only a person can confirm
//! (icon by the clock, starting with the computer, phone and email alerts,
//! SNMP v3 sign-in, and a GitHub release for in-app updates), with plain results.

use super::*;
use std::collections::HashMap;

#[derive(Clone, Default)]
pub(crate) struct CheckResult {
    pub running: bool,
    pub ok: bool,
    pub text: String,
}

pub(crate) type Checks = Arc<Mutex<HashMap<String, CheckResult>>>;

fn start(checks: &Checks, key: &str, work: impl FnOnce() -> Result<String, String> + Send + 'static, ctx: egui::Context) {
    let key = key.to_string();
    checks.lock().unwrap().insert(key.clone(), CheckResult { running: true, ok: false, text: "Checking…".into() });
    let checks = checks.clone();
    std::thread::spawn(move || {
        let r = work();
        checks.lock().unwrap().insert(key, CheckResult { running: false, ok: r.is_ok(), text: r.unwrap_or_else(|e| e) });
        ctx.request_repaint();
    });
}

fn row_head(ui: &mut Ui, title: &str, state: Option<bool>, status: &str) {
    ui.horizontal(|ui| {
        let (c, icon) = match state {
            Some(true) => (GREEN, "✔"),
            Some(false) => (AMBER, "⚠"),
            None => (FAINT, "•"),
        };
        ui.label(RichText::new(icon).color(c).strong());
        ui.label(RichText::new(title).strong());
        ui.label(RichText::new(status).color(c));
    });
}

fn show_result(ui: &mut Ui, checks: &Checks, key: &str) {
    if let Some(r) = checks.lock().unwrap().get(key).cloned() {
        ui.horizontal_wrapped(|ui| {
            if r.running {
                ui.spinner();
                ui.label(RichText::new(&r.text).color(MUTED));
            } else {
                ui.label(RichText::new(if r.ok { "✔" } else { "✖" }).color(if r.ok { GREEN } else { RED }));
                ui.label(RichText::new(&r.text).color(if r.ok { GREEN } else { RED }));
            }
        });
    }
}

/// Try to sign in to a device's SNMP and describe what it reports.
pub(crate) fn snmp_test(ip: std::net::Ipv4Addr, sec: crate::snmp::Security) -> Result<String, String> {
    let (p, _) = crate::perf::collect(ip, &sec, &Default::default())?;
    let what = match (p.vendor.label(), p.model.as_str()) {
        ("", _) => p.sys_name.clone(),
        (v, "") => v.to_string(),
        (v, m) => format!("{} {}", v, m),
    };
    Ok(format!(
        "Signed in. It reports: {} · {} volume{} · {} disk{}{}",
        if what.is_empty() { "a device" } else { &what },
        p.volumes.len(),
        if p.volumes.len() == 1 { "" } else { "s" },
        p.disks.len(),
        if p.disks.len() == 1 { "" } else { "s" },
        p.temp_c.map(|t| format!(" · {:.0}°C", t)).unwrap_or_default()
    ))
}

impl App {
    pub(super) fn checkup(&mut self, ui: &mut Ui, s: &mut Shared) {
        let ctx = ui.ctx().clone();
        let checks = self.checks.clone();
        egui::CollapsingHeader::new(RichText::new("CHECKUP: MAKE SURE EVERYTHING WORKS").size(13.0).color(MUTED).strong())
            .id_salt("checkup")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(RichText::new("Try each one; the result appears right under it.").size(12.5).color(MUTED));
                ui.add_space(8.0);

                // 1. Icon by the clock
                #[cfg(any(windows, target_os = "macos"))]
                {
                    let ok = self.tray.is_some();
                    row_head(ui, "Icon by the clock", Some(ok), if ok { "It's there." } else { "It couldn't be created on this computer." });
                    if ok {
                        ui.label(
                            RichText::new(if cfg!(windows) {
                                "Look for the gold HomeNet Watch icon by the clock (it may be behind the ^ arrow; drag it out to keep it visible). Click it to open the window; right-click it for Open, Pause and Quit."
                            } else {
                                "Look for the gold HomeNet Watch icon in the menu bar. Click it for Open, Pause and Quit."
                            })
                            .size(12.5)
                            .color(MUTED),
                        );
                        if !s.settings.keep_running {
                            ui.label(RichText::new("\"Keep watching when I close the window\" is off, so closing the window quits instead of hiding.").size(12.5).color(AMBER));
                        }
                        if ui.button("Hide the window now, then bring it back from the icon").clicked() {
                            crate::background::hide_window(&ctx);
                            s.window_active = false;
                        }
                    }
                }
                #[cfg(not(any(windows, target_os = "macos")))]
                row_head(ui, "Icon by the clock", None, "Not available on Linux: closing the window minimises it to the taskbar instead.");
                ui.add_space(10.0);

                // 2. Start with the computer
                match crate::background::autostart_points_here() {
                    None => {
                        row_head(ui, "Start with the computer", None, "Off.");
                        if ui.button("Turn it on").clicked() {
                            match crate::background::set_autostart(true) {
                                Ok(()) => self.autostart = true,
                                Err(e) => self.toast = Some((format!("Couldn't turn it on: {}", e), Instant::now())),
                            }
                        }
                    }
                    Some(true) => {
                        row_head(ui, "Start with the computer", Some(true), "On: this copy starts, hidden, when you sign in.");
                        ui.label(RichText::new("To try it: sign out (or restart) and sign back in. HomeNet Watch starts by itself; its icon appears by the clock, but no window opens.").size(12.5).color(MUTED));
                        if ui.button("Turn it off").clicked() {
                            if crate::background::set_autostart(false).is_ok() {
                                self.autostart = false;
                            }
                        }
                    }
                    Some(false) => {
                        row_head(ui, "Start with the computer", Some(false), "On, but it starts a different copy (an older or moved file).");
                        if ui.button("Fix it: start this copy instead").clicked() {
                            match crate::background::set_autostart(true) {
                                Ok(()) => self.toast = Some(("Fixed: this copy now starts when you sign in.".into(), Instant::now())),
                                Err(e) => self.toast = Some((format!("Couldn't fix it: {}", e), Instant::now())),
                            }
                        }
                    }
                }
                ui.add_space(10.0);

                // 3. Phone
                if s.settings.ntfy_enabled && !s.settings.ntfy_topic.trim().is_empty() {
                    row_head(ui, "Phone alerts (ntfy)", Some(true), &format!("Topic {}", s.settings.ntfy_topic.trim()));
                    if ui.button("Send a test to my phone").clicked() {
                        let st = s.settings.clone();
                        start(&checks, "ntfy", move || crate::notify::send_ntfy(&st, "HomeNet Watch test", "If you can read this, alerts will reach your phone.", Severity::Info).map(|_| "Sent. It should appear on your phone within a few seconds; if it doesn't, check the topic in the ntfy app matches exactly.".into()), ctx.clone());
                    }
                    show_result(ui, &checks, "ntfy");
                } else {
                    row_head(ui, "Phone alerts (ntfy)", None, "Not set up yet: see Phone & email alerts further down.");
                }
                ui.add_space(10.0);

                // 4. Email
                if s.settings.email_enabled {
                    row_head(ui, "Email alerts", Some(true), &format!("To {}", if s.settings.email_to.trim().is_empty() { "(nobody yet)" } else { s.settings.email_to.trim() }));
                    if ui.button("Send a test email").clicked() {
                        let st = s.settings.clone();
                        start(&checks, "email", move || crate::notify::send_email(&st, "HomeNet Watch test", "If you can read this, alerts will reach you by email.").map(|_| "Sent. Check your inbox (and the spam folder the first time).".into()), ctx.clone());
                    }
                    show_result(ui, &checks, "email");
                } else {
                    row_head(ui, "Email alerts", None, "Not set up yet: see Phone & email alerts further down.");
                }
                ui.add_space(10.0);

                // 5. SNMP v3
                let v3: Vec<(u64, String, std::net::Ipv4Addr, crate::snmp::Security, Option<String>, bool)> = s
                    .devices
                    .iter()
                    .filter(|d| d.snmp && d.snmp_version == 3)
                    .map(|d| (d.id, d.name.clone(), d.ip, d.snmp_security(), d.perf_error.clone(), d.perf.is_some()))
                    .collect();
                if v3.is_empty() {
                    row_head(ui, "Secure NAS stats (SNMP v3)", None, "No device uses SNMP v3 yet.");
                    ui.label(RichText::new("To set it up on a Synology (DSM 7):").size(12.5).color(MUTED));
                    for step in [
                        "1. In DSM open Control Panel > Terminal & SNMP > SNMP.",
                        "2. Tick \"Enable SNMP service\" and \"SNMPv3 service\".",
                        "3. Enter a user name, choose protocol SHA and a password (at least 8 characters).",
                        "4. Tick \"Enable SNMP privacy\", choose AES and a second password. Click Apply.",
                        "5. In HomeNet Watch: Devices > your NAS > System stats > v3 (secure). Enter the same user name, SHA-1 with the first password, AES-128 with the second.",
                        "6. Come back here and click Test sign-in.",
                    ] {
                        ui.label(RichText::new(step).size(12.5));
                    }
                    if let Some(id) = s.devices.iter().find(|d| d.kind == DeviceKind::Storage).map(|d| d.id) {
                        if ui.button("Open my NAS's details").clicked() {
                            self.selected = Some(id);
                            self.page = Page::Devices;
                        }
                    }
                } else {
                    for (id, name, ip, sec, err, has_perf) in v3 {
                        let state = if err.is_some() { Some(false) } else if has_perf { Some(true) } else { None };
                        let status = match (&err, has_perf) {
                            (Some(e), _) => e.clone(),
                            (None, true) => "Reading stats.".into(),
                            (None, false) => "Waiting for the first reading.".into(),
                        };
                        row_head(ui, &format!("Secure NAS stats (SNMP v3): {}", name), state, &status);
                        let key = format!("snmp-{}", id);
                        if ui.button("Test sign-in now").clicked() {
                            start(&checks, &key, move || snmp_test(ip, sec), ctx.clone());
                        }
                        show_result(ui, &checks, &key);
                    }
                }
                ui.add_space(10.0);

                // 6. In-app updates
                let repo = crate::update::repo();
                if repo.is_empty() {
                    row_head(ui, "In-app updates", None, "This copy was built locally, so it can't update itself.");
                    ui.label(
                        RichText::new(
                            "To try updates: download the Windows file of an older release from your GitHub Releases page, run that copy, \
                             publish a newer version, then use About > Check for updates in that copy. Meanwhile you can check a release below.",
                        )
                        .size(12.5)
                        .color(MUTED),
                    );
                } else {
                    row_head(ui, "In-app updates", Some(true), &format!("This copy updates from github.com/{}", repo));
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Repository").color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut self.repo_input).hint_text("yourname/homenet-watch").desired_width(240.0));
                    if ui.button("Check the latest release").clicked() {
                        let r = self.repo_input.trim().trim_start_matches("https://github.com/").trim_end_matches('/').to_string();
                        start(&checks, "release", move || check_release(&r), ctx.clone());
                    }
                });
                show_result(ui, &checks, "release");
            });
    }
}

/// Look at a repository's latest release and say whether it has every file the app needs.
fn check_release(repo: &str) -> Result<String, String> {
    if repo.split('/').count() != 2 {
        return Err("Type the repository as owner/name, for example yourname/homenet-watch.".into());
    }
    let rel = crate::update::fetch_latest_from(repo)?;
    let health = crate::update::release_health(&rel);
    let missing: Vec<&str> = health.iter().filter(|(_, ok)| !ok).map(|(n, _)| *n).collect();
    let have = format!("v{}.{}.{}", rel.version.0, rel.version.1, rel.version.2);
    let newer = if rel.version > crate::update::current() {
        format!(" It's newer than this copy ({}), so copies from older releases will be offered it.", env!("CARGO_PKG_VERSION"))
    } else {
        String::new()
    };
    if missing.is_empty() {
        Ok(format!("Latest release {} has everything: Windows, Mac and Linux files, fingerprints and the user guide.{}", have, newer))
    } else {
        Err(format!(
            "Latest release {} is missing: {}. If it was just published, wait for the Actions build to finish (10–15 min); if the build failed, open the Actions tab to see why.",
            have,
            missing.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    // Need the home network / internet: cargo test -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_checkup_pieces() {
        let nas: std::net::Ipv4Addr = "10.10.10.155".parse().unwrap();
        println!("snmp v2c: {:?}", super::snmp_test(nas, crate::snmp::Security::V2c("public".into())));
        println!("release (a public project without our files): {:?}", super::check_release("BurntSushi/ripgrep"));
        println!("bad repo text: {:?}", super::check_release("not a repo"));
        println!("start with computer: {:?}", crate::background::autostart_points_here());
    }
}
