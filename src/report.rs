//! "This week" report: compares the last 7 days with the 7 before and saves a
//! standalone HTML page (no internet needed to open it).

use crate::model::{alert_info, now, Shared};
use crate::traffic::{bytes, day_key};
use std::collections::BTreeMap;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn change(now: f64, before: f64) -> String {
    if before <= 0.0 {
        if now > 0.0 {
            "<span class=up>new</span>".into()
        } else {
            "—".into()
        }
    } else {
        let p = (now - before) / before * 100.0;
        let cls = if p > 15.0 { "up" } else if p < -15.0 { "down" } else { "same" };
        format!("<span class={}>{}{:.0}%</span>", cls, if p >= 0.0 { "+" } else { "" }, p)
    }
}

pub fn weekly_html(s: &Shared) -> String {
    let t = now();
    let week = 7 * 86400;
    let (t0, t1) = (t - week, t - 2 * week);
    let tr = &s.traffic;
    let (rx, tx) = tr.total_since(t0);
    let (prx, ptx) = tr.total_between(t1, t0);
    let days: Vec<String> = (0..7).map(|i| day_key(t - i * 86400)).collect();
    let prev_days: Vec<String> = (7..14).map(|i| day_key(t - i * 86400)).collect();

    // programs
    let mut apps: Vec<(String, u64, u64, u64, u64, bool)> = tr
        .saved
        .apps
        .values()
        .map(|a| {
            let sum = |ds: &Vec<String>| ds.iter().filter_map(|d| a.daily.get(d)).fold((0u64, 0u64), |acc, u| (acc.0 + u.conns, acc.1 + u.rx + u.tx));
            let (c, b) = sum(&days);
            let (pc, pb) = sum(&prev_days);
            (a.name.clone(), c, b, pc, pb, a.first_seen >= t0)
        })
        .filter(|x| x.1 > 0)
        .collect();
    let by_bytes = tr.per_app_bytes || apps.iter().any(|a| a.2 > 0);
    apps.sort_by(|a, b| if by_bytes { b.2.cmp(&a.2) } else { b.1.cmp(&a.1) });

    let new_apps: Vec<&str> = tr.saved.apps.values().filter(|a| a.first_seen >= t0).map(|a| a.name.as_str()).collect();
    let new_hosts = tr.saved.hosts.values().filter(|h| h.first_seen >= t0).count();
    let new_devices: Vec<&str> = s.devices.iter().filter(|d| d.first_seen >= t0).map(|d| d.name.as_str()).collect();

    // countries
    let mut countries: BTreeMap<String, (u64, bool)> = BTreeMap::new();
    for h in tr.saved.hosts.values().filter(|h| h.last_seen >= t0) {
        if let Some(c) = &h.country {
            let e = countries.entry(c.clone()).or_insert((0, true));
            e.0 += h.conns;
            if h.first_seen < t0 {
                e.1 = false;
            }
        }
    }
    let mut countries: Vec<(String, u64, bool)> = countries.into_iter().map(|(k, v)| (k, v.0, v.1)).collect();
    countries.sort_by(|a, b| b.1.cmp(&a.1));

    // alerts
    let mut kinds: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for e in &s.events {
        if e.kind == "scan" {
            continue;
        }
        if e.t >= t0 {
            kinds.entry(e.kind.clone()).or_default().0 += 1;
        } else if e.t >= t1 {
            kinds.entry(e.kind.clone()).or_default().1 += 1;
        }
    }
    let speeds: Vec<_> = tr.saved.speed.iter().filter(|r| r.t >= t0).collect();

    let mut h = String::new();
    h += "<!doctype html><html><head><meta charset=utf-8><title>HomeNet Watch — this week</title><style>";
    h += "body{background:#0e141f;color:#dce3ee;font-family:Segoe UI,Helvetica,Arial,sans-serif;margin:0;padding:32px}";
    h += ".wrap{max-width:980px;margin:auto}h1{margin:0;font-size:28px}h2{font-size:15px;color:#8a97ab;text-transform:uppercase;letter-spacing:.05em;margin:28px 0 10px}";
    h += ".sub{color:#8a97ab;margin-top:4px}.grid{display:grid;grid-template-columns:repeat(4,1fr);gap:12px}.card{background:#172131;border:1px solid #253348;border-radius:10px;padding:14px 16px}";
    h += ".big{font-size:26px;font-weight:600;margin:6px 0}.muted{color:#8a97ab;font-size:13px}table{width:100%;border-collapse:collapse;background:#172131;border:1px solid #253348;border-radius:10px;overflow:hidden}";
    h += "td,th{padding:9px 12px;border-bottom:1px solid #22304a;text-align:left;font-size:14px}th{color:#8a97ab;font-weight:500}.up{color:#f2992e}.down{color:#34c77b}.same{color:#8a97ab}";
    h += ".tag{display:inline-block;background:#223047;border-radius:10px;padding:2px 9px;margin:2px;font-size:13px}.brand{color:#e9b63b;font-weight:600}footer{margin-top:30px;color:#556276;font-size:12px}";
    h += "</style></head><body><div class=wrap>";
    h += &format!(
        "<div class=brand>HomeNet Watch by DDID</div><h1>This week on your network</h1><div class=sub>{} – {} · compared with the 7 days before</div>",
        esc(&crate::widgets::fmt_time(t0)),
        esc(&crate::widgets::fmt_time(t))
    );
    h += "<h2>Usage</h2><div class=grid>";
    let card = |title: &str, big: String, sub: String| format!("<div class=card><div class=muted>{}</div><div class=big>{}</div><div class=muted>{}</div></div>", title, big, sub);
    h += &card("Downloaded", bytes(rx), format!("{} vs last week", change(rx as f64, prx as f64)));
    h += &card("Uploaded", bytes(tx), format!("{} vs last week", change(tx as f64, ptx as f64)));
    h += &card("New programs online", new_apps.len().to_string(), format!("{} new internet destinations", new_hosts));
    h += &card("New devices at home", new_devices.len().to_string(), format!("{} devices watched", s.devices.len()));
    h += "</div>";

    h += "<h2>Busiest programs</h2><table><tr><th>Program</th>";
    h += if by_bytes { "<th>Data</th><th>Change</th><th>Connections</th></tr>" } else { "<th>Connections</th><th>Change</th><th></th></tr>" };
    for (name, c, b, pc, pb, is_new) in apps.iter().take(12) {
        let tag = if *is_new { " <span class=tag>new</span>" } else { "" };
        if by_bytes {
            h += &format!("<tr><td>{}{}</td><td>{}</td><td>{}</td><td>{}</td></tr>", esc(name), tag, bytes(*b), change(*b as f64, *pb as f64), c);
        } else {
            h += &format!("<tr><td>{}{}</td><td>{}</td><td>{}</td><td></td></tr>", esc(name), tag, c, change(*c as f64, *pc as f64));
        }
    }
    if apps.is_empty() {
        h += "<tr><td colspan=4 class=muted>No program activity recorded yet.</td></tr>";
    }
    h += "</table>";

    h += "<h2>New this week</h2><div class=card>";
    if new_apps.is_empty() && new_devices.is_empty() {
        h += "<span class=muted>Nothing new.</span>";
    }
    for a in new_apps.iter().take(40) {
        h += &format!("<span class=tag>💻 {}</span>", esc(a));
    }
    for d in new_devices.iter().take(40) {
        h += &format!("<span class=tag>📶 {}</span>", esc(d));
    }
    h += "</div>";

    h += "<h2>Countries</h2><div class=card>";
    if countries.is_empty() {
        h += "<span class=muted>Country lookup is off (Settings → Internet destinations).</span>";
    }
    for (c, n, is_new) in countries.iter().take(30) {
        h += &format!("<span class=tag>{} · {}{}</span>", esc(c), n, if *is_new { " · new" } else { "" });
    }
    h += "</div>";

    h += "<h2>Alerts</h2><table><tr><th>Type</th><th>This week</th><th>Last week</th><th>MITRE ATT&amp;CK</th></tr>";
    for (k, (a, b)) in &kinds {
        let info = alert_info(k);
        let mitre = if info.mitre.is_empty() {
            "—".to_string()
        } else {
            format!("<a href=\"https://attack.mitre.org/techniques/{}/\" style=\"color:#4c9aff\">{}</a>", info.mitre.replace('.', "/"), esc(&crate::model::mitre_label(info.mitre)))
        };
        h += &format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>", esc(info.title), a, b, mitre);
    }
    if kinds.is_empty() {
        h += "<tr><td colspan=4 class=muted>No alerts.</td></tr>";
    }
    h += "</table>";

    h += "<h2>Speed tests</h2><table><tr><th>When</th><th>Download</th><th>Upload</th><th>Response</th></tr>";
    for r in speeds.iter().rev() {
        h += &format!("<tr><td>{}</td><td>{:.0} Mb/s</td><td>{:.0} Mb/s</td><td>{:.0} ms</td></tr>", esc(&crate::widgets::fmt_time(r.t)), r.down_mbps, r.up_mbps, r.ping_ms);
    }
    if speeds.is_empty() {
        h += "<tr><td colspan=4 class=muted>No speed tests this week.</td></tr>";
    }
    h += "</table>";
    h += "<footer>Made with HomeNet Watch by DDID · DaleDumbsITDown.com · everything in this report stayed on your computer.</footer></div></body></html>";
    h
}

/// Save the report into the Documents folder and return its path.
pub fn save_weekly(s: &Shared) -> std::io::Result<std::path::PathBuf> {
    let dir = crate::util::documents_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("HomeNetWatch-week-{}.html", chrono::Local::now().format("%Y-%m-%d")));
    std::fs::write(&path, weekly_html(s))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn mitre_labels() {
        assert_eq!(crate::model::mitre_label("T1557.002"), "T1557.002 ARP Cache Poisoning (Credential Access, Collection)");
        assert_eq!(crate::model::mitre_label(""), "");
        // every technique an alert uses has a name and tactic
        for k in crate::model::ALERT_KINDS {
            let m = crate::model::alert_info(k).mitre;
            if !m.is_empty() {
                assert!(crate::model::mitre_details(m).is_some(), "missing details for {}", m);
            }
        }
    }
}
