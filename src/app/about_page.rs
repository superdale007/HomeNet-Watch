//! About: who makes HomeNet Watch, support link, privacy, and a glossary.

use super::*;

/// Ways to support DDID.
pub(super) const PAYPAL_NAME: &str = "@dalemeredith";
pub(super) const PAYPAL_URL: &str = "https://paypal.me/dalemeredith";
pub(super) const KOFI_NAME: &str = "ko-fi.com/superdale";
pub(super) const KOFI_URL: &str = "https://ko-fi.com/superdale";
pub(super) const SITE_URL: &str = "https://daledumbsitdown.com";

/// Version history shown under "What's new" (newest first).
const CHANGES: &[(&str, &[&str])] = &[
    (
        "0.8",
        &[
            "MITRE ATT&CK details: technique name and the attacker's goal (tactic) on security alerts",
            "Alerts: new Security filter and Save as spreadsheet (with explanations and techniques)",
            "MITRE techniques in the weekly report and the alert Tuning tab",
            "Settings Checkup: one-click tests for the tray icon, start with the computer, phone, email, SNMP v3 sign-in and your GitHub release",
        ],
    ),
    (
        "0.7",
        &[
            "Keeps watching when the window is closed, with an icon by the clock (Windows, Mac)",
            "Start with the computer, pop-up notifications on this computer",
            "Alerts on your phone (ntfy) and by email",
            "Countries: click a country to see which programs connect there",
            "Network map can group devices by type or room",
            "Quieter alerts: TVs, speakers and consoles start with offline alerts off; slow alerts only for key gear",
        ],
    ),
    (
        "0.6",
        &[
            "Secure SNMP v3 for NAS boxes (user name, MD5/SHA authentication, DES/AES encryption)",
            "Storage pools no longer warn when fully assigned to volumes",
            "Separate temperature limit for the NAS itself",
            "Remove a device from the Performance page",
            "HomeNet Watch no longer flags itself as unsigned",
            "Calmer Insights on the first day",
            "User guide (About > User guide), in-app updates from GitHub releases",
        ],
    ),
    (
        "0.5",
        &[
            "Traffic page with live and historical graphs and a timeline",
            "Apps & sites: programs, destinations, world map, live connections",
            "Insights, speed test, weekly report, backup and restore",
            "Program signatures and fingerprint trust",
            "Security alerts with explanations, MITRE tags and alert tuning",
            "HomeNet Watch by DDID branding, PayPal and Ko-fi support buttons",
        ],
    ),
    ("0.4", &["Runs on Mac and Linux as well as Windows", "Shows your outside internet address"]),
    ("0.3", &["NAS monitoring (Synology, QNAP, UGREEN)", "Web page and DNS checks, rooms/groups", "Alert sound, taskbar flash, quiet hours"]),
    ("0.2", &["Find every device at home, watch them, network map, dashboard, alerts and reports"]),
];

const GLOSSARY: &[(&str, &str)] = &[
    ("IP address", "A device's number on a network, like a house number. Home devices usually have ones starting with 192.168 or 10."),
    ("Outside address", "The one address your whole home shares on the internet. Websites see this, not your devices' home addresses."),
    ("Router / gateway", "The box that connects your home to the internet and hands out addresses."),
    ("MAC (hardware) address", "A device's built-in network ID. Phones may show a different, \"private\" one to each network for privacy."),
    ("Ping", "A tiny \"are you there?\" message. The time it takes to answer is the response time."),
    ("Response time (latency)", "How long a device takes to answer, in milliseconds (ms). Lower is better."),
    ("Port", "A numbered door on a device for one kind of service, like 80 for web pages or 445 for file sharing."),
    ("DNS", "The internet's phone book: it turns names like example.com into addresses."),
    ("Download / upload", "Data coming in to you / going out from you."),
    ("Mb/s", "Megabits per second, how internet speed is measured. 8 Mb/s moves about 1 megabyte each second."),
    ("Content delivery network (CDN)", "Companies that keep copies of websites and downloads close to you so they load quickly."),
    ("Analytics / telemetry", "Data apps send home about how they're used."),
    ("Phoning home", "A program contacting its maker's servers, often in the background."),
    ("Regular check-ins (beaconing)", "Contacting the same place at very regular times. Updaters do it; so does some malware."),
    ("Digital signature", "A seal showing who made a program and that it hasn't been changed since."),
    ("Fingerprint (SHA-256)", "A unique code calculated from a file. If one byte changes, the fingerprint changes."),
    ("Trusting / pinning a program", "Remembering a program's fingerprint so you're told if the file ever changes."),
    ("ARP spoofing", "A trick where a device pretends to be your router to secretly read traffic."),
    ("NAS", "Network-attached storage: a box of disks on your network for files and backups."),
    ("SNMP", "A standard way for devices like NAS boxes and routers to share their stats."),
    ("RAID / storage pool", "Several disks working together so one can fail without losing data."),
    ("MITRE ATT&CK", "A public catalogue of attacker techniques. Some alerts show a matching technique number (like T1557) for reference."),
];

