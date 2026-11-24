//! Apps & sites: which programs on this computer use the internet, where they
//! connect, countries on a world map, live connections, and details panels.

use super::*;
use crate::traffic::{bytes, day_key, AppStat, Host};
use crate::trust::SigStatus;

const TABS: [&str; 4] = ["Programs", "Destinations", "Countries", "Connections now"];

fn cat_color(c: &str) -> Color32 {
    match c {
        "Advertising" => RED,
        "Analytics" => AMBER,
        "Content delivery" => BLUE,
        "Updates" => GREEN,
        "Cloud" => Color32::from_rgb(0x5B, 0xC0, 0xDE),
        "Social" => Color32::from_rgb(0xE8, 0x6E, 0xB5),
        "Streaming" => Color32::from_rgb(0xA7, 0x7B, 0xF3),
        "Gaming" => Color32::from_rgb(0x8B, 0xD3, 0x46),
        "This network" => GRAY,
        _ => FAINT,
    }
}

pub(super) fn trust_chip(a: &AppStat) -> (&'static str, Color32) {
    if crate::util::is_self(&a.path) && !a.changed_since_pin {
        ("This app (built on your computer)", GREEN)
    } else if a.changed_since_pin {
        ("Changed since trusted", RED)
    } else if a.pinned.is_some() {
        ("Trusted", GREEN)
    } else if a.path.is_empty() {
        ("Protected (can't check)", FAINT)
    } else {
        match a.sig {
            SigStatus::Signed => ("Signed", BLUE),
            SigStatus::System => ("System", GRAY),
            SigStatus::Unsigned => ("Not signed", AMBER),
            SigStatus::Invalid => ("Signature broken", RED),
            SigStatus::NotApplicable => ("—", FAINT),
            SigStatus::Unknown => ("Checking…", FAINT),
        }
    }
}

fn host_label(h: &Host) -> String {
    h.name.clone().unwrap_or_else(|| h.ip.clone())
}

impl App {
    pub(super) fn apps(&mut self, ui: &mut Ui, s: &mut Shared) {
        let mut export = false;
        self.header(ui, s, "Apps & sites", "Which programs on this computer use the internet, and where they connect", |_this, ui, _s| {
            if ui.button("💾  Save as spreadsheet").clicked() {
                export = true;
            }
        });
        if export {
            self.toast = Some((
                match export_csv(s) {
                    Ok(p) => format!("Saved to {}", p),
                    Err(e) => format!("Couldn't save: {}", e),
                },
                Instant::now(),
            ));
        }
        ui.horizontal(|ui| {
            for (i, t) in TABS.iter().enumerate() {
                if ui.selectable_label(self.apps_tab == i as u8, *t).clicked() {
                    self.apps_tab = i as u8;
                }
            }
            ui.add_space(12.0);
            ui.add(egui::TextEdit::singleline(&mut self.apps_search).hint_text("🔎 Search…").desired_width(220.0));
        });
        ui.add_space(6.0);
        // what we can measure on this system
        if !s.traffic.per_app_bytes {
            ui.horizontal(|ui| {
                #[cfg(windows)]
                {
                    ui.label(RichText::new("Data per program needs administrator rights; connections are counted meanwhile.").size(12.0).color(FAINT));
                    if ui.small_button("Restart as administrator").clicked() {
                        if crate::util::restart_as_admin() {
                            std::process::exit(0);
                        }
                    }
                }
                #[cfg(not(windows))]
                ui.label(RichText::new("Data per program isn't measured on this system yet; connections are counted.").size(12.0).color(FAINT));
            });
        }
        ui.add_space(6.0);
        match self.apps_tab {
            0 => self.apps_programs(ui, s),
            1 => self.apps_hosts(ui, s),
            2 => self.apps_countries(ui, s),
            _ => self.apps_live(ui, s),
        }
    }

