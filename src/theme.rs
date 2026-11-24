use crate::model::{Severity, Status};
use eframe::egui::{self, Color32};

pub const BG: Color32 = Color32::from_rgb(0x0E, 0x14, 0x1F);
pub const NAV: Color32 = Color32::from_rgb(0x11, 0x19, 0x26);
pub const CARD: Color32 = Color32::from_rgb(0x17, 0x21, 0x31);
pub const CARD_HI: Color32 = Color32::from_rgb(0x20, 0x2D, 0x42);
pub const BORDER: Color32 = Color32::from_rgb(0x25, 0x33, 0x48);
pub const TEXT: Color32 = Color32::from_rgb(0xDC, 0xE3, 0xEE);
pub const MUTED: Color32 = Color32::from_rgb(0x8A, 0x97, 0xAB);
pub const FAINT: Color32 = Color32::from_rgb(0x55, 0x62, 0x76);
pub const GOLD: Color32 = Color32::from_rgb(0xE9, 0xB6, 0x3B);
pub const BLUE: Color32 = Color32::from_rgb(0x4C, 0x9A, 0xFF);
pub const GREEN: Color32 = Color32::from_rgb(0x34, 0xC7, 0x7B);
pub const AMBER: Color32 = Color32::from_rgb(0xF2, 0x99, 0x2E);
pub const RED: Color32 = Color32::from_rgb(0xEF, 0x53, 0x50);
pub const GRAY: Color32 = Color32::from_rgb(0x6B, 0x77, 0x8A);

pub fn status_color(s: Status) -> Color32 {
    match s {
        Status::Up => GREEN,
        Status::Warning => AMBER,
        Status::Down => RED,
        Status::Paused => GRAY,
        Status::Unknown => BLUE,
    }
}

pub fn severity_color(s: Severity) -> Color32 {
    match s {
        Severity::Info => BLUE,
        Severity::Warning => AMBER,
        Severity::Critical => RED,
    }
}

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.window_stroke = egui::Stroke::new(1.0f32, BORDER);
    v.extreme_bg_color = Color32::from_rgb(0x0B, 0x10, 0x19);
    v.faint_bg_color = CARD_HI;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = Color32::from_rgb(0x2B, 0x55, 0x8F);
    v.selection.stroke = egui::Stroke::new(1.0f32, BLUE);
    v.hyperlink_color = BLUE;
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0f32, BORDER);
    v.widgets.inactive.bg_fill = CARD_HI;
    v.widgets.inactive.weak_bg_fill = CARD_HI;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0f32, BORDER);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x2A, 0x3A, 0x54);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x2A, 0x3A, 0x54);
    v.widgets.active.bg_fill = Color32::from_rgb(0x30, 0x44, 0x63);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x30, 0x44, 0x63);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = egui::CornerRadius::same(6);
    }
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        use egui::{FontFamily, FontId, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(14.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(11.5, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Heading, FontId::new(22.0, FontFamily::Proportional));
    });
}