impl App {
    pub(super) fn about(&mut self, ui: &mut Ui, s: &mut Shared) {
        self.header(ui, s, "About", "HomeNet Watch by DDID", |_this, _ui, _s| {});
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(860.0);
            flow_card(ui, "", |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(r, 18.0, GOLD);
                    let c = r.center();
                    let k = 2.0;
                    p.add(egui::Shape::line(vec![c + vec2(-10.0, -1.0) * k, c + vec2(0.0, -10.0) * k, c + vec2(10.0, -1.0) * k], Stroke::new(4.5f32, NAV)));
                    p.add(egui::Shape::line(
                        vec![c + vec2(-10.0, 5.0) * k, c + vec2(-4.0, 5.0) * k, c + vec2(-1.5, 0.0) * k, c + vec2(1.5, 9.0) * k, c + vec2(4.0, 5.0) * k, c + vec2(10.0, 5.0) * k],
                        Stroke::new(4.0f32, NAV),
                    ));
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("HomeNet Watch").size(26.0).strong());
                        ui.label(RichText::new("by DDID · DaleDumbsITDown.com").size(15.0).color(GOLD));
                        ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).size(12.0).color(MUTED));
                    });
                });
                ui.add_space(10.0);
                ui.label("Keep an eye on every device at home, see what your computer talks to on the internet, and get plain-language alerts when something changes.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("📖  User guide").on_hover_text("Opens the full guide in your web browser (works offline)").clicked() {
                        open_user_guide();
                    }
                    if ui.button("🌐  Visit DaleDumbsITDown.com").clicked() {
                        crate::util::open(SITE_URL);
                    }
                    support_button(ui);
                });
            });
            ui.add_space(12.0);
            flow_card(ui, "VERSION & UPDATES", |ui| {
                egui::Grid::new("version").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                    ui.label(RichText::new("Version").color(MUTED));
                    ui.label(RichText::new(env!("CARGO_PKG_VERSION")).strong());
                    ui.end_row();
                    ui.label(RichText::new("Built (UTC)").color(MUTED));
                    ui.label(env!("HNW_BUILD_DATE"));
                    ui.end_row();
                    ui.label(RichText::new("Your data").color(MUTED));
                    ui.add(egui::Label::new(RichText::new(data_dir().display().to_string()).monospace().size(12.0)).wrap());
                    ui.end_row();
                });
                ui.add_space(6.0);
                let repo = crate::update::repo();
                if repo.is_empty() {
                    ui.label("This copy was built locally. Copies downloaded from the published GitHub releases can update themselves from here.");
                } else {
                    ui.label(RichText::new(format!("Updates come from github.com/{}", repo)).size(12.5).color(MUTED));
                }
                let up = self.update.lock().unwrap().clone();
                let mut do_check = false;
                let mut do_install = false;
                ui.horizontal(|ui| {
                    if ui.add_enabled(!repo.is_empty() && !up.checking && !up.installing, egui::Button::new("⟳  Check for updates")).clicked() {
                        do_check = true;
                    }
                    if up.checking {
                        ui.spinner();
                        ui.label(RichText::new("Checking…").color(MUTED));
                    }
                    if let Some(rel) = &up.latest {
                        if !rel.page.is_empty() && ui.link("Release page").clicked() {
                            crate::util::open(&rel.page);
                        }
                    }
                });
                if let Some(e) = &up.error {
                    ui.label(RichText::new(e).color(AMBER));
                }
                if let (Some(rel), false) = (&up.latest, up.installed) {
                    if up.newer {
                        egui::Frame::new().fill(CARD_HI).corner_radius(8.0).inner_margin(12.0).show(ui, |ui| {
                            ui.label(RichText::new(format!("Version {} is available (you have {})", rel.tag.trim_start_matches('v'), env!("CARGO_PKG_VERSION"))).strong().color(GOLD));
                            if !rel.notes.is_empty() {
                                let notes: String = rel.notes.chars().take(1200).collect();
                                ui.label(RichText::new(notes).size(12.5).color(MUTED));
                            }
                            ui.add_space(6.0);
                            if up.installing {
                                ui.add(egui::ProgressBar::new(up.progress).desired_width(320.0).text("Downloading and checking…"));
                            } else if rel.asset_url.is_some() {
                                if ui.add(egui::Button::new(RichText::new("⬇  Download and install").color(Color32::WHITE)).fill(Color32::from_rgb(0x2E, 0x6F, 0xD8))).clicked() {
                                    do_install = true;
                                }
                                ui.label(RichText::new("The download's fingerprint is checked against the published one before anything is replaced.").size(11.5).color(FAINT));
                            } else {
                                ui.label(RichText::new("This release has no download for your system yet; use the release page.").color(MUTED));
                            }
                        });
                    } else if up.checked {
                        ui.label(RichText::new(format!("✔ You have the latest version ({}).", env!("CARGO_PKG_VERSION"))).color(GREEN));
                    }
                }
                if up.installed {
                    egui::Frame::new().fill(CARD_HI).corner_radius(8.0).inner_margin(12.0).show(ui, |ui| {
                        ui.label(RichText::new("✔ The new version is installed. Restart HomeNet Watch to start using it.").color(GREEN).strong());
                        if ui.button("Restart now").clicked() {
                            crate::update::restart();
                        }
                    });
                }
                if do_check {
                    crate::update::check(self.update.clone(), ui.ctx().clone());
                }
                if do_install {
                    crate::update::install(self.update.clone(), ui.ctx().clone());
                }
                if !repo.is_empty() {
                    let mut auto = s.settings.check_updates;
                    if ui.checkbox(&mut auto, "Check for updates when HomeNet Watch starts").changed() {
                        s.settings.check_updates = auto;
                        s.dirty = true;
                    }
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Your devices, names, settings, trusted programs and history are kept in the data folder above, separate from the program, so they carry over to every new version.")
                        .size(12.5)
                        .color(MUTED),
                );
                ui.add_space(8.0);
                ui.label(RichText::new("What's new").strong());
                for (i, (ver, items)) in CHANGES.iter().enumerate() {
                    egui::CollapsingHeader::new(RichText::new(format!("Version {}", ver)).color(if i == 0 { GOLD } else { TEXT }))
                        .id_salt(("changes", *ver))
                        .default_open(i == 0)
                        .show(ui, |ui| {
                            for it in *items {
                                ui.label(RichText::new(format!("• {}", it)).color(MUTED));
                            }
                        });
                }
            });
            ui.add_space(12.0);
            flow_card(ui, "YOUR PRIVACY", |ui| {
                ui.label("Everything HomeNet Watch records stays on this computer. It only contacts the internet for:");
                for line in [
                    "• Your outside address: asks api.ipify.org every 30 minutes.",
                    "• Speed tests: only when you press Start (speed.cloudflare.com).",
                    "• Countries on the world map: only if you turn it on (ip-api.com receives internet addresses, never your home devices).",
                    "• Web page and DNS checks you set up yourself.",
                    "• Updates: published copies ask api.github.com for the newest version (you can turn this off below).",
                    "• Phone alerts, only if you turn them on: the alert text goes through ntfy.sh (or your own ntfy server).",
                    "• Email alerts, only if you turn them on: sent through your own mail provider.",
                ] {
                    ui.label(RichText::new(line).color(MUTED));
                }
            });
            ui.add_space(12.0);
            flow_card(ui, "GLOSSARY", |ui| {
                egui::Grid::new("glossary").num_columns(2).spacing([18.0, 8.0]).striped(true).show(ui, |ui| {
                    for (term, meaning) in GLOSSARY {
                        ui.label(RichText::new(*term).strong().color(TEXT));
                        ui.add(egui::Label::new(RichText::new(*meaning).color(MUTED)).wrap());
                        ui.end_row();
                    }
                });
            });
        });
    }
}