    fn apps_programs(&mut self, ui: &mut Ui, s: &mut Shared) {
        let q = self.apps_search.to_lowercase();
        let today = day_key(now());
        let mut apps: Vec<&AppStat> = s.traffic.saved.apps.values().filter(|a| q.is_empty() || a.name.to_lowercase().contains(&q) || a.path.to_lowercase().contains(&q)).collect();
        apps.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        let active: std::collections::HashMap<&str, usize> = s.traffic.conns.iter().fold(Default::default(), |mut m, c| {
            *m.entry(c.app.as_str()).or_default() += 1;
            m
        });
        let show_bytes = s.traffic.per_app_bytes || apps.iter().any(|a| a.rx + a.tx > 0);
        let cols: Vec<(&str, f32)> = vec![("Program", 0.30), ("Trust", 0.16), ("Open now", 0.09), ("Today", 0.09), ("All time", 0.10), (if show_bytes { "Data" } else { "Places" }, 0.12), ("Last active", 0.14)];
        let mut click = None;
        table(ui, &cols, apps.len(), |i, rect, p, xs| {
            let a = apps[i];
            let y = rect.center().y;
            p.text(pos2(xs[0], y - 8.0), Align2::LEFT_CENTER, &a.name, FontId::proportional(13.5), TEXT);
            let sub = a.signer.clone().unwrap_or_else(|| a.path.clone());
            p.with_clip_rect(egui::Rect::from_min_max(rect.min, pos2(xs[1] - 8.0, rect.bottom()))).text(pos2(xs[0], y + 9.0), Align2::LEFT_CENTER, sub, FontId::proportional(10.5), FAINT);
            let (chip, col) = trust_chip(a);
            p.circle_filled(pos2(xs[1] + 4.0, y), 4.0, col);
            p.text(pos2(xs[1] + 14.0, y), Align2::LEFT_CENTER, chip, FontId::proportional(12.5), col);
            let open = active.get(a.key.as_str()).cloned().unwrap_or(0);
            p.text(pos2(xs[2], y), Align2::LEFT_CENTER, if open > 0 { open.to_string() } else { "—".into() }, FontId::proportional(13.0), if open > 0 { GREEN } else { FAINT });
            p.text(pos2(xs[3], y), Align2::LEFT_CENTER, a.daily.get(&today).map(|d| d.conns).unwrap_or(0).to_string(), FontId::proportional(13.0), TEXT);
            p.text(pos2(xs[4], y), Align2::LEFT_CENTER, a.conns.to_string(), FontId::proportional(13.0), MUTED);
            let v = if show_bytes { bytes(a.rx + a.tx) } else { a.dests.len().to_string() };
            p.text(pos2(xs[5], y), Align2::LEFT_CENTER, v, FontId::proportional(13.0), TEXT);
            p.text(pos2(xs[6], y), Align2::LEFT_CENTER, ago(Some(a.last_seen)), FontId::proportional(12.5), MUTED);
        }, |i| click = Some(i), self.sel_app.as_ref().and_then(|k| apps.iter().position(|a| &a.key == k)));
        if let Some(i) = click {
            self.sel_app = Some(apps[i].key.clone());
            self.sel_host = None;
        }
        if apps.is_empty() {
            ui.label(RichText::new("Watching for programs that use the internet…").color(MUTED));
        }
    }

