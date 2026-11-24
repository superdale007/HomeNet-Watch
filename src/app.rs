//! The window: navigation, dashboard; other pages live in app/pages.rs.

mod about_page;
mod apps_page;
mod insights_page;
mod pages;
mod perf_page;
mod traffic_page;

use crate::model::*;
use crate::monitor::{self, SharedRef};
use crate::net;
use crate::theme::*;
use crate::widgets::*;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontId, Layout, RichText, Sense, Stroke, StrokeKind, Ui};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    Dashboard,
    Devices,
    Map,
    Traffic,
    Apps,
    Insights,
    Speed,
    Performance,
    Alerts,
    Reports,
    Settings,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Filter {
    All,
    Online,
    Problems,
    Offline,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortCol {
    Status,
    Name,
    Ip,
    Kind,
    Response,
    Uptime,
    LastSeen,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlertFilter {
    All,
    Open,
    Critical,
}

pub(crate) struct AddDialog {
    ip: String,
    name: String,
    kind: DeviceKind,
    error: String,
}

pub struct App {
    shared: SharedRef,
    page: Page,
    selected: Option<u64>,
    search: String,
    filter: Filter,
    sort: (SortCol, bool),
    alert_filter: AlertFilter,
    add: Option<AddDialog>,
    new_port: String,
    confirm_remove: Option<u64>,
    confirm_reset: bool,
    toast: Option<(String, Instant)>,
    screenshot: Option<PathBuf>,
    frames: u32,
    shot_requested: bool,
    started: Instant,
    chart_minutes: i64,
    group_filter: String,
    /// Performance card asking "stop reading stats?"
    perf_confirm: Option<u64>,
    update: crate::update::UpdateRef,
    traffic_range: i64,
    scrub: f32,
    apps_tab: u8,
    apps_search: String,
    sel_app: Option<String>,
    sel_host: Option<String>,
    world: Option<egui::TextureHandle>,
    alerts_tab: u8,
    open_alert: Option<u64>,
    backup_traffic: bool,
    restore: Option<(PathBuf, Result<crate::backup::Backup, String>)>,
    restore_choice: crate::backup::RestoreChoice,
}

pub(crate) fn status_rank(s: Status) -> u8 {
    match s {
        Status::Down => 0,
        Status::Warning => 1,
        Status::Unknown => 2,
        Status::Up => 3,
        Status::Paused => 4,
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>, page: Option<String>) -> Self {
        apply(&cc.egui_ctx);
        let saved = load();
        let local_ip = net::local_ipv4();
        let gateway = net::default_gateway();
        let mut shared = Shared {
            devices: saved.devices,
            events: saved.events.into_iter().collect::<VecDeque<_>>(),
            settings: saved.settings.unwrap_or_default(),
            scan: ScanState { last_finished: saved.last_scan, ..Default::default() },
            next_id: saved.next_id,
            local_ip,
            gateway,
            dirty: false,
            persist: screenshot.is_none(),
            check_now: true,
            attention: false,
            external_ip: None,
            traffic: crate::traffic::Traffic { saved: crate::traffic::load(), ..Default::default() },
            last_gateway: saved.last_gateway,
            gateway_mac: saved.gateway_mac,
        };
        let first_run = shared.devices.is_empty();
        if first_run {
            let id = shared.take_id();
            let mut inet = Device::new(id, "Internet".into(), std::net::Ipv4Addr::new(1, 1, 1, 1), DeviceKind::Internet);
            inet.notes = "Checks a public address to tell whether your internet connection works.".into();
            inet.dns_name = "example.com".into();
            shared.devices.push(inet);
            if let Some(gw) = gateway {
                let id = shared.take_id();
                shared.devices.push(Device::new(id, "Home router".into(), gw, DeviceKind::Router));
            }
            shared.add_event(Severity::Info, None, "Welcome! Looking for devices on your home network.".into());
        }
        let need_scan = first_run || shared.scan.last_finished.is_none();
        let shared = Arc::new(Mutex::new(shared));
        monitor::start_monitor(shared.clone(), cc.egui_ctx.clone());
        crate::traffic::start(shared.clone(), cc.egui_ctx.clone());
        // Updates: tidy up after a previous update, then look for a new version (published copies only).
        crate::update::clean_up_old();
        let update_state: crate::update::UpdateRef = Default::default();
        let want_check = !crate::update::repo().is_empty() && screenshot.is_none() && shared.lock().unwrap().settings.check_updates;
        if want_check {
            crate::update::check(update_state.clone(), cc.egui_ctx.clone());
        }
        if need_scan {
            monitor::start_scan(shared.clone(), cc.egui_ctx.clone());
        }
        let page_arg = page.clone();
        let page = match page.as_deref() {
            Some("devices") => Page::Devices,
            Some("map") => Page::Map,
            Some("performance") => Page::Performance,
            Some("traffic") => Page::Traffic,
            Some("apps") | Some("countries") | Some("destinations") => Page::Apps,
            Some("insights") => Page::Insights,
            Some("speed") => Page::Speed,
            Some("about") => Page::About,
            Some("alerts") => Page::Alerts,
            Some("reports") => Page::Reports,
            Some("settings") => Page::Settings,
            _ => Page::Dashboard,
        };
        App {
            shared,
            page,
            selected: None,
            search: String::new(),
            filter: Filter::All,
            sort: (SortCol::Status, false),
            alert_filter: AlertFilter::All,
            add: None,
            new_port: String::new(),
            confirm_remove: None,
            confirm_reset: false,
            toast: None,
            screenshot,
            frames: 0,
            shot_requested: false,
            started: Instant::now(),
            chart_minutes: 30,
            group_filter: String::new(),
            perf_confirm: None,
            update: update_state,
            traffic_range: 3600,
            scrub: 1.0,
            apps_tab: match page_arg.as_deref() { Some("destinations") => 1, Some("countries") => 2, _ => 0 },
            apps_search: String::new(),
            sel_app: None,
            sel_host: None,
            world: None,
            alerts_tab: 0,
            open_alert: None,
            backup_traffic: false,
            restore: None,
            restore_choice: crate::backup::RestoreChoice { settings: true, devices: true, programs: true, speed: true, traffic: false },
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context, s: &Shared) {
        let Some(path) = self.screenshot.clone() else { return };
        self.frames += 1;
        ctx.request_repaint();
        let settled = !s.scan.running && s.devices.iter().all(|d| d.paused || d.last_check.is_some());
        let elapsed = self.started.elapsed().as_secs();
        if self.selected.is_none() && matches!(self.page, Page::Devices | Page::Map) {
            self.selected = s.devices.iter().find(|d| d.kind == DeviceKind::Router).map(|d| d.id);
        }
        if !self.shot_requested && self.frames >= 120 && (settled || elapsed >= 40) {
            self.shot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let shots: Vec<Arc<egui::ColorImage>> = ctx.input(|i| {
            i.raw
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
                .collect()
        });
        if let Some(img) = shots.into_iter().next() {
            let [w, h] = img.size;
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                if let Err(e) = buf.save(&path) {
                    eprintln!("could not save screenshot: {e}");
                }
            }
            std::process::exit(0);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let shared = self.shared.clone();
        let mut s = shared.lock().unwrap();
        self.handle_screenshot(ctx, &s);
        ctx.request_repaint_after(std::time::Duration::from_secs(1));

        // Something went wrong: flash the taskbar button and beep (not in quiet hours).
        if std::mem::take(&mut s.attention) && self.screenshot.is_none() && !s.settings.quiet_now() {
            if s.settings.flash {
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Critical));
            }
            if s.settings.sound {
                beep();
            }
        }

        self.nav(ctx, &mut s);
        self.banner(ctx, &mut s);

        if matches!(self.page, Page::Devices | Page::Map) {
            if let Some(id) = self.selected {
                if s.device(id).is_some() {
                    egui::SidePanel::right("detail")
                        .exact_width(350.0)
                        .resizable(false)
                        .frame(egui::Frame::new().fill(NAV).inner_margin(16.0).stroke(Stroke::new(1.0f32, BORDER)))
                        .show(ctx, |ui| self.detail(ui, &mut s, id));
                } else {
                    self.selected = None;
                }
            }
        }
        if self.page == Page::Apps && (self.sel_app.is_some() || self.sel_host.is_some()) {
            egui::SidePanel::right("app_detail")
                .exact_width(370.0)
                .resizable(false)
                .frame(egui::Frame::new().fill(NAV).inner_margin(16.0).stroke(Stroke::new(1.0f32, BORDER)))
                .show(ctx, |ui| self.app_detail(ui, &mut s));
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin { left: 24, right: 24, top: 18, bottom: 14 }))
            .show(ctx, |ui| match self.page {
                Page::Dashboard => self.dashboard(ui, &mut s),
                Page::Devices => self.devices(ui, &mut s),
                Page::Map => self.map(ui, &mut s),
                Page::Performance => self.performance(ui, &mut s),
                Page::Traffic => self.traffic(ui, &mut s),
                Page::Apps => self.apps(ui, &mut s),
                Page::Insights => self.insights(ui, &mut s),
                Page::Speed => self.speed(ui, &mut s),
                Page::About => self.about(ui, &mut s),
                Page::Alerts => self.alerts(ui, &mut s),
                Page::Reports => self.reports(ui, &mut s),
                Page::Settings => self.settings(ui, &mut s),
            });

        self.add_dialog(ctx, &mut s);
        self.confirm_dialogs(ctx, &mut s);

        if let Some((msg, at)) = &self.toast {
            if at.elapsed().as_secs() < 6 {
                egui::Area::new(egui::Id::new("toast"))
                    .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -24.0))
                    .show(ctx, |ui| {
                        egui::Frame::new()
                            .fill(CARD_HI)
                            .stroke(Stroke::new(1.0f32, BLUE))
                            .corner_radius(8.0)
                            .inner_margin(12.0)
                            .show(ui, |ui| ui.label(msg.as_str()));
                    });
            } else {
                self.toast = None;
            }
        }
        drop(s);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let s = self.shared.lock().unwrap();
        if s.persist {
            let _ = save(&s.to_saved());
        }
    }
}