/// Donation buttons (shown on About and Settings): PayPal and Ko-fi.
pub(super) fn support_button(ui: &mut Ui) {
    let paypal = egui::Button::new(RichText::new(format!("♥  PayPal  {}", PAYPAL_NAME)).color(Color32::WHITE)).fill(Color32::from_rgb(0x00, 0x5E, 0xA6));
    if ui.add(paypal).on_hover_text(format!("Opens {} in your browser", PAYPAL_URL)).clicked() {
        crate::util::open(PAYPAL_URL);
    }
    let kofi = egui::Button::new(RichText::new(format!("☕  Ko-fi  {}", KOFI_NAME)).color(Color32::WHITE)).fill(Color32::from_rgb(0xC0, 0x3B, 0x5A));
    if ui.add(kofi).on_hover_text(format!("Opens {} in your browser", KOFI_URL)).clicked() {
        crate::util::open(KOFI_URL);
    }
}

/// The user guide ships inside the program; write it next to the app's data and open it.
const USER_GUIDE: &str = include_str!("../../docs/user-guide.html");

pub(super) fn open_user_guide() {
    let path = data_dir().join("HomeNet-Watch-User-Guide.html");
    let _ = std::fs::create_dir_all(data_dir());
    if std::fs::write(&path, USER_GUIDE).is_ok() {
        crate::util::open(&path.to_string_lossy());
    }
}
