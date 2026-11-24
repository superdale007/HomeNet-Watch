//! Traffic page (live and historical upload/download with a timeline scrubber)
//! and the Speed test page.

use super::*;
use crate::perf::bps;
use crate::traffic::{bytes, Minute};

impl App {
    pub(super) fn traffic(&mut self, ui: &mut Ui, s: &mut Shared) {
        self.header(ui, s, "Traffic", "How much this computer downloads and uploads, now and over time", |_this, _ui, _s| {});
        let t = now();
        let tr = &s.traffic;
        let live = tr.live.back().cloned();
        let today0 = {
            use chrono::{Local, TimeZone};
            let d = Local::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
            Local.from_local_datetime(&d).single().map(|x| x.timestamp()).unwrap_or(t - 86400)
        };
        let (drx, dtx) = tr.total_since(today0);
        let (wrx, wtx) = tr.total_since(t - 7 * 86400);
        let (prx, ptx) = tr.total_between(t - 14 * 86400, t - 7 * 86400);
        let w = ui.available_width();
        let gap = 14.0;
        let tw = (w - gap * 3.0) / 4.0;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                let pct = |a: u64, b: u64| if b == 0 { "first week".to_string() } else { format!("{:+.0}% vs last week", (a as f64 - b as f64) / b as f64 * 100.0) };
                for (title, big, sub, col) in [
                    ("Downloading now", bps(live.map(|l| l.rx_bps)), "this computer".to_string(), BLUE),
                    ("Uploading now", bps(live.map(|l| l.tx_bps)), "this computer".to_string(), GOLD),
                    ("Today", bytes(drx), format!("down · {} up", bytes(dtx)), GREEN),
                    ("Last 7 days", bytes(wrx + wtx), pct(wrx + wtx, prx + ptx), PURPLE_SOFT),
                ] {
                    let (rect, _) = ui.allocate_exact_size(vec2(tw, 92.0), Sense::hover());
                    let p = ui.painter();
                    p.rect(rect, 10.0, CARD, Stroke::new(1.0f32, BORDER), StrokeKind::Inside);
                    p.rect_filled(egui::Rect::from_min_size(rect.min + vec2(0.0, 16.0), vec2(4.0, 60.0)), 2.0, col);
                    p.text(rect.min + vec2(20.0, 14.0), Align2::LEFT_TOP, title, FontId::proportional(13.0), MUTED);
                    p.text(rect.min + vec2(20.0, 34.0), Align2::LEFT_TOP, big, FontId::proportional(25.0), TEXT);
                    p.text(rect.min + vec2(20.0, 68.0), Align2::LEFT_TOP, sub, FontId::proportional(12.0), MUTED);
                }
            });
            ui.add_space(gap);

            // live
            flow_card(ui, "LIVE · LAST 5 MINUTES", |ui| {
                let pts: Vec<(f64, f64, f64)> = s.traffic.live.iter().map(|l| (l.t, l.rx_bps, l.tx_bps)).collect();
                live_chart(ui, &pts, 300.0);
                legend(ui);
            });
            ui.add_space(gap);

            // history with scrubber
            let mut range = self.traffic_range;
            let mut scrub = self.scrub;
            flow_card(ui, "", |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("HISTORY").size(13.0).color(MUTED).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        for (secs, l) in [(14 * 86400, "14 days"), (7 * 86400, "7 days"), (86400, "24 h"), (3600 * 6, "6 h"), (3600, "1 h")] {
                            if ui.selectable_label(range == secs, l).clicked() {
                                range = secs;
                                scrub = 1.0;
                            }
                        }
                    });
                });
                let mins = &s.traffic.saved.minutes;
                let oldest = mins.front().map(|m| m.t).unwrap_or(t);
                let span = (t - oldest).max(range);
                // With less than one full period recorded, show the period ending now.
                let end = if span > range { oldest + range + ((span - range) as f32 * scrub) as i64 } else { t + 60 };
                let start = end - range;
                let window: Vec<Minute> = mins.iter().filter(|m| m.t >= start && m.t < end).cloned().collect();
                history_chart(ui, &window, start, end);
                legend(ui);
                ui.add_space(6.0);
                // timeline scrubber
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Timeline").size(12.0).color(MUTED));
                    let enabled = span > range;
                    ui.add_enabled(enabled, egui::Slider::new(&mut scrub, 0.0..=1.0).show_value(false).trailing_fill(true));
                    if ui.add_enabled(scrub < 1.0, egui::Button::new("Now")).clicked() {
                        scrub = 1.0;
                    }
                });
                let (rx, tx) = window.iter().fold((0u64, 0u64), |a, m| (a.0 + m.rx, a.1 + m.tx));
                ui.label(
                    RichText::new(format!("{} – {}  ·  {} down, {} up", fmt_time(start), fmt_time(end.min(t)), bytes(rx), bytes(tx)))
                        .size(12.5)
                        .color(MUTED),
                );
                if mins.len() < 5 {
                    ui.label(RichText::new("History builds up while the app is open; leave it running to fill this in.").size(12.0).color(FAINT));
                }
            });
            self.traffic_range = range;
            self.scrub = scrub;
        });
    }

    pub(super) fn speed(&mut self, ui: &mut Ui, s: &mut Shared) {
        let mut run = false;
        self.header(ui, s, "Speed test", "How fast your internet is right now", |_this, ui, s| {
            if ui.add_enabled(!s.traffic.speed.running, egui::Button::new(RichText::new("▶  Start test").color(Color32::WHITE)).fill(Color32::from_rgb(0x2E, 0x6F, 0xD8))).clicked() {
                run = true;
            }
        });
        if run {
            crate::traffic::start_speed_test(s, self.shared.clone(), ui.ctx().clone());
        }
        let sp = s.traffic.speed.clone();
        let w = ui.available_width();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            flow_card(ui, "", |ui| {
                ui.horizontal(|ui| {
                    // big dial
                    let (rect, _) = ui.allocate_exact_size(vec2(230.0, 200.0), Sense::hover());
                    let p = ui.painter_at(rect);
                    let c = rect.center() + vec2(0.0, 12.0);
                    let r = 86.0;
                    let start = std::f32::consts::PI * 0.75;
                    let sweep = std::f32::consts::PI * 1.5;
                    let arc = |frac: f32, col: Color32| {
                        let n = 60;
                        let pts: Vec<egui::Pos2> = (0..=n).map(|i| {
                            let a = start + sweep * frac * i as f32 / n as f32;
                            c + vec2(a.cos(), a.sin()) * r
                        }).collect();
                        p.add(egui::Shape::line(pts, Stroke::new(14.0f32, col)));
                    };
                    arc(1.0, Color32::from_rgb(0x0F, 0x16, 0x22));
                    let shown = if sp.running { sp.live_mbps } else { sp.down.unwrap_or(0.0) };
                    // log-like scale up to 1000 Mb/s
                    let frac = ((shown.max(0.1)).log10() + 1.0) / 4.0;
                    if shown > 0.0 {
                        arc(frac.clamp(0.02, 1.0), if sp.phase.contains("upload") { GOLD } else { BLUE });
                    }
                    p.text(c - vec2(0.0, 6.0), Align2::CENTER_CENTER, format!("{:.0}", shown), FontId::proportional(40.0), TEXT);
                    p.text(c + vec2(0.0, 26.0), Align2::CENTER_CENTER, "Mb/s", FontId::proportional(13.0), MUTED);
                    ui.vertical(|ui| {
                        ui.add_space(20.0);
                        let status = if sp.running {
                            sp.phase.clone()
                        } else if let Some(e) = &sp.error {
                            e.clone()
                        } else if sp.down.is_some() {
                            "Finished".into()
                        } else {
                            "Press Start to measure your connection".into()
                        };
                        ui.label(RichText::new(status).size(16.0).strong());
                        if sp.running {
                            ui.add(egui::ProgressBar::new(sp.progress).desired_width(300.0));
                        }
                        ui.add_space(10.0);
                        egui::Grid::new("speed_now").num_columns(3).spacing([30.0, 6.0]).show(ui, |ui| {
                            for (l, v, c) in [
                                ("Download", sp.down.map(|x| format!("{:.1} Mb/s", x)), BLUE),
                                ("Upload", sp.up.map(|x| format!("{:.1} Mb/s", x)), GOLD),
                                ("Response", sp.ping_ms.map(|x| format!("{:.0} ms", x)), GREEN),
                            ] {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(l).size(12.0).color(MUTED));
                                    ui.label(RichText::new(v.unwrap_or_else(|| "—".into())).size(22.0).color(c));
                                });
                            }
                            ui.end_row();
                        });
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Uses Cloudflare's public speed test (speed.cloudflare.com). A test uses about 50–200 MB of data.")
                                .size(12.0)
                                .color(FAINT),
                        );
                    });
                });
            });
            ui.add_space(14.0);
            let hist = s.traffic.saved.speed.clone();
            flow_card(ui, "RECENT RESULTS", |ui| {
                if hist.is_empty() {
                    ui.label(RichText::new("No tests yet.").color(MUTED));
                    return;
                }
                // small chart of results
                if hist.len() >= 2 {
                    let (rect, _) = ui.allocate_exact_size(vec2(w - 40.0, 120.0), Sense::hover());
                    let p = ui.painter_at(rect);
                    p.rect_filled(rect, 6.0, Color32::from_rgb(0x0F, 0x16, 0x22));
                    let maxv = hist.iter().map(|r| r.down_mbps.max(r.up_mbps)).fold(1.0, f32::max) * 1.15;
                    let n = hist.len().min(40);
                    let shown = &hist[hist.len() - n..];
                    for (vals, col) in [(shown.iter().map(|r| r.down_mbps).collect::<Vec<_>>(), BLUE), (shown.iter().map(|r| r.up_mbps).collect(), GOLD)] {
                        let pts: Vec<egui::Pos2> = vals
                            .iter()
                            .enumerate()
                            .map(|(i, v)| pos2(rect.left() + 10.0 + (rect.width() - 20.0) * i as f32 / (n - 1).max(1) as f32, rect.bottom() - 8.0 - (rect.height() - 16.0) * v / maxv))
                            .collect();
                        for q in &pts {
                            p.circle_filled(*q, 3.0, col);
                        }
                        p.add(egui::Shape::line(pts, Stroke::new(2.0f32, col)));
                    }
                    p.text(rect.left_top() + vec2(8.0, 6.0), Align2::LEFT_TOP, format!("{:.0} Mb/s", maxv), FontId::proportional(10.5), FAINT);
                    ui.add_space(6.0);
                }
                egui::Grid::new("speed_hist").num_columns(4).spacing([40.0, 6.0]).striped(true).show(ui, |ui| {
                    for h in ["When", "Download", "Upload", "Response"] {
                        ui.label(RichText::new(h).size(12.5).color(MUTED));
                    }
                    ui.end_row();
                    for r in hist.iter().rev().take(25) {
                        ui.label(fmt_time(r.t));
                        ui.label(RichText::new(format!("{:.1} Mb/s", r.down_mbps)).color(BLUE));
                        ui.label(RichText::new(format!("{:.1} Mb/s", r.up_mbps)).color(GOLD));
                        ui.label(format!("{:.0} ms", r.ping_ms));
                        ui.end_row();
                    }
                });
            });
        });
    }
}

