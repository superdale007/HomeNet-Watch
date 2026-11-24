//! Background work: checking devices on a schedule, and finding new ones.

use crate::model::*;
use crate::net;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub type SharedRef = Arc<Mutex<Shared>>;

/// Ports tried during discovery (to guess what a device is). 5000/5001 are
/// Synology's admin pages, 8080 QNAP's, 9999 UGREEN's (estimated).
pub const PROBE_PORTS: [u16; 19] = [80, 443, 22, 445, 139, 3389, 554, 631, 9100, 8008, 8009, 62078, 5000, 5001, 8080, 9999, 1883, 8123, 32400];

struct Job {
    id: u64,
    ip: Ipv4Addr,
    ping: bool,
    ports: Vec<u16>,
    arp: bool,
    web: Option<WebCheck>,
    dns: String,
}

struct JobResult {
    id: u64,
    ping: Option<f32>,
    ping_tried: bool,
    ports: Vec<(u16, Option<f32>)>,
    arp: Option<f32>,
    web: Option<Result<(u16, f32), String>>,
    dns: Option<Result<(f32, Option<Ipv4Addr>), String>>,
}

pub fn start_monitor(shared: SharedRef, ctx: eframe::egui::Context) {
    // The home's outside address: check now, then every 30 minutes (sooner while unknown).
    {
        let shared = shared.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || loop {
            let found = crate::web::external_ip().ok();
            let known = found.is_some();
            if let Some(ip) = found {
                let mut s = shared.lock().unwrap();
                if s.external_ip.as_deref() != Some(ip.as_str()) {
                    s.external_ip = Some(ip);
                    ctx.request_repaint();
                }
            }
            std::thread::sleep(Duration::from_secs(if known { 1800 } else { 120 }));
        });
    }
    std::thread::spawn(move || {
        let mut last_save = Instant::now();
        let mut last_minute = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(400));
            let (jobs, timeout, want_rescan) = {
                let mut s = shared.lock().unwrap();
                let t = now();
                let interval = s.settings.interval_secs.max(5) as i64;
                let force = std::mem::take(&mut s.check_now);
                let monitoring = s.settings.monitoring;
                let lan = s.local_ip;
                let mut jobs = vec![];
                for d in s.devices.iter_mut() {
                    if d.paused || !monitoring {
                        d.status = Status::Paused;
                        continue;
                    }
                    if d.status == Status::Paused {
                        d.status = Status::Unknown;
                    }
                    if d.busy {
                        continue;
                    }
                    let due = force || d.last_check.map(|lc| t - lc >= interval).unwrap_or(true);
                    if due {
                        d.busy = true;
                        jobs.push(Job {
                            id: d.id,
                            ip: d.ip,
                            ping: d.ping || (d.ports.is_empty() && !d.arp),
                            ports: d.ports.iter().map(|p| p.port).collect(),
                            arp: d.arp && lan.map(|l| l.octets()[..3] == d.ip.octets()[..3]).unwrap_or(false),
                            web: d.web.clone(),
                            dns: d.dns_name.clone(),
                        });
                    }
                }
                // System stats (SNMP) on their own, slower schedule.
                let perf_every = s.settings.perf_interval_secs.max(15) as i64;
                for d in s.devices.iter_mut() {
                    if !monitoring || d.paused || !d.snmp || d.perf_busy {
                        continue;
                    }
                    if force || d.perf_last.map(|lp| t - lp >= perf_every).unwrap_or(true) {
                        d.perf_busy = true;
                        d.perf_last = Some(t);
                        let (id, ip, security, counters) = (d.id, d.ip, d.snmp_security(), d.counters.clone());
                        let sh = shared.clone();
                        let cx = ctx.clone();
                        std::thread::spawn(move || {
                            let r = crate::perf::collect(ip, &security, &counters);
                            let mut s = sh.lock().unwrap();
                            apply_perf(&mut s, id, r);
                            drop(s);
                            cx.request_repaint();
                        });
                    }
                }
                let rescan = s.settings.rescan_hours > 0
                    && !s.scan.running
                    && monitoring
                    && s.scan.last_finished.map(|lf| t - lf > s.settings.rescan_hours as i64 * 3600).unwrap_or(false);
                (jobs, s.settings.timeout_ms, rescan)
            };

            if want_rescan {
                start_scan(shared.clone(), ctx.clone());
            }

            if !jobs.is_empty() {
                let results = run_jobs(jobs, timeout);
                let mut s = shared.lock().unwrap();
                for r in results {
                    apply_result(&mut s, r);
                }
                ctx.request_repaint();
            }

            if last_minute.elapsed() > Duration::from_secs(60) {
                last_minute = Instant::now();
                let gw_now = net::default_gateway();
                let mut s = shared.lock().unwrap();
                // Router address changed?
                if let Some(g) = gw_now {
                    if s.gateway != Some(g) {
                        s.gateway = Some(g);
                    }
                    match s.last_gateway {
                        Some(old) if old != g => {
                            s.add_alert("gateway-change", Severity::Warning, None, format!("Your router's address changed from {} to {}", old, g));
                            s.last_gateway = Some(g);
                            s.gateway_mac = None;
                        }
                        None => s.last_gateway = Some(g),
                        _ => {}
                    }
                }
                // Devices not seen for a while
                let hours = s.settings.departed_hours as i64;
                if hours > 0 {
                    let t = now();
                    let mut gone = vec![];
                    for d in s.devices.iter_mut() {
                        if d.kind == DeviceKind::Internet || d.departed || d.paused {
                            continue;
                        }
                        if let Some(ls) = d.last_seen {
                            if t - ls > hours * 3600 && d.ok_checks >= 3 {
                                d.departed = true;
                                gone.push(d.clone());
                            }
                        }
                    }
                    for d in gone {
                        s.add_alert("departed", Severity::Info, Some(&d), format!("Hasn't been seen for {} hours", hours));
                    }
                }
            }

            if last_save.elapsed() > Duration::from_secs(15) {
                let saved = {
                    let mut s = shared.lock().unwrap();
                    if s.persist && s.dirty {
                        s.dirty = false;
                        Some(s.to_saved())
                    } else {
                        None
                    }
                };
                if let Some(saved) = saved {
                    let _ = save(&saved);
                }
                last_save = Instant::now();
            }
        }
    });
}

