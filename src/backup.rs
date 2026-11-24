//! Backup and restore: settings, device names, trusted programs and speed
//! history (traffic history optional). The file has a one-line header with a
//! SHA-256 fingerprint of the contents, so a damaged or edited file is caught
//! before anything is changed.

use crate::model::{Device, DeviceKind, PortCheck, Settings, Shared, WebCheck};
use crate::traffic::{SpeedResult, TrafficSaved};
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

const FORMAT: &str = "homenet-watch-backup";

#[derive(Serialize, Deserialize, Clone)]
pub struct DeviceId {
    pub name: String,
    pub ip: Ipv4Addr,
    pub mac: Option<String>,
    pub auto_name: Option<String>,
    pub kind: DeviceKind,
    pub group: String,
    pub notes: String,
    pub alerts: bool,
    pub ping: bool,
    pub arp: bool,
    pub ports: Vec<u16>,
    pub web: Option<WebCheck>,
    pub dns_name: String,
    pub snmp: bool,
    pub snmp_community: String,
    pub maker: String,
    // SNMP v3 sign-in, without the passwords (they must be entered again after a restore)
    #[serde(default)]
    pub snmp_version: Option<u8>,
    #[serde(default)]
    pub snmp_user: String,
    #[serde(default)]
    pub snmp_auth: crate::snmpv3::AuthProto,
    #[serde(default)]
    pub snmp_priv: crate::snmpv3::PrivProto,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Program {
    pub key: String,
    pub name: String,
    pub path: String,
    pub pinned: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Backup {
    pub created: i64,
    pub app_version: String,
    pub settings: Settings,
    pub devices: Vec<DeviceId>,
    pub programs: Vec<Program>,
    pub speed: Vec<SpeedResult>,
    pub traffic: Option<TrafficSaved>,
}

#[derive(Serialize, Deserialize)]
struct Header {
    format: String,
    version: u32,
    sha256: String,
}

#[derive(Clone, Copy)]
pub struct RestoreChoice {
    pub settings: bool,
    pub devices: bool,
    pub programs: bool,
    pub speed: bool,
    pub traffic: bool,
}

pub fn make(s: &Shared, with_traffic: bool) -> Backup {
    Backup {
        created: crate::model::now(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        settings: s.settings.clone(),
        devices: s
            .devices
            .iter()
            .map(|d| DeviceId {
                name: d.name.clone(),
                ip: d.ip,
                mac: d.mac.clone(),
                auto_name: d.auto_name.clone(),
                kind: d.kind,
                group: d.group.clone(),
                notes: d.notes.clone(),
                alerts: d.alerts,
                ping: d.ping,
                arp: d.arp,
                ports: d.ports.iter().map(|p| p.port).collect(),
                web: d.web.clone(),
                dns_name: d.dns_name.clone(),
                snmp: d.snmp,
                snmp_community: d.snmp_community.clone(),
                maker: d.maker.clone(),
                snmp_version: Some(d.snmp_version),
                snmp_user: d.snmp_user.clone(),
                snmp_auth: d.snmp_auth,
                snmp_priv: d.snmp_priv,
            })
            .collect(),
        programs: s
            .traffic
            .saved
            .apps
            .values()
            .filter(|a| a.pinned.is_some())
            .map(|a| Program { key: a.key.clone(), name: a.name.clone(), path: a.path.clone(), pinned: a.pinned.clone() })
            .collect(),
        speed: s.traffic.saved.speed.clone(),
        traffic: with_traffic.then(|| s.traffic.saved.clone()),
    }
}

pub fn write(dir: &Path, b: &Backup) -> Result<PathBuf, String> {
    let body = serde_json::to_string(b).map_err(|e| e.to_string())?;
    let header = Header { format: FORMAT.into(), version: 1, sha256: crate::sha256::hex(body.as_bytes()) };
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("HomeNetWatch-backup-{}.json", chrono::Local::now().format("%Y%m%d-%H%M")));
    let text = format!("{}\n{}", serde_json::to_string(&header).unwrap_or_default(), body);
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Read and check a backup file. Nothing is changed yet.
pub fn read(path: &Path) -> Result<Backup, String> {
    let text = std::fs::read_to_string(path).map_err(|_| "Couldn't open that file.".to_string())?;
    let (head, body) = text.split_once('\n').ok_or("This isn't a HomeNet Watch backup.")?;
    let h: Header = serde_json::from_str(head).map_err(|_| "This isn't a HomeNet Watch backup.".to_string())?;
    if h.format != FORMAT {
        return Err("This isn't a HomeNet Watch backup.".into());
    }
    if h.version > 1 {
        return Err("This backup was made by a newer version of HomeNet Watch.".into());
    }
    if crate::sha256::hex(body.as_bytes()) != h.sha256 {
        return Err("The backup file is damaged or was changed after it was made (its fingerprint doesn't match).".into());
    }
    serde_json::from_str(body).map_err(|e| format!("The backup couldn't be read: {}", e))
}

/// Backup files found in the usual places, newest first.
pub fn find() -> Vec<PathBuf> {
    let mut v = vec![];
    for dir in [crate::util::documents_dir(), crate::model::data_dir()] {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.starts_with("HomeNetWatch-backup-") && n.ends_with(".json") {
                    v.push(e.path());
                }
            }
        }
    }
    v.sort();
    v.dedup();
    v.reverse();
    v
}

pub fn summary(b: &Backup) -> Vec<(String, String)> {
    let mut out = vec![
        ("Made".to_string(), crate::widgets::fmt_time(b.created)),
        ("Settings".to_string(), format!("check every {} s, {} muted alert types", b.settings.interval_secs, b.settings.muted.len())),
        ("Devices".to_string(), format!("{} (names, rooms, notes, what to watch)", b.devices.len())),
        ("Trusted programs".to_string(), b.programs.len().to_string()),
        ("Speed tests".to_string(), b.speed.len().to_string()),
    ];
    out.push((
        "Traffic history".to_string(),
        match &b.traffic {
            Some(t) => format!("{} days, {} programs, {} destinations", t.minutes.len() / 1440, t.apps.len(), t.hosts.len()),
            None => "not included".into(),
        },
    ));
    out
}

pub fn apply(s: &mut Shared, b: &Backup, c: RestoreChoice) -> String {
    let mut done = vec![];
    if c.settings {
        s.settings = b.settings.clone();
        done.push("settings".to_string());
    }
    if c.devices {
        let (mut updated, mut added) = (0, 0);
        for id in &b.devices {
            let pos = s
                .devices
                .iter()
                .position(|d| id.mac.is_some() && d.mac == id.mac && id.maker != "Private address")
                .or_else(|| s.devices.iter().position(|d| id.auto_name.is_some() && d.auto_name == id.auto_name))
                .or_else(|| s.devices.iter().position(|d| d.ip == id.ip));
            let d = match pos {
                Some(i) => {
                    updated += 1;
                    &mut s.devices[i]
                }
                None => {
                    added += 1;
                    let nid = s.take_id();
                    s.devices.push(Device::new(nid, id.name.clone(), id.ip, id.kind));
                    s.devices.last_mut().unwrap()
                }
            };
            d.name = id.name.clone();
            d.kind = id.kind;
            d.group = id.group.clone();
            d.notes = id.notes.clone();
            d.alerts = id.alerts;
            d.ping = id.ping;
            d.arp = id.arp;
            d.ports = id.ports.iter().map(|p| PortCheck { port: *p, open: None }).collect();
            d.web = id.web.clone();
            d.dns_name = id.dns_name.clone();
            d.snmp = id.snmp;
            d.snmp_community = id.snmp_community.clone();
            d.snmp_version = id.snmp_version.unwrap_or(2);
            d.snmp_user = id.snmp_user.clone();
            d.snmp_auth = id.snmp_auth;
            d.snmp_priv = id.snmp_priv;
            if d.mac.is_none() {
                d.mac = id.mac.clone();
            }
            if d.auto_name.is_none() {
                d.auto_name = id.auto_name.clone();
            }
            if d.maker.is_empty() {
                d.maker = id.maker.clone();
            }
        }
        done.push(format!("{} devices updated, {} added", updated, added));
        if b.devices.iter().any(|d| d.snmp_version == Some(3)) {
            done.push("(enter the SNMP v3 passwords again: backups never contain passwords)".into());
        }
    }
    if c.traffic {
        if let Some(t) = &b.traffic {
            s.traffic.saved = t.clone();
            done.push("traffic history".into());
        }
    }
    if c.programs {
        for p in &b.programs {
            let a = s.traffic.saved.apps.entry(p.key.clone()).or_insert_with(|| crate::traffic::AppStat {
                key: p.key.clone(),
                name: p.name.clone(),
                path: p.path.clone(),
                first_seen: crate::model::now(),
                ..Default::default()
            });
            a.pinned = p.pinned.clone();
            a.changed_since_pin = false;
        }
        done.push(format!("{} trusted programs", b.programs.len()));
    }
    if c.speed {
        for r in &b.speed {
            if !s.traffic.saved.speed.iter().any(|x| x.t == r.t) {
                s.traffic.saved.speed.push(r.clone());
            }
        }
        s.traffic.saved.speed.sort_by_key(|r| r.t);
        done.push("speed history".into());
    }
    s.dirty = true;
    s.traffic.dirty = true;
    s.check_now = true;
    format!("Restored {}.", done.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Device, DeviceKind, Severity, Shared};
    use crate::traffic::{AppStat, Minute, SpeedResult, Traffic};

    fn empty() -> Shared {
        Shared {
            devices: vec![],
            events: Default::default(),
            settings: Settings::default(),
            scan: Default::default(),
            next_id: 0,
            local_ip: None,
            gateway: None,
            dirty: false,
            persist: false,
            check_now: false,
            attention: false,
            external_ip: None,
            traffic: Traffic::default(),
            last_gateway: None,
            gateway_mac: None,
        }
    }

    fn sample() -> Shared {
        let mut s = empty();
        let mut d = Device::new(1, "Office NAS".into(), "10.0.0.5".parse().unwrap(), DeviceKind::Storage);
        d.mac = Some("00:11:32:AA:BB:CC".into());
        d.group = "Office".into();
        d.snmp = true;
        s.devices.push(d);
        s.next_id = 5;
        s.settings.muted = vec!["beacon".into()];
        let t = crate::model::now();
        s.traffic.saved.apps.insert(
            "c:/tools/app.exe".into(),
            AppStat { key: "c:/tools/app.exe".into(), name: "app".into(), path: "C:/tools/app.exe".into(), first_seen: t, last_seen: t, conns: 3, pinned: Some("ab".repeat(32)), ..Default::default() },
        );
        s.traffic.saved.speed.push(SpeedResult { t, down_mbps: 300.0, up_mbps: 40.0, ping_ms: 12.0 });
        s.traffic.saved.minutes.push_back(Minute { t: t / 60 * 60, rx: 5_000_000, tx: 1_000_000 });
        s.add_alert("new-device", Severity::Warning, None, "New device joined the network".into());
        s
    }

    #[test]
    fn backup_roundtrip_and_tamper_check() {
        let dir = std::env::temp_dir().join(format!("hnw-test-{}", std::process::id()));
        let s = sample();
        let path = write(&dir, &make(&s, true)).expect("write");
        // reads back and the fingerprint matches
        let b = read(&path).expect("read");
        assert_eq!(b.devices.len(), 1);
        assert_eq!(b.programs.len(), 1);
        assert!(b.traffic.is_some());
        assert!(summary(&b).iter().any(|(k, _)| k == "Devices"));
        // a changed file is refused
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, text.replace("Office NAS", "Hacked NAS")).unwrap();
        match read(&path) {
            Ok(_) => panic!("a changed backup was accepted"),
            Err(e) => assert!(e.contains("fingerprint"), "{}", e),
        }
        // restoring into a fresh app brings names, rooms, trust and speed history back
        let mut fresh = empty();
        let msg = apply(&mut fresh, &b, RestoreChoice { settings: true, devices: true, programs: true, speed: true, traffic: false });
        assert!(msg.starts_with("Restored"));
        assert_eq!(fresh.devices[0].name, "Office NAS");
        assert_eq!(fresh.devices[0].group, "Office");
        assert!(fresh.devices[0].snmp);
        assert_eq!(fresh.settings.muted, vec!["beacon".to_string()]);
        assert!(fresh.traffic.saved.apps.values().any(|a| a.pinned.is_some()));
        assert_eq!(fresh.traffic.saved.speed.len(), 1);
        // restoring twice updates instead of duplicating
        apply(&mut fresh, &b, RestoreChoice { settings: false, devices: true, programs: true, speed: true, traffic: false });
        assert_eq!(fresh.devices.len(), 1);
        assert_eq!(fresh.traffic.saved.speed.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn weekly_report_builds() {
        let s = sample();
        let html = crate::report::weekly_html(&s);
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("This week on your network"));
        assert!(html.contains("app"));
        assert!(html.contains("300 Mb/s"));
        assert!(html.contains("New device joined"));
        assert!(html.ends_with("</html>"));
    }
}