pub(super) const PURPLE_SOFT: Color32 = Color32::from_rgb(0xA7, 0x7B, 0xF3);

fn legend(ui: &mut Ui) {
    ui.horizontal(|ui| {
        for (l, c) in [("Download (in)", BLUE), ("Upload (out)", GOLD)] {
            let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
            ui.painter().rect_filled(r.shrink(2.0), 2.0, c);
            ui.label(RichText::new(l).size(11.5).color(MUTED));
            ui.add_space(8.0);
        }
    });
}

/// Download above the middle line, upload below, last `secs` seconds.
fn live_chart(ui: &mut Ui, pts: &[(f64, f64, f64)], secs: f64) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 170.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 6.0, Color32::from_rgb(0x0F, 0x16, 0x22));
    let mid = rect.center().y + 20.0;
    p.line_segment([pos2(rect.left(), mid), pos2(rect.right(), mid)], Stroke::new(1.0f32, BORDER));
    let now = pts.last().map(|x| x.0).unwrap_or(0.0);
    let maxv = pts.iter().map(|x| x.1.max(x.2)).fold(100_000.0, f64::max) * 1.1;
    let x = |t: f64| rect.right() - (now - t) as f32 / secs as f32 * rect.width();
    let up_h = mid - rect.top() - 6.0;
    let down_h = rect.bottom() - mid - 6.0;
    let rx: Vec<egui::Pos2> = pts.iter().filter(|q| now - q.0 <= secs).map(|q| pos2(x(q.0), mid - (q.1 / maxv) as f32 * up_h)).collect();
    let tx: Vec<egui::Pos2> = pts.iter().filter(|q| now - q.0 <= secs).map(|q| pos2(x(q.0), mid + (q.2 / maxv) as f32 * down_h)).collect();
    for (line, col) in [(rx, BLUE), (tx, GOLD)] {
        if line.len() > 1 {
            for wnd in line.windows(2) {
                let quad = vec![wnd[0], wnd[1], pos2(wnd[1].x, mid), pos2(wnd[0].x, mid)];
                p.add(egui::Shape::convex_polygon(quad, col.linear_multiply(0.18), Stroke::NONE));
            }
            p.add(egui::Shape::line(line, Stroke::new(1.8f32, col)));
        }
    }
    p.text(rect.left_top() + vec2(8.0, 6.0), Align2::LEFT_TOP, bps(Some(maxv)), FontId::proportional(10.5), FAINT);
    p.text(rect.left_bottom() + vec2(8.0, -6.0), Align2::LEFT_BOTTOM, "5 min ago", FontId::proportional(10.5), FAINT);
    p.text(rect.right_bottom() + vec2(-8.0, -6.0), Align2::RIGHT_BOTTOM, "now", FontId::proportional(10.5), FAINT);
    if pts.is_empty() {
        p.text(rect.center(), Align2::CENTER_CENTER, "Measuring…", FontId::proportional(13.0), FAINT);
    }
    if let Some(h) = resp.hover_pos() {
        if let Some(q) = pts.iter().min_by(|a, b| (x(a.0) - h.x).abs().total_cmp(&(x(b.0) - h.x).abs())) {
            p.line_segment([pos2(x(q.0), rect.top()), pos2(x(q.0), rect.bottom())], Stroke::new(1.0f32, MUTED.linear_multiply(0.5)));
            let txt = format!("in {}\nout {}", bps(Some(q.1)), bps(Some(q.2)));
            let g = p.layout_no_wrap(txt, FontId::proportional(11.5), TEXT);
            let r = egui::Rect::from_min_size(pos2((x(q.0) + 10.0).min(rect.right() - g.size().x - 14.0), rect.top() + 6.0), g.size() + vec2(12.0, 8.0));
            p.rect_filled(r, 5.0, CARD_HI);
            p.galley(r.min + vec2(6.0, 4.0), g, TEXT);
        }
    }
}