fn run_jobs(jobs: Vec<Job>, timeout: u32) -> Vec<JobResult> {
    let out = Mutex::new(Vec::with_capacity(jobs.len()));
    let next = AtomicUsize::new(0);
    let workers = jobs.len().min(48);
    std::thread::scope(|sc| {
        for _ in 0..workers {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= jobs.len() {
                    break;
                }
                let j = &jobs[i];
                let mut ping = None;
                if j.ping {
                    ping = net::ping(j.ip, timeout);
                    if ping.is_none() {
                        ping = net::ping(j.ip, timeout);
                    }
                }
                let ports = j.ports.iter().map(|p| (*p, net::tcp_open(j.ip, *p, timeout))).collect();
                // Presence check: ask the device for its hardware address on the home network.
                let arp = if j.arp {
                    let t0 = Instant::now();
                    net::arp_mac(j.ip).map(|_| t0.elapsed().as_secs_f32() * 1000.0)
                } else {
                    None
                };
                let web = j.web.as_ref().map(|w| crate::checks::http_check(j.ip, w.port, &w.path, &w.expect, timeout));
                let dns = if j.dns.trim().is_empty() { None } else { Some(crate::checks::dns_check(j.ip, &j.dns, timeout)) };
                out.lock().unwrap().push(JobResult { id: j.id, ping, ping_tried: j.ping, ports, arp, web, dns });
            });
        }
    });
    out.into_inner().unwrap()
}

fn human_dur(secs: i64) -> String {
    if secs < 90 {
        format!("{} s", secs)
    } else if secs < 5400 {
        format!("{} min", secs / 60)
    } else if secs < 172800 {
        format!("{:.1} h", secs as f32 / 3600.0)
    } else {
        format!("{} days", secs / 86400)
    }
}