// ───────────────────────────── navigation ─────────────────────────────
impl App {
    fn nav(&mut self, ctx: &egui::Context, s: &mut Shared) {
        egui::SidePanel::left("nav")
            .exact_width(216.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(NAV).inner_margin(egui::Margin::symmetric(14, 18)).stroke(Stroke::new(1.0f32, BORDER)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(r, 9.0, GOLD);
                    let c = r.center();
                    let roof = vec![c + vec2(-10.0, -1.0), c + vec2(0.0, -10.0), c + vec2(10.0, -1.0)];
                    p.add(egui::Shape::line(roof, Stroke::new(2.4f32, NAV)));
                    let pulse = vec![
                        c + vec2(-10.0, 5.0),
                        c + vec2(-4.0, 5.0),
                        c + vec2(-1.5, 0.0),
                        c + vec2(1.5, 9.0),
                        c + vec2(4.0, 5.0),
                        c + vec2(10.0, 5.0),
                    ];
                    p.add(egui::Shape::line(pulse, Stroke::new(2.2f32, NAV)));
                    ui.vertical(|ui| {
                        ui.add_space(1.0);
                        ui.label(RichText::new("HomeNet Watch").size(16.0).strong().color(TEXT));
                        ui.label(RichText::new("by DDID").size(11.0).color(GOLD));
                    });
                });
                ui.add_space(14.0);
                let unacked = s.unacked();
                let down = s.devices.iter().filter(|d| d.status == Status::Down).count();
                let perf_issues = s.devices.iter().filter(|d| d.snmp && !d.problems.is_empty()).count();
                let items = [
                    (Page::Dashboard, "🏠", "Dashboard", 0usize, BLUE),
                    (Page::Devices, "🖥", "Devices", down, RED),
                    (Page::Map, "🗺", "Network map", 0, BLUE),
                    (Page::Traffic, "📉", "Traffic", 0, BLUE),
                    (Page::Apps, "💻", "Apps & sites", 0, BLUE),
                    (Page::Insights, "💡", "Insights", 0, BLUE),
                    (Page::Speed, "⏱", "Speed test", 0, BLUE),
                    (Page::Performance, "📊", "Performance", perf_issues, AMBER),
                    (Page::Alerts, "🔔", "Alerts", unacked, AMBER),
                    (Page::Reports, "📈", "Reports", 0, BLUE),
                    (Page::Settings, "⚙", "Settings", 0, BLUE),
                    (Page::About, "ℹ", "About", 0, BLUE),
                ];
                for (page, icon, label, badge, badge_col) in items {
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 33.0), Sense::click());
                    let active = self.page == page;
                    let p = ui.painter();
                    if active {
                        p.rect_filled(rect, 8.0, CARD_HI);
                        p.rect_filled(egui::Rect::from_min_size(rect.min + vec2(0.0, 7.0), vec2(3.0, 19.0)), 2.0, GOLD);
                    } else if resp.hovered() {
                        p.rect_filled(rect, 8.0, CARD);
                    }
                    let col = if active { TEXT } else { MUTED };
                    p.text(rect.left_center() + vec2(16.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(16.0), col);
                    p.text(rect.left_center() + vec2(44.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(14.5), col);
                    if badge > 0 {
                        let txt = badge.to_string();
                        let br = egui::Rect::from_center_size(rect.right_center() - vec2(22.0, 0.0), vec2(14.0 + 7.0 * txt.len() as f32, 18.0));
                        p.rect_filled(br, 9.0, badge_col);
                        p.text(br.center(), Align2::CENTER_CENTER, txt, FontId::proportional(11.5), Color32::WHITE);
                    }
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.page = page;
                    }
                    ui.add_space(1.0);
                }

                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    let mon = s.settings.monitoring;
                    let txt = if mon { "⏸  Pause monitoring" } else { "▶  Resume monitoring" };
                    if ui.add_sized([ui.available_width(), 30.0], egui::Button::new(txt)).clicked() {
                        s.settings.monitoring = !mon;
                        s.check_now = true;
                        s.dirty = true;
                    }
                    ui.add_space(6.0);
                    let (c, t) = if mon {
                        (GREEN, format!("Checking every {} s", s.settings.interval_secs))
                    } else {
                        (GRAY, "Monitoring paused".to_string())
                    };
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.0, c);
                        ui.label(RichText::new(t).size(12.0).color(MUTED));
                    });
                    let ip = s.local_ip.map(|i| i.to_string()).unwrap_or_else(|| "unknown".into());
                    let gw = s.gateway.map(|i| i.to_string()).unwrap_or_else(|| "unknown".into());
                    let ext = s.external_ip.clone().unwrap_or_else(|| "checking…".into());
                    ui.label(RichText::new(format!("Internet  {}", ext)).size(12.0).color(MUTED))
                        .on_hover_text("Your outside address: what websites see when anyone at home goes online");
                    ui.label(RichText::new(format!("Router  {}", gw)).size(12.0).color(MUTED));
                    ui.label(RichText::new(format!("This computer  {}", ip)).size(12.0).color(MUTED));
                    if let Some(l) = s.traffic.live.back() {
                        ui.label(RichText::new(format!("Now  {} in · {} out", crate::perf::bps(Some(l.rx_bps)), crate::perf::bps(Some(l.tx_bps)))).size(12.0).color(MUTED));
                    }
                    ui.label(RichText::new("YOUR NETWORK").size(10.5).color(FAINT).strong());
                });
            });
    }

    pub(crate) fn header(&mut self, ui: &mut Ui, s: &mut Shared, title: &str, subtitle: &str, extra: impl FnOnce(&mut Self, &mut Ui, &mut Shared)) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(title).size(24.0).strong().color(TEXT));
                ui.label(RichText::new(subtitle).size(13.0).color(MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                self.scan_button(ui, s);
                extra(self, ui, s);
            });
        });
        ui.add_space(14.0);
    }

    fn scan_button(&mut self, ui: &mut Ui, s: &mut Shared) {
        if s.scan.running {
            let frac = if s.scan.total > 0 { s.scan.done as f32 / s.scan.total as f32 } else { 0.0 };
            ui.add(
                egui::ProgressBar::new(frac)
                    .desired_width(240.0)
                    .text(format!("Scanning… {} found ({}/{})", s.scan.found, s.scan.done, s.scan.total))
                    .fill(BLUE.linear_multiply(0.7)),
            );
        } else {
            let b = egui::Button::new(RichText::new("🔍  Scan network").color(Color32::WHITE)).fill(Color32::from_rgb(0x2E, 0x6F, 0xD8));
            if ui.add(b).on_hover_text(format!("Look for devices in {}", s.settings.scan_range)).clicked() {
                self.start_scan_from_ui(ui.ctx(), s);
            }
        }
    }

    pub(crate) fn start_scan_from_ui(&mut self, ctx: &egui::Context, s: &mut Shared) {
        // The scan needs the lock we hold, so start it from another thread.
        let shared = self.shared.clone();
        let ctx2 = ctx.clone();
        s.scan.message = "Starting scan…".into();
        std::thread::spawn(move || monitor::start_scan(shared, ctx2));
    }
}

