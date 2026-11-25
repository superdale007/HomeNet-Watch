//! Devices, device detail, network map, alerts, reports, settings and dialogs.

use super::*;

// ───────────────────────────── devices ─────────────────────────────
impl App {
    pub(super) fn devices(&mut self, ui: &mut Ui, s: &mut Shared) {
        let n = s.devices.len();
        self.header(ui, s, "Devices", &format!("{} devices on your network", n), |this, ui, _s| {
            if ui.button("+  Add device").clicked() {
                this.add = Some(AddDialog { ip: String::new(), name: String::new(), kind: DeviceKind::Unknown, error: String::new() });
            }
        });

        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("🔎 Search name, address, maker…").desired_width(280.0));
            ui.add_space(10.0);
            let count = |f: Filter| s.devices.iter().filter(|d| filter_match(f, d)).count();
            for (f, l) in [(Filter::All, "All"), (Filter::Online, "Online"), (Filter::Problems, "Needs attention"), (Filter::Offline, "Offline")] {
                if ui.selectable_label(self.filter == f, format!("{} ({})", l, count(f))).clicked() {
                    self.filter = f;
                }
            }
            let mut groups: Vec<String> = s.devices.iter().map(|d| d.group.trim().to_string()).filter(|g| !g.is_empty()).collect();
            groups.sort();
            groups.dedup();
            if !groups.is_empty() {
                ui.add_space(10.0);
                let label = if self.group_filter.is_empty() { "All rooms".to_string() } else { self.group_filter.clone() };
                egui::ComboBox::from_id_salt("group_filter").selected_text(label).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.group_filter, String::new(), "All rooms");
                    for g in groups {
                        ui.selectable_value(&mut self.group_filter, g.clone(), g);
                    }
                });
            }
        });
        ui.add_space(10.0);

        let q = self.search.to_lowercase();
        let gf = self.group_filter.clone();
        let mut rows: Vec<&Device> = s
            .devices
            .iter()
            .filter(|d| filter_match(self.filter, d))
            .filter(|d| gf.is_empty() || d.group.trim() == gf)
            .filter(|d| {
                q.is_empty()
                    || d.name.to_lowercase().contains(&q)
                    || d.ip.to_string().contains(&q)
                    || d.maker.to_lowercase().contains(&q)
                    || d.kind.label().to_lowercase().contains(&q)
                    || d.mac.as_deref().unwrap_or("").to_lowercase().contains(&q)
            })
            .collect();
        let (col, desc) = self.sort;
        rows.sort_by(|a, b| {
            let o = match col {
                SortCol::Status => status_rank(a.status).cmp(&status_rank(b.status)).then(u32::from(a.ip).cmp(&u32::from(b.ip))),
                SortCol::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortCol::Ip => u32::from(a.ip).cmp(&u32::from(b.ip)),
                SortCol::Kind => a.kind.label().cmp(b.kind.label()),
                SortCol::Response => a.latency.unwrap_or(f32::MAX).total_cmp(&b.latency.unwrap_or(f32::MAX)),
                SortCol::Uptime => a.uptime_pct().unwrap_or(-1.0).total_cmp(&b.uptime_pct().unwrap_or(-1.0)),
                SortCol::LastSeen => b.last_seen.cmp(&a.last_seen),
            };
            if desc {
                o.reverse()
            } else {
                o
            }
        });

        let w = ui.available_width();
        let mut cols: [(Option<SortCol>, &str, f32); 8] = [
            (Some(SortCol::Status), "Status", 0.12),
            (Some(SortCol::Name), "Name", 0.21),
            (Some(SortCol::Ip), "Address", 0.13),
            (Some(SortCol::Kind), "Type", 0.12),
            (None, "Maker", 0.12),
            (Some(SortCol::Response), "Response", 0.09),
            (Some(SortCol::Uptime), "Uptime", 0.09),
            (Some(SortCol::LastSeen), "Last seen", 0.12),
        ];
        // When space is tight (details panel open), drop the less important columns.
        if w < 900.0 {
            cols[4].2 = 0.0;
            cols[7].2 = 0.0;
        }
        let total: f32 = cols.iter().map(|c| c.2).sum();
        for c in cols.iter_mut() {
            c.2 /= total;
        }
        let mut xs = vec![];
        let mut acc = 0.0;
        for c in cols.iter() {
            xs.push(acc);
            acc += c.2 * w;
        }

        egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0f32, BORDER)).corner_radius(10.0).show(ui, |ui| {
            ui.set_width(w);
            let (hrect, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
            ui.painter().line_segment([hrect.left_bottom(), hrect.right_bottom()], Stroke::new(1.0f32, BORDER));
            for (i, (c, label, frac)) in cols.iter().enumerate() {
                if *frac == 0.0 {
                    continue;
                }
                let r = egui::Rect::from_min_size(pos2(hrect.left() + xs[i], hrect.top()), vec2(frac * w, 36.0));
                let Some(c) = c else {
                    ui.painter().text(pos2(r.left() + 14.0, r.center().y), Align2::LEFT_CENTER, *label, FontId::proportional(12.5), MUTED);
                    continue;
                };
                let resp = ui.interact(r, ui.id().with(("hdr", i)), Sense::click());
                let active = self.sort.0 == *c;
                let tc = if resp.hovered() || active { TEXT } else { MUTED };
                let tr = ui.painter().text(pos2(r.left() + 14.0, r.center().y), Align2::LEFT_CENTER, *label, FontId::proportional(12.5), tc);
                if active {
                    // small triangle: up = ascending, down = descending
                    let cx = tr.right() + 9.0;
                    let cy = r.center().y;
                    let pts = if self.sort.1 {
                        vec![pos2(cx - 4.0, cy - 2.5), pos2(cx + 4.0, cy - 2.5), pos2(cx, cy + 3.0)]
                    } else {
                        vec![pos2(cx - 4.0, cy + 2.5), pos2(cx + 4.0, cy + 2.5), pos2(cx, cy - 3.0)]
                    };
                    ui.painter().add(egui::Shape::convex_polygon(pts, BLUE, Stroke::NONE));
                }
                if resp.clicked() {
                    if self.sort.0 == *c {
                        self.sort.1 = !self.sort.1;
                    } else {
                        self.sort = (*c, false);
                    }
                }
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if rows.is_empty() {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(if s.scan.running { "Looking for devices…" } else { "No devices match" }).color(MUTED));
                    });
                }
                for d in &rows {
                    let (rect, resp) = ui.allocate_exact_size(vec2(w, 46.0), Sense::click());
                    let p = ui.painter_at(rect);
                    let sel = self.selected == Some(d.id);
                    if sel {
                        p.rect_filled(rect, 0.0, Color32::from_rgb(0x1E, 0x33, 0x52));
                    } else if resp.hovered() {
                        p.rect_filled(rect, 0.0, CARD_HI);
                    }
                    p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0f32, BORDER.linear_multiply(0.6)));
                    let y = rect.center().y;
                    let x = |i: usize| rect.left() + xs[i] + 14.0;
                    let clip = |i: usize| egui::Rect::from_min_max(pos2(rect.left() + xs[i], rect.top()), pos2(rect.left() + xs[i] + cols[i].2 * w - 6.0, rect.bottom()));
                    dot(&p, pos2(x(0) + 6.0, y), status_color(d.status));
                    p.with_clip_rect(clip(0)).text(pos2(x(0) + 20.0, y), Align2::LEFT_CENTER, d.status.label(), FontId::proportional(13.0), status_color(d.status));
                    let pc = p.with_clip_rect(clip(1));
                    pc.text(pos2(x(1), y), Align2::LEFT_CENTER, d.kind.icon(), FontId::proportional(16.0), MUTED);
                    pc.text(pos2(x(1) + 26.0, y - 8.0), Align2::LEFT_CENTER, &d.name, FontId::proportional(13.5), TEXT);
                    let mut parts = vec![];
                    if d.ping {
                        parts.push("ping".to_string());
                    }
                    if d.arp {
                        parts.push("presence".to_string());
                    }
                    if d.web.is_some() {
                        parts.push("web".to_string());
                    }
                    if !d.dns_name.is_empty() {
                        parts.push("dns".to_string());
                    }
                    if d.snmp {
                        parts.push("stats".to_string());
                    }
                    if !d.group.trim().is_empty() {
                        parts.push(d.group.trim().to_string());
                    }
                    if !d.ports.is_empty() {
                        parts.push(format!("ports {}", d.ports.iter().map(|p| p.port.to_string()).collect::<Vec<_>>().join(", ")));
                    }
                    if !d.alerts {
                        parts.push("alerts off".into());
                    }
                    pc.text(pos2(x(1) + 26.0, y + 9.0), Align2::LEFT_CENTER, parts.join(" · "), FontId::proportional(11.0), FAINT);
                    p.with_clip_rect(clip(2)).text(pos2(x(2), y), Align2::LEFT_CENTER, d.ip.to_string(), FontId::proportional(13.0), TEXT);
                    p.with_clip_rect(clip(3)).text(pos2(x(3), y), Align2::LEFT_CENTER, d.kind.label(), FontId::proportional(13.0), MUTED);
                    let maker = if d.maker.is_empty() { "—" } else { d.maker.as_str() };
                    if cols[4].2 > 0.0 {
                        p.with_clip_rect(clip(4)).text(pos2(x(4), y), Align2::LEFT_CENTER, maker, FontId::proportional(13.0), MUTED);
                    }
                    let lat_col = match d.latency {
                        Some(l) if l > s.settings.slow_ms as f32 => AMBER,
                        Some(_) => TEXT,
                        None => FAINT,
                    };
                    p.text(pos2(x(5), y), Align2::LEFT_CENTER, ms(d.latency), FontId::proportional(13.0), lat_col);
                    let up = d.uptime_pct().map(|v| format!("{:.1}%", v)).unwrap_or_else(|| "—".into());
                    p.text(pos2(x(6), y), Align2::LEFT_CENTER, up, FontId::proportional(13.0), TEXT);
                    if cols[7].2 > 0.0 {
                        p.with_clip_rect(clip(7)).text(pos2(x(7), y), Align2::LEFT_CENTER, ago(d.last_seen), FontId::proportional(12.5), MUTED);
                    }
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.selected = if sel { None } else { Some(d.id) };
                    }
                }
            });
        });
    }
}

