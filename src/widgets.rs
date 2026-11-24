//! Small drawing helpers: cards, status pills, charts, donut.

use crate::model::Sample;
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontId, Layout, Rect, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder};

/// A rounded card of a fixed size with an optional title.
pub fn card(ui: &mut Ui, w: f32, h: f32, title: &str, add: impl FnOnce(&mut Ui)) {
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    ui.painter().rect(rect, 10.0, CARD, Stroke::new(1.0f32, BORDER), StrokeKind::Inside);
    let inner = rect.shrink(14.0);
    let mut child = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
    child.set_clip_rect(inner.expand(4.0));
    if !title.is_empty() {
        child.label(egui::RichText::new(title).size(13.0).color(MUTED).strong());
        child.add_space(4.0);
    }
    add(&mut child);
}

/// A card that grows with its content, full width.
pub fn flow_card(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0f32, BORDER))
        .corner_radius(10.0)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                ui.label(egui::RichText::new(title).size(13.0).color(MUTED).strong());
                ui.add_space(4.0);
            }
            add(ui);
        });
}

pub fn pill(ui: &mut Ui, text: &str, color: Color32) -> egui::Response {
    let font = FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let size = vec2(galley.size().x + 22.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let bg = color.linear_multiply(0.16);
    ui.painter().rect_filled(rect, 10.0, bg);
    ui.painter().circle_filled(pos2(rect.left() + 9.0, rect.center().y), 3.5, color);
    ui.painter().galley(pos2(rect.left() + 16.0, rect.center().y - galley.size().y / 2.0), galley, color);
    resp
}

pub fn dot(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    painter.circle_filled(center, 8.0, color.linear_multiply(0.22));
    painter.circle_filled(center, 4.5, color);
}

pub struct Series<'a> {
    pub samples: &'a [Sample],
    pub color: Color32,
    pub label: String,
}

/// Response-time chart: one line per series, red ticks where there was no answer.
pub fn latency_chart(ui: &mut Ui, size: egui::Vec2, series: &[Series], window_secs: i64) {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter_at(rect);
    let plot = Rect::from_min_max(pos2(rect.left() + 44.0, rect.top() + 6.0), pos2(rect.right() - 6.0, rect.bottom() - 20.0));
    let now = crate::model::now();
    let t0 = now - window_secs;
    let mut ymax: f32 = 20.0;
    for s in series {
        for smp in s.samples.iter().filter(|x| x.t >= t0) {
            if let Some(ms) = smp.ms {
                ymax = ymax.max(ms);
            }
        }
    }
    let ymax = nice_ceiling(ymax * 1.15);
    // grid
    for i in 0..=4 {
        let y = plot.bottom() - plot.height() * i as f32 / 4.0;
        p.line_segment([pos2(plot.left(), y), pos2(plot.right(), y)], Stroke::new(1.0f32, BORDER.linear_multiply(0.7)));
        p.text(
            pos2(plot.left() - 6.0, y),
            Align2::RIGHT_CENTER,
            format!("{:.0} ms", ymax * i as f32 / 4.0),
            FontId::proportional(10.5),
            FAINT,
        );
    }
    let ticks = 6;
    for i in 0..=ticks {
        let x = plot.left() + plot.width() * i as f32 / ticks as f32;
        let mins = (window_secs as f32 * (1.0 - i as f32 / ticks as f32)) / 60.0;
        let label = if i == ticks { "now".to_string() } else { format!("-{:.0}m", mins) };
        let align = if i == ticks { Align2::RIGHT_TOP } else { Align2::CENTER_TOP };
        p.text(pos2(x, plot.bottom() + 4.0), align, label, FontId::proportional(10.5), FAINT);
    }
    let to_x = |t: i64| plot.left() + plot.width() * ((t - t0) as f32 / window_secs as f32);
    let to_y = |ms: f32| plot.bottom() - plot.height() * (ms / ymax).min(1.0);
    let mut any = false;
    for s in series {
        let mut run: Vec<egui::Pos2> = vec![];
        let flush = |run: &mut Vec<egui::Pos2>, p: &egui::Painter, c: Color32| {
            if run.len() == 1 {
                p.circle_filled(run[0], 2.0, c);
            } else if run.len() > 1 {
                p.add(Shape::line(run.clone(), Stroke::new(2.0f32, c)));
            }
            run.clear();
        };
        for smp in s.samples.iter().filter(|x| x.t >= t0) {
            any = true;
            match smp.ms {
                Some(ms) => run.push(pos2(to_x(smp.t), to_y(ms))),
                None => {
                    flush(&mut run, &p, s.color);
                    let x = to_x(smp.t);
                    p.line_segment([pos2(x, plot.bottom() - 8.0), pos2(x, plot.bottom())], Stroke::new(2.0f32, RED));
                }
            }
        }
        flush(&mut run, &p, s.color);
    }
    if !any {
        p.text(plot.center(), Align2::CENTER_CENTER, "Collecting data…", FontId::proportional(13.0), FAINT);
    }
    // hover readout
    if let Some(hp) = resp.hover_pos() {
        if plot.contains(hp) {
            p.line_segment([pos2(hp.x, plot.top()), pos2(hp.x, plot.bottom())], Stroke::new(1.0f32, MUTED.linear_multiply(0.5)));
            let t = t0 + ((hp.x - plot.left()) / plot.width() * window_secs as f32) as i64;
            let mut lines = vec![];
            for s in series {
                if let Some(smp) = s.samples.iter().min_by_key(|x| (x.t - t).abs()) {
                    if (smp.t - t).abs() < 120 {
                        lines.push(match smp.ms {
                            Some(ms) => format!("{}: {:.0} ms", s.label, ms),
                            None => format!("{}: no answer", s.label),
                        });
                    }
                }
            }
            if !lines.is_empty() {
                let txt = lines.join("\n");
                let g = p.layout_no_wrap(txt, FontId::proportional(11.5), TEXT);
                let mut r = Rect::from_min_size(pos2(hp.x + 10.0, plot.top() + 4.0), g.size() + vec2(12.0, 8.0));
                if r.right() > plot.right() {
                    r = r.translate(vec2(-r.width() - 20.0, 0.0));
                }
                p.rect_filled(r, 5.0, CARD_HI);
                p.galley(r.min + vec2(6.0, 4.0), g, TEXT);
            }
        }
    }
}