// ───────────────────────────── dashboard ─────────────────────────────
impl App {
    fn dashboard(&mut self, ui: &mut Ui, s: &mut Shared) {
        let subtitle = if s.scan.message.is_empty() {
            format!("{} devices watched", s.devices.len())
        } else {
            s.scan.message.clone()
        };
        self.header(ui, s, "Dashboard", &subtitle, |this, ui, s| {
            if ui.button("⟳  Check all now").clicked() {
                s.check_now = true;
                this.toast = Some(("Checking every device now…".into(), Instant::now()));
            }
        });

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let w = ui.available_width();
            let gap = 14.0;
            let watched: Vec<&Device> = s.devices.iter().filter(|d| d.kind != DeviceKind::Internet).collect();
            let up = watched.iter().filter(|d| d.status == Status::Up).count();
            let warn = watched.iter().filter(|d| d.status == Status::Warning).count();
            let down = watched.iter().filter(|d| d.status == Status::Down).count();
            let paused = watched.iter().filter(|d| d.status == Status::Paused).count();
            let unknown = watched.iter().filter(|d| d.status == Status::Unknown).count();
            let inet = s.devices.iter().find(|d| d.kind == DeviceKind::Internet);
            let router = s.devices.iter().find(|d| d.kind == DeviceKind::Router);

            // stat tiles
            let tile_w = (w - gap * 3.0) / 4.0;
            let mut go_devices = None;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                let (istatus, ilat) = inet.map(|d| (d.status, d.latency)).unwrap_or((Status::Unknown, None));
                let itext = match istatus {
                    Status::Up => "Connected",
                    Status::Warning => "Slow",
                    Status::Down => "Down",
                    Status::Paused => "Paused",
                    Status::Unknown => "Checking…",
                };
                let tiles = [
                    ("Internet", itext.to_string(), format!("Response {}", ms(ilat)), status_color(istatus), "🌐", None),
                    ("Online", format!("{}", up + warn), format!("of {} devices", watched.len()), GREEN, "✔", Some(Filter::Online)),
                    ("Need attention", format!("{}", warn), "slow or partly answering".to_string(), AMBER, "⚠", Some(Filter::Problems)),
                    (
                        "Offline",
                        format!("{}", down),
                        if down == 0 { "everything answering".to_string() } else { "not responding".to_string() },
                        RED,
                        "✖",
                        Some(Filter::Offline),
                    ),
                ];
                for (label, big, small, col, icon, filt) in tiles {
                    let (rect, resp) = ui.allocate_exact_size(vec2(tile_w, 96.0), Sense::click());
                    let p = ui.painter();
                    p.rect(rect, 10.0, if resp.hovered() { CARD_HI } else { CARD }, Stroke::new(1.0f32, BORDER), StrokeKind::Inside);
                    p.rect_filled(egui::Rect::from_min_size(rect.min + vec2(0.0, 16.0), vec2(4.0, 64.0)), 2.0, col);
                    p.text(rect.min + vec2(20.0, 16.0), Align2::LEFT_TOP, label, FontId::proportional(13.0), MUTED);
                    p.text(rect.min + vec2(20.0, 36.0), Align2::LEFT_TOP, big, FontId::proportional(28.0), TEXT);
                    p.text(rect.min + vec2(20.0, 72.0), Align2::LEFT_TOP, small, FontId::proportional(12.0), MUTED);
                    let ic = pos2(rect.right() - 34.0, rect.top() + 32.0);
                    p.circle_filled(ic, 18.0, col.linear_multiply(0.15));
                    p.text(ic, Align2::CENTER_CENTER, icon, FontId::proportional(17.0), col);
                    if let Some(f) = filt {
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            go_devices = Some(f);
                        }
                    }
                }
            });
            if let Some(f) = go_devices {
                self.filter = f;
                self.page = Page::Devices;
            }
            ui.add_space(gap);

            // chart + donut
            let left_w = (w - gap) * 0.64;
            let right_w = w - gap - left_w;
            let mut minutes = self.chart_minutes;
            let inet_v: Vec<Sample> = inet.map(|d| d.history.iter().cloned().collect()).unwrap_or_default();
            let rout_v: Vec<Sample> = router.map(|d| d.history.iter().cloned().collect()).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                card(ui, left_w, 300.0, "", |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Internet & router response time").size(13.0).color(MUTED).strong());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            for (m, l) in [(360, "6 h"), (60, "1 h"), (30, "30 min")] {
                                if ui.selectable_label(minutes == m, l).clicked() {
                                    minutes = m;
                                }
                            }
                        });
                    });
                    let mut series = vec![];
                    if inet.is_some() {
                        series.push(Series { samples: &inet_v, color: BLUE, label: "Internet".into() });
                    }
                    if router.is_some() {
                        series.push(Series { samples: &rout_v, color: GOLD, label: "Router".into() });
                    }
                    let avail = ui.available_size();
                    latency_chart(ui, vec2(avail.x, avail.y - 24.0), &series, minutes * 60);
                    ui.horizontal(|ui| {
                        for sr in &series {
                            let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                            ui.painter().rect_filled(r.shrink(3.0), 2.0, sr.color);
                            ui.label(RichText::new(&sr.label).size(12.0).color(MUTED));
                            ui.add_space(8.0);
                        }
                        let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                        ui.painter().rect_filled(r.shrink(3.0), 1.0, RED);
                        ui.label(RichText::new("No answer").size(12.0).color(MUTED));
                    });
                });
                card(ui, right_w, 300.0, "Device status", |ui| {
                    ui.horizontal(|ui| {
                        let total = watched.len();
                        donut(
                            ui,
                            150.0,
                            &[(up as f32, GREEN), (warn as f32, AMBER), (down as f32, RED), (unknown as f32, BLUE), (paused as f32, GRAY)],
                            &format!("{}", total),
                            "devices",
                        );
                        ui.vertical(|ui| {
                            ui.add_space(22.0);
                            for (l, n, c) in [("Online", up, GREEN), ("Slow / partial", warn, AMBER), ("Offline", down, RED), ("Checking", unknown, BLUE), ("Paused", paused, GRAY)] {
                                ui.horizontal(|ui| {
                                    let (r, _) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::hover());
                                    ui.painter().circle_filled(r.center(), 4.5, c);
                                    ui.label(RichText::new(l).size(13.0).color(MUTED));
                                    ui.label(RichText::new(n.to_string()).size(13.0).strong());
                                });
                            }
                        });
                    });
                    ui.add_space(6.0);
                    ui.separator();
                    let mut kinds: Vec<(DeviceKind, usize)> = DeviceKind::ALL
                        .iter()
                        .filter(|k| **k != DeviceKind::Internet)
                        .map(|k| (*k, watched.iter().filter(|d| d.kind == *k).count()))
                        .filter(|x| x.1 > 0)
                        .collect();
                    kinds.sort_by(|a, b| b.1.cmp(&a.1));
                    ui.horizontal_wrapped(|ui| {
                        for (k, n) in kinds.iter().take(8) {
                            ui.label(RichText::new(format!("{} {} {}", k.icon(), k.label(), n)).size(12.0).color(MUTED));
                            ui.add_space(4.0);
                        }
                    });
                });
            });
            self.chart_minutes = minutes;
            ui.add_space(gap);

            // attention + recent alerts
            let half = (w - gap) / 2.0;
            let mut select: Option<u64> = None;
            let mut goto_alerts = false;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                card(ui, half, 270.0, "Needs attention", |ui| {
                    let mut probs: Vec<&Device> = s.devices.iter().filter(|d| matches!(d.status, Status::Down | Status::Warning)).collect();
                    probs.sort_by_key(|d| status_rank(d.status));
                    if probs.is_empty() {
                        ui.add_space(40.0);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("✔").size(30.0).color(GREEN));
                            ui.label(RichText::new("All devices are answering normally").color(MUTED));
                        });
                    }
                    egui::ScrollArea::vertical().id_salt("probs").show(ui, |ui| {
                        for d in probs.iter().take(20) {
                            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
                            let p = ui.painter();
                            if resp.hovered() {
                                p.rect_filled(rect, 6.0, CARD_HI);
                            }
                            dot(p, rect.left_center() + vec2(12.0, 0.0), status_color(d.status));
                            p.text(rect.left_center() + vec2(28.0, 0.0), Align2::LEFT_CENTER, d.kind.icon(), FontId::proportional(14.0), MUTED);
                            p.text(rect.left_center() + vec2(50.0, -8.0), Align2::LEFT_CENTER, &d.name, FontId::proportional(13.5), TEXT);
                            let detail = match d.status {
                                Status::Down => format!("{} · offline since {}", d.ip, d.down_since.map(fmt_time).unwrap_or_default()),
                                _ => format!("{} · {}", d.ip, if d.slow { format!("slow, {}", ms(d.latency)) } else { "a check is failing".into() }),
                            };
                            p.text(rect.left_center() + vec2(50.0, 9.0), Align2::LEFT_CENTER, detail, FontId::proportional(11.5), MUTED);
                            p.text(rect.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, d.status.label(), FontId::proportional(12.0), status_color(d.status));
                            if resp.clicked() {
                                select = Some(d.id);
                            }
                        }
                    });
                });
                card(ui, half, 270.0, "", |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Recent alerts").size(13.0).color(MUTED).strong());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.link("See all").clicked() {
                                goto_alerts = true;
                            }
                        });
                    });
                    egui::ScrollArea::vertical().id_salt("recent").show(ui, |ui| {
                        for e in s.events.iter().take(12) {
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(4.0, 30.0), Sense::hover());
                                ui.painter().rect_filled(r, 2.0, severity_color(e.severity));
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    ui.label(RichText::new(format!("{} — {}", e.device_name, e.message)).size(13.0));
                                    ui.label(RichText::new(format!("{} · {}", fmt_time(e.t), e.severity.label())).size(11.0).color(FAINT));
                                });
                            });
                        }
                        if s.events.is_empty() {
                            ui.label(RichText::new("No alerts yet").color(MUTED));
                        }
                    });
                });
            });
            if let Some(id) = select {
                self.selected = Some(id);
                self.page = Page::Devices;
            }
            if goto_alerts {
                self.page = Page::Alerts;
            }
        });
    }
}