fn filter_match(f: Filter, d: &Device) -> bool {
    match f {
        Filter::All => true,
        Filter::Online => matches!(d.status, Status::Up | Status::Warning),
        Filter::Problems => matches!(d.status, Status::Warning | Status::Down),
        Filter::Offline => d.status == Status::Down,
    }
}

fn check_state(ui: &mut Ui, st: &Option<Result<String, String>>) {
    let (c, t) = match st {
        Some(Ok(m)) => (GREEN, m.clone()),
        Some(Err(e)) => (RED, e.clone()),
        None => (BLUE, "waiting for the next check".into()),
    };
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 4.0, c);
        ui.label(RichText::new(t).size(12.0).color(c));
    });
}

fn section(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(11.0).color(FAINT).strong());
    ui.add_space(2.0);
}

// ───────────────────────────── device detail ─────────────────────────────
impl App {
    pub(super) fn detail(&mut self, ui: &mut Ui, s: &mut Shared, id: u64) {
        let slow_ms = s.settings.slow_ms as f32;
        let mut close = false;
        let mut check = false;
        let mut changed = false;
        let mut remove = false;
        let mut open_perf = false;
        let mut groups: Vec<String> = s.devices.iter().map(|d| d.group.trim().to_string()).filter(|g| !g.is_empty()).collect();
        groups.sort();
        groups.dedup();
        let dev_events: Vec<Event> = s.events.iter().filter(|e| e.device_id == Some(id)).take(8).cloned().collect();
        {
            let Some(d) = s.device_mut(id) else { return };
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                    ui.painter().rect_filled(r, 12.0, status_color(d.status).linear_multiply(0.18));
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, d.kind.icon(), FontId::proportional(24.0), status_color(d.status));
                    ui.vertical(|ui| {
                        ui.set_max_width(220.0);
                        ui.label(RichText::new(&d.name).size(18.0).strong());
                        pill(ui, d.status.label(), status_color(d.status));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        if ui.small_button("✖").on_hover_text("Close").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("⟳  Check now").clicked() {
                        check = true;
                    }
                    let ptxt = if d.paused { "▶  Resume" } else { "⏸  Pause" };
                    if ui.button(ptxt).clicked() {
                        d.paused = !d.paused;
                        if !d.paused {
                            d.status = Status::Unknown;
                            d.last_check = None;
                        }
                        changed = true;
                    }
                    if ui.button(RichText::new("🗑  Remove").color(RED)).clicked() {
                        remove = true;
                    }
                });
                ui.add_space(12.0);

                section(ui, "RESPONSE");
                let (mn, avg, mx) = d.latency_stats();
                ui.horizontal(|ui| {
                    for (l, v) in [("Now", d.latency), ("Average", avg), ("Best", mn), ("Worst", mx)] {
                        ui.vertical(|ui| {
                            ui.set_width(70.0);
                            ui.label(RichText::new(l).size(11.0).color(FAINT));
                            ui.label(RichText::new(ms(v)).size(15.0).strong());
                        });
                    }
                });
                ui.add_space(6.0);
                let samples: Vec<Sample> = d.history.iter().cloned().collect();
                history_strip(ui, vec2(ui.available_width(), 46.0), &samples, slow_ms);
                ui.label(RichText::new("Recent checks · green answered, amber slow, red missed").size(11.0).color(FAINT));
                ui.add_space(12.0);

                section(ui, "DETAILS");
                egui::Grid::new("info").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label(RichText::new("Name").color(MUTED));
                    if ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(210.0)).changed() {
                        changed = true;
                    }
                    ui.end_row();
                    ui.label(RichText::new("Type").color(MUTED));
                    egui::ComboBox::from_id_salt("kind")
                        .width(210.0)
                        .selected_text(format!("{} {}", d.kind.icon(), d.kind.label()))
                        .show_ui(ui, |ui| {
                            for k in DeviceKind::ALL {
                                if ui.selectable_value(&mut d.kind, k, format!("{} {}", k.icon(), k.label())).changed() {
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();
                    ui.label(RichText::new("Address").color(MUTED));
                    ui.label(RichText::new(d.ip.to_string()).monospace());
                    ui.end_row();
                    ui.label(RichText::new("Hardware").color(MUTED));
                    ui.label(RichText::new(d.mac.clone().unwrap_or_else(|| "—".into())).monospace());
                    ui.end_row();
                    ui.label(RichText::new("Maker").color(MUTED));
                    if ui.add(egui::TextEdit::singleline(&mut d.maker).hint_text("unknown").desired_width(210.0)).changed() {
                        changed = true;
                    }
                    ui.end_row();
                    ui.label(RichText::new("Room / group").color(MUTED));
                    ui.horizontal(|ui| {
                        if ui.add(egui::TextEdit::singleline(&mut d.group).hint_text("e.g. Office").desired_width(170.0)).changed() {
                            changed = true;
                        }
                        if !groups.is_empty() {
                            ui.menu_button("Pick", |ui| {
                                for g in &groups {
                                    if ui.button(g).clicked() {
                                        d.group = g.clone();
                                        changed = true;
                                        ui.close_menu();
                                    }
                                }
                            });
                        }
                    });
                    ui.end_row();
                    ui.label(RichText::new("First seen").color(MUTED));
                    ui.label(fmt_time(d.first_seen));
                    ui.end_row();
                    ui.label(RichText::new("Last seen").color(MUTED));
                    ui.label(ago(d.last_seen));
                    ui.end_row();
                    ui.label(RichText::new("Uptime").color(MUTED));
                    ui.label(match d.uptime_pct() {
                        Some(v) => format!("{:.2}%  ({} of {} checks)", v, d.ok_checks, d.checks),
                        None => "—".into(),
                    });
                    ui.end_row();
                    ui.label(RichText::new("Outages").color(MUTED));
                    let live = d.down_since.map(|t| now() - t).unwrap_or(0);
                    ui.label(format!("{}  ·  offline {} in total", d.outages, dur(d.downtime_secs + live)));
                    ui.end_row();
                });
                ui.add_space(12.0);

                section(ui, "WHAT TO WATCH");
                if ui.checkbox(&mut d.arp, "Check it is present (for devices that ignore ping)").changed() {
                    changed = true;
                }
                if ui.checkbox(&mut d.ping, "Answers to ping").changed() {
                    changed = true;
                }
                if ui.checkbox(&mut d.alerts, "Alert me when it goes offline or slows down").changed() {
                    changed = true;
                }
                ui.label(RichText::new("Services (ports) to check:").size(12.5).color(MUTED));
                let mut remove_port = None;
                for (i, pc) in d.ports.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let (c, t) = match pc.open {
                            Some(true) => (GREEN, "answering"),
                            Some(false) => (RED, "not answering"),
                            None => (BLUE, "waiting"),
                        };
                        let (r, _) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.0, c);
                        ui.label(format!("{} ({})", port_name(pc.port), pc.port));
                        ui.label(RichText::new(t).size(12.0).color(c));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("✖").clicked() {
                                remove_port = Some(i);
                            }
                        });
                    });
                }
                if d.ports.is_empty() {
                    ui.label(RichText::new("none").size(12.0).color(FAINT));
                }
                if let Some(i) = remove_port {
                    d.ports.remove(i);
                    changed = true;
                }
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.new_port).hint_text("port, e.g. 80").desired_width(110.0));
                    if ui.button("Add").clicked() {
                        if let Ok(p) = self.new_port.trim().parse::<u16>() {
                            if p > 0 && !d.ports.iter().any(|x| x.port == p) {
                                d.ports.push(PortCheck { port: p, open: None });
                                changed = true;
                                check = true;
                            }
                        }
                        self.new_port.clear();
                    }
                });
                let suggestions: Vec<u16> = d.open_ports_seen.iter().cloned().filter(|p| !d.ports.iter().any(|x| x.port == *p)).collect();
                if !suggestions.is_empty() {
                    ui.label(RichText::new("Answering during the last scan:").size(11.5).color(FAINT));
                    ui.horizontal_wrapped(|ui| {
                        for p in suggestions {
                            if ui.small_button(format!("+ {} ({})", port_name(p), p)).clicked() {
                                d.ports.push(PortCheck { port: p, open: None });
                                changed = true;
                                check = true;
                            }
                        }
                    });
                }

                // Web page check
                ui.add_space(8.0);
                let mut web_on = d.web.is_some();
                if ui.checkbox(&mut web_on, "Check a web page loads (http)").changed() {
                    d.web = if web_on {
                        let port = [80u16, 8080, 5000, 9999].into_iter().find(|p| d.open_ports_seen.contains(p)).unwrap_or(80);
                        Some(WebCheck { port, path: "/".into(), expect: String::new() })
                    } else {
                        None
                    };
                    d.web_state = None;
                    changed = true;
                    check = true;
                }
                if let Some(w) = d.web.as_mut() {
                    egui::Grid::new("web").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                        ui.label(RichText::new("Port").size(12.5).color(MUTED));
                        let mut port = w.port.to_string();
                        if ui.add(egui::TextEdit::singleline(&mut port).desired_width(70.0)).changed() {
                            if let Ok(p) = port.trim().parse::<u16>() {
                                w.port = p;
                                changed = true;
                            }
                        }
                        ui.end_row();
                        ui.label(RichText::new("Page").size(12.5).color(MUTED));
                        if ui.add(egui::TextEdit::singleline(&mut w.path).desired_width(200.0)).changed() {
                            changed = true;
                        }
                        ui.end_row();
                        ui.label(RichText::new("Must show").size(12.5).color(MUTED));
                        if ui.add(egui::TextEdit::singleline(&mut w.expect).hint_text("optional words").desired_width(200.0)).changed() {
                            changed = true;
                        }
                        ui.end_row();
                    });
                    check_state(ui, &d.web_state);
                }

                // DNS check
                let mut dns_on = !d.dns_name.is_empty();
                if ui.checkbox(&mut dns_on, "Check it can look up web addresses (DNS)").changed() {
                    d.dns_name = if dns_on { "example.com".into() } else { String::new() };
                    d.dns_state = None;
                    changed = true;
                    check = true;
                }
                if !d.dns_name.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Look up").size(12.5).color(MUTED));
                        if ui.add(egui::TextEdit::singleline(&mut d.dns_name).desired_width(200.0)).changed() {
                            changed = true;
                        }
                    });
                    check_state(ui, &d.dns_state);
                }

                // System stats (SNMP)
                ui.add_space(12.0);
                section(ui, "SYSTEM STATS (NAS, ROUTER, COMPUTER)");
                if ui.checkbox(&mut d.snmp, "Read storage, disks, temperature, processor and traffic (SNMP)").changed() {
                    d.perf_last = None;
                    d.perf_error = None;
                    if !d.snmp {
                        d.perf = None;
                        d.problems.clear();
                    }
                    changed = true;
                }
                if d.snmp {
                    use crate::snmpv3::{AuthProto, PrivProto};
                    let mut reread = false;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Version").size(12.5).color(MUTED));
                        reread |= ui.selectable_value(&mut d.snmp_version, 2, "v1 / v2c").changed();
                        reread |= ui.selectable_value(&mut d.snmp_version, 3, "v3 (secure)").changed();
                    });
                    if d.snmp_version == 3 {
                        egui::Grid::new("snmpv3").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                            ui.label(RichText::new("User name").size(12.5).color(MUTED));
                            reread |= ui.add(egui::TextEdit::singleline(&mut d.snmp_user).desired_width(170.0)).lost_focus();
                            ui.end_row();
                            ui.label(RichText::new("Authentication").size(12.5).color(MUTED));
                            egui::ComboBox::from_id_salt("snmp_auth").width(170.0).selected_text(d.snmp_auth.label()).show_ui(ui, |ui| {
                                for a in AuthProto::ALL {
                                    reread |= ui.selectable_value(&mut d.snmp_auth, a, a.label()).changed();
                                }
                            });
                            ui.end_row();
                            if d.snmp_auth != AuthProto::None {
                                ui.label(RichText::new("Auth password").size(12.5).color(MUTED));
                                reread |= ui.add(egui::TextEdit::singleline(&mut d.snmp_auth_pass).password(true).desired_width(170.0)).lost_focus();
                                ui.end_row();
                                ui.label(RichText::new("Privacy").size(12.5).color(MUTED));
                                egui::ComboBox::from_id_salt("snmp_priv").width(170.0).selected_text(d.snmp_priv.label()).show_ui(ui, |ui| {
                                    for p in PrivProto::ALL {
                                        reread |= ui.selectable_value(&mut d.snmp_priv, p, p.label()).changed();
                                    }
                                });
                                ui.end_row();
                                if d.snmp_priv != PrivProto::None {
                                    ui.label(RichText::new("Privacy password").size(12.5).color(MUTED));
                                    reread |= ui.add(egui::TextEdit::singleline(&mut d.snmp_priv_pass).password(true).desired_width(170.0)).lost_focus();
                                    ui.end_row();
                                }
                            }
                        });
                        let level = match (d.snmp_auth, d.snmp_priv) {
                            (AuthProto::None, _) => "No password, not encrypted",
                            (_, PrivProto::None) => "Password checked, not encrypted",
                            _ => "Password checked and encrypted (most secure)",
                        };
                        ui.label(RichText::new(level).size(11.5).color(FAINT));
                        #[cfg(windows)]
                        ui.label(RichText::new("Passwords are saved encrypted for your Windows account.").size(11.5).color(FAINT));
                        #[cfg(not(windows))]
                        ui.label(RichText::new("Passwords are saved in your private settings file.").size(11.5).color(FAINT));
                    } else {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Community").size(12.5).color(MUTED));
                            reread |= ui.add(egui::TextEdit::singleline(&mut d.snmp_community).desired_width(130.0)).lost_focus();
                        });
                    }
                    if reread {
                        d.perf_last = None;
                        d.perf_error = None;
                        changed = true;
                    }
                    match (&d.perf, &d.perf_error) {
                        (_, Some(e)) => {
                            ui.label(RichText::new(e).size(12.0).color(AMBER));
                        }
                        (Some(p), None) => {
                            let mut bits = vec![];
                            if let Some(c) = p.cpu_pct {
                                bits.push(format!("CPU {:.0}%", c));
                            }
                            if let Some(m) = p.mem_pct {
                                bits.push(format!("memory {:.0}%", m));
                            }
                            if let Some(t) = p.temp_c {
                                bits.push(format!("{:.0}°C", t));
                            }
                            if !p.volumes.is_empty() {
                                bits.push(format!("{} volume{}", p.volumes.len(), if p.volumes.len() == 1 { "" } else { "s" }));
                            }
                            ui.label(RichText::new(bits.join(" · ")).size(12.5).color(GREEN));
                        }
                        (None, None) => {
                            ui.label(RichText::new("Reading…").size(12.0).color(MUTED));
                        }
                    }
                    for (_, sev, msg) in &d.problems {
                        ui.label(RichText::new(msg).size(12.5).color(severity_color(*sev)));
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Test sign-in now").on_hover_text("Signs in with the settings above right away and tells you what happened").clicked() {
                            let (ip, sec, out) = (d.ip, d.snmp_security(), self.test_result.clone());
                            self.toast = Some(("Testing the sign-in…".into(), Instant::now()));
                            std::thread::spawn(move || {
                                let msg = match super::checkup::snmp_test(ip, sec) {
                                    Ok(m) => m,
                                    Err(e) => format!("Sign-in failed: {}", e),
                                };
                                *out.lock().unwrap() = Some(msg);
                            });
                            d.perf_last = None; // also refresh the stats now
                        }
                        if ui.link("Open the performance view").clicked() {
                            open_perf = true;
                        }
                    });
                } else if d.kind == DeviceKind::Storage {
                    ui.label(RichText::new("Turn on SNMP in your NAS settings first; the Performance page explains how for Synology, QNAP and UGREEN.").size(12.0).color(MUTED));
                }

                // Recent events (state change timeline)
                if !dev_events.is_empty() {
                    ui.add_space(12.0);
                    section(ui, "RECENT EVENTS");
                    for e in &dev_events {
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(4.0, 28.0), Sense::hover());
                            ui.painter().rect_filled(r, 2.0, severity_color(e.severity));
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.label(RichText::new(&e.message).size(12.5));
                                ui.label(RichText::new(fmt_time(e.t)).size(10.5).color(FAINT));
                            });
                        });
                    }
                }
                ui.add_space(12.0);
                section(ui, "NOTES");
                if ui
                    .add(egui::TextEdit::multiline(&mut d.notes).desired_rows(3).desired_width(f32::INFINITY).hint_text("Where it is, who owns it…"))
                    .changed()
                {
                    changed = true;
                }
            });
            if check {
                d.last_check = None;
            }
        }
        if remove {
            self.confirm_remove = Some(id);
        }
        if open_perf {
            self.page = Page::Performance;
        }
        if close {
            self.selected = None;
        }
        if changed || check {
            s.dirty = true;
        }
    }
}

