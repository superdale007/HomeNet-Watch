//! Performance page: NAS boxes, routers and computers that share system stats
//! over SNMP — processor, memory, temperature, storage, disks and traffic.

use super::*;
use crate::perf::{bps, bytes, PerfSample};

const NAS_HELP: [(&str, &str); 3] = [
    ("Synology", "Control Panel > Terminal & SNMP > SNMP tab > tick “Enable SNMP service”. Choose SNMPv1/v2c with a community name (for example “public”), or SNMPv3 with a user name and passwords for a secure connection."),
    ("QNAP", "Control Panel > Network & File Services > SNMP > enable the SNMP service with SNMP V1/V2 and a community name, or SNMP V3 with a user name and passwords."),
    ("UGREEN", "In the NAS Control Panel, find the SNMP setting (under network or services), switch it on with v2c and a community name, or v3 with a user name and passwords."),
];

impl App {
    pub(super) fn performance(&mut self, ui: &mut Ui, s: &mut Shared) {
        let watched = s.devices.iter().filter(|d| d.snmp).count();
        self.header(
            ui,
            s,
            "Performance",
            &format!("System stats from {} device{} · NAS storage, disks, temperature and traffic", watched, if watched == 1 { "" } else { "s" }),
            |_this, ui, s| {
                if ui.button("⟳  Read stats now").clicked() {
                    for d in s.devices.iter_mut().filter(|d| d.snmp) {
                        d.perf_last = None;
                    }
                }
            },
        );

        let mut open_device: Option<u64> = None;
        let mut act: Option<CardAction> = None;
        let confirming = self.perf_confirm;
        let mut enable: Option<u64> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let ids: Vec<u64> = {
                let mut v: Vec<&Device> = s.devices.iter().filter(|d| d.snmp).collect();
                v.sort_by_key(|d| (d.kind != DeviceKind::Storage, status_rank(d.status), d.name.to_lowercase()));
                v.iter().map(|d| d.id).collect()
            };
            if ids.is_empty() {
                self.perf_intro(ui, s, &mut enable);
                return;
            }
            let w = ui.available_width();
            let gap = 14.0;
            let two = w > 1000.0;
            let col_w = if two { (w - gap) / 2.0 } else { w };
            let settings = s.settings.clone();
            if two {
                ui.columns(2, |cols| {
                    for (i, id) in ids.iter().enumerate() {
                        if let Some(d) = s.device(*id) {
                            let ui = &mut cols[i % 2];
                            device_card(ui, d, &settings, col_w, confirming == Some(d.id), &mut act);
                            ui.add_space(gap);
                        }
                    }
                });
            } else {
                for id in &ids {
                    if let Some(d) = s.device(*id) {
                        device_card(ui, d, &settings, col_w, confirming == Some(d.id), &mut act);
                        ui.add_space(gap);
                    }
                }
            }
            ui.add_space(6.0);
            self.perf_add_more(ui, s, &mut enable);
        });
        if let Some(id) = enable {
            if let Some(d) = s.device_mut(id) {
                d.snmp = true;
                d.perf_last = None;
                d.perf_error = None;
            }
            s.dirty = true;
        }
        match act {
            Some(CardAction::Open(id)) => open_device = Some(id),
            Some(CardAction::AskRemove(id)) => self.perf_confirm = Some(id),
            Some(CardAction::Cancel) => self.perf_confirm = None,
            Some(CardAction::Remove(id)) => {
                self.perf_confirm = None;
                if let Some(d) = s.device_mut(id) {
                    d.snmp = false;
                    d.perf = None;
                    d.perf_error = None;
                    d.perf_history.clear();
                    d.problems.clear();
                    if d.status == Status::Warning {
                        d.last_check = None; // recheck soon so the warning clears
                    }
                    let name = d.name.clone();
                    self.toast = Some((format!("Stopped reading stats from {}. It's still in your Devices list.", name), Instant::now()));
                }
                s.dirty = true;
            }
            None => {}
        }
        if let Some(id) = open_device {
            self.selected = Some(id);
            self.page = Page::Devices;
        }
    }

    fn perf_intro(&mut self, ui: &mut Ui, s: &Shared, enable: &mut Option<u64>) {
        flow_card(ui, "WATCH YOUR NAS", |ui| {
            ui.label(
                RichText::new("See storage space, disk health, temperature, processor, memory and network traffic for your NAS — and get an alert when a disk fails or a volume fills up.")
                    .size(14.0),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("This uses SNMP, a standard way devices share their stats. It's usually switched off, so turn it on in the NAS first:")
                    .color(MUTED),
            );
            ui.add_space(8.0);
            for (vendor, how) in NAS_HELP {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(vendor).strong().color(GOLD));
                    ui.label(RichText::new(how).color(TEXT));
                });
                ui.add_space(4.0);
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Routers, managed switches and Linux or Windows computers with SNMP switched on work too.").size(12.5).color(MUTED));
        });
        ui.add_space(14.0);
        self.perf_add_more(ui, s, enable);
    }

    fn perf_add_more(&mut self, ui: &mut Ui, s: &Shared, enable: &mut Option<u64>) {
        let mut cands: Vec<&Device> = s.devices.iter().filter(|d| !d.snmp && d.kind != DeviceKind::Internet).collect();
        cands.sort_by_key(|d| {
            let rank = match d.kind {
                DeviceKind::Storage => 0,
                DeviceKind::Router | DeviceKind::NetworkGear => 1,
                DeviceKind::Computer => 2,
                DeviceKind::Printer => 3,
                _ => 4,
            };
            (rank, u32::from(d.ip))
        });
        if cands.is_empty() {
            return;
        }
        flow_card(ui, "READ STATS FROM A DEVICE", |ui| {
            ui.label(RichText::new("Pick a device that has SNMP switched on (community “public”; you can change it in the device's details).").size(12.5).color(MUTED));
            ui.add_space(6.0);
            let likely: Vec<&&Device> = cands.iter().filter(|d| matches!(d.kind, DeviceKind::Storage | DeviceKind::Router | DeviceKind::NetworkGear)).collect();
            ui.horizontal_wrapped(|ui| {
                for d in likely.iter().take(12) {
                    if ui.button(format!("{} {}  ·  {}", d.kind.icon(), d.name, d.ip)).clicked() {
                        *enable = Some(d.id);
                    }
                }
            });
            ui.add_space(4.0);
            let mut pick: Option<u64> = None;
            egui::ComboBox::from_id_salt("perf_pick").width(320.0).selected_text("Any other device…").show_ui(ui, |ui| {
                for d in &cands {
                    if ui.selectable_label(false, format!("{} {}  ·  {}", d.kind.icon(), d.name, d.ip)).clicked() {
                        pick = Some(d.id);
                    }
                }
            });
            if pick.is_some() {
                *enable = pick;
            }
        });
    }
}