fn apply_result(s: &mut Shared, r: JobResult) {
    let settings = s.settings.clone();
    let t = now();
    let mut events: Vec<(Severity, String)> = vec![];
    let Some(d) = s.device_mut(r.id) else { return };
    d.busy = false;
    if d.paused {
        return;
    }
    d.last_check = Some(t);
    d.checks += 1;

    let mut port_fail = false;
    let mut best_port: Option<f32> = None;
    for (port, res) in &r.ports {
        let open = res.is_some();
        if let Some(ms) = res {
            best_port = Some(best_port.map_or(*ms, |b: f32| b.min(*ms)));
        } else {
            port_fail = true;
        }
        if let Some(pc) = d.ports.iter_mut().find(|p| p.port == *port) {
            if pc.open == Some(true) && !open && d.alerts {
                events.push((Severity::Warning, format!("{} (port {}) stopped answering", port_name(*port), port)));
            }
            if pc.open == Some(false) && open && d.alerts {
                events.push((Severity::Info, format!("{} (port {}) is answering again", port_name(*port), port)));
            }
            pc.open = Some(open);
        }
    }
    // Web page and DNS checks
    let mut extra_ok = false;
    for (label, res, state) in [
        (
            "Web page",
            r.web.as_ref().map(|w| w.as_ref().map(|(code, ms)| format!("loads (status {}, {:.0} ms)", code, ms)).map_err(|e| e.clone())),
            &mut d.web_state,
        ),
        (
            "DNS lookup",
            r.dns.as_ref().map(|x| {
                x.as_ref()
                    .map(|(ms, ip)| match ip {
                        Some(ip) => format!("works ({:.0} ms, answered {})", ms, ip),
                        None => format!("works ({:.0} ms)", ms),
                    })
                    .map_err(|e| e.clone())
            }),
            &mut d.dns_state,
        ),
    ] {
        let Some(res) = res else {
            *state = None;
            continue;
        };
        let was_ok = state.as_ref().map(|s| s.is_ok());
        match &res {
            Ok(_) => {
                extra_ok = true;
                if was_ok == Some(false) && d.alerts {
                    events.push((Severity::Info, format!("{} check is working again", label)));
                }
            }
            Err(e) => {
                port_fail = true;
                if was_ok != Some(false) && d.alerts {
                    events.push((Severity::Warning, format!("{} check failed: {}", label, e)));
                }
            }
        }
        *state = Some(res);
    }
    let reachable = r.ping.is_some() || best_port.is_some() || r.arp.is_some() || extra_ok;
    let latency = r.ping.or(best_port).or(r.arp);
    d.latency = latency;
    d.push_sample(Sample { t, ms: latency });

    let prev = d.status;
    // "Slow" means slow for a while: the last 3 answers all above the limit.
    // (Wi-Fi gadgets that doze often take a few hundred ms to answer once.)
    let recent: Vec<f32> = d.history.iter().rev().take(3).filter_map(|s| s.ms).collect();
    // A presence-only answer has no meaningful response time, so it never counts as slow.
    let slow = r.ping.or(best_port).is_some() && recent.len() == 3 && recent.iter().all(|l| *l > settings.slow_ms as f32);
    let new_status = if reachable {
        d.ok_checks += 1;
        d.last_seen = Some(t);
        d.departed = false;
        d.fails_in_row = 0;
        if slow || port_fail || (r.ping_tried && r.ping.is_none()) || !d.problems.is_empty() {
            Status::Warning
        } else {
            Status::Up
        }
    } else {
        d.fails_in_row += 1;
        if d.fails_in_row >= settings.fails_before_alert.max(1) {
            Status::Down
        } else if prev == Status::Unknown {
            Status::Unknown
        } else {
            Status::Warning
        }
    };

    if new_status == Status::Down && prev != Status::Down {
        d.outages += 1;
        d.down_since = Some(t);
        if d.alerts {
            events.push((Severity::Critical, "Stopped responding".to_string()));
        }
    }
    if prev == Status::Down && new_status != Status::Down {
        let since = d.down_since.take().unwrap_or(t);
        d.downtime_secs += t - since;
        if d.alerts {
            events.push((Severity::Info, format!("Back online after {}", human_dur(t - since))));
        }
    }
    if reachable {
        if slow && !d.slow && d.alerts {
            events.push((
                Severity::Warning,
                format!("Responding slowly ({:.0} ms, limit {} ms)", latency.unwrap_or(0.0), settings.slow_ms),
            ));
        }
        d.slow = slow;
    }
    d.status = new_status;
    let snapshot = d.clone();
    for (sev, msg) in events {
        s.add_event(sev, Some(&snapshot), msg);
    }
    s.dirty = true;
}