// ───────────────────────────── network map ─────────────────────────────
impl App {
    pub(super) fn map(&mut self, ui: &mut Ui, s: &mut Shared) {
        let mut relayout = false;
        self.header(ui, s, "Network map", "How your devices connect · drag to move · click for details", |_this, ui, s| {
            if ui.button("✨  Arrange automatically").clicked() {
                relayout = true;
            }
            ui.add_space(8.0);
            for (v, l) in [(2u8, "Room"), (1, "Type"), (0, "None")] {
                if ui.selectable_label(s.settings.map_group == v, l).clicked() {
                    s.settings.map_group = v;
                    s.dirty = true;
                }
            }
            ui.label(RichText::new("Group by").color(MUTED));
        });
        if relayout {
            for d in s.devices.iter_mut() {
                d.map_pos = None;
            }
            s.dirty = true;
        }

        // legend
        ui.horizontal(|ui| {
            for (l, c) in [("Online", GREEN), ("Slow / partial", AMBER), ("Offline", RED), ("Checking", BLUE), ("Paused", GRAY)] {
                let (r, _) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.5, c);
                ui.label(RichText::new(l).size(12.0).color(MUTED));
                ui.add_space(8.0);
            }
            ui.label(RichText::new("·  dashed line = offline").size(12.0).color(FAINT));
        });
        ui.add_space(6.0);