enum CardAction {
    Open(u64),
    AskRemove(u64),
    Remove(u64),
    Cancel,
}

fn device_card(ui: &mut Ui, d: &Device, st: &Settings, w: f32, confirming: bool, act: &mut Option<CardAction>) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0f32, if d.problems.iter().any(|p| p.1 == Severity::Critical) { RED.linear_multiply(0.6) } else { BORDER }))
        .corner_radius(10.0)
        .inner_margin(16.0)
        .show(ui, |ui| {
            ui.set_width(w - 34.0);
            // header
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(42.0, 42.0), Sense::hover());
                ui.painter().rect_filled(r, 10.0, status_color(d.status).linear_multiply(0.18));
                ui.painter().text(r.center(), Align2::CENTER_CENTER, d.kind.icon(), FontId::proportional(21.0), status_color(d.status));
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(&d.name).size(16.0).strong());
                    let p = d.perf.as_ref();
                    let mut sub = vec![d.ip.to_string()];
                    if let Some(p) = p {
                        let brand = p.vendor.label();
                        let what = match (brand.is_empty(), p.model.is_empty()) {
                            (false, false) => format!("{} {}", brand, p.model),
                            (false, true) => brand.to_string(),
                            _ => short_descr(&p.descr),
                        };
                        if !what.is_empty() {
                            sub.push(what);
                        }
                        if let Some(u) = p.uptime_secs {
                            sub.push(format!("up {}", dur(u as i64)));
                        }
                    }
                    ui.label(RichText::new(sub.join("  ·  ")).size(12.0).color(MUTED));
                });
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    if ui.small_button("✖ Remove").on_hover_text("Stop reading stats from this device (it stays in your Devices list)").clicked() {
                        *act = Some(CardAction::AskRemove(d.id));
                    }
                    if ui.small_button("Details").clicked() {
                        *act = Some(CardAction::Open(d.id));
                    }
                    pill(ui, d.status.label(), status_color(d.status));
                });
            });
            if confirming {
                ui.add_space(6.0);
                egui::Frame::new().fill(CARD_HI).corner_radius(8.0).inner_margin(10.0).show(ui, |ui| {
                    ui.label(RichText::new(format!("Stop reading stats from {}?", d.name)).strong());
                    ui.label(
                        RichText::new("It disappears from this page, but stays in Devices and is still checked for being online. You can add it back below or in its details.")
                            .size(12.0)
                            .color(MUTED),
                    );
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Remove from Performance").color(RED)).clicked() {
                            *act = Some(CardAction::Remove(d.id));
                        }
                        if ui.button("Cancel").clicked() {
                            *act = Some(CardAction::Cancel);
                        }
                    });
                });
            }
            ui.add_space(8.0);

            let Some(p) = d.perf.as_ref() else {
                match &d.perf_error {
                    Some(e) => {
                        ui.label(RichText::new(format!("No stats: {}", e)).color(AMBER));
                        ui.label(
                            RichText::new("Check SNMP is switched on in the device (v1/v2c) and that the community name matches the one in its details.")
                                .size(12.0)
                                .color(MUTED),
                        );
                    }
                    None => {
                        ui.label(RichText::new("Reading stats…").color(MUTED));
                    }
                }
                return;
            };
            if let Some(e) = &d.perf_error {
                ui.label(RichText::new(format!("Last read failed: {} (showing earlier stats)", e)).size(12.0).color(AMBER));
            }

            // problems
            for (_, sev, msg) in &d.problems {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if *sev == Severity::Critical { "✖" } else { "⚠" }).color(severity_color(*sev)));
                    ui.label(RichText::new(msg).color(severity_color(*sev)).strong());
                });
            }
            if !d.problems.is_empty() {
                ui.add_space(4.0);
            }

            // gauges
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                gauge(ui, p.cpu_pct, 100.0, st.cpu_warn as f32, p.cpu_pct.map(|v| format!("{:.0}%", v)), "Processor");
                gauge(ui, p.mem_pct, 100.0, 95.0, p.mem_pct.map(|v| format!("{:.0}%", v)), &match p.mem_total {
                    Some(t) => format!("Memory · {}", bytes(t)),
                    None => "Memory".into(),
                });
                gauge(ui, p.temp_c, 100.0, st.sys_temp_warn as f32, p.temp_c.map(|v| format!("{:.0}°C", v)), "Temperature");
                if let Some(ok) = p.system_ok {
                    ui.vertical(|ui| {
                        ui.add_space(26.0);
                        pill(ui, if ok { "System normal" } else { "System problem" }, if ok { GREEN } else { RED });
                    });
                }
            });
            ui.add_space(8.0);

            // storage
            if !p.volumes.is_empty() {
                section_title(ui, "STORAGE");
                // volumes first (where files live), then pools
                let mut vols: Vec<&crate::perf::Volume> = p.volumes.iter().collect();
                vols.sort_by_key(|v| v.is_pool);
                for v in vols {
                    let pct = v.pct();
                    let col = if !v.healthy {
                        RED
                    } else if v.is_pool {
                        GRAY
                    } else if pct >= st.disk_warn as f32 {
                        AMBER
                    } else if pct >= st.disk_warn as f32 - 10.0 {
                        GOLD
                    } else {
                        BLUE
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&v.name).strong());
                        if let Some(status) = &v.status {
                            pill(ui, status, if v.healthy { GREEN } else { RED });
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let free = v.total.saturating_sub(v.used);
                            let txt = if v.is_pool {
                                if pct >= 99.0 {
                                    format!("{} pool · all assigned to volumes", bytes(v.total))
                                } else {
                                    format!("{} pool · {} not yet assigned", bytes(v.total), bytes(free))
                                }
                            } else {
                                format!("{} free of {}", bytes(free), bytes(v.total))
                            };
                            ui.label(RichText::new(txt).size(12.0).color(MUTED))
                                .on_hover_text(if v.is_pool { "A storage pool is space set aside for volumes. Being fully assigned is normal; watch the volumes for free space." } else { "Free space on this volume" });
                        });
                    });
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 12.0), Sense::hover());
                    ui.painter().rect_filled(r, 6.0, Color32::from_rgb(0x0F, 0x16, 0x22));
                    let mut fr = r;
                    fr.set_width(r.width() * (pct / 100.0).clamp(0.0, 1.0));
                    ui.painter().rect_filled(fr, 6.0, col);
                    let tag = if v.is_pool { format!("{:.0}% assigned", pct) } else { format!("{:.0}%", pct) };
                    ui.painter().text(r.right_center() + vec2(-6.0, 0.0), Align2::RIGHT_CENTER, tag, FontId::proportional(10.0), TEXT);
                    ui.add_space(4.0);
                }
                ui.add_space(4.0);
            }

            // disks
            if !p.disks.is_empty() {
                section_title(ui, "DISKS");
                egui::Grid::new(("disks", d.id)).num_columns(4).spacing([14.0, 4.0]).show(ui, |ui| {
                    for k in &p.disks {
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
                            ui.painter().circle_filled(r.center(), 4.0, if k.healthy { GREEN } else { RED });
                            ui.label(&k.name);
                        });
                        ui.label(RichText::new(k.model.trim()).size(12.0).color(MUTED));
                        let tc = match k.temp_c {
                            Some(t) if t >= st.temp_warn as f32 => AMBER,
                            _ => TEXT,
                        };
                        ui.label(RichText::new(k.temp_c.map(|t| format!("{:.0}°C", t)).unwrap_or_else(|| "—".into())).color(tc));
                        ui.label(RichText::new(&k.status).size(12.0).color(if k.healthy { MUTED } else { RED }));
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
            }

            // network
            if !p.ifaces.is_empty() {
                section_title(ui, "NETWORK TRAFFIC");
                for i in &p.ifaces {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&i.name).monospace().size(12.0));
                        ui.label(RichText::new(format!("in {}", bps(i.rx_bps))).color(BLUE));
                        ui.label(RichText::new(format!("out {}", bps(i.tx_bps))).color(GOLD));
                    });
                }
                ui.add_space(4.0);
            }

            // history
            let hist: Vec<PerfSample> = d.perf_history.iter().cloned().collect();
            if hist.len() >= 2 {
                section_title(ui, "LAST HOUR");
                trend(ui, &hist);
            }
        });
}