// ───────────────────────────── alert banner & sound ─────────────────────────────
impl App {
    /// A red strip across the top while there are offline / critical alerts nobody has looked at.
    fn banner(&mut self, ctx: &egui::Context, s: &mut Shared) {
        let open: Vec<&Event> = s.events.iter().filter(|e| !e.acknowledged && e.severity == Severity::Critical).collect();
        if open.is_empty() {
            return;
        }
        let first = format!("{} — {}", open[0].device_name, open[0].message);
        let n = open.len();
        let mut go = false;
        let mut seen = false;
        egui::TopBottomPanel::top("banner")
            .exact_height(40.0)
            .frame(egui::Frame::new().fill(Color32::from_rgb(0x4A, 0x1C, 0x22)).inner_margin(egui::Margin::symmetric(20, 6)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("⚠").size(16.0).color(RED));
                    let txt = if n == 1 { first.clone() } else { format!("{}  (and {} more)", first, n - 1) };
                    ui.label(RichText::new(txt).color(TEXT).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Mark as seen").clicked() {
                            seen = true;
                        }
                        if ui.button("View alerts").clicked() {
                            go = true;
                        }
                    });
                });
            });
        if go {
            self.page = Page::Alerts;
            self.alert_filter = AlertFilter::Open;
        }
        if seen {
            for e in s.events.iter_mut().filter(|e| e.severity == Severity::Critical) {
                e.acknowledged = true;
            }
            s.dirty = true;
        }
    }
}

/// Play the system warning sound.
fn beep() {
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Diagnostics::Debug::MessageBeep(windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONEXCLAMATION);
    }
    #[cfg(not(windows))]
    crate::unix::beep();
}