        let router_id = s
            .gateway
            .and_then(|g| s.devices.iter().find(|d| d.ip == g && d.kind != DeviceKind::Internet))
            .or_else(|| s.devices.iter().find(|d| d.kind == DeviceKind::Router))
            .map(|d| d.id);
        let inet_id = s.devices.iter().find(|d| d.kind == DeviceKind::Internet).map(|d| d.id);
        let mut others: Vec<(u8, u32, u64)> = s
            .devices
            .iter()
            .filter(|d| Some(d.id) != router_id && Some(d.id) != inet_id)
            .map(|d| (d.kind as u8, u32::from(d.ip), d.id))
            .collect();
        others.sort();
        let others: Vec<u64> = others.into_iter().map(|x| x.2).collect();
        let auto: Vec<u64> = others.iter().cloned().filter(|id| s.device(*id).map(|d| d.map_pos.is_none()).unwrap_or(false)).collect();

        let avail = ui.available_size();
        let w = avail.x - 14.0;
        let margin = 24.0;
        let mut cols = (((w - 2.0 * margin) / 124.0).floor() as usize).max(2);
        if cols % 2 == 1 {
            cols -= 1; // even, so the trunk line runs between columns
        }
        // Group the auto-placed devices (by type or room); each group starts on a new row with a heading.
        let mut groups: Vec<(String, Vec<u64>)> = vec![];
        for id in &auto {
            let Some(d) = s.device(*id) else { continue };
            let key = match s.settings.map_group {
                1 => d.kind.label().to_string(),
                2 => if d.group.trim().is_empty() { "No room set".to_string() } else { d.group.trim().to_string() },
                _ => String::new(),
            };
            match groups.iter_mut().find(|g| g.0 == key) {
                Some(g) => g.1.push(*id),
                None => groups.push((key, vec![*id])),
            }
        }
        if s.settings.map_group == 2 {
            groups.sort_by(|a, b| (a.0 == "No room set", a.0.to_lowercase()).cmp(&(b.0 == "No room set", b.0.to_lowercase())));
        }
        let group_gap = if s.settings.map_group == 0 { 0.0 } else { 34.0 };
        let rows: usize = groups.iter().map(|g| (g.1.len() + cols - 1) / cols).sum();
        let (top_inet, top_router, y0, row_h) = (58.0, 178.0, 318.0 + group_gap, 106.0);
        let dragged_bottom = s.devices.iter().filter_map(|d| d.map_pos).map(|p| p[1] + 70.0).fold(0.0f32, f32::max);
        let content_h = (y0 + rows as f32 * row_h + groups.len() as f32 * group_gap).max(avail.y - 4.0).max(dragged_bottom);
        let cellw = (w - 2.0 * margin) / cols as f32;

        let mut clicked: Option<u64> = None;
        let mut drag: Option<(u64, egui::Pos2)> = None;
        let mut bg_clicked = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let (rect, bg_resp) = ui.allocate_exact_size(vec2(w, content_h), Sense::click());
            bg_clicked = bg_resp.clicked();
            let p = ui.painter_at(rect);
            p.rect(rect, 12.0, Color32::from_rgb(0x10, 0x17, 0x24), Stroke::new(1.0f32, BORDER), StrokeKind::Inside);
            let step = 28.0;
            let mut gx = rect.left() + step;
            while gx < rect.right() {
                let mut gy = rect.top() + step;
                while gy < rect.bottom() {
                    p.circle_filled(pos2(gx, gy), 1.0, BORDER);
                    gy += step;
                }
                gx += step;
            }

            // where everything goes
            let cx = rect.center().x;
            let mut pos: std::collections::HashMap<u64, egui::Pos2> = std::collections::HashMap::new();
            for d in &s.devices {
                if let Some(mp) = d.map_pos {
                    pos.insert(d.id, rect.min + vec2(mp[0], mp[1]));
                }
            }
            if let Some(id) = inet_id {
                pos.entry(id).or_insert(pos2(cx, rect.top() + top_inet));
            }
            if let Some(id) = router_id {
                pos.entry(id).or_insert(pos2(cx, rect.top() + top_router));
            }
            let mut headings: Vec<(String, f32)> = vec![];
            let mut row_base = 0usize;
            for (gi, (label, ids)) in groups.iter().enumerate() {
                let gy = rect.top() + y0 + row_base as f32 * row_h + gi as f32 * group_gap;
                if !label.is_empty() {
                    headings.push((format!("{}  ·  {}", label, ids.len()), gy - 72.0));
                }
                for (i, id) in ids.iter().enumerate() {
                    let row = i / cols;
                    let col = i % cols;
                    let in_row = (ids.len() - row * cols).min(cols);
                    let x0 = rect.left() + margin + (cols - in_row) as f32 * cellw / 2.0;
                    pos.insert(*id, pos2(x0 + (col as f32 + 0.5) * cellw, gy + row as f32 * row_h));
                }
                row_base += (ids.len() + cols - 1) / cols;
            }
            for (h, y) in &headings {
                let r = egui::Rect::from_min_size(pos2(rect.left() + margin - 6.0, *y - 11.0), vec2(w - 2.0 * margin + 12.0, 22.0));
                p.rect_filled(r, 6.0, CARD.linear_multiply(0.9));
                p.text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, h, FontId::proportional(12.5), GOLD);
            }
            let hub = router_id.and_then(|id| pos.get(&id).cloned()).unwrap_or(pos2(cx, rect.top() + top_router));

            // links: internet to router, then elbow lines from the router to each device
            if let (Some(id), Some(ip)) = (inet_id, inet_id.and_then(|id| pos.get(&id).cloned())) {
                let st = s.device(id).map(|d| d.status).unwrap_or_default();
                let path = [ip, hub];
                if st == Status::Down {
                    p.extend(egui::Shape::dashed_line(&path, Stroke::new(3.0f32, RED), 8.0, 6.0));
                } else {
                    p.line_segment(path, Stroke::new(3.0f32, BLUE.linear_multiply(0.8)));
                }
            }
            let mut ordered: Vec<&Device> = others.iter().filter_map(|id| s.device(*id)).collect();
            ordered.sort_by_key(|d| match d.status {
                Status::Down => 3,
                Status::Warning => 2,
                Status::Paused => 0,
                _ => 1,
            });
            for d in ordered {
                let Some(pt) = pos.get(&d.id).cloned() else { continue };
                let (c, wdt) = match d.status {
                    Status::Down => (RED.linear_multiply(0.85), 1.8f32),
                    Status::Warning => (AMBER.linear_multiply(0.7), 1.8),
                    Status::Paused => (GRAY.linear_multiply(0.5), 1.4),
                    _ => (Color32::from_rgb(0x2E, 0x50, 0x78), 1.6),
                };
                let bus_y = pt.y - 46.0;
                let path = if bus_y > hub.y + 20.0 {
                    vec![hub, pos2(hub.x, bus_y), pos2(pt.x, bus_y), pt]
                } else {
                    vec![hub, pt]
                };
                if d.status == Status::Down {
                    p.extend(egui::Shape::dashed_line(&path, Stroke::new(wdt, c), 6.0, 5.0));
                } else {
                    p.add(egui::Shape::line(path, Stroke::new(wdt, c)));
                }
            }

            // nodes
            let time = ui.input(|i| i.time) as f32;
            let max_chars = ((cellw - 10.0) / 6.8) as usize;
            for d in &s.devices {
                let Some(c) = pos.get(&d.id).cloned() else { continue };
                let big = Some(d.id) == router_id || Some(d.id) == inet_id;
                let r = if big { 30.0 } else { 21.0 };
                let node_rect = egui::Rect::from_center_size(c, vec2(r * 2.0, r * 2.0));
                let resp = ui.interact(node_rect, ui.id().with(("node", d.id)), Sense::click_and_drag());
                let col = status_color(d.status);
                let sel = self.selected == Some(d.id);
                if sel {
                    p.circle_filled(c, r + 9.0, BLUE.linear_multiply(0.2));
                }
                if d.status == Status::Down {
                    let pulse = (time * 3.0).sin() * 0.5 + 0.5;
                    p.circle_stroke(c, r + 4.0 + pulse * 4.0, Stroke::new(2.0f32, RED.linear_multiply(0.5 * (1.0 - pulse))));
                    ui.ctx().request_repaint();
                }
                p.circle_filled(c, r, if resp.hovered() { CARD_HI } else { CARD });
                p.circle_stroke(c, r, Stroke::new(if sel { 3.0f32 } else { 2.2 }, col));
                p.text(c, Align2::CENTER_CENTER, d.kind.icon(), FontId::proportional(if big { 24.0 } else { 16.0 }), TEXT);
                let badge = c + vec2(r * 0.72, -r * 0.72);
                p.circle_filled(badge, 5.0, col);
                p.circle_stroke(badge, 5.0, Stroke::new(1.5f32, BG));
                let limit = if big { 30 } else { max_chars.max(8) };
                let mut name = d.name.clone();
                if name.chars().count() > limit {
                    name = name.chars().take(limit - 1).collect::<String>() + "…";
                }
                p.text(c + vec2(0.0, r + 10.0), Align2::CENTER_CENTER, name, FontId::proportional(12.5), TEXT);
                let sub = if d.kind == DeviceKind::Internet { ms(d.latency) } else { format!("{} · {}", d.ip, ms(d.latency)) };
                p.text(c + vec2(0.0, r + 25.0), Align2::CENTER_CENTER, sub, FontId::proportional(10.5), MUTED);
                let resp = resp.on_hover_text(format!(
                    "{}\n{} · {}\n{} · {}\nUptime {}",
                    d.name,
                    d.kind.label(),
                    d.ip,
                    d.status.label(),
                    ms(d.latency),
                    d.uptime_pct().map(|v| format!("{:.1}%", v)).unwrap_or_else(|| "—".into())
                ));
                if resp.dragged() {
                    let np = c + resp.drag_delta();
                    let np = pos2(np.x.clamp(rect.left() + 30.0, rect.right() - 30.0), np.y.clamp(rect.top() + 30.0, rect.bottom() - 40.0));
                    drag = Some((d.id, pos2(np.x - rect.left(), np.y - rect.top())));
                }
                if resp.clicked() {
                    clicked = Some(d.id);
                }
            }
            if s.devices.len() <= 2 && s.scan.running {
                p.text(pos2(cx, rect.top() + y0), Align2::CENTER_CENTER, "Looking for devices…", FontId::proportional(15.0), MUTED);
            }
        });
        if let Some((id, np)) = drag {
            if let Some(d) = s.device_mut(id) {
                d.map_pos = Some([np.x, np.y]);
            }
            s.dirty = true;
        }
        if let Some(id) = clicked {
            self.selected = Some(id);
        } else if bg_clicked {
            self.selected = None;
        }
    }
}