    fn apps_hosts(&mut self, ui: &mut Ui, s: &mut Shared) {
        let q = self.apps_search.to_lowercase();
        let mut hosts: Vec<&Host> = s
            .traffic
            .saved
            .hosts
            .values()
            .filter(|h| q.is_empty() || h.ip.contains(&q) || h.name.as_deref().unwrap_or("").to_lowercase().contains(&q) || h.category.to_lowercase().contains(&q) || h.country.as_deref().unwrap_or("").to_lowercase().contains(&q))
            .collect();
        hosts.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        let show_bytes = hosts.iter().any(|h| h.rx + h.tx > 0);
        // category summary chips
        let mut cats: std::collections::BTreeMap<&str, usize> = Default::default();
        for h in &hosts {
            *cats.entry(if h.category.is_empty() { "Other" } else { h.category.as_str() }).or_default() += 1;
        }
        ui.horizontal_wrapped(|ui| {
            for (c, n) in &cats {
                pill(ui, &format!("{} {}", c, n), cat_color(c)).on_hover_text(crate::categories::describe(c));
            }
        });
        ui.add_space(6.0);
        let cols: Vec<(&str, f32)> = vec![("Destination", 0.32), ("Category", 0.15), ("Country", 0.12), ("Programs", 0.17), ("Connections", 0.10), (if show_bytes { "Data" } else { "Ports" }, 0.07), ("Last", 0.07)];
        let mut click = None;
        table(ui, &cols, hosts.len(), |i, rect, p, xs| {
            let h = hosts[i];
            let y = rect.center().y;
            let clip = egui::Rect::from_min_max(rect.min, pos2(xs[1] - 8.0, rect.bottom()));
            p.with_clip_rect(clip).text(pos2(xs[0], y - 8.0), Align2::LEFT_CENTER, host_label(h), FontId::proportional(13.5), TEXT);
            p.text(pos2(xs[0], y + 9.0), Align2::LEFT_CENTER, &h.ip, FontId::proportional(10.5), FAINT);
            let c = if h.category.is_empty() { "Other" } else { h.category.as_str() };
            p.circle_filled(pos2(xs[1] + 4.0, y), 4.0, cat_color(c));
            p.text(pos2(xs[1] + 14.0, y), Align2::LEFT_CENTER, c, FontId::proportional(12.5), TEXT);
            p.text(pos2(xs[2], y), Align2::LEFT_CENTER, h.country.clone().unwrap_or_else(|| "—".into()), FontId::proportional(12.5), MUTED);
            p.with_clip_rect(egui::Rect::from_min_max(pos2(xs[3], rect.top()), pos2(xs[4] - 8.0, rect.bottom())))
                .text(pos2(xs[3], y), Align2::LEFT_CENTER, h.apps.join(", "), FontId::proportional(12.5), MUTED);
            p.text(pos2(xs[4], y), Align2::LEFT_CENTER, h.conns.to_string(), FontId::proportional(13.0), TEXT);
            let v = if show_bytes { bytes(h.rx + h.tx) } else { h.ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",") };
            p.text(pos2(xs[5], y), Align2::LEFT_CENTER, v, FontId::proportional(12.5), MUTED);
            p.text(pos2(xs[6], y), Align2::LEFT_CENTER, ago(Some(h.last_seen)), FontId::proportional(12.0), MUTED);
        }, |i| click = Some(i), self.sel_host.as_ref().and_then(|k| hosts.iter().position(|h| &h.ip == k)));
        if let Some(i) = click {
            self.sel_host = Some(hosts[i].ip.clone());
            self.sel_app = None;
        }
    }

    fn apps_countries(&mut self, ui: &mut Ui, s: &mut Shared) {
        if !s.settings.geo_lookup {
            flow_card(ui, "COUNTRIES", |ui| {
                ui.label("To show which countries your traffic goes to, HomeNet Watch needs to look up each internet address.");
                ui.label(
                    RichText::new("This sends the addresses (not their content) to the free service ip-api.com. Your home's devices are never sent.")
                        .size(12.5)
                        .color(MUTED),
                );
                ui.add_space(6.0);
                if ui.button("Turn on country lookup").clicked() {
                    s.settings.geo_lookup = true;
                    s.dirty = true;
                }
            });
            ui.add_space(10.0);
        }
        // world map
        if self.world.is_none() {
            let img = crate::worldmap::render(1440, Color32::from_rgb(0x26, 0x35, 0x4C), Color32::from_rgb(0x10, 0x17, 0x24));
            self.world = Some(ui.ctx().load_texture("world", img, egui::TextureOptions::LINEAR));
        }
        let mut by_country: std::collections::BTreeMap<String, (u64, f32, f32, usize)> = Default::default();
        for h in s.traffic.saved.hosts.values() {
            if let (Some(c), Some(lat), Some(lon)) = (&h.country, h.lat, h.lon) {
                let e = by_country.entry(c.clone()).or_insert((0, 0.0, 0.0, 0));
                e.0 += h.conns.max(1);
                e.1 += lat;
                e.2 += lon;
                e.3 += 1;
            }
        }
        let w = ui.available_width();
        let map_w = w;
        let map_h = map_w * (crate::worldmap::LAT_TOP - crate::worldmap::LAT_BOTTOM) / 360.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(map_w, map_h.min(ui.available_height() - 140.0).max(200.0)), Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 10.0, Color32::from_rgb(0x10, 0x17, 0x24));
        if let Some(tex) = &self.world {
            p.image(tex.id(), rect, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        let maxc = by_country.values().map(|v| v.0).max().unwrap_or(1) as f32;
        let mut hover_txt = None;
        for (name, (n, lat, lon, k)) in &by_country {
            let (fx, fy) = crate::worldmap::project(lat / *k as f32, lon / *k as f32);
            let c = pos2(rect.left() + fx * rect.width(), rect.top() + fy * rect.height());
            let r = 4.0 + 16.0 * (*n as f32 / maxc).sqrt();
            p.circle_filled(c, r, BLUE.linear_multiply(0.35));
            p.circle_stroke(c, r, Stroke::new(1.5f32, BLUE));
            if let Some(hp) = resp.hover_pos() {
                if hp.distance(c) < r + 3.0 {
                    hover_txt = Some(format!("{}: {} connections", name, n));
                }
            }
        }
        if let (Some(t), Some(hp)) = (hover_txt, resp.hover_pos()) {
            let g = p.layout_no_wrap(t, FontId::proportional(12.0), TEXT);
            let r = egui::Rect::from_min_size(hp + vec2(12.0, 8.0), g.size() + vec2(12.0, 8.0));
            p.rect_filled(r, 5.0, CARD_HI);
            p.galley(r.min + vec2(6.0, 4.0), g, TEXT);
        }
        if by_country.is_empty() {
            p.text(rect.center(), Align2::CENTER_CENTER, if s.settings.geo_lookup { "Looking up countries…" } else { "Country lookup is off" }, FontId::proportional(14.0), MUTED);
        }
        ui.add_space(8.0);
        let mut list: Vec<(&String, &(u64, f32, f32, usize))> = by_country.iter().collect();
        list.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        ui.horizontal_wrapped(|ui| {
            for (name, v) in list.iter().take(40) {
                ui.label(RichText::new(format!("{}  {}", name, v.0)).size(12.5).color(MUTED));
                ui.add_space(10.0);
            }
        });
    }

    fn apps_live(&mut self, ui: &mut Ui, s: &mut Shared) {
        let mut conns = s.traffic.conns.clone();
        conns.sort_by(|a, b| a.app.cmp(&b.app).then(a.since.cmp(&b.since)));
        let q = self.apps_search.to_lowercase();
        let rows: Vec<(String, String, String, u16, i64, u64)> = conns
            .iter()
            .map(|c| {
                let app = s.traffic.saved.apps.get(&c.app).map(|a| a.name.clone()).unwrap_or_else(|| c.app.clone());
                let h = s.traffic.saved.hosts.get(&c.remote.to_string());
                let dest = h.map(host_label).unwrap_or_else(|| c.remote.to_string());
                let cat = h.map(|h| h.category.clone()).unwrap_or_default();
                (app, dest, cat, c.rport, c.since, c.rx + c.tx)
            })
            .filter(|r| q.is_empty() || r.0.to_lowercase().contains(&q) || r.1.to_lowercase().contains(&q))
            .collect();
        let cols: Vec<(&str, f32)> = vec![("Program", 0.24), ("Connected to", 0.36), ("Category", 0.14), ("Port", 0.08), ("Open for", 0.10), ("Data", 0.08)];
        let mut click = None;
        table(ui, &cols, rows.len(), |i, rect, p, xs| {
            let r = &rows[i];
            let y = rect.center().y;
            p.text(pos2(xs[0], y), Align2::LEFT_CENTER, &r.0, FontId::proportional(13.0), TEXT);
            p.with_clip_rect(egui::Rect::from_min_max(pos2(xs[1], rect.top()), pos2(xs[2] - 8.0, rect.bottom()))).text(pos2(xs[1], y), Align2::LEFT_CENTER, &r.1, FontId::proportional(13.0), TEXT);
            p.text(pos2(xs[2], y), Align2::LEFT_CENTER, &r.2, FontId::proportional(12.5), cat_color(&r.2));
            p.text(pos2(xs[3], y), Align2::LEFT_CENTER, r.3.to_string(), FontId::proportional(12.5), MUTED);
            p.text(pos2(xs[4], y), Align2::LEFT_CENTER, dur(now() - r.4), FontId::proportional(12.5), MUTED);
            p.text(pos2(xs[5], y), Align2::LEFT_CENTER, if r.5 > 0 { bytes(r.5) } else { "—".into() }, FontId::proportional(12.5), MUTED);
        }, |i| click = Some(i), None);
        if let Some(i) = click {
            if let Some(c) = conns.get(i) {
                self.sel_host = Some(c.remote.to_string());
                self.sel_app = None;
            }
        }
        if rows.is_empty() {
            ui.label(RichText::new("No open internet connections right now.").color(MUTED));
        }
    }

    /// Right-hand details for a program or a destination.
    pub(super) fn app_detail(&mut self, ui: &mut Ui, s: &mut Shared) {
        let mut close = false;
        let mut goto_host: Option<String> = None;
        let mut goto_app: Option<String> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    if ui.small_button("✖").clicked() {
                        close = true;
                    }
                });
            });
            if let Some(key) = self.sel_app.clone() {
                let hosts = s.traffic.saved.hosts.clone();
                let Some(a) = s.traffic.saved.apps.get_mut(&key) else {
                    close = true;
                    return;
                };
                ui.label(RichText::new(&a.name).size(19.0).strong());
                let (chip, col) = trust_chip(a);
                pill(ui, chip, col);
                if crate::util::is_self(&a.path) {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "This is HomeNet Watch itself. It has no digital signature because it was built on your computer; signatures come from a paid \
                             code-signing certificate. That's expected and safe. Its internet use is its own checks: your outside address, speed tests, \
                             web and DNS checks, and (if turned on) country lookups.",
                        )
                        .size(12.5)
                        .color(MUTED),
                    );
                }
                ui.add_space(8.0);
                egui::Grid::new("appinfo").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    let row = |ui: &mut Ui, k: &str, v: String| {
                        ui.label(RichText::new(k).color(MUTED));
                        ui.add(egui::Label::new(v).wrap());
                        ui.end_row();
                    };
                    row(ui, "Location", if a.path.is_empty() { "unknown (needs more rights)".into() } else { a.path.clone() });
                    row(ui, "Started by", a.parent.clone().unwrap_or_else(|| "—".into()));
                    row(ui, "Signature", a.sig.label().to_string());
                    row(ui, "Signed by", a.signer.clone().unwrap_or_else(|| "—".into()));
                    row(ui, "First seen", fmt_time(a.first_seen));
                    row(ui, "Last active", ago(Some(a.last_seen)));
                    row(ui, "Connections", format!("{} in total", a.conns));
                    if a.rx + a.tx > 0 {
                        row(ui, "Data", format!("{} in · {} out", bytes(a.rx), bytes(a.tx)));
                    }
                    let today_hours = a.hours.iter().filter(|h| now() / 3600 - **h < 24).count();
                    row(ui, "Active hours", format!("{} of the last 24", today_hours));
                });
                if let Some(why) = crate::trust::odd_location(&a.path) {
                    ui.label(RichText::new(format!("⚠ This program {}.", why)).color(AMBER));
                }
                ui.add_space(10.0);
                section_title(ui, "FINGERPRINT (SHA-256)");
                ui.label(RichText::new(a.sha256.clone().unwrap_or_else(|| "calculating…".into())).monospace().size(10.5).color(MUTED));
                if a.changed_since_pin {
                    ui.label(RichText::new("This program is different from the version you trusted. If it just updated, trust the new version.").color(RED));
                }
                ui.horizontal(|ui| {
                    if let Some(h) = a.sha256.clone() {
                        let label = if a.pinned.as_ref() == Some(&h) { "Trusted ✔" } else { "Trust this version" };
                        if ui.add_enabled(a.pinned.as_ref() != Some(&h), egui::Button::new(label)).on_hover_text("Remember this exact version and alert me if the program file ever changes").clicked() {
                            a.pinned = Some(h);
                            a.changed_since_pin = false;
                            s.traffic.dirty = true;
                        }
                    }
                    if a.pinned.is_some() && ui.button("Stop trusting").clicked() {
                        a.pinned = None;
                        a.changed_since_pin = false;
                        s.traffic.dirty = true;
                    }
                });
                ui.add_space(10.0);
                section_title(ui, "CONNECTS TO");
                let mut ds: Vec<&Host> = a.dests.iter().filter_map(|ip| hosts.get(ip)).collect();
                ds.sort_by(|x, y| y.last_seen.cmp(&x.last_seen));
                for h in ds.iter().take(25) {
                    ui.horizontal(|ui| {
                        let c = if h.category.is_empty() { "Other" } else { h.category.as_str() };
                        let (r, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.0, cat_color(c));
                        if ui.link(host_label(h)).clicked() {
                            goto_host = Some(h.ip.clone());
                        }
                        ui.label(RichText::new(c).size(11.5).color(FAINT));
                    });
                }
            } else if let Some(ip) = self.sel_host.clone() {
                let Some(h) = s.traffic.saved.hosts.get(&ip).cloned() else {
                    close = true;
                    return;
                };
                ui.label(RichText::new(host_label(&h)).size(18.0).strong());
                let c = if h.category.is_empty() { "Other" } else { h.category.as_str() };
                pill(ui, c, cat_color(c));
                ui.label(RichText::new(crate::categories::describe(c)).size(12.0).color(MUTED));
                ui.add_space(8.0);
                egui::Grid::new("hostinfo").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    let row = |ui: &mut Ui, k: &str, v: String| {
                        ui.label(RichText::new(k).color(MUTED));
                        ui.add(egui::Label::new(v).wrap());
                        ui.end_row();
                    };
                    row(ui, "Address", h.ip.clone());
                    row(ui, "Name", h.name.clone().unwrap_or_else(|| if h.named { "no name published".into() } else { "looking up…".into() }));
                    row(ui, "Location", match (&h.country, s.settings.geo_lookup) {
                        (Some(c), _) => c.clone(),
                        (None, false) => "country lookup is off".into(),
                        (None, true) => "looking up…".into(),
                    });
                    row(ui, "Ports", h.ports.iter().map(|p| format!("{} ({})", p, port_name(*p))).collect::<Vec<_>>().join(", "));
                    row(ui, "First seen", fmt_time(h.first_seen));
                    row(ui, "Last seen", ago(Some(h.last_seen)));
                    row(ui, "Connections", h.conns.to_string());
                    if h.rx + h.tx > 0 {
                        row(ui, "Data", format!("{} in · {} out", bytes(h.rx), bytes(h.tx)));
                    }
                });
                if let Some(every) = crate::traffic::beacon_score(&h.starts) {
                    ui.label(RichText::new(format!("Checked in like clockwork: about every {}.", dur(every as i64))).color(AMBER));
                }
                ui.add_space(10.0);
                section_title(ui, "PROGRAMS THAT CONNECT HERE");
                for name in &h.apps {
                    let key = s.traffic.saved.apps.values().find(|a| &a.name == name).map(|a| a.key.clone());
                    if ui.link(name).clicked() {
                        goto_app = key;
                    }
                }
            }
        });
        if close {
            self.sel_app = None;
            self.sel_host = None;
        }
        if let Some(h) = goto_host {
            self.sel_host = Some(h);
            self.sel_app = None;
        }
        if let Some(a) = goto_app {
            self.sel_app = Some(a);
            self.sel_host = None;
        }
    }
}

