//! Settings: running in the background, and alerts that reach you away from the computer.

use super::*;

impl App {
    pub(super) fn settings_away(&mut self, ui: &mut Ui, s: &mut Shared) {
        flow_card(ui, "RUNNING IN THE BACKGROUND", |ui| {
            ui.checkbox(&mut s.settings.keep_running, "Keep watching when I close the window");
            let how = if cfg!(any(windows, target_os = "macos")) {
                "Closing the window hides it; HomeNet Watch keeps checking and alerting. Open it again from its icon by the clock, where you can also quit."
            } else {
                "Closing the window minimises it; HomeNet Watch keeps checking and alerting. Starting it again brings the window back."
            };
            ui.label(RichText::new(how).size(12.0).color(MUTED));
            ui.add_space(6.0);
            let mut auto = self.autostart;
            if ui.checkbox(&mut auto, "Start HomeNet Watch when I sign in to this computer").changed() {
                match crate::background::set_autostart(auto) {
                    Ok(()) => {
                        self.autostart = auto;
                        self.toast = Some((if auto { "HomeNet Watch will start quietly in the background when you sign in." } else { "HomeNet Watch won't start by itself any more." }.into(), Instant::now()));
                    }
                    Err(e) => self.toast = Some((format!("Couldn't change that: {}", e), Instant::now())),
                }
            }
            ui.label(RichText::new("It starts hidden and keeps watching; your alerts reach you as pop-ups, on your phone or by email.").size(12.0).color(MUTED));
            ui.add_space(6.0);
            if ui.button("⏻  Quit HomeNet Watch now").on_hover_text("Stops watching until you start it again").clicked() {
                super::quit_now(&self.shared);
            }
        });
        ui.add_space(12.0);

        let mut send_test: Option<u8> = None;
        flow_card(ui, "PHONE & EMAIL ALERTS", |ui| {
            ui.label("Get alerts when you're away from this computer. They're sent by this computer, so HomeNet Watch needs to be running (it can run in the background).");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Send");
                ui.selectable_value(&mut s.settings.remote_min, 2, "critical alerts only (something went offline, a disk failed…)");
                ui.selectable_value(&mut s.settings.remote_min, 1, "warnings too");
            });
            ui.checkbox(&mut s.settings.quiet_remote, "Hold them back during quiet hours too");
            ui.add_space(10.0);

            // ── phone (ntfy)
            ui.label(RichText::new("📱 Phone notifications (ntfy)").strong());
            ui.checkbox(&mut s.settings.ntfy_enabled, "Send alerts to my phone");
            if s.settings.ntfy_enabled {
                if s.settings.ntfy_topic.trim().is_empty() {
                    s.settings.ntfy_topic = crate::notify::new_topic();
                }
                egui::Grid::new("ntfy").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label(RichText::new("Topic").color(MUTED));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut s.settings.ntfy_topic).desired_width(230.0));
                        if ui.small_button("New").on_hover_text("Make a new hard-to-guess topic name").clicked() {
                            s.settings.ntfy_topic = crate::notify::new_topic();
                        }
                        if ui.small_button("Copy").clicked() {
                            ui.ctx().copy_text(s.settings.ntfy_topic.clone());
                        }
                    });
                    ui.end_row();
                    ui.label(RichText::new("Server").color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut s.settings.ntfy_server).desired_width(230.0));
                    ui.end_row();
                });
                ui.label(
                    RichText::new(
                        "1. Install the free ntfy app (Android or iPhone).  2. Tap + / Subscribe and enter the topic above.  3. Press Send a test. \
                         The topic works like a password: anyone who knows it can read your alerts, so keep it private.",
                    )
                    .size(12.0)
                    .color(MUTED),
                );
                if ui.button("Send a test to my phone").clicked() {
                    send_test = Some(0);
                }
            }
            ui.add_space(10.0);

            // ── email
            ui.label(RichText::new("✉ Email").strong());
            ui.checkbox(&mut s.settings.email_enabled, "Send alerts by email");
            if s.settings.email_enabled {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Quick setup:").size(12.5).color(MUTED));
                    for (name, host, port) in [("Gmail", "smtp.gmail.com", 587u16), ("iCloud", "smtp.mail.me.com", 587), ("Yahoo", "smtp.mail.yahoo.com", 465)] {
                        if ui.small_button(name).clicked() {
                            s.settings.smtp_host = host.into();
                            s.settings.smtp_port = port;
                        }
                    }
                });
                egui::Grid::new("smtp").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label(RichText::new("Mail server").color(MUTED));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut s.settings.smtp_host).hint_text("smtp.example.com").desired_width(200.0));
                        ui.label(RichText::new("port").color(MUTED));
                        ui.add(egui::DragValue::new(&mut s.settings.smtp_port).range(1..=65535));
                    });
                    ui.end_row();
                    ui.label(RichText::new("User name").color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut s.settings.smtp_user).hint_text("you@example.com").desired_width(230.0));
                    ui.end_row();
                    ui.label(RichText::new("Password").color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut s.settings.smtp_pass).password(true).hint_text("app password").desired_width(230.0));
                    ui.end_row();
                    ui.label(RichText::new("Send from").color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut s.settings.email_from).hint_text("same as user name").desired_width(230.0));
                    ui.end_row();
                    ui.label(RichText::new("Send to").color(MUTED));
                    ui.add(egui::TextEdit::singleline(&mut s.settings.email_to).hint_text("you@example.com (several: separate with commas)").desired_width(300.0));
                    ui.end_row();
                });
                ui.label(
                    RichText::new(
                        "Gmail, iCloud and Yahoo need an \"app password\" (made in your account's security settings), not your normal password. \
                         The connection is always encrypted. The password is saved encrypted on Windows and is never put in backups.",
                    )
                    .size(12.0)
                    .color(MUTED),
                );
                if ui.button("Send a test email").clicked() {
                    send_test = Some(1);
                }
            }
        });
        if let Some(kind) = send_test {
            let settings = s.settings.clone();
            let out = self.test_result.clone();
            self.toast = Some(("Sending a test…".into(), Instant::now()));
            std::thread::spawn(move || {
                let title = "HomeNet Watch test";
                let body = "If you can read this, HomeNet Watch alerts will reach you here.";
                let r = if kind == 0 {
                    crate::notify::send_ntfy(&settings, title, body, Severity::Info)
                } else {
                    crate::notify::send_email(&settings, title, body)
                };
                *out.lock().unwrap() = Some(match r {
                    Ok(()) => if kind == 0 { "Test sent: check your phone.".into() } else { "Test email sent: check your inbox.".into() },
                    Err(e) => format!("The test didn't go through: {}", e),
                });
            });
        }
    }
}