// ───────────────────────────── alerts ─────────────────────────────
impl App {
    pub(super) fn alerts(&mut self, ui: &mut Ui, s: &mut Shared) {
        let open = s.unacked();
        self.header(ui, s, "Alerts", &format!("{} waiting for you to look at", open), |this, ui, s| {
            if ui.button("💾  Save as spreadsheet").on_hover_text("All alerts with type, explanation and MITRE ATT&CK technique, as a CSV file in Documents").clicked() {
                this.toast = Some((
                    match export_alerts_csv(s) {
                        Ok(p) => format!("Saved {}", p.display()),
                        Err(e) => format!("Couldn't save: {}", e),
                    },
                    Instant::now(),
                ));
            }
            if ui.button("🗑  Clear history").clicked() {
                s.events.clear();
                s.dirty = true;
            }
            if ui.button("✔  Mark all as seen").clicked() {
                for e in s.events.iter_mut() {
                    e.acknowledged = true;
                }
                s.dirty = true;
            }
        });
        ui.horizontal(|ui| {
            for (i, l) in ["Alerts", "Tuning"].iter().enumerate() {
                if ui.selectable_label(self.alerts_tab == i as u8, RichText::new(*l).strong()).clicked() {
                    self.alerts_tab = i as u8;
                }
            }
            ui.separator();
            if self.alerts_tab == 0 {
                let crit = s.events.iter().filter(|e| e.severity == Severity::Critical && !e.muted).count();
                let all = s.events.iter().filter(|e| !e.muted).count();
                let security = s.events.iter().filter(|e| !e.muted && !e.mitre().is_empty()).count();
                for (f, l) in [
                    (AlertFilter::All, format!("All ({})", all)),
                    (AlertFilter::Open, format!("Not seen yet ({})", open)),
                    (AlertFilter::Critical, format!("Critical ({})", crit)),
                    (AlertFilter::Security, format!("Security ({})", security)),
                ] {
                    if ui.selectable_label(self.alert_filter == f, l).clicked() {
                        self.alert_filter = f;
                    }
                }
                let muted = s.events.iter().filter(|e| e.muted).count();
                if muted > 0 {
                    ui.label(RichText::new(format!("{} muted alerts hidden", muted)).size(12.0).color(FAINT));
                }
            }
        });
        ui.add_space(10.0);
        if self.alerts_tab == 1 {
            self.alert_tuning(ui, s);
            return;
        }
        let mut mute_kind: Option<String> = None;
        let mut goto: Option<u64> = None;
        let mut changed = false;
        egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0f32, BORDER)).corner_radius(10.0).inner_margin(6.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let filt = self.alert_filter;
                let mut any = false;
                for e in s.events.iter_mut().filter(|e| !e.muted).filter(|e| match filt {
                    AlertFilter::All => true,
                    AlertFilter::Open => !e.acknowledged,
                    AlertFilter::Critical => e.severity == Severity::Critical,
                    AlertFilter::Security => !e.mitre().is_empty(),
                }) {
                    any = true;
                    let w = ui.available_width();
                    let (rect, row_resp) = ui.allocate_exact_size(vec2(w, 52.0), Sense::click());
                    let is_open = self.open_alert == Some(e.id);
                    if row_resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.open_alert = if is_open { None } else { Some(e.id) };
                        e.opened = true;
                        changed = true;
                    }
                    let p = ui.painter_at(rect);
                    let col = severity_color(e.severity);
                    if !e.acknowledged {
                        p.rect_filled(rect.shrink(2.0), 8.0, col.linear_multiply(0.07));
                    }
                    p.rect_filled(egui::Rect::from_min_size(rect.min + vec2(6.0, 11.0), vec2(4.0, 30.0)), 2.0, col);
                    let icon = match e.severity {
                        Severity::Critical => "✖",
                        Severity::Warning => "⚠",
                        Severity::Info => "ℹ",
                    };
                    p.circle_filled(rect.left_center() + vec2(32.0, 0.0), 14.0, col.linear_multiply(0.18));
                    p.text(rect.left_center() + vec2(32.0, 0.0), Align2::CENTER_CENTER, icon, FontId::proportional(14.0), col);
                    p.text(rect.left_center() + vec2(58.0, -9.0), Align2::LEFT_CENTER, &e.message, FontId::proportional(14.0), TEXT);
                    p.text(
                        rect.left_center() + vec2(58.0, 10.0),
                        Align2::LEFT_CENTER,
                        if e.mitre().is_empty() {
                            format!("{} · {} · {}", e.device_name, fmt_time(e.t), e.severity.label())
                        } else {
                            format!("{} · {} · {} · MITRE {}", e.device_name, fmt_time(e.t), e.severity.label(), e.mitre())
                        },
                        FontId::proportional(11.5),
                        MUTED,
                    );
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(egui::Rect::from_min_max(pos2(rect.right() - 260.0, rect.top()), rect.right_bottom() - vec2(10.0, 0.0)))
                            .layout(Layout::right_to_left(Align::Center)),
                    );
                    if !e.acknowledged {
                        if child.button("Mark seen").clicked() {
                            e.acknowledged = true;
                            changed = true;
                        }
                    } else {
                        child.label(RichText::new("seen").size(11.5).color(FAINT));
                    }
                    if let Some(id) = e.device_id {
                        if child.button("Open device").clicked() {
                            goto = Some(id);
                            e.opened = true;
                        }
                    }
                    child.label(RichText::new(if self.open_alert == Some(e.id) { "hide" } else { "details" }).size(11.5).color(FAINT));
                    if self.open_alert == Some(e.id) {
                        let info = alert_info(if e.kind.is_empty() { infer_kind(&e.message) } else { &e.kind });
                        egui::Frame::new().fill(CARD_HI).corner_radius(8.0).inner_margin(12.0).show(ui, |ui| {
                            ui.set_width(w - 30.0);
                            ui.label(RichText::new(info.title).strong());
                            ui.label(RichText::new("What it means").size(11.5).color(FAINT));
                            ui.label(info.explain);
                            ui.add_space(4.0);
                            ui.label(RichText::new("What you can do").size(11.5).color(FAINT));
                            ui.label(info.action);
                            ui.horizontal(|ui| {
                                if !info.mitre.is_empty() {
                                    let url = format!("https://attack.mitre.org/techniques/{}/", info.mitre.replace('.', "/"));
                                    if ui
                                        .link(format!("MITRE ATT&CK {}", mitre_label(info.mitre)))
                                        .on_hover_text("The matching technique in the public MITRE ATT&CK catalogue; in brackets, the attacker's goal (tactic)")
                                        .clicked()
                                    {
                                        crate::util::open(&url);
                                    }
                                    ui.add_space(10.0);
                                }
                                if ui.small_button("Mute alerts like this").clicked() {
                                    mute_kind = Some(if e.kind.is_empty() { infer_kind(&e.message).to_string() } else { e.kind.clone() });
                                }
                            });
                        });
                    }
                    ui.painter().line_segment(
                        [rect.left_bottom() + vec2(10.0, 0.0), rect.right_bottom() - vec2(10.0, 0.0)],
                        Stroke::new(1.0f32, BORDER.linear_multiply(0.5)),
                    );
                }
                if !any {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| ui.label(RichText::new("Nothing here — all quiet.").color(MUTED)));
                }
            });
        });
        if changed {
            s.dirty = true;
        }
        if let Some(k) = mute_kind {
            if !s.settings.muted.contains(&k) {
                s.settings.muted.push(k.clone());
            }
            for e in s.events.iter_mut().filter(|e| e.kind == k) {
                e.muted = true;
                e.acknowledged = true;
            }
            s.dirty = true;
            self.toast = Some((format!("Muted \"{}\" alerts. Undo in Alerts > Tuning.", alert_info(&k).title), Instant::now()));
        }
        if let Some(id) = goto {
            if s.device(id).is_some() {
                self.selected = Some(id);
                self.page = Page::Devices;
            }
        }
    }

    /// Which alert types arrive, which ones get opened, and muting.
    fn alert_tuning(&mut self, ui: &mut Ui, s: &mut Shared) {
        let t = now();
        flow_card(ui, "ALERT TUNING · LAST 30 DAYS", |ui| {
            ui.label(RichText::new("Types you never open are good candidates to mute. Muted alerts are still recorded, just hidden and silent.").size(12.5).color(MUTED));
            ui.add_space(8.0);
            egui::Grid::new("tuning").num_columns(6).spacing([28.0, 8.0]).striped(true).show(ui, |ui| {
                for h in ["Alert type", "MITRE ATT&CK", "Received", "Opened", "", "Mute"] {
                    ui.label(RichText::new(h).size(12.5).color(MUTED));
                }
                ui.end_row();
                for kind in ALERT_KINDS {
                    let evs: Vec<&Event> = s.events.iter().filter(|e| t - e.t < 30 * 86400).filter(|e| (if e.kind.is_empty() { infer_kind(&e.message) } else { e.kind.as_str() }) == kind).collect();
                    let n = evs.len();
                    let opened = evs.iter().filter(|e| e.opened).count();
                    let info = alert_info(kind);
                    ui.label(info.title).on_hover_text(info.explain);
                    match mitre_details(info.mitre) {
                        Some((name, tactic)) => {
                            ui.label(RichText::new(info.mitre).color(BLUE)).on_hover_text(format!("{}\nTactic: {}", name, tactic));
                        }
                        None => {
                            ui.label(RichText::new("—").color(FAINT));
                        }
                    }
                    ui.label(n.to_string());
                    ui.label(if n == 0 { "—".to_string() } else { format!("{} ({:.0}%)", opened, opened as f32 * 100.0 / n as f32) });
                    if n >= 5 && opened == 0 {
                        ui.label(RichText::new("never opened: consider muting").size(12.0).color(AMBER));
                    } else {
                        ui.label("");
                    }
                    let mut muted = s.settings.muted.iter().any(|k| k == kind);
                    if ui.checkbox(&mut muted, "").changed() {
                        if muted {
                            s.settings.muted.push(kind.to_string());
                        } else {
                            s.settings.muted.retain(|k| k != kind);
                            for e in s.events.iter_mut().filter(|e| e.kind == kind) {
                                e.muted = false;
                            }
                        }
                        s.dirty = true;
                    }
                    ui.end_row();
                }
            });
        });
    }
}