/// Mini bar strip of recent checks (green = answered, red = missed).
pub fn history_strip(ui: &mut Ui, size: egui::Vec2, samples: &[Sample], slow_ms: f32) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, Color32::from_rgb(0x0F, 0x16, 0x22));
    let n = (rect.width() / 4.0) as usize;
    let start = samples.len().saturating_sub(n);
    let shown = &samples[start..];
    let maxms = shown.iter().filter_map(|s| s.ms).fold(slow_ms, f32::max);
    for (i, s) in shown.iter().enumerate() {
        let x = rect.right() - (shown.len() - i) as f32 * 4.0;
        let (h, c) = match s.ms {
            Some(ms) => (
                (4.0 + (rect.height() - 6.0) * (ms / maxms).clamp(0.05, 1.0)),
                if ms > slow_ms { AMBER } else { GREEN },
            ),
            None => (rect.height() - 2.0, RED),
        };
        p.rect_filled(
            Rect::from_min_max(pos2(x, rect.bottom() - h - 1.0), pos2(x + 3.0, rect.bottom() - 1.0)),
            1.0,
            c.linear_multiply(0.9),
        );
    }
}

/// Ring chart with a big number in the middle.
pub fn donut(ui: &mut Ui, size: f32, parts: &[(f32, Color32)], center_text: &str, sub: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let p = ui.painter_at(rect);
    let c = rect.center();
    let r = size / 2.0 - 10.0;
    let total: f32 = parts.iter().map(|x| x.0).sum();
    let thick: f32 = 14.0;
    if total <= 0.0 {
        p.circle_stroke(c, r, Stroke::new(thick, BORDER));
    } else {
        let mut a0 = -std::f32::consts::FRAC_PI_2;
        for (v, col) in parts {
            if *v <= 0.0 {
                continue;
            }
            let sweep = v / total * std::f32::consts::TAU;
            let steps = ((sweep * 30.0) as usize).max(2);
            let pts: Vec<egui::Pos2> = (0..=steps)
                .map(|i| {
                    let a = a0 + sweep * i as f32 / steps as f32;
                    c + vec2(a.cos(), a.sin()) * r
                })
                .collect();
            p.add(Shape::line(pts, Stroke::new(thick, *col)));
            a0 += sweep;
        }
    }
    p.text(c - vec2(0.0, 8.0), Align2::CENTER_CENTER, center_text, FontId::proportional(26.0), TEXT);
    p.text(c + vec2(0.0, 16.0), Align2::CENTER_CENTER, sub, FontId::proportional(11.5), MUTED);
}

fn nice_ceiling(v: f32) -> f32 {
    for step in [10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 250.0, 500.0, 1000.0, 2000.0, 5000.0] {
        if v <= step {
            return step;
        }
    }
    (v / 1000.0).ceil() * 1000.0
}

pub fn fmt_time(ts: i64) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_opt(ts, 0).single() {
        Some(dt) => {
            let today = chrono::Local::now().date_naive();
            if dt.date_naive() == today {
                dt.format("%H:%M:%S").to_string()
            } else {
                dt.format("%b %d, %H:%M").to_string()
            }
        }
        None => "—".into(),
    }
}

pub fn ago(ts: Option<i64>) -> String {
    match ts {
        None => "never".into(),
        Some(t) => {
            let d = (crate::model::now() - t).max(0);
            if d < 10 {
                "just now".into()
            } else if d < 60 {
                format!("{} s ago", d)
            } else if d < 3600 {
                format!("{} min ago", d / 60)
            } else if d < 86400 {
                format!("{} h ago", d / 3600)
            } else {
                format!("{} days ago", d / 86400)
            }
        }
    }
}

pub fn dur(secs: i64) -> String {
    if secs <= 0 {
        "none".into()
    } else if secs < 60 {
        format!("{} s", secs)
    } else if secs < 3600 {
        format!("{} min", secs / 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

pub fn ms(v: Option<f32>) -> String {
    match v {
        Some(x) if x < 1.0 => "<1 ms".into(),
        Some(x) => format!("{:.0} ms", x),
        None => "—".into(),
    }
}