/// Bars per time slot: download up, upload down.
fn history_chart(ui: &mut Ui, mins: &[Minute], start: i64, end: i64) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 200.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 6.0, Color32::from_rgb(0x0F, 0x16, 0x22));
    let slots = 120usize;
    let slot = ((end - start) as f64 / slots as f64).max(60.0);
    let mut rx = vec![0u64; slots];
    let mut tx = vec![0u64; slots];
    for m in mins {
        let i = (((m.t - start) as f64) / slot) as usize;
        if i < slots {
            rx[i] += m.rx;
            tx[i] += m.tx;
        }
    }
    let maxv = rx.iter().chain(tx.iter()).cloned().max().unwrap_or(0).max(1) as f32;
    let mid = rect.top() + rect.height() * 0.62;
    p.line_segment([pos2(rect.left(), mid), pos2(rect.right(), mid)], Stroke::new(1.0f32, BORDER));
    let bw = rect.width() / slots as f32;
    for i in 0..slots {
        let x0 = rect.left() + i as f32 * bw + 0.5;
        let hu = (rx[i] as f32 / maxv) * (mid - rect.top() - 18.0);
        let hd = (tx[i] as f32 / maxv) * (rect.bottom() - mid - 18.0);
        if hu > 0.0 {
            p.rect_filled(egui::Rect::from_min_max(pos2(x0, mid - hu), pos2(x0 + bw - 1.0, mid)), 1.0, BLUE);
        }
        if hd > 0.0 {
            p.rect_filled(egui::Rect::from_min_max(pos2(x0, mid), pos2(x0 + bw - 1.0, mid + hd)), 1.0, GOLD);
        }
    }
    p.text(rect.left_top() + vec2(8.0, 4.0), Align2::LEFT_TOP, format!("{} per slot", bytes(maxv as u64)), FontId::proportional(10.5), FAINT);
    for i in 0..=4 {
        let tt = start + (end - start) * i / 4;
        let xx = rect.left() + rect.width() * i as f32 / 4.0;
        let al = if i == 0 { Align2::LEFT_BOTTOM } else if i == 4 { Align2::RIGHT_BOTTOM } else { Align2::CENTER_BOTTOM };
        p.text(pos2(xx.clamp(rect.left() + 4.0, rect.right() - 4.0), rect.bottom() - 3.0), al, fmt_time(tt), FontId::proportional(10.0), FAINT);
    }
    if mins.is_empty() {
        p.text(rect.center(), Align2::CENTER_CENTER, "No traffic recorded in this period", FontId::proportional(13.0), FAINT);
    }
    if let Some(h) = resp.hover_pos() {
        let i = ((h.x - rect.left()) / bw) as usize;
        if i < slots {
            let t0 = start + (i as f64 * slot) as i64;
            let txt = format!("{}\nin {}\nout {}", fmt_time(t0), bytes(rx[i]), bytes(tx[i]));
            let g = p.layout_no_wrap(txt, FontId::proportional(11.5), TEXT);
            let r = egui::Rect::from_min_size(pos2((h.x + 10.0).min(rect.right() - g.size().x - 14.0), rect.top() + 6.0), g.size() + vec2(12.0, 8.0));
            p.rect_filled(r, 5.0, CARD_HI);
            p.galley(r.min + vec2(6.0, 4.0), g, TEXT);
        }
    }
}
