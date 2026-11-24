//! Traffic of this computer: live upload/download, minute-by-minute history,
//! which programs connect where, destination details, program trust, speed
//! tests — and the detections built on top (pinned program changed, regular
//! check-ins, many home devices contacted).

use crate::model::{now, Severity, Shared};
use crate::sysnet::{self, RawConn};
use crate::trust::SigStatus;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const MINUTES_KEPT: usize = 60 * 24 * 14;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct Minute {
    pub t: i64,
    pub rx: u64,
    pub tx: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct DayUse {
    pub conns: u64,
    pub rx: u64,
    pub tx: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AppStat {
    pub key: String,
    pub name: String,
    pub path: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub conns: u64,
    pub rx: u64,
    pub tx: u64,
    /// Hours (unix time / 3600) in which it opened new connections
    #[serde(default)]
    pub hours: Vec<i64>,
    #[serde(default)]
    pub dests: Vec<String>,
    #[serde(default)]
    pub daily: BTreeMap<String, DayUse>,
    #[serde(default)]
    pub sig: SigStatus,
    #[serde(default)]
    pub signer: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    /// Fingerprint the person trusted ("pinned")
    #[serde(default)]
    pub pinned: Option<String>,
    #[serde(default)]
    pub changed_since_pin: bool,
    #[serde(default)]
    pub parent: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Host {
    pub ip: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub named: bool,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub cc: Option<String>,
    #[serde(default)]
    pub lat: Option<f32>,
    #[serde(default)]
    pub lon: Option<f32>,
    #[serde(default)]
    pub geo_done: bool,
    pub first_seen: i64,
    pub last_seen: i64,
    pub conns: u64,
    pub rx: u64,
    pub tx: u64,
    #[serde(default)]
    pub ports: Vec<u16>,
    #[serde(default)]
    pub apps: Vec<String>,
    /// When recent connections to it started (for regular check-in detection)
    #[serde(default)]
    pub starts: Vec<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SpeedResult {
    pub t: i64,
    pub down_mbps: f32,
    pub up_mbps: f32,
    pub ping_ms: f32,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct TrafficSaved {
    #[serde(default)]
    pub minutes: VecDeque<Minute>,
    #[serde(default)]
    pub apps: HashMap<String, AppStat>,
    #[serde(default)]
    pub hosts: HashMap<String, Host>,
    #[serde(default)]
    pub speed: Vec<SpeedResult>,
    #[serde(default)]
    pub beacons_alerted: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct Live {
    pub t: f64,
    pub rx_bps: f64,
    pub tx_bps: f64,
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // pid and local port kept for the coming firewall controls
pub struct Conn {
    pub app: String,
    pub pid: u32,
    pub remote: Ipv4Addr,
    pub rport: u16,
    pub lport: u16,
    pub since: i64,
    pub rx: u64,
    pub tx: u64,
}

#[derive(Default, Clone)]
pub struct SpeedRun {
    pub running: bool,
    pub phase: String,
    pub progress: f32,
    pub live_mbps: f32,
    pub ping_ms: Option<f32>,
    pub down: Option<f32>,
    pub up: Option<f32>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Traffic {
    pub saved: TrafficSaved,
    pub live: VecDeque<Live>,
    pub conns: Vec<Conn>,
    /// Byte counts per program available (Windows, run as administrator)
    pub per_app_bytes: bool,
    pub per_app_checked: bool,
    pub dirty: bool,
    pub speed: SpeedRun,
    pub lan_admin: Vec<(i64, Ipv4Addr, u16)>,
    pub lateral_alerted: i64,
}

impl Traffic {
    pub fn total_since(&self, t0: i64) -> (u64, u64) {
        self.saved.minutes.iter().filter(|m| m.t >= t0).fold((0, 0), |a, m| (a.0 + m.rx, a.1 + m.tx))
    }
    pub fn total_between(&self, t0: i64, t1: i64) -> (u64, u64) {
        self.saved.minutes.iter().filter(|m| m.t >= t0 && m.t < t1).fold((0, 0), |a, m| (a.0 + m.rx, a.1 + m.tx))
    }
}

pub fn day_key(t: i64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_opt(t, 0).single().map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default()
}

fn file() -> std::path::PathBuf {
    crate::model::data_dir().join("traffic.json")
}

pub fn load() -> TrafficSaved {
    std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(t: &TrafficSaved) {
    let _ = std::fs::create_dir_all(crate::model::data_dir());
    let tmp = crate::model::data_dir().join("traffic.json.tmp");
    if std::fs::write(&tmp, serde_json::to_vec(t).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(tmp, file());
    }
}

fn push_unique<T: PartialEq>(v: &mut Vec<T>, x: T, cap: usize) {
    if !v.contains(&x) {
        v.push(x);
        if v.len() > cap {
            v.remove(0);
        }
    }
}

struct Track {
    app: String,
    since: i64,
    rx: u64,
    tx: u64,
    counting: bool,
}

pub type SharedRef = Arc<Mutex<Shared>>;

/// Start the background work. Everything runs on its own threads.
pub fn start(shared: SharedRef, ctx: eframe::egui::Context) {
    // 1) interface totals every second + connections every 3 seconds
    {
        let shared = shared.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut prev: Option<(u64, u64, Instant)> = None;
            let mut tracks: HashMap<(u32, SocketAddrV4, SocketAddrV4), Track> = HashMap::new();
            let mut procs: HashMap<u32, sysnet::ProcInfo> = HashMap::new();
            let mut tick: u64 = 0;
            let mut last_save = Instant::now();
            loop {
                let t_loop = Instant::now();
                if let Some((rx, tx)) = sysnet::iface_totals() {
                    if let Some((prx, ptx, pt)) = prev {
                        let secs = pt.elapsed().as_secs_f64().max(0.2);
                        let drx = rx.saturating_sub(prx);
                        let dtx = tx.saturating_sub(ptx);
                        // ignore counter resets / absurd jumps
                        if drx < 20_000_000_000 && dtx < 20_000_000_000 {
                            let mut s = shared.lock().unwrap();
                            let tr = &mut s.traffic;
                            let t = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
                            tr.live.push_back(Live { t, rx_bps: drx as f64 * 8.0 / secs, tx_bps: dtx as f64 * 8.0 / secs });
                            while tr.live.len() > 600 {
                                tr.live.pop_front();
                            }
                            let m = now() / 60 * 60;
                            match tr.saved.minutes.back_mut() {
                                Some(last) if last.t == m => {
                                    last.rx += drx;
                                    last.tx += dtx;
                                }
                                _ => tr.saved.minutes.push_back(Minute { t: m, rx: drx, tx: dtx }),
                            }
                            while tr.saved.minutes.len() > MINUTES_KEPT {
                                tr.saved.minutes.pop_front();
                            }
                            tr.dirty = true;
                        }
                    }
                    prev = Some((rx, tx, Instant::now()));
                }

                if tick % 3 == 0 {
                    if tick % 15 == 0 || procs.is_empty() {
                        procs = sysnet::processes();
                    }
                    let raw = sysnet::connections();
                    poll_connections(&shared, raw, &mut tracks, &mut procs);
                }
                tick += 1;
                ctx.request_repaint();

                if last_save.elapsed() > Duration::from_secs(60) {
                    let snap = {
                        let mut s = shared.lock().unwrap();
                        if s.persist && s.traffic.dirty {
                            s.traffic.dirty = false;
                            Some(s.traffic.saved.clone())
                        } else {
                            None
                        }
                    };
                    if let Some(snap) = snap {
                        save(&snap);
                    }
                    last_save = Instant::now();
                }
                let spent = t_loop.elapsed();
                std::thread::sleep(Duration::from_millis(1000).saturating_sub(spent));
            }
        });
    }

    // 2) names for new destinations
    for _ in 0..3 {
        let shared = shared.clone();
        std::thread::spawn(move || loop {
            let next = {
                let mut s = shared.lock().unwrap();
                let ip = s.traffic.saved.hosts.values_mut().filter(|h| !h.named).max_by_key(|h| h.last_seen).map(|h| {
                    h.named = true;
                    h.ip.clone()
                });
                ip
            };
            let Some(ip_s) = next else {
                std::thread::sleep(Duration::from_secs(2));
                continue;
            };
            let Ok(ip) = ip_s.parse::<Ipv4Addr>() else { continue };
            // Home devices: use the name from the Devices list.
            let known = shared.lock().unwrap().devices.iter().find(|d| d.ip == ip).map(|d| d.name.clone());
            let name = known.or_else(|| crate::net::reverse_name(ip));
            let mut s = shared.lock().unwrap();
            if let Some(h) = s.traffic.saved.hosts.get_mut(&ip_s) {
                h.category = crate::categories::classify(name.as_deref(), ip).to_string();
                h.name = name;
                s.traffic.dirty = true;
            }
        });
    }

    // 3a) program signatures (quick) — once per program each time the app starts
    {
        let shared = shared.clone();
        std::thread::spawn(move || {
            let mut checked: HashSet<String> = HashSet::new();
            loop {
                let next = {
                    let s = shared.lock().unwrap();
                    s.traffic.saved.apps.values().find(|a| !a.path.is_empty() && !checked.contains(&a.key)).map(|a| (a.key.clone(), a.path.clone()))
                };
                let Some((key, path)) = next else {
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                };
                checked.insert(key.clone());
                let (sig, signer) = crate::trust::signature(&path);
                let mut s = shared.lock().unwrap();
                if let Some(a) = s.traffic.saved.apps.get_mut(&key) {
                    a.sig = sig;
                    a.signer = signer;
                }
                s.traffic.dirty = true;
            }
        });
    }
    // 3b) program fingerprints (big programs take a moment) and pinned-program checks
    {
        let shared = shared.clone();
        std::thread::spawn(move || {
            let mut checked: HashSet<String> = HashSet::new();
            loop {
                let next = {
                    let s = shared.lock().unwrap();
                    // pinned programs first, so a change is caught quickly
                    let mut v: Vec<(&String, &AppStat)> = s.traffic.saved.apps.iter().filter(|(k, a)| !a.path.is_empty() && !checked.contains(*k)).collect();
                    v.sort_by_key(|(_, a)| a.pinned.is_none());
                    v.first().map(|(k, a)| ((*k).clone(), a.path.clone()))
                };
                let Some((key, path)) = next else {
                    std::thread::sleep(Duration::from_secs(3));
                    continue;
                };
                checked.insert(key.clone());
                let sha = crate::sha256::file(&path);
                let mut s = shared.lock().unwrap();
                let mut alert = None;
                if let Some(a) = s.traffic.saved.apps.get_mut(&key) {
                    if sha.is_some() {
                        a.sha256 = sha.clone();
                    }
                    if let (Some(p), Some(h)) = (&a.pinned, &sha) {
                        if p != h && !a.changed_since_pin {
                            a.changed_since_pin = true;
                            alert = Some(format!("{} changed since you trusted it (its fingerprint is different)", a.name));
                        }
                    }
                }
                s.traffic.dirty = true;
                if let Some(msg) = alert {
                    s.add_alert("program-changed", Severity::Critical, None, msg);
                }
            }
        });
    }

    // 4) countries (only if the person turned it on) and regular check-in detection
    {
        let shared = shared.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(30));
            detect_beacons(&shared);
            let batch: Vec<String> = {
                let s = shared.lock().unwrap();
                if !s.settings.geo_lookup {
                    continue;
                }
                s.traffic
                    .saved
                    .hosts
                    .values()
                    .filter(|h| !h.geo_done && h.ip.parse::<Ipv4Addr>().map(|ip| !sysnet::is_local(ip)).unwrap_or(false))
                    .map(|h| h.ip.clone())
                    .take(100)
                    .collect()
            };
            if batch.is_empty() {
                continue;
            }
            let body = serde_json::to_vec(&batch).unwrap_or_default();
            let r = crate::web::request(
                "http://ip-api.com/batch?fields=status,country,countryCode,lat,lon,query",
                crate::web::Opts { method: "POST", body: Some(&body), content_type: Some("application/json"), timeout_ms: 10000, ..Default::default() },
            );
            if let Ok(r) = r {
                if let Ok(serde_json::Value::Array(items)) = serde_json::from_slice::<serde_json::Value>(&r.body) {
                    let mut s = shared.lock().unwrap();
                    for it in items {
                        let Some(ip) = it.get("query").and_then(|v| v.as_str()) else { continue };
                        if let Some(h) = s.traffic.saved.hosts.get_mut(ip) {
                            h.geo_done = true;
                            if it.get("status").and_then(|v| v.as_str()) == Some("success") {
                                h.country = it.get("country").and_then(|v| v.as_str()).map(String::from);
                                h.cc = it.get("countryCode").and_then(|v| v.as_str()).map(String::from);
                                h.lat = it.get("lat").and_then(|v| v.as_f64()).map(|v| v as f32);
                                h.lon = it.get("lon").and_then(|v| v.as_f64()).map(|v| v as f32);
                            }
                        }
                    }
                    s.traffic.dirty = true;
                }
            }
        });
    }
}

fn app_key(info: Option<&sysnet::ProcInfo>, pid: u32) -> (String, String, String) {
    match info {
        Some(p) if !p.path.is_empty() => (p.path.to_lowercase(), display_name(&p.name, &p.path), p.path.clone()),
        Some(p) if !p.name.is_empty() => (p.name.to_lowercase(), p.name.clone(), String::new()),
        _ if pid == 0 => ("(unknown)".into(), "Unknown program".into(), String::new()),
        _ => (format!("pid:{}", pid), format!("Program #{}", pid), String::new()),
    }
}

fn display_name(name: &str, path: &str) -> String {
    let base = path.rsplit(['/', '\\']).next().unwrap_or(name);
    let base = base.strip_suffix(".exe").or_else(|| base.strip_suffix(".EXE")).unwrap_or(base);
    // macOS app bundles: use the bundle name
    if let Some(i) = path.find(".app/") {
        let bundle = &path[..i];
        return bundle.rsplit('/').next().unwrap_or(base).to_string();
    }
    if base.is_empty() {
        name.to_string()
    } else {
        base.to_string()
    }
}

fn poll_connections(
    shared: &SharedRef,
    raw: Vec<RawConn>,
    tracks: &mut HashMap<(u32, SocketAddrV4, SocketAddrV4), Track>,
    procs: &mut HashMap<u32, sysnet::ProcInfo>,
) {
    let t = now();
    let day = day_key(t);
    let hour = t / 3600;
    // new processes since the last snapshot
    if raw.iter().any(|c| c.pid != 0 && !procs.contains_key(&c.pid)) {
        *procs = sysnet::processes();
    }
    let mut s = shared.lock().unwrap();
    // Try once whether per-connection byte counting is allowed (Windows administrator).
    if !s.traffic.per_app_checked {
        if let Some(c) = raw.iter().find(|c| !c.remote.ip().is_loopback()) {
            s.traffic.per_app_bytes = sysnet::enable_bytes(c);
            s.traffic.per_app_checked = true;
        }
    }
    let counting = s.traffic.per_app_bytes;
    let mut seen = HashSet::new();
    let mut conns = vec![];
    for c in raw {
        let rip = *c.remote.ip();
        if rip.is_loopback() || rip.is_unspecified() {
            continue;
        }
        let key = (c.pid, c.local, c.remote);
        seen.insert(key);
        let info = procs.get(&c.pid);
        let (akey, aname, apath) = app_key(info, c.pid);
        let is_new = !tracks.contains_key(&key);
        if is_new {
            let enabled = counting && sysnet::enable_bytes(&c);
            tracks.insert(key, Track { app: akey.clone(), since: t, rx: 0, tx: 0, counting: enabled });
            let parent = info.and_then(|p| p.parent).and_then(|pp| procs.get(&pp)).map(|p| p.name.clone());
            let a = s.traffic.saved.apps.entry(akey.clone()).or_insert_with(|| AppStat {
                key: akey.clone(),
                name: aname.clone(),
                path: apath.clone(),
                first_seen: t,
                ..Default::default()
            });
            a.last_seen = t;
            a.conns += 1;
            if a.parent.is_none() {
                a.parent = parent;
            }
            push_unique(&mut a.hours, hour, 24 * 14);
            push_unique(&mut a.dests, rip.to_string(), 500);
            a.daily.entry(day.clone()).or_default().conns += 1;
            while a.daily.len() > 60 {
                let first = a.daily.keys().next().cloned().unwrap_or_default();
                a.daily.remove(&first);
            }
            let h = s.traffic.saved.hosts.entry(rip.to_string()).or_insert_with(|| Host {
                ip: rip.to_string(),
                first_seen: t,
                category: crate::categories::classify(None, rip).to_string(),
                ..Default::default()
            });
            h.last_seen = t;
            h.conns += 1;
            push_unique(&mut h.ports, c.remote.port(), 20);
            push_unique(&mut h.apps, aname.clone(), 20);
            h.starts.push(t);
            if h.starts.len() > 40 {
                h.starts.remove(0);
            }
            // Many home devices contacted on remote-control / file-sharing ports?
            if sysnet::is_local(rip) && [22, 135, 139, 445, 3389, 5985, 5986].contains(&c.remote.port()) {
                s.traffic.lan_admin.push((t, rip, c.remote.port()));
            }
        }
        // bytes
        let tr = tracks.get_mut(&key).unwrap();
        let (mut din, mut dout) = (0u64, 0u64);
        if tr.counting {
            if let Some((bin, bout)) = sysnet::conn_bytes(&c) {
                din = bin.saturating_sub(tr.rx);
                dout = bout.saturating_sub(tr.tx);
                tr.rx = bin;
                tr.tx = bout;
            }
        }
        if din + dout > 0 {
            if let Some(a) = s.traffic.saved.apps.get_mut(&tr.app) {
                a.rx += din;
                a.tx += dout;
                let d = a.daily.entry(day.clone()).or_default();
                d.rx += din;
                d.tx += dout;
            }
            if let Some(h) = s.traffic.saved.hosts.get_mut(&rip.to_string()) {
                h.rx += din;
                h.tx += dout;
                h.last_seen = t;
            }
        }
        if let Some(a) = s.traffic.saved.apps.get_mut(&tr.app) {
            a.last_seen = t;
        }
        conns.push(Conn { app: tr.app.clone(), pid: c.pid, remote: rip, rport: c.remote.port(), lport: c.local.port(), since: tr.since, rx: tr.rx, tx: tr.tx });
    }
    tracks.retain(|k, _| seen.contains(k));
    s.traffic.conns = conns;
    s.traffic.dirty = true;

    // keep the destination table a sensible size
    if s.traffic.saved.hosts.len() > 4000 {
        let mut v: Vec<(String, i64)> = s.traffic.saved.hosts.iter().map(|(k, h)| (k.clone(), h.last_seen)).collect();
        v.sort_by_key(|x| x.1);
        for (k, _) in v.iter().take(500) {
            s.traffic.saved.hosts.remove(k);
        }
    }

    // lateral movement: this computer reaching many home devices on admin ports
    s.traffic.lan_admin.retain(|x| t - x.0 < 600);
    let distinct: HashSet<Ipv4Addr> = s.traffic.lan_admin.iter().map(|x| x.1).collect();
    if distinct.len() >= 6 && t - s.traffic.lateral_alerted > 3600 {
        s.traffic.lateral_alerted = t;
        let n = distinct.len();
        s.add_alert(
            "lateral",
            Severity::Warning,
            None,
            format!("This computer contacted {} home devices on remote-control or file-sharing ports in 10 minutes", n),
        );
    }
}

/// Programs that contact the same place at very regular intervals.
pub fn beacon_score(starts: &[i64]) -> Option<f64> {
    if starts.len() < 6 {
        return None;
    }
    let iv: Vec<f64> = starts.windows(2).map(|w| (w[1] - w[0]) as f64).filter(|d| *d > 0.0).collect();
    if iv.len() < 5 {
        return None;
    }
    let mean = iv.iter().sum::<f64>() / iv.len() as f64;
    if !(20.0..=7200.0).contains(&mean) {
        return None;
    }
    let var = iv.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / iv.len() as f64;
    let cv = var.sqrt() / mean;
    (cv < 0.12).then_some(mean)
}

fn detect_beacons(shared: &SharedRef) {
    let mut s = shared.lock().unwrap();
    let t = now();
    let mut found = vec![];
    for h in s.traffic.saved.hosts.values() {
        let recent: Vec<i64> = h.starts.iter().cloned().filter(|x| t - x < 6 * 3600).collect();
        if let Some(every) = beacon_score(&recent) {
            if !s.traffic.saved.beacons_alerted.contains(&h.ip) {
                found.push((h.ip.clone(), h.name.clone().unwrap_or_else(|| h.ip.clone()), h.apps.join(", "), every));
            }
        }
    }
    for (ip, name, apps, every) in found {
        s.traffic.saved.beacons_alerted.push(ip);
        let apps = if apps.is_empty() { "A program".to_string() } else { apps };
        s.add_alert(
            "beacon",
            Severity::Info,
            None,
            format!("{} checks in with {} like clockwork (every {})", apps, name, crate::widgets::dur(every as i64)),
        );
    }
}

/// Run a speed test against Cloudflare's public speed test service.
///
/// `s` is the shared state the caller (the window) already holds locked; the
/// test itself runs on its own thread and only locks briefly to post progress.
/// (Locking `shared` here again would freeze the app, since the window holds it.)
pub fn start_speed_test(s: &mut Shared, shared: SharedRef, ctx: eframe::egui::Context) {
    if s.traffic.speed.running {
        return;
    }
    s.traffic.speed = SpeedRun { running: true, phase: "Measuring response time…".into(), ..Default::default() };
    std::thread::spawn(move || {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        let set = |f: &dyn Fn(&mut SpeedRun)| {
            let mut s = shared.lock().unwrap();
            f(&mut s.traffic.speed);
            ctx.request_repaint();
        };
        // response time
        let mut times = vec![];
        for _ in 0..6 {
            let t0 = Instant::now();
            if crate::web::get("https://speed.cloudflare.com/__down?bytes=0", 8000).is_ok() {
                times.push(t0.elapsed().as_secs_f32() * 1000.0);
            }
        }
        if times.is_empty() {
            set(&|r| {
                r.running = false;
                r.error = Some("Couldn't reach the speed test server. Is the internet working?".into());
            });
            return;
        }
        times.sort_by(|a, b| a.total_cmp(b));
        let ping = times[times.len() / 2];
        set(&|r| {
            r.ping_ms = Some(ping);
            r.phase = "Testing download…".into();
        });

        // download: 4 parallel streams for ~10 seconds
        let bytes = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let start = Instant::now();
        let mut hs = vec![];
        for _ in 0..4 {
            let bytes = bytes.clone();
            let stop = stop.clone();
            hs.push(std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let mut last = 0u64;
                    let mut cb = |n: u64| {
                        bytes.fetch_add(n - last, Ordering::Relaxed);
                        last = n;
                        !stop.load(Ordering::Relaxed)
                    };
                    let r = crate::web::request(
                        "https://speed.cloudflare.com/__down?bytes=25000000",
                        crate::web::Opts { method: "GET", timeout_ms: 15000, on_bytes: Some(&mut cb), discard: true, ..Default::default() },
                    );
                    if r.is_err() {
                        break;
                    }
                }
            }));
        }
        while start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(250));
            let mbps = bytes.load(Ordering::Relaxed) as f32 * 8.0 / start.elapsed().as_secs_f32() / 1e6;
            let p = start.elapsed().as_secs_f32() / 10.0 * 0.5;
            set(&|r| {
                r.live_mbps = mbps;
                r.progress = p;
            });
        }
        stop.store(true, Ordering::Relaxed);
        let down = bytes.load(Ordering::Relaxed) as f32 * 8.0 / start.elapsed().as_secs_f32() / 1e6;
        for h in hs {
            let _ = h.join();
        }
        set(&|r| {
            r.down = Some(down);
            r.phase = "Testing upload…".into();
            r.live_mbps = 0.0;
        });

        // upload: 3 parallel streams for ~8 seconds
        let sent = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        let mut hs = vec![];
        for _ in 0..3 {
            let sent = sent.clone();
            hs.push(std::thread::spawn(move || {
                let body = vec![b'0'; 4_000_000];
                while start.elapsed() < Duration::from_secs(8) {
                    let r = crate::web::request(
                        "https://speed.cloudflare.com/__up",
                        crate::web::Opts { method: "POST", body: Some(&body), content_type: Some("application/octet-stream"), timeout_ms: 15000, ..Default::default() },
                    );
                    if r.is_err() {
                        break;
                    }
                    sent.fetch_add(body.len() as u64, Ordering::Relaxed);
                }
            }));
        }
        while hs.iter().any(|h| !h.is_finished()) {
            std::thread::sleep(Duration::from_millis(250));
            let mbps = sent.load(Ordering::Relaxed) as f32 * 8.0 / start.elapsed().as_secs_f32() / 1e6;
            let p = 0.5 + (start.elapsed().as_secs_f32() / 8.0).min(1.0) * 0.5;
            set(&|r| {
                r.live_mbps = mbps;
                r.progress = p;
            });
        }
        let up = sent.load(Ordering::Relaxed) as f32 * 8.0 / start.elapsed().as_secs_f32() / 1e6;
        let mut s = shared.lock().unwrap();
        s.traffic.saved.speed.push(SpeedResult { t: now(), down_mbps: down, up_mbps: up, ping_ms: ping });
        while s.traffic.saved.speed.len() > 200 {
            s.traffic.saved.speed.remove(0);
        }
        s.traffic.dirty = true;
        s.traffic.speed.running = false;
        s.traffic.speed.up = Some(up);
        s.traffic.speed.progress = 1.0;
        s.traffic.speed.phase = "Done".into();
        drop(s);
        ctx.request_repaint();
    });
}

pub fn bytes(b: u64) -> String {
    crate::perf::bytes(b)
}