fn section_title(ui: &mut Ui, t: &str) {
    ui.label(RichText::new(t).size(11.0).color(FAINT).strong());
}

/// A simple clickable table: header plus `n` rows drawn by `row`.
fn table(
    ui: &mut Ui,
    cols: &[(&str, f32)],
    n: usize,
    mut row: impl FnMut(usize, egui::Rect, &egui::Painter, &[f32]),
    mut on_click: impl FnMut(usize),
    selected: Option<usize>,
) {
    let w = ui.available_width();
    egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0f32, BORDER)).corner_radius(10.0).show(ui, |ui| {
        ui.set_width(w);
        let (h, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        let mut xs = vec![];
        let mut acc = 0.0;
        for (_, f) in cols {
            xs.push(h.left() + acc + 14.0);
            acc += f * w;
        }
        for (i, (l, _)) in cols.iter().enumerate() {
            ui.painter().text(pos2(xs[i], h.center().y), Align2::LEFT_CENTER, *l, FontId::proportional(12.5), MUTED);
        }
        ui.painter().line_segment([h.left_bottom(), h.right_bottom()], Stroke::new(1.0f32, BORDER));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, 44.0, n, |ui, range| {
            for i in range {
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 44.0), Sense::click());
                let p = ui.painter_at(rect);
                if selected == Some(i) {
                    p.rect_filled(rect, 0.0, Color32::from_rgb(0x1E, 0x33, 0x52));
                } else if resp.hovered() {
                    p.rect_filled(rect, 0.0, CARD_HI);
                }
                p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0f32, BORDER.linear_multiply(0.5)));
                let rxs: Vec<f32> = xs.iter().map(|x| x - h.left() + rect.left()).collect();
                row(i, rect, &p, &rxs);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    on_click(i);
                }
            }
        });
    });
}