/// Store new system stats and raise / clear threshold alerts.
fn apply_perf(s: &mut Shared, id: u64, r: Result<(crate::perf::Perf, crate::perf::Counters), String>) {
    let st = s.settings.clone();
    let mut events: Vec<(Severity, String)> = vec![];
    let Some(d) = s.device_mut(id) else { return };
    d.perf_busy = false;
    match r {
        Err(e) => {
            if d.perf_error.is_none() && d.perf.is_some() && d.alerts {
                events.push((Severity::Warning, format!("Stopped sharing system stats: {}", e)));
            }
            d.perf_error = Some(e);
        }
        Ok((p, counters)) => {
            d.perf_error = None;
            d.counters = counters;
            let rx: Option<f64> = p.ifaces.iter().filter_map(|i| i.rx_bps).reduce(|a, b| a + b);
            let tx: Option<f64> = p.ifaces.iter().filter_map(|i| i.tx_bps).reduce(|a, b| a + b);
            d.perf_history.push_back(crate::perf::PerfSample { t: p.t, cpu: p.cpu_pct, mem: p.mem_pct, rx, tx });
            while d.perf_history.len() > 720 {
                d.perf_history.pop_front();
            }
            // What's wrong right now?
            let mut now_problems: Vec<(String, Severity, String)> = vec![];
            if p.system_ok == Some(false) {
                now_problems.push(("sys".into(), Severity::Critical, "The device reports a system problem".into()));
            }
            for v in &p.volumes {
                // "Volume 1" reads fine on its own; plain paths like "/share/DATA" get a word in front
                let label = if v.name.to_lowercase().contains("volume") || v.is_pool { v.name.clone() } else { format!("Storage {}", v.name) };
                if !v.healthy {
                    now_problems.push((
                        format!("vol-health:{}", v.name),
                        Severity::Critical,
                        format!("{} is {}", label, v.status.clone().unwrap_or_else(|| "not healthy".into()).to_lowercase()),
                    ));
                }
                // A storage pool being fully handed out to its volumes is normal: only volumes get the "full" warning.
                if !v.is_pool && v.total > 0 && v.pct() >= st.disk_warn as f32 {
                    now_problems.push((format!("vol-full:{}", v.name), Severity::Warning, format!("{} is {:.0}% full", label, v.pct())));
                }
            }
            for k in &p.disks {
                if !k.healthy {
                    now_problems.push((format!("disk:{}", k.name), Severity::Critical, format!("{} health: {}", k.name, k.status)));
                }
                if let Some(t) = k.temp_c {
                    if t >= st.temp_warn as f32 {
                        now_problems.push((format!("disk-hot:{}", k.name), Severity::Warning, format!("{} is hot ({:.0}°C)", k.name, t)));
                    }
                }
            }
            if let Some(t) = p.temp_c {
                if t >= st.sys_temp_warn as f32 {
                    now_problems.push(("hot".into(), Severity::Warning, format!("Running hot ({:.0}°C)", t)));
                }
            }
            let busy = d.perf_history.len() >= 3 && d.perf_history.iter().rev().take(3).all(|x| x.cpu.map(|c| c >= st.cpu_warn as f32).unwrap_or(false));
            if busy {
                now_problems.push(("cpu".into(), Severity::Warning, format!("Processor busy ({:.0}%)", p.cpu_pct.unwrap_or(0.0))));
            }
            if d.alerts {
                for (k, sev, msg) in &now_problems {
                    if !d.problems.iter().any(|(pk, _, _)| pk == k) {
                        events.push((*sev, msg.clone()));
                    }
                }
                for (k, _, msg) in &d.problems {
                    if !now_problems.iter().any(|(nk, _, _)| nk == k) {
                        events.push((Severity::Info, format!("Back to normal: {}", msg.to_lowercase())));
                    }
                }
            }
            if !now_problems.is_empty() && d.status == Status::Up {
                d.status = Status::Warning;
            }
            d.problems = now_problems;
            if d.maker.is_empty() && !p.vendor.label().is_empty() {
                d.maker = p.vendor.label().to_string();
            }
            d.perf = Some(p);
        }
    }
    let snap = d.clone();
    for (sev, msg) in events {
        s.add_event(sev, Some(&snap), msg);
    }
}