fn section_title(ui: &mut Ui, t: &str) {
    ui.label(RichText::new(t).size(11.0).color(FAINT).strong());
}

fn short_descr(d: &str) -> String {
    let first = d.lines().next().unwrap_or("").trim();
    if first.chars().count() > 60 {
        first.chars().take(59).collect::<String>() + "…"
    } else {
        first.to_string()
    }
}

/// Ring gauge with the value in the middle.
fn gauge(ui: &mut Ui, v: Option<f32>, max: f32, warn: f32, text: Option<String>, label: &str) {
    ui.vertical(|ui| {
        let size: f32 = 86.0;
        let (rect, _) = ui.allocate_exact_size(vec2(size.max(label.len() as f32 * 6.2), size), Sense::hover());
        let p = ui.painter_at(rect.expand(2.0));
        let c = pos2(rect.center().x, rect.top() + size / 2.0);
        let r = size / 2.0 - 8.0;
        let start = std::f32::consts::PI * 0.75;
        let sweep_all = std::f32::consts::PI * 1.5;
        let arc = |frac: f32, col: Color32| {
            let steps = 48;
            let pts: Vec<egui::Pos2> = (0..=steps)
                .map(|i| {
                    let a = start + sweep_all * frac * i as f32 / steps as f32;
                    c + vec2(a.cos(), a.sin()) * r
                })
                .collect();
            p.add(egui::Shape::line(pts, Stroke::new(9.0f32, col)));
        };
        arc(1.0, Color32::from_rgb(0x0F, 0x16, 0x22));
        if let Some(v) = v {
            let col = if v >= warn { RED } else if v >= warn * 0.85 { AMBER } else { GREEN };
            arc((v / max).clamp(0.01, 1.0), col);
        }
        p.text(c, Align2::CENTER_CENTER, text.unwrap_or_else(|| "—".into()), FontId::proportional(17.0), TEXT);
        p.text(pos2(c.x, rect.bottom() - 4.0), Align2::CENTER_BOTTOM, label, FontId::proportional(11.0), MUTED);
    });
}

