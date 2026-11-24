//! Performance monitoring over SNMP: CPU, memory, temperature, storage
//! volumes, disk health and network traffic. Works with any device that
//! speaks standard SNMP; adds extra detail for Synology and QNAP NAS boxes.
//!
//! Standard MIBs used: SNMPv2-MIB (system), HOST-RESOURCES-MIB (CPU, storage),
//! UCD-SNMP-MIB (Linux memory/CPU), IF-MIB (traffic).
//! Vendor MIBs: Synology (1.3.6.1.4.1.6574), QNAP (1.3.6.1.4.1.24681).
//! UGREEN NAS boxes run Linux with standard SNMP, so they use the standard set.

use crate::snmp::{oid, Client, Value};
use std::collections::HashMap;
use std::net::Ipv4Addr;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Vendor {
    Synology,
    Qnap,
    Ugreen,
    #[default]
    Generic,
}

impl Vendor {
    pub fn label(&self) -> &'static str {
        match self {
            Vendor::Synology => "Synology",
            Vendor::Qnap => "QNAP",
            Vendor::Ugreen => "UGREEN",
            Vendor::Generic => "",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Volume {
    pub name: String,
    pub total: u64,
    pub used: u64,
    /// e.g. "Normal", "Degraded", "Crashed"
    pub status: Option<String>,
    pub healthy: bool,
    /// A storage pool (space that volumes are carved out of). Being fully
    /// assigned to volumes is normal, so it never gets a "full" warning.
    pub is_pool: bool,
}

impl Volume {
    pub fn pct(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.used as f32 * 100.0 / self.total as f32
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Disk {
    pub name: String,
    pub model: String,
    pub temp_c: Option<f32>,
    pub status: String,
    pub healthy: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Iface {
    pub name: String,
    pub rx_bps: Option<f64>,
    pub tx_bps: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct Perf {
    pub t: i64,
    pub vendor: Vendor,
    pub descr: String,
    pub sys_name: String,
    pub model: String,
    pub uptime_secs: Option<u64>,
    pub cpu_pct: Option<f32>,
    pub mem_pct: Option<f32>,
    pub mem_total: Option<u64>,
    pub temp_c: Option<f32>,
    pub system_ok: Option<bool>,
    pub volumes: Vec<Volume>,
    pub disks: Vec<Disk>,
    pub ifaces: Vec<Iface>,
}

#[derive(Clone, Copy, Debug)]
pub struct PerfSample {
    pub t: i64,
    pub cpu: Option<f32>,
    pub mem: Option<f32>,
    pub rx: Option<f64>,
    pub tx: Option<f64>,
}

/// Interface counters from the previous poll: index -> (rx bytes, tx bytes, time ms)
pub type Counters = HashMap<u32, (u64, u64, i64)>;

/// Quick check used during discovery: does the device answer SNMP with this community?
pub fn probe(ip: Ipv4Addr, community: &str, timeout_ms: u64) -> Option<String> {
    let mut c = Client::new(ip, crate::snmp::Security::V2c(community.to_string()), timeout_ms).ok()?;
    c.get_one("1.3.6.1.2.1.1.1.0").and_then(|v| v.as_string())
}

pub fn collect(ip: Ipv4Addr, sec: &crate::snmp::Security, prev: &Counters) -> Result<(Perf, Counters), String> {
    let mut c = Client::new(ip, sec.clone(), 2000)?;
    let sys = c.get(&[
        oid("1.3.6.1.2.1.1.1.0"), // sysDescr
        oid("1.3.6.1.2.1.1.2.0"), // sysObjectID
        oid("1.3.6.1.2.1.1.3.0"), // sysUpTime
        oid("1.3.6.1.2.1.1.5.0"), // sysName
    ])?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut p = Perf {
        t: now_ms / 1000,
        descr: sys[0].as_string().unwrap_or_default(),
        sys_name: sys[3].as_string().unwrap_or_default(),
        uptime_secs: sys[2].as_u64().map(|ticks| ticks / 100),
        ..Default::default()
    };
    let obj = sys[1].as_string().unwrap_or_default();

    // Which kind of box is it?
    let syn_model = c.get_one("1.3.6.1.4.1.6574.1.5.1.0");
    let qnap_cpu = if syn_model.is_none() { c.get_one("1.3.6.1.4.1.24681.1.2.1.0") } else { None };
    let d = p.descr.to_lowercase();
    p.vendor = if syn_model.is_some() || obj.starts_with("1.3.6.1.4.1.6574") || d.contains("synology") {
        Vendor::Synology
    } else if qnap_cpu.is_some() || obj.starts_with("1.3.6.1.4.1.24681") || obj.starts_with("1.3.6.1.4.1.55062") || d.contains("qnap") {
        Vendor::Qnap
    } else if d.contains("ugreen") || d.contains("ugos") || p.sys_name.to_lowercase().contains("ugreen") || p.sys_name.to_lowercase().contains("dxp") {
        Vendor::Ugreen
    } else {
        Vendor::Generic
    };
    if let Some(m) = syn_model.and_then(|v| v.as_string()) {
        p.model = m;
    }

    // ── CPU
    let loads: Vec<f64> = c.column("1.3.6.1.2.1.25.3.3.1.2").iter().filter_map(|(_, v)| v.first_number()).collect();
    if !loads.is_empty() {
        p.cpu_pct = Some((loads.iter().sum::<f64>() / loads.len() as f64) as f32);
    } else if let Some(v) = qnap_cpu.as_ref().and_then(|v| v.first_number()) {
        p.cpu_pct = Some(v as f32);
    } else if let Some(idle) = c.get_one("1.3.6.1.4.1.2021.11.11.0").and_then(|v| v.first_number()) {
        p.cpu_pct = Some((100.0 - idle).clamp(0.0, 100.0) as f32);
    }

    // ── Memory (Linux view: used = total - available, so cache isn't counted)
    let ucd = c.get(&[
        oid("1.3.6.1.4.1.2021.4.5.0"),  // memTotalReal (KB)
        oid("1.3.6.1.4.1.2021.4.6.0"),  // memAvailReal
        oid("1.3.6.1.4.1.2021.4.14.0"), // memBuffer
        oid("1.3.6.1.4.1.2021.4.15.0"), // memCached
    ]);
    // ── Storage table (also a memory fallback)
    let descrs = c.column("1.3.6.1.2.1.25.2.3.1.3");
    let units = c.column("1.3.6.1.2.1.25.2.3.1.4");
    let sizes = c.column("1.3.6.1.2.1.25.2.3.1.5");
    let useds = c.column("1.3.6.1.2.1.25.2.3.1.6");
    let types = c.column("1.3.6.1.2.1.25.2.3.1.2");
    let lookup = |col: &Vec<(Vec<u32>, Value)>, idx: &Vec<u32>| col.iter().find(|(i, _)| i == idx).map(|(_, v)| v.clone());
    let mut storage: Vec<(String, u64, u64, String)> = vec![];
    for (idx, dv) in &descrs {
        let name = dv.as_string().unwrap_or_default();
        let unit = lookup(&units, idx).and_then(|v| v.as_u64()).unwrap_or(0);
        let size = lookup(&sizes, idx).and_then(|v| v.as_u64()).unwrap_or(0);
        let used = lookup(&useds, idx).and_then(|v| v.as_u64()).unwrap_or(0);
        let ty = lookup(&types, idx).and_then(|v| v.as_string()).unwrap_or_default();
        storage.push((name, size * unit, used * unit, ty));
    }
    if let (Ok(u), true) = (&ucd, ucd.as_ref().map(|u| u[0].present()).unwrap_or(false)) {
        let total = u[0].as_u64().unwrap_or(0);
        let avail = u[1].as_u64().unwrap_or(0);
        let buf = u[2].as_u64().unwrap_or(0);
        let cached = u[3].as_u64().unwrap_or(0);
        if total > 0 {
            let used = total.saturating_sub(avail).saturating_sub(buf).saturating_sub(cached);
            p.mem_pct = Some(used as f32 * 100.0 / total as f32);
            p.mem_total = Some(total * 1024);
        }
    }
    if p.mem_pct.is_none() {
        if let Some(ram) = storage.iter().find(|s| s.3.ends_with("25.2.1.2") || s.0.to_lowercase().contains("physical memory")) {
            if ram.1 > 0 {
                p.mem_pct = Some(ram.2 as f32 * 100.0 / ram.1 as f32);
                p.mem_total = Some(ram.1);
            }
        }
    }

    // ── Vendor extras
    match p.vendor {
        Vendor::Synology => {
            let g = c.get(&[oid("1.3.6.1.4.1.6574.1.1.0"), oid("1.3.6.1.4.1.6574.1.2.0")]).unwrap_or_default();
            p.system_ok = g.first().and_then(|v| v.as_i64()).map(|s| s == 1);
            p.temp_c = g.get(1).and_then(|v| v.first_number()).map(|v| v as f32);
            // disks
            let ids = c.column("1.3.6.1.4.1.6574.2.1.1.2");
            let models = c.column("1.3.6.1.4.1.6574.2.1.1.3");
            let stats = c.column("1.3.6.1.4.1.6574.2.1.1.5");
            let temps = c.column("1.3.6.1.4.1.6574.2.1.1.6");
            for (idx, v) in &ids {
                let st = lookup(&stats, idx).and_then(|v| v.as_i64()).unwrap_or(0);
                let (status, healthy) = match st {
                    1 => ("Normal", true),
                    2 => ("Initialized", true),
                    3 => ("Not initialized", true),
                    4 => ("System partition failed", false),
                    5 => ("Crashed", false),
                    _ => ("Unknown", true),
                };
                p.disks.push(Disk {
                    name: v.as_string().unwrap_or_default(),
                    model: lookup(&models, idx).and_then(|v| v.as_string()).unwrap_or_default(),
                    temp_c: lookup(&temps, idx).and_then(|v| v.first_number()).map(|v| v as f32),
                    status: status.into(),
                    healthy,
                });
            }
            // storage pools / RAID
            let names = c.column("1.3.6.1.4.1.6574.3.1.1.2");
            let rstat = c.column("1.3.6.1.4.1.6574.3.1.1.3");
            let free = c.column("1.3.6.1.4.1.6574.3.1.1.4");
            let total = c.column("1.3.6.1.4.1.6574.3.1.1.5");
            for (idx, v) in &names {
                let st = lookup(&rstat, idx).and_then(|v| v.as_i64()).unwrap_or(0);
                let (status, healthy) = match st {
                    1 => ("Normal", true),
                    2 => ("Repairing", false),
                    3..=10 => ("Working", true),
                    11 => ("Degraded", false),
                    12 => ("Crashed", false),
                    13..=21 => ("Working", true),
                    _ => ("Unknown", true),
                };
                let t = lookup(&total, idx).and_then(|v| v.as_u64()).unwrap_or(0);
                let f = lookup(&free, idx).and_then(|v| v.as_u64()).unwrap_or(0);
                let name = v.as_string().unwrap_or_default();
                // DSM lists storage pools ("Storage Pool 1") next to volumes; a pool's
                // "used" space is what's been handed to volumes, not your files.
                let lower = name.to_lowercase();
                let is_pool = lower.contains("pool") || lower.contains("cache");
                p.volumes.push(Volume { name, total: t, used: t.saturating_sub(f), status: Some(status.into()), healthy, is_pool });
            }
        }
        Vendor::Qnap => {
            p.temp_c = c.get_one("1.3.6.1.4.1.24681.1.2.6.0").and_then(|v| v.first_number()).map(|v| v as f32);
            let descr = c.column("1.3.6.1.4.1.24681.1.2.11.1.2");
            let temps = c.column("1.3.6.1.4.1.24681.1.2.11.1.3");
            let stats = c.column("1.3.6.1.4.1.24681.1.2.11.1.4");
            let models = c.column("1.3.6.1.4.1.24681.1.2.11.1.5");
            let smart = c.column("1.3.6.1.4.1.24681.1.2.11.1.7");
            for (idx, v) in &descr {
                let st = lookup(&stats, idx).and_then(|v| v.as_i64()).unwrap_or(0);
                if st == -5 {
                    continue; // empty bay
                }
                let sm = lookup(&smart, idx).and_then(|v| v.as_string()).unwrap_or_default();
                let healthy = st == 0 && (sm.is_empty() || sm.eq_ignore_ascii_case("good"));
                let status = if !sm.is_empty() { sm.clone() } else if st == 0 { "Ready".into() } else { format!("Problem ({})", st) };
                p.disks.push(Disk {
                    name: v.as_string().unwrap_or_default(),
                    model: lookup(&models, idx).and_then(|v| v.as_string()).unwrap_or_default(),
                    temp_c: lookup(&temps, idx).and_then(|v| v.first_number()).map(|v| v as f32),
                    status,
                    healthy,
                });
            }
        }
        _ => {}
    }

    // Volumes from the standard storage table when the vendor gave none.
    if p.volumes.is_empty() {
        let mut vols: Vec<Volume> = storage
            .iter()
            .filter(|s| s.1 > 1_000_000_000 && (s.3.ends_with("25.2.1.4") || s.0.starts_with('/')))
            .filter(|s| {
                let n = s.0.as_str();
                !(n == "/" || n.starts_with("/boot") || n.starts_with("/dev") || n.starts_with("/run") || n.starts_with("/sys") || n.starts_with("/proc") || n.starts_with("/tmp") || n.contains("docker") || n.starts_with("/var/lib"))
            })
            .map(|s| Volume { name: s.0.clone(), total: s.1, used: s.2, status: None, healthy: true, is_pool: false })
            .collect();
        // Prefer the data volumes NAS boxes use.
        let data: Vec<Volume> = vols
            .iter()
            .filter(|v| v.name.starts_with("/volume") || v.name.starts_with("/share") || v.name.to_lowercase().contains("data") || v.name.starts_with("/mnt"))
            .cloned()
            .collect();
        if !data.is_empty() {
            vols = data;
        }
        // Same pool mounted twice shows identical sizes; keep one.
        let mut seen = std::collections::HashSet::new();
        vols.retain(|v| seen.insert((v.total, v.used)));
        vols.sort_by(|a, b| b.total.cmp(&a.total));
        vols.truncate(6);
        p.volumes = vols;
    }

    // ── Network traffic
    let names = c.column("1.3.6.1.2.1.31.1.1.1.1");
    let names = if names.is_empty() { c.column("1.3.6.1.2.1.2.2.1.2") } else { names };
    let oper = c.column("1.3.6.1.2.1.2.2.1.8");
    let mut rx = c.column("1.3.6.1.2.1.31.1.1.1.6");
    let mut tx = c.column("1.3.6.1.2.1.31.1.1.1.10");
    if rx.is_empty() {
        rx = c.column("1.3.6.1.2.1.2.2.1.10");
        tx = c.column("1.3.6.1.2.1.2.2.1.16");
    }
    let mut counters = Counters::new();
    let mut ifaces: Vec<(u64, Iface)> = vec![];
    for (idx, v) in &names {
        let name = v.as_string().unwrap_or_default();
        let ln = name.to_lowercase();
        if ln == "lo" || ln.starts_with("docker") || ln.starts_with("veth") || ln.starts_with("sit") || ln.starts_with("tun") || ln.starts_with("ip6") || ln.starts_with("gre") || ln.contains("loopback") || ln.starts_with("tap") || ln.starts_with("br-") {
            continue;
        }
        let up = lookup(&oper, idx).and_then(|v| v.as_i64()).map(|s| s == 1).unwrap_or(true);
        let (Some(r), Some(t)) = (lookup(&rx, idx).and_then(|v| v.as_u64()), lookup(&tx, idx).and_then(|v| v.as_u64())) else { continue };
        let i = *idx.first().unwrap_or(&0);
        counters.insert(i, (r, t, now_ms));
        if !up {
            continue;
        }
        let (rbps, tbps) = match prev.get(&i) {
            Some((pr, pt, pms)) if now_ms > *pms && r >= *pr && t >= *pt => {
                let secs = (now_ms - pms) as f64 / 1000.0;
                (Some((r - pr) as f64 * 8.0 / secs), Some((t - pt) as f64 * 8.0 / secs))
            }
            _ => (None, None),
        };
        ifaces.push((r + t, Iface { name, rx_bps: rbps, tx_bps: tbps }));
    }
    ifaces.sort_by(|a, b| b.0.cmp(&a.0));
    p.ifaces = ifaces.into_iter().take(4).map(|x| x.1).collect();
    Ok((p, counters))
}

pub fn bytes(b: u64) -> String {
    let f = b as f64;
    if f >= 1e12 {
        format!("{:.1} TB", f / 1e12)
    } else if f >= 1e9 {
        format!("{:.0} GB", f / 1e9)
    } else if f >= 1e6 {
        format!("{:.0} MB", f / 1e6)
    } else {
        format!("{:.0} KB", f / 1e3)
    }
}

pub fn bps(v: Option<f64>) -> String {
    match v {
        None => "—".into(),
        Some(x) if x >= 1e9 => format!("{:.2} Gb/s", x / 1e9),
        Some(x) if x >= 1e6 => format!("{:.1} Mb/s", x / 1e6),
        Some(x) if x >= 1e3 => format!("{:.0} kb/s", x / 1e3),
        Some(x) => format!("{:.0} b/s", x),
    }
}