// ───────────────────────────── reports ─────────────────────────────
impl App {
    pub(super) fn reports(&mut self, ui: &mut Ui, s: &mut Shared) {
        self.header(ui, s, "Reports", "Availability and response since each device was first watched", |this, ui, s| {
            if ui.button("🗓  This week's report").on_hover_text("Compares this week with last week and opens it as a web page you can keep or share").clicked() {
                match crate::report::save_weekly(s) {
                    Ok(p) => {
                        crate::util::open(&p.to_string_lossy());
                        this.toast = Some((format!("Saved {}", p.display()), Instant::now()));
                    }
                    Err(e) => this.toast = Some((format!("Could not save the report: {}", e), Instant::now())),
                }
            }
            if ui.button("💾  Save as spreadsheet (CSV)").clicked() {
                match export_csv(s) {
                    Ok(p) => this.toast = Some((format!("Saved report to {}", p.display()), Instant::now())),
                    Err(e) => this.toast = Some((format!("Could not save report: {}", e), Instant::now())),
                }
            }
        });
        let t = now();
        let total_checks: u64 = s.devices.iter().map(|d| d.checks).sum();
        let total_ok: u64 = s.devices.iter().map(|d| d.ok_checks).sum();
        let outages: u32 = s.devices.iter().map(|d| d.outages).sum();
        let worst = s
            .devices
            .iter()
            .filter(|d| d.checks >= 3 && d.uptime_pct().unwrap_or(100.0) < 100.0)
            .min_by(|a, b| a.uptime_pct().unwrap_or(100.0).total_cmp(&b.uptime_pct().unwrap_or(100.0)));
        let inet = s.devices.iter().find(|d| d.kind == DeviceKind::Internet);
        let w = ui.available_width();
        let gap = 14.0;
        let tw = (w - gap * 3.0) / 4.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            let overall = if total_checks > 0 { format!("{:.2}%", total_ok as f64 * 100.0 / total_checks as f64) } else { "—".into() };
            let inet_up = inet.and_then(|d| d.uptime_pct()).map(|v| format!("{:.2}%", v)).unwrap_or_else(|| "—".into());
            let inet_down = inet.map(|d| dur(d.downtime_secs + d.down_since.map(|x| t - x).unwrap_or(0))).unwrap_or_default();
            let worst_txt = worst
                .map(|d| if d.name.chars().count() > 18 { d.name.chars().take(17).collect::<String>() + "…" } else { d.name.clone() })
                .unwrap_or_else(|| "none yet".into());
            let worst_sub = worst.and_then(|d| d.uptime_pct()).map(|v| format!("{:.1}% available", v)).unwrap_or_default();
            for (title, big, sub) in [
                ("All devices available", overall, format!("{} checks made", total_checks)),
                ("Internet available", inet_up, format!("offline {} in total", inet_down)),
                ("Outages", outages.to_string(), "times a device went offline".to_string()),
                ("Least reliable", worst_txt, worst_sub),
            ] {
                card(ui, tw, 96.0, title, |ui| {
                    ui.label(RichText::new(big).size(22.0).strong());
                    ui.label(RichText::new(sub).size(12.0).color(MUTED));
                });
            }
        });
        ui.add_space(gap);

        let mut rows: Vec<&Device> = s.devices.iter().collect();
        rows.sort_by(|a, b| a.uptime_pct().unwrap_or(101.0).total_cmp(&b.uptime_pct().unwrap_or(101.0)));
        let cols = [
            ("Device", 0.24),
            ("Type", 0.12),
            ("Availability", 0.22),
            ("Average", 0.09),
            ("Worst", 0.08),
            ("Outages", 0.08),
            ("Offline total", 0.10),
            ("Checks", 0.07),
        ];
        let mut xs = vec![];
        let mut acc = 0.0;
        for c in cols.iter() {
            xs.push(acc);
            acc += c.1 * w;
        }
        egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0f32, BORDER)).corner_radius(10.0).show(ui, |ui| {
            ui.set_width(w);
            let (h, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
            for (i, (l, _)) in cols.iter().enumerate() {
                ui.painter().text(pos2(h.left() + xs[i] + 14.0, h.center().y), Align2::LEFT_CENTER, *l, FontId::proportional(12.5), MUTED);
            }
            ui.painter().line_segment([h.left_bottom(), h.right_bottom()], Stroke::new(1.0f32, BORDER));
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for d in rows {
                    let (r, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
                    let p = ui.painter_at(r);
                    let y = r.center().y;
                    let x = |i: usize| r.left() + xs[i] + 14.0;
                    p.text(pos2(x(0), y), Align2::LEFT_CENTER, d.kind.icon(), FontId::proportional(14.0), MUTED);
                    p.with_clip_rect(egui::Rect::from_min_max(r.min, pos2(r.left() + xs[1] - 6.0, r.bottom())))
                        .text(pos2(x(0) + 24.0, y), Align2::LEFT_CENTER, &d.name, FontId::proportional(13.5), TEXT);
                    p.text(pos2(x(1), y), Align2::LEFT_CENTER, d.kind.label(), FontId::proportional(13.0), MUTED);
                    let pct = d.uptime_pct();
                    let bar_w = (cols[2].1 * w - 100.0).max(40.0);
                    let br = egui::Rect::from_min_size(pos2(x(2), y - 5.0), vec2(bar_w, 10.0));
                    p.rect_filled(br, 5.0, Color32::from_rgb(0x0F, 0x16, 0x22));
                    if let Some(v) = pct {
                        let col = if v >= 99.0 { GREEN } else if v >= 95.0 { AMBER } else { RED };
                        let mut fr = br;
                        fr.set_width(bar_w * (v / 100.0).clamp(0.0, 1.0));
                        p.rect_filled(fr, 5.0, col);
                    }
                    p.text(
                        pos2(x(2) + bar_w + 10.0, y),
                        Align2::LEFT_CENTER,
                        pct.map(|v| format!("{:.2}%", v)).unwrap_or_else(|| "—".into()),
                        FontId::proportional(13.0),
                        TEXT,
                    );
                    let (_, avg, mx) = d.latency_stats();
                    p.text(pos2(x(3), y), Align2::LEFT_CENTER, ms(avg), FontId::proportional(13.0), TEXT);
                    p.text(pos2(x(4), y), Align2::LEFT_CENTER, ms(mx), FontId::proportional(13.0), TEXT);
                    p.text(pos2(x(5), y), Align2::LEFT_CENTER, d.outages.to_string(), FontId::proportional(13.0), if d.outages > 0 { AMBER } else { TEXT });
                    let off = d.downtime_secs + d.down_since.map(|x| t - x).unwrap_or(0);
                    p.text(pos2(x(6), y), Align2::LEFT_CENTER, dur(off), FontId::proportional(13.0), MUTED);
                    p.text(pos2(x(7), y), Align2::LEFT_CENTER, d.checks.to_string(), FontId::proportional(13.0), MUTED);
                    p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0f32, BORDER.linear_multiply(0.5)));
                }
            });
        });
    }
}