/// Processor & memory (0-100%) over the last hour, plus total traffic.
fn trend(ui: &mut Ui, hist: &[PerfSample]) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 70.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 6.0, Color32::from_rgb(0x0F, 0x16, 0x22));
    let t1 = hist.last().map(|h| h.t).unwrap_or(0);
    let t0 = t1 - 3600;
    let x = |t: i64| rect.left() + rect.width() * ((t - t0) as f32 / 3600.0).clamp(0.0, 1.0);
    let line = |vals: Vec<(i64, f32)>, maxv: f32, col: Color32| {
        let pts: Vec<egui::Pos2> = vals.iter().map(|(t, v)| pos2(x(*t), rect.bottom() - 4.0 - (rect.height() - 8.0) * (v / maxv).clamp(0.0, 1.0))).collect();
        if pts.len() > 1 {
            p.add(egui::Shape::line(pts, Stroke::new(1.8f32, col)));
        }
    };
    let traffic: Vec<(i64, f32)> = hist.iter().filter(|h| h.t >= t0).filter_map(|h| Some((h.t, (h.rx? + h.tx.unwrap_or(0.0)) as f32))).collect();
    let tmax = traffic.iter().map(|x| x.1).fold(1.0, f32::max);
    line(traffic, tmax * 1.1, PURPLE_SOFT);
    line(hist.iter().filter(|h| h.t >= t0).filter_map(|h| Some((h.t, h.mem?))).collect(), 100.0, BLUE);
    line(hist.iter().filter(|h| h.t >= t0).filter_map(|h| Some((h.t, h.cpu?))).collect(), 100.0, GREEN);
    ui.horizontal(|ui| {
        for (l, c) in [("Processor %", GREEN), ("Memory %", BLUE), ("Traffic", PURPLE_SOFT)] {
            let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
            ui.painter().rect_filled(r.shrink(2.0), 2.0, c);
            ui.label(RichText::new(l).size(11.5).color(MUTED));
            ui.add_space(6.0);
        }
    });
}

const PURPLE_SOFT: Color32 = Color32::from_rgb(0xA7, 0x7B, 0xF3);