fn export_csv(s: &Shared) -> Result<String, String> {
    let dir = crate::util::documents_dir();
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M");
    let q = |v: &str| format!("\"{}\"", v.replace('"', "\"\""));
    let mut a = String::from("Program,Path,Signature,Signed by,Trusted,First seen,Last active,Connections,Data in,Data out,Destinations\n");
    for x in s.traffic.saved.apps.values() {
        a += &format!(
            "{},{},{},{},{},{},{},{},{},{},{}\n",
            q(&x.name),
            q(&x.path),
            q(x.sig.label()),
            q(x.signer.as_deref().unwrap_or("")),
            x.pinned.is_some(),
            q(&fmt_time(x.first_seen)),
            q(&fmt_time(x.last_seen)),
            x.conns,
            x.rx,
            x.tx,
            x.dests.len()
        );
    }
    let mut h = String::from("Address,Name,Category,Country,Programs,Ports,First seen,Last seen,Connections,Data in,Data out\n");
    for x in s.traffic.saved.hosts.values() {
        h += &format!(
            "{},{},{},{},{},{},{},{},{},{},{}\n",
            x.ip,
            q(x.name.as_deref().unwrap_or("")),
            q(&x.category),
            q(x.country.as_deref().unwrap_or("")),
            q(&x.apps.join("; ")),
            q(&x.ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(" ")),
            q(&fmt_time(x.first_seen)),
            q(&fmt_time(x.last_seen)),
            x.conns,
            x.rx,
            x.tx
        );
    }
    let pa = dir.join(format!("HomeNetWatch-programs-{}.csv", stamp));
    let ph = dir.join(format!("HomeNetWatch-destinations-{}.csv", stamp));
    std::fs::write(&pa, a).map_err(|e| e.to_string())?;
    std::fs::write(&ph, h).map_err(|e| e.to_string())?;
    Ok(format!("{} and {}", pa.display(), ph.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()))
}
