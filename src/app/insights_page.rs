//! Insights: plain-language findings built from what the monitor has seen.

use super::*;
use crate::traffic::{bytes, day_key, AppStat};
use crate::trust::SigStatus;

struct Item {
    text: String,
    detail: String,
    color: Color32,
    app: Option<String>,
    host: Option<String>,
    device: Option<u64>,
}

impl App {
    pub(super) fn insights(&mut self, ui: &mut Ui, s: &mut Shared) {
        self.header(ui, s, "Insights", "What stands out, in plain words", |_this, _ui, _s| {});
        let t = now();
        let tr = &s.traffic.saved;
        let today = day_key(t);
        let yesterday = day_key(t - 86400);
        // HomeNet Watch itself is left out: its own web checks and lookups are expected.
        let apps: Vec<&AppStat> = tr.apps.values().filter(|a| !crate::util::is_self(&a.path)).collect();

        // 1. phoning home
        let mut phoning: Vec<Item> = apps
            .iter()
            .filter_map(|a| {
                let hrs = a.hours.iter().filter(|h| t / 3600 - **h < 24).count();
                (hrs >= 8).then(|| {
                    let mut cats: std::collections::BTreeMap<String, usize> = Default::default();
                    for ip in &a.dests {
                        if let Some(h) = tr.hosts.get(ip) {
                            *cats.entry(if h.category.is_empty() { "Other".into() } else { h.category.clone() }).or_default() += 1;
                        }
                    }
                    let mut cv: Vec<(String, usize)> = cats.into_iter().collect();
                    cv.sort_by(|a, b| b.1.cmp(&a.1));
                    let top = cv.iter().take(2).map(|c| c.0.to_lowercase()).collect::<Vec<_>>().join(" and ");
                    Item {
                        text: format!("{} went online in {} of the last 24 hours", a.name, hrs),
                        detail: if top.is_empty() { "Often apps check for updates or messages in the background.".into() } else { format!("Mostly {} services. Often this is updates or sync running in the background.", top) },
                        color: if hrs >= 18 { AMBER } else { BLUE },
                        app: Some(a.key.clone()),
                        host: None,
                        device: None,
                    }
                })
            })
            .collect();
        phoning.sort_by(|a, b| b.text.cmp(&a.text));

        // 2. what changed since yesterday
        let mut changed = vec![];
        // On the first day everything is "new"; summarise instead of listing it all.
        let started = s
            .devices
            .iter()
            .filter(|d| d.kind != DeviceKind::Internet)
            .map(|d| d.first_seen)
            .chain(apps.iter().map(|a| a.first_seen))
            .min()
            .unwrap_or(t);
        let first_day = t - started < 86400;
        if first_day {
            let n_dev = s.devices.iter().filter(|d| d.kind != DeviceKind::Internet).count();
            let n_hosts = tr.hosts.len();
            changed.push(Item {
                text: format!("HomeNet Watch started watching {}: {} devices at home and {} programs online so far", ago(Some(started)), n_dev, apps.len()),
                detail: format!(
                    "Everything counts as new on the first day, so it isn't listed here. From tomorrow this shows only what's changed. ({} internet places contacted so far.)",
                    n_hosts
                ),
                color: BLUE,
                app: None,
                host: None,
                device: None,
            });
        }
        let new_apps: Vec<&&AppStat> = if first_day { vec![] } else { apps.iter().filter(|a| t - a.first_seen < 86400).collect() };
        for a in new_apps.iter().take(8) {
            changed.push(Item { text: format!("New program online: {}", a.name), detail: format!("First connected {}.", ago(Some(a.first_seen))), color: BLUE, app: Some(a.key.clone()), host: None, device: None });
        }
        let new_hosts = if first_day { 0 } else { tr.hosts.values().filter(|h| t - h.first_seen < 86400).count() };
        if new_hosts > 0 {
            changed.push(Item { text: format!("{} internet places contacted for the first time", new_hosts), detail: "Normal when you visit new websites or apps update.".into(), color: FAINT, app: None, host: None, device: None });
        }
        let old_countries: std::collections::HashSet<&str> = tr.hosts.values().filter(|h| t - h.first_seen >= 86400).filter_map(|h| h.country.as_deref()).collect();
        let mut new_countries: Vec<&str> = tr.hosts.values().filter(|h| t - h.first_seen < 86400).filter_map(|h| h.country.as_deref()).filter(|c| !old_countries.contains(c)).collect();
        new_countries.sort();
        new_countries.dedup();
        if !new_countries.is_empty() && !old_countries.is_empty() {
            changed.push(Item { text: format!("Traffic to new countries: {}", new_countries.join(", ")), detail: "Big services use data centers worldwide, so this is often harmless.".into(), color: AMBER, app: None, host: None, device: None });
        }
        for d in s.devices.iter().filter(|d| !first_day && t - d.first_seen < 86400 && d.kind != DeviceKind::Internet).take(8) {
            changed.push(Item { text: format!("New device at home: {}", d.name), detail: format!("{} · {}", d.ip, d.kind.label()), color: BLUE, app: None, host: None, device: Some(d.id) });
        }
        for d in s.devices.iter().filter(|d| d.departed).take(8) {
            changed.push(Item { text: format!("{} hasn't been seen lately", d.name), detail: format!("Last seen {}.", ago(d.last_seen)), color: FAINT, app: None, host: None, device: Some(d.id) });
        }
        let (rx_t, tx_t) = apps.iter().filter_map(|a| a.daily.get(&today)).fold((0u64, 0u64), |a, d| (a.0 + d.conns, a.1 + d.rx + d.tx));
        let (rx_y, _) = apps.iter().filter_map(|a| a.daily.get(&yesterday)).fold((0u64, 0u64), |a, d| (a.0 + d.conns, a.1 + d.rx + d.tx));
        if rx_y > 0 {
            let pct = (rx_t as f64 - rx_y as f64) / rx_y as f64 * 100.0;
            changed.push(Item {
                text: format!("{} connections today vs {} yesterday ({:+.0}%)", rx_t, rx_y, pct),
                detail: if tx_t > 0 { format!("{} of data so far today.", bytes(tx_t)) } else { "Counts every new connection programs open.".into() },
                color: FAINT,
                app: None,
                host: None,
                device: None,
            });
        }

        // 3. worth a closer look
        let mut look = vec![];
        for a in &apps {
            if a.changed_since_pin {
                look.push(Item { text: format!("{} changed since you trusted it", a.name), detail: "If it just updated, trust the new version. If not, scan your computer.".into(), color: RED, app: Some(a.key.clone()), host: None, device: None });
            }
            if matches!(a.sig, SigStatus::Unsigned | SigStatus::Invalid) {
                look.push(Item {
                    text: format!("{} uses the internet and {}", a.name, if a.sig == SigStatus::Invalid { "its signature is broken" } else { "isn't signed" }),
                    detail: "Signed programs say who made them. Many small tools aren't signed; check you recognise this one.".into(),
                    color: if a.sig == SigStatus::Invalid { RED } else { AMBER },
                    app: Some(a.key.clone()),
                    host: None,
                    device: None,
                });
            }
            if let Some(why) = crate::trust::odd_location(&a.path) {
                look.push(Item { text: format!("{} {}", a.name, why), detail: "Programs normally live in Program Files or Applications.".into(), color: AMBER, app: Some(a.key.clone()), host: None, device: None });
            }
            let countries: std::collections::HashSet<&str> = a.dests.iter().filter_map(|ip| tr.hosts.get(ip)).filter_map(|h| h.country.as_deref()).collect();
            if countries.len() >= 6 {
                look.push(Item { text: format!("{} talks to {} countries", a.name, countries.len()), detail: "Browsers and games do this; other programs usually don't.".into(), color: BLUE, app: Some(a.key.clone()), host: None, device: None });
            }
        }
        for h in tr.hosts.values() {
            if let Some(every) = crate::traffic::beacon_score(&h.starts) {
                look.push(Item {
                    text: format!("{} checks in with {} every {}", if h.apps.is_empty() { "A program".to_string() } else { h.apps.join(", ") }, h.name.clone().unwrap_or_else(|| h.ip.clone()), dur(every as i64)),
                    detail: "Clockwork check-ins are typical for updaters and chat apps; unknown programs doing it deserve a look.".into(),
                    color: BLUE,
                    app: None,
                    host: Some(h.ip.clone()),
                    device: None,
                });
            }
        }

        // 4. ads & tracking
        let mut tracking: Vec<(String, usize)> = vec![];
        for a in &apps {
            let n = a.dests.iter().filter_map(|ip| tr.hosts.get(ip)).filter(|h| h.category == "Advertising" || h.category == "Analytics").count();
            if n > 0 {
                tracking.push((a.name.clone(), n));
            }
        }
        tracking.sort_by(|a, b| b.1.cmp(&a.1));

        let mut open: Option<Item> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut section = |ui: &mut Ui, title: &str, empty: &str, items: Vec<Item>| {
                flow_card(ui, title, |ui| {
                    if items.is_empty() {
                        ui.label(RichText::new(empty).color(MUTED));
                    }
                    for it in items.into_iter().take(12) {
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(4.0, 34.0), Sense::hover());
                            ui.painter().rect_filled(r, 2.0, it.color);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                let clickable = it.app.is_some() || it.host.is_some() || it.device.is_some();
                                if clickable {
                                    if ui.link(RichText::new(&it.text).size(14.0)).clicked() {
                                        open = Some(Item { text: String::new(), detail: String::new(), color: it.color, app: it.app.clone(), host: it.host.clone(), device: it.device });
                                    }
                                } else {
                                    ui.label(RichText::new(&it.text).size(14.0));
                                }
                                ui.label(RichText::new(&it.detail).size(12.0).color(MUTED));
                            });
                        });
                        ui.add_space(4.0);
                    }
                });
                ui.add_space(12.0);
            };
            section(ui, "WORTH A CLOSER LOOK", "Nothing unusual right now. ✔", look);
            section(ui, "WHAT CHANGED SINCE YESTERDAY", "Nothing new in the last 24 hours.", changed);
            section(ui, "PHONING HOME", "No program has been checking in all day.", phoning);
            let tr_items: Vec<Item> = tracking
                .iter()
                .take(8)
                .map(|(n, c)| Item { text: format!("{} contacted {} advertising or analytics services", n, c), detail: "These count how apps are used or show ads.".into(), color: AMBER, app: apps.iter().find(|a| &a.name == n).map(|a| a.key.clone()), host: None, device: None })
                .collect();
            section(ui, "ADS & TRACKING", "No advertising or analytics services seen yet.", tr_items);
        });
        if let Some(it) = open {
            if let Some(d) = it.device {
                self.selected = Some(d);
                self.page = Page::Devices;
            } else {
                self.sel_app = it.app;
                self.sel_host = it.host;
                self.page = Page::Apps;
            }
        }
    }
}