fn export_csv(s: &Shared) -> std::io::Result<PathBuf> {
    let dir = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|h| PathBuf::from(h).join("Documents"))
        .filter(|p| p.exists())
        .unwrap_or_else(data_dir);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("home-network-report-{}.csv", chrono::Local::now().format("%Y%m%d-%H%M")));
    let t = now();
    let mut out = String::from(
        "Device,Type,Address,Hardware,Maker,Status,Availability %,Average ms,Worst ms,Outages,Offline seconds,Checks,First seen,Last seen\n",
    );
    let q = |v: &str| format!("\"{}\"", v.replace('"', "\"\""));
    for d in &s.devices {
        let (_, avg, mx) = d.latency_stats();
        out += &format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            q(&d.name),
            q(d.kind.label()),
            d.ip,
            d.mac.clone().unwrap_or_default(),
            q(&d.maker),
            d.status.label(),
            d.uptime_pct().map(|v| format!("{:.2}", v)).unwrap_or_default(),
            avg.map(|v| format!("{:.1}", v)).unwrap_or_default(),
            mx.map(|v| format!("{:.1}", v)).unwrap_or_default(),
            d.outages,
            d.downtime_secs + d.down_since.map(|x| t - x).unwrap_or(0),
            d.checks,
            q(&fmt_time(d.first_seen)),
            q(&d.last_seen.map(fmt_time).unwrap_or_default()),
        );
    }
    std::fs::write(&path, out)?;
    Ok(path)
}

// ───────────────────────────── settings ─────────────────────────────
impl App {
    pub(super) fn settings(&mut self, ui: &mut Ui, s: &mut Shared) {
        self.header(ui, s, "Settings", "How often to check, when to warn you, and where to look", |_this, _ui, _s| {});
        let before = serde_json::to_string(&s.settings).unwrap_or_default();
        let mut scan = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(800.0);
            egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0f32, BLUE.linear_multiply(0.5))).corner_radius(10.0).inner_margin(14.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.checkup(ui, s);
            });
            ui.add_space(12.0);
            flow_card(ui, "CHECKING", |ui| {
                egui::Grid::new("chk").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                    ui.label("Check each device every");
                    ui.add(egui::Slider::new(&mut s.settings.interval_secs, 10..=300).suffix(" s"));
                    ui.end_row();
                    ui.label("Wait for an answer up to");
                    ui.add(egui::Slider::new(&mut s.settings.timeout_ms, 200..=5000).suffix(" ms"));
                    ui.end_row();
                    ui.label("Monitoring");
                    ui.checkbox(&mut s.settings.monitoring, "On");
                    ui.end_row();
                });
            });
            ui.add_space(12.0);
            flow_card(ui, "ALERTS", |ui| {
                egui::Grid::new("al").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                    ui.label("Call a device slow above");
                    ui.add(egui::Slider::new(&mut s.settings.slow_ms, 20..=1000).suffix(" ms"));
                    ui.end_row();
                    ui.label("Call it offline after");
                    ui.add(egui::Slider::new(&mut s.settings.fails_before_alert, 1..=10).suffix(" missed checks"));
                    ui.end_row();
                    ui.label("New devices");
                    ui.checkbox(&mut s.settings.alert_new_devices, "Alert me when a new device joins");
                    ui.end_row();
                    ui.label("Slow alerts");
                    ui.checkbox(&mut s.settings.slow_important_only, "Only for the router, internet, NAS and network gear");
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Phones, tablets, laptops, TVs, speakers and game consoles sleep or leave the house, so their \"went offline\" alerts start switched off. You can turn them on for each device.")
                        .size(12.0)
                        .color(MUTED),
                );
                if ui.button("Apply these quieter defaults to all my devices").on_hover_text("Sets each device's \"Alert me\" switch to the default for its type").clicked() {
                    let mut changed = 0;
                    for d in s.devices.iter_mut() {
                        let want = d.kind.alerts_by_default();
                        if d.alerts != want {
                            d.alerts = want;
                            changed += 1;
                        }
                    }
                    s.dirty = true;
                    self.toast = Some((format!("Updated alerts on {} devices.", changed), Instant::now()));
                }
            });
            ui.add_space(12.0);
            flow_card(ui, "WHEN SOMETHING GOES WRONG", |ui| {
                egui::Grid::new("act").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                    ui.label("Sound");
                    ui.checkbox(&mut s.settings.sound, "Play the warning sound");
                    ui.end_row();
                    ui.label("Taskbar");
                    ui.checkbox(&mut s.settings.flash, "Flash the app in the taskbar (bounce it in the Dock on a Mac)");
                    ui.end_row();
                    ui.label("Pop-ups");
                    ui.checkbox(&mut s.settings.notify_desktop, "Show a notification on this computer when the window isn't in front");
                    ui.end_row();
                    ui.label("Quiet hours");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut s.settings.quiet, "No sound or flashing from");
                        ui.add_enabled(s.settings.quiet, egui::DragValue::new(&mut s.settings.quiet_from).range(0..=23).suffix(":00"));
                        ui.label("to");
                        ui.add_enabled(s.settings.quiet, egui::DragValue::new(&mut s.settings.quiet_to).range(0..=23).suffix(":00"));
                    });
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.label(RichText::new("Alerts are always recorded, and a red bar at the top shows anything that went offline until you've seen it.").size(12.0).color(MUTED));
            });
            ui.add_space(12.0);
            self.settings_away(ui, s);
            ui.add_space(12.0);
            flow_card(ui, "NAS & SYSTEM STATS", |ui| {
                egui::Grid::new("perfset").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                    ui.label("Read stats every");
                    ui.add(egui::Slider::new(&mut s.settings.perf_interval_secs, 30..=600).suffix(" s"));
                    ui.end_row();
                    ui.label("Warn when storage is fuller than");
                    ui.add(egui::Slider::new(&mut s.settings.disk_warn, 50..=99).suffix(" %"));
                    ui.end_row();
                    ui.label("Warn when a disk is hotter than");
                    ui.add(egui::Slider::new(&mut s.settings.temp_warn, 35..=90).suffix(" °C"));
                    ui.end_row();
                    ui.label("Warn when the NAS itself is hotter than");
                    ui.add(egui::Slider::new(&mut s.settings.sys_temp_warn, 40..=100).suffix(" °C"));
                    ui.end_row();
                    ui.label("Warn when the processor stays above");
                    ui.add(egui::Slider::new(&mut s.settings.cpu_warn, 50..=100).suffix(" %"));
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.label(RichText::new("A failed disk or a damaged storage pool always raises a critical alert. Storage pools never get the \"full\" warning: a pool is normally handed out entirely to its volumes, so only volumes are checked for free space.").size(12.0).color(MUTED));
            });
            ui.add_space(12.0);
            flow_card(ui, "FINDING DEVICES", |ui| {
                egui::Grid::new("sc").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                    ui.label("Addresses to scan");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut s.settings.scan_range).desired_width(200.0));
                        if ui.button("Use my network").clicked() {
                            s.settings.scan_range = net::default_range();
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    match net::parse_range(&s.settings.scan_range) {
                        Ok(v) => ui.label(RichText::new(format!("{} addresses", v.len())).size(12.0).color(MUTED)),
                        Err(e) => ui.label(RichText::new(e).size(12.0).color(RED)),
                    };
                    ui.end_row();
                    ui.label("Look for new devices every");
                    ui.add(
                        egui::Slider::new(&mut s.settings.rescan_hours, 0..=48)
                            .custom_formatter(|v, _| if v == 0.0 { "never".into() } else { format!("{} h", v) }),
                    );
                    ui.end_row();
                    ui.label("Last scan");
                    ui.label(ago(s.scan.last_finished));
                    ui.end_row();
                });
                ui.add_space(4.0);
                if ui.add_enabled(!s.scan.running, egui::Button::new("🔍  Scan now")).clicked() {
                    scan = true;
                }
            });
            ui.add_space(12.0);
            flow_card(ui, "INTERNET DESTINATIONS & DEVICES", |ui| {
                ui.checkbox(&mut s.settings.geo_lookup, "Show which countries this computer's traffic goes to");
                ui.label(
                    RichText::new("Sends internet addresses (never your home devices, never content) to the free service ip-api.com to find their country. Off by default.")
                        .size(12.0)
                        .color(MUTED),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Tell me when a device hasn't been seen for");
                    ui.add(egui::Slider::new(&mut s.settings.departed_hours, 0..=168).custom_formatter(|v, _| if v == 0.0 { "never".into() } else { format!("{} h", v) }));
                });
            });
            ui.add_space(12.0);
            let mut restore_msg: Option<String> = None;
            flow_card(ui, "BACKUP & RESTORE", |ui| {
                ui.label(RichText::new("Saves settings, device names and rooms, trusted programs and speed test history to one file in your Documents folder.").size(12.5).color(MUTED));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.backup_traffic, "Include traffic history (bigger file)");
                    if ui.button("💾  Make a backup").clicked() {
                        let b = crate::backup::make(s, self.backup_traffic);
                        self.toast = Some((
                            match crate::backup::write(&crate::util::documents_dir(), &b) {
                                Ok(p) => format!("Backup saved: {}", p.display()),
                                Err(e) => format!("Couldn't save the backup: {}", e),
                            },
                            Instant::now(),
                        ));
                    }
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Restore from a backup (newest first):").size(12.5).color(MUTED));
                let files = crate::backup::find();
                if files.is_empty() {
                    ui.label(RichText::new("No backups found in Documents yet.").size(12.0).color(FAINT));
                }
                ui.horizontal_wrapped(|ui| {
                    for f in files.iter().take(6) {
                        let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        if ui.small_button(name).clicked() {
                            self.restore = Some((f.clone(), crate::backup::read(f)));
                        }
                    }
                });
                let mut cancel = false;
                if let Some((path, res)) = &self.restore {
                    ui.add_space(6.0);
                    egui::Frame::new().fill(CARD_HI).corner_radius(8.0).inner_margin(12.0).show(ui, |ui| {
                        ui.label(RichText::new(format!("Checking {}", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())).strong());
                        match res {
                            Err(e) => {
                                ui.label(RichText::new(e).color(RED));
                                if ui.button("Close").clicked() {
                                    cancel = true;
                                }
                            }
                            Ok(b) => {
                                ui.label(RichText::new("✔ File checks out (fingerprint matches). Nothing has been changed yet. Here's what's inside:").color(GREEN));
                                egui::Grid::new("restore_preview").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                                    for (k, v) in crate::backup::summary(b) {
                                        ui.label(RichText::new(k).color(MUTED));
                                        ui.label(v);
                                        ui.end_row();
                                    }
                                });
                                ui.add_space(6.0);
                                let c = &mut self.restore_choice;
                                ui.horizontal_wrapped(|ui| {
                                    ui.checkbox(&mut c.settings, "Settings");
                                    ui.checkbox(&mut c.devices, "Device names & watch settings");
                                    ui.checkbox(&mut c.programs, "Trusted programs");
                                    ui.checkbox(&mut c.speed, "Speed history");
                                    ui.add_enabled(b.traffic.is_some(), egui::Checkbox::new(&mut c.traffic, "Traffic history (replaces current)"));
                                });
                                ui.horizontal(|ui| {
                                    if ui.button(RichText::new("Restore selected").color(Color32::WHITE)).clicked() {
                                        restore_msg = Some(crate::backup::apply(s, b, self.restore_choice));
                                        cancel = true;
                                    }
                                    if ui.button("Cancel").clicked() {
                                        cancel = true;
                                    }
                                });
                            }
                        }
                    });
                }
                if cancel {
                    self.restore = None;
                }
            });
            if let Some(m) = restore_msg {
                self.toast = Some((m, Instant::now()));
            }
            ui.add_space(12.0);
            flow_card(ui, "YOUR DATA", |ui| {
                ui.label(RichText::new("Everything stays on this computer, saved in:").color(MUTED));
                ui.label(RichText::new(data_file().display().to_string()).monospace().size(12.0));
                ui.add_space(6.0);
                if ui.button("Forget all devices and history…").clicked() {
                    self.confirm_reset = true;
                }
            });
            ui.add_space(12.0);
            flow_card(ui, "SUPPORT HOMENET WATCH", |ui| {
                ui.label("HomeNet Watch is made by DDID (DaleDumbsITDown.com).");
                ui.horizontal(|ui| {
                    super::about_page::support_button(ui);
                    if ui.link("User guide").clicked() {
                        super::about_page::open_user_guide();
                    }
                    if ui.link("About & glossary").clicked() {
                        self.page = Page::About;
                    }
                });
            });
        });
        if scan {
            self.start_scan_from_ui(ui.ctx(), s);
        }
        if before != serde_json::to_string(&s.settings).unwrap_or_default() {
            s.dirty = true;
        }
    }
}