struct Found {
    ip: Ipv4Addr,
    mac: Option<String>,
    name: Option<String>,
    ms: Option<f32>,
    open: Vec<u16>,
    /// What the device says about itself over SNMP ("public"), if it answers
    snmp: Option<String>,
}

/// Start looking for devices in the configured range (does nothing if a scan is running).
pub fn start_scan(shared: SharedRef, ctx: eframe::egui::Context) {
    let (ips, local, gateway) = {
        let mut s = shared.lock().unwrap();
        if s.scan.running {
            return;
        }
        let ips = match net::parse_range(&s.settings.scan_range) {
            Ok(v) => v,
            Err(e) => {
                s.scan.message = format!("Can't scan: {}", e);
                return;
            }
        };
        s.scan = ScanState {
            running: true,
            done: 0,
            total: ips.len(),
            found: 0,
            new_found: 0,
            last_finished: s.scan.last_finished,
            message: "Looking for devices…".into(),
        };
        (ips, s.local_ip, s.gateway)
    };
    std::thread::spawn(move || {
        let found = Mutex::new(Vec::<Found>::new());
        let next = AtomicUsize::new(0);
        let same_lan = |ip: Ipv4Addr| match local {
            Some(l) => l.octets()[..3] == ip.octets()[..3],
            None => true,
        };
        std::thread::scope(|sc| {
            for _ in 0..64 {
                sc.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= ips.len() {
                        break;
                    }
                    let ip = ips[i];
                    let mut ms = net::ping(ip, 600);
                    if ms.is_none() {
                        ms = net::ping(ip, 600);
                    }
                    let mac = if same_lan(ip) && Some(ip) != local { net::arp_mac(ip) } else { None };
                    let alive = ms.is_some() || mac.is_some() || Some(ip) == local;
                    if alive {
                        let open: Vec<u16> = std::thread::scope(|ps| {
                            let hs: Vec<_> = PROBE_PORTS
                                .iter()
                                .map(|p| ps.spawn(move || net::tcp_open(ip, *p, 350).map(|_| *p)))
                                .collect();
                            hs.into_iter().filter_map(|h| h.join().ok().flatten()).collect()
                        });
                        let name = net::reverse_name(ip);
                        let snmp = crate::perf::probe(ip, "public", 400);
                        found.lock().unwrap().push(Found { ip, mac, name, ms, open, snmp });
                    }
                    let mut s = shared.lock().unwrap();
                    s.scan.done += 1;
                    if alive {
                        s.scan.found += 1;
                    }
                    drop(s);
                    ctx.request_repaint();
                });
            }
        });

        let found = found.into_inner().unwrap();
        let mut s = shared.lock().unwrap();
        let t = now();
        let mut new_count = 0;
        let first_scan = s.scan.last_finished.is_none();
        let gw_mac = gateway.and_then(|g| found.iter().find(|f| f.ip == g)).and_then(|f| f.mac.clone());
        for f in &found {
            let maker = f.mac.as_deref().map(net::maker_from_mac).unwrap_or_default();
            // Recognise known devices even when an address changed:
            // 1) same hardware address (unless it's a randomised "private" one)
            // 2) same name the network gave it when first found
            // 3) same network address
            let by_mac = s.devices.iter().position(|d| match (&d.mac, &f.mac) {
                (Some(a), Some(b)) => a == b && maker != "Private address",
                _ => false,
            });
            let by_name = || {
                f.name.as_ref().and_then(|n| {
                    s.devices.iter().position(|d| d.auto_name.as_deref().map(|a| a.eq_ignore_ascii_case(n)).unwrap_or(false) || (d.auto_name.is_none() && d.name.eq_ignore_ascii_case(n)))
                })
            };
            let existing = by_mac.or_else(by_name).or_else(|| s.devices.iter().position(|d| d.ip == f.ip));
            if let Some(i) = existing {
                let mut events: Vec<(&str, Severity, String)> = vec![];
                let d = &mut s.devices[i];
                if d.ip != f.ip || (f.mac.is_some() && d.mac.is_some() && d.mac != f.mac) {
                    let what = if d.ip != f.ip { format!("address ({} to {})", d.ip, f.ip) } else { "hardware address".to_string() };
                    events.push(("address-changed", Severity::Info, format!("Recognised under a new {}", what)));
                    d.ip = f.ip;
                }
                if f.mac.is_some() {
                    d.mac = f.mac.clone();
                }
                if d.maker.is_empty() {
                    d.maker = maker;
                }
                if d.auto_name.is_none() {
                    d.auto_name = f.name.clone();
                }
                d.last_seen = Some(t);
                d.departed = false;
                // Services newly offered since the last scan
                if !d.open_ports_seen.is_empty() || d.checks > 0 {
                    for p in &f.open {
                        if !d.open_ports_seen.contains(p) {
                            events.push(("new-open-port", Severity::Warning, format!("Port {} ({}) is newly open", p, port_name(*p))));
                        }
                    }
                }
                d.open_ports_seen = f.open.clone();
                let snap = d.clone();
                for (k, sev, m) in events {
                    s.add_alert(k, sev, Some(&snap), m);
                }
                continue;
            }
            let id = s.take_id();
            let is_self = Some(f.ip) == local;
            let name = f.name.clone().unwrap_or_else(|| {
                if is_self {
                    "This computer".into()
                } else if Some(f.ip) == gateway {
                    "Home router".into()
                } else if !maker.is_empty() && maker != "Private address" {
                    format!("{} device .{}", maker, f.ip.octets()[3])
                } else {
                    format!("Device .{}", f.ip.octets()[3])
                }
            });
            let about = format!("{} {}", name, f.snmp.clone().unwrap_or_default());
            let mut kind = guess_kind(f.ip, gateway, &f.open, &maker, &about);
            if is_self && kind == DeviceKind::Unknown {
                kind = DeviceKind::Computer;
            }
            let mut d = Device::new(id, name, f.ip, kind);
            d.mac = f.mac.clone();
            d.maker = maker;
            d.last_seen = Some(t);
            d.auto_name = f.name.clone();
            d.open_ports_seen = f.open.clone();
            d.latency = f.ms;
            // It already shares system stats: read them.
            if f.snmp.is_some() {
                d.snmp = true;
            }
            // Devices that ignore ping: watch an answering port, or else check presence on the home network.
            if f.ms.is_none() {
                if let Some(p) = d.open_ports_seen.first() {
                    d.ping = false;
                    d.ports.push(PortCheck { port: *p, open: None });
                } else if d.mac.is_some() {
                    d.ping = false;
                    d.arp = true;
                }
            }
            let alert_new = s.settings.alert_new_devices && s.scan.last_finished.is_some();
            let snap = d.clone();
            s.devices.push(d);
            new_count += 1;
            if alert_new {
                s.add_event(
                    Severity::Warning,
                    Some(&snap),
                    format!("New device joined the network ({}, {})", snap.ip, snap.kind.label()),
                );
            }
        }
        // Router impersonation: another address answering with the router's hardware address,
        // or the router's hardware address changing.
        if let (Some(g), Some(mac)) = (gateway, gw_mac.clone()) {
            let twins: Vec<Ipv4Addr> = found.iter().filter(|f| f.ip != g && f.mac.as_deref() == Some(mac.as_str())).map(|f| f.ip).collect();
            if !twins.is_empty() {
                let list = twins.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
                s.add_alert("arp-spoof", Severity::Critical, None, format!("{} answered with the router's hardware address ({})", list, mac));
            }
            match s.gateway_mac.clone() {
                Some(old) if old != mac && !first_scan => {
                    s.add_alert("arp-spoof", Severity::Warning, None, format!("The router's hardware address changed from {} to {}", old, mac));
                    s.gateway_mac = Some(mac);
                }
                None => s.gateway_mac = Some(mac),
                _ => {}
            }
        }
        s.scan.running = false;
        s.scan.new_found = new_count;
        s.scan.last_finished = Some(t);
        s.scan.message = format!(
            "Found {} devices ({} new) at {}",
            s.scan.found,
            new_count,
            chrono::Local::now().format("%H:%M")
        );
        let msg = s.scan.message.clone();
        s.add_event(Severity::Info, None, format!("Network scan finished: {}", msg.to_lowercase()));
        s.check_now = true;
        s.dirty = true;
        drop(s);
        ctx.request_repaint();
    });
}