// ───────────────────────────── dialogs ─────────────────────────────
impl App {
    pub(super) fn add_dialog(&mut self, ctx: &egui::Context, s: &mut Shared) {
        let Some(dlg) = self.add.as_mut() else { return };
        let mut open = true;
        let mut done = false;
        egui::Window::new("Add a device")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.label(RichText::new("Watch a device the scan didn't find, or something outside your home such as a website or game server.").color(MUTED));
                ui.add_space(8.0);
                egui::Grid::new("add").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label("Address");
                    ui.add(egui::TextEdit::singleline(&mut dlg.ip).hint_text("192.168.1.50").desired_width(220.0));
                    ui.end_row();
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut dlg.name).hint_text("Living room TV").desired_width(220.0));
                    ui.end_row();
                    ui.label("Type");
                    egui::ComboBox::from_id_salt("addkind").selected_text(dlg.kind.label()).show_ui(ui, |ui| {
                        for k in DeviceKind::ALL {
                            ui.selectable_value(&mut dlg.kind, k, format!("{} {}", k.icon(), k.label()));
                        }
                    });
                    ui.end_row();
                });
                if !dlg.error.is_empty() {
                    ui.label(RichText::new(&dlg.error).color(RED));
                }
                ui.add_space(6.0);
                if ui.button("Add and start watching").clicked() {
                    match dlg.ip.trim().parse::<std::net::Ipv4Addr>() {
                        Ok(ip) if s.devices.iter().any(|d| d.ip == ip) => dlg.error = "That address is already being watched.".into(),
                        Ok(_) => done = true,
                        Err(_) => dlg.error = "Please type an address like 192.168.1.50".into(),
                    }
                }
            });
        if done {
            if let Some(dlg) = self.add.take() {
                if let Ok(ip) = dlg.ip.trim().parse::<std::net::Ipv4Addr>() {
                    let id = s.take_id();
                    let name = if dlg.name.trim().is_empty() { format!("Device .{}", ip.octets()[3]) } else { dlg.name.trim().to_string() };
                    s.devices.push(Device::new(id, name, ip, dlg.kind));
                    s.dirty = true;
                    self.selected = Some(id);
                    self.page = Page::Devices;
                }
            }
        } else if !open {
            self.add = None;
        }
    }

    pub(super) fn confirm_dialogs(&mut self, ctx: &egui::Context, s: &mut Shared) {
        if let Some(id) = self.confirm_remove {
            let name = s.device(id).map(|d| d.name.clone()).unwrap_or_default();
            let mut keep = true;
            egui::Window::new("Remove device?")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(format!("Stop watching \"{}\" and forget its history?", name));
                    ui.label(RichText::new("If it's still on the network, the next scan will find it again.").size(12.0).color(MUTED));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Remove").color(RED)).clicked() {
                            s.devices.retain(|d| d.id != id);
                            s.dirty = true;
                            self.selected = None;
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            if !keep {
                self.confirm_remove = None;
            }
        }
        if self.confirm_reset {
            egui::Window::new("Forget everything?")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label("This removes every device, all history and all alerts. Settings are kept.");
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Forget everything").color(RED)).clicked() {
                            s.devices.clear();
                            s.events.clear();
                            let id = s.take_id();
                            s.devices.push(Device::new(id, "Internet".into(), std::net::Ipv4Addr::new(1, 1, 1, 1), DeviceKind::Internet));
                            if let Some(gw) = s.gateway {
                                let id = s.take_id();
                                s.devices.push(Device::new(id, "Home router".into(), gw, DeviceKind::Router));
                            }
                            s.scan.last_finished = None;
                            s.dirty = true;
                            self.selected = None;
                            self.confirm_reset = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_reset = false;
                        }
                    });
                });
        }
    }
}

/// All alerts as a spreadsheet (CSV) in Documents, including MITRE ATT&CK details.
fn export_alerts_csv(s: &Shared) -> std::io::Result<PathBuf> {
    let dir = crate::util::documents_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("HomeNetWatch-alerts-{}.csv", chrono::Local::now().format("%Y%m%d-%H%M")));
    let q = |v: &str| format!("\"{}\"", v.replace('"', "\"\""));
    let mut out = String::from("When,Severity,Device,Alert type,Message,Seen,Muted,MITRE technique,Technique name,Tactic,What it means,What to do\n");
    for e in &s.events {
        let info = alert_info(e.kind_or_guess());
        let (tname, tactic) = mitre_details(info.mitre).unwrap_or(("", ""));
        out += &format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            q(&fmt_time(e.t)),
            e.severity.label(),
            q(&e.device_name),
            q(info.title),
            q(&e.message),
            if e.acknowledged { "yes" } else { "no" },
            if e.muted { "yes" } else { "no" },
            info.mitre,
            q(tname),
            q(tactic),
            q(info.explain),
            q(info.action)
        );
    }
    std::fs::write(&path, out)?;
    Ok(path)
}
