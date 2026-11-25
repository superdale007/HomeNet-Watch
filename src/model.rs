//! What the monitor knows: devices, their history, alerts and settings.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::net::Ipv4Addr;
use std::path::PathBuf;

pub const HISTORY_LEN: usize = 720;

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DeviceKind {
    Internet,
    Router,
    Computer,
    Laptop,
    Phone,
    Tablet,
    Tv,
    Speaker,
    Printer,
    Camera,
    Console,
    Storage,
    SmartHome,
    NetworkGear,
    #[default]
    Unknown,
}

impl DeviceKind {
    pub const ALL: [DeviceKind; 15] = [
        DeviceKind::Internet,
        DeviceKind::Router,
        DeviceKind::Computer,
        DeviceKind::Laptop,
        DeviceKind::Phone,
        DeviceKind::Tablet,
        DeviceKind::Tv,
        DeviceKind::Speaker,
        DeviceKind::Printer,
        DeviceKind::Camera,
        DeviceKind::Console,
        DeviceKind::Storage,
        DeviceKind::SmartHome,
        DeviceKind::NetworkGear,
        DeviceKind::Unknown,
    ];
    pub fn label(self) -> &'static str {
        match self {
            DeviceKind::Internet => "Internet",
            DeviceKind::Router => "Router",
            DeviceKind::Computer => "Computer",
            DeviceKind::Laptop => "Laptop",
            DeviceKind::Phone => "Phone",
            DeviceKind::Tablet => "Tablet",
            DeviceKind::Tv => "TV / Streamer",
            DeviceKind::Speaker => "Speaker",
            DeviceKind::Printer => "Printer",
            DeviceKind::Camera => "Camera",
            DeviceKind::Console => "Game console",
            DeviceKind::Storage => "Storage (NAS)",
            DeviceKind::SmartHome => "Smart home",
            DeviceKind::NetworkGear => "Wi-Fi / switch",
            DeviceKind::Unknown => "Other",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            DeviceKind::Internet => "🌐",
            DeviceKind::Router => "📶",
            DeviceKind::Computer => "🖥",
            DeviceKind::Laptop => "💻",
            DeviceKind::Phone => "📱",
            DeviceKind::Tablet => "📱",
            DeviceKind::Tv => "📺",
            DeviceKind::Speaker => "🔊",
            DeviceKind::Printer => "🖶",
            DeviceKind::Camera => "📷",
            DeviceKind::Console => "🎮",
            DeviceKind::Storage => "🗄",
            DeviceKind::SmartHome => "💡",
            DeviceKind::NetworkGear => "📡",
            DeviceKind::Unknown => "❓",
        }
    }
    /// Devices that sleep or leave the house often shouldn't raise alarms by default.
    /// Devices that sleep, go into standby or leave the house (phones, TVs, speakers…)
    /// start with "went offline" alerts off; always-on gear starts with them on.
    pub fn alerts_by_default(self) -> bool {
        !matches!(
            self,
            DeviceKind::Phone | DeviceKind::Tablet | DeviceKind::Laptop | DeviceKind::Tv | DeviceKind::Speaker | DeviceKind::Console | DeviceKind::Unknown
        )
    }

    /// Gear whose slowness affects everyone at home.
    pub fn important(self) -> bool {
        matches!(self, DeviceKind::Router | DeviceKind::Internet | DeviceKind::Storage | DeviceKind::NetworkGear)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Status {
    #[default]
    Unknown,
    Up,
    Warning,
    Down,
    Paused,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Unknown => "Checking…",
            Status::Up => "Online",
            Status::Warning => "Slow / partial",
            Status::Down => "Offline",
            Status::Paused => "Paused",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Sample {
    pub t: i64,
    /// None = no answer.
    pub ms: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PortCheck {
    pub port: u16,
    #[serde(skip)]
    pub open: Option<bool>,
}

pub fn port_name(port: u16) -> &'static str {
    match port {
        21 => "FTP",
        22 => "SSH",
        23 => "Telnet",
        53 => "DNS",
        80 => "Web",
        139 => "File sharing",
        443 => "Secure web",
        445 => "File sharing",
        554 => "Video stream",
        631 => "Printing",
        1883 => "Smart home (MQTT)",
        3389 => "Remote desktop",
        5000 | 5001 => "NAS admin",
        8008 | 8009 => "Casting",
        8080 => "Web (alt)",
        8123 => "Home hub",
        9100 => "Printing",
        32400 => "Media server",
        62078 => "Phone sync",
        _ => "Service",
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Device {
    pub id: u64,
    pub name: String,
    pub ip: Ipv4Addr,
    #[serde(default)]
    pub mac: Option<String>,
    #[serde(default)]
    pub maker: String,
    pub kind: DeviceKind,
    #[serde(default = "yes")]
    pub ping: bool,
    /// Presence check on the home network (for devices that ignore ping).
    #[serde(default)]
    pub arp: bool,
    #[serde(default)]
    pub ports: Vec<PortCheck>,
    #[serde(default)]
    pub paused: bool,
    #[serde(default = "yes")]
    pub alerts: bool,
    #[serde(default)]
    pub notes: String,
    pub first_seen: i64,
    #[serde(default)]
    pub last_seen: Option<i64>,
    #[serde(default)]
    pub map_pos: Option<[f32; 2]>,
    #[serde(default)]
    pub history: VecDeque<Sample>,
    #[serde(default)]
    pub checks: u64,
    #[serde(default)]
    pub ok_checks: u64,
    #[serde(default)]
    pub outages: u32,
    #[serde(default)]
    pub downtime_secs: i64,
    #[serde(default)]
    pub open_ports_seen: Vec<u16>,
    /// Room or group, e.g. "Office"
    #[serde(default)]
    pub group: String,
    /// Read system stats (CPU, memory, storage, disks, traffic) over SNMP
    #[serde(default)]
    pub snmp: bool,
    #[serde(default = "public")]
    pub snmp_community: String,
    /// 2 = v1/v2c (community), 3 = v3 (user name and passwords)
    #[serde(default = "two")]
    pub snmp_version: u8,
    #[serde(default)]
    pub snmp_user: String,
    #[serde(default)]
    pub snmp_auth: crate::snmpv3::AuthProto,
    #[serde(default, with = "crate::secret")]
    pub snmp_auth_pass: String,
    #[serde(default)]
    pub snmp_priv: crate::snmpv3::PrivProto,
    #[serde(default, with = "crate::secret")]
    pub snmp_priv_pass: String,
    /// Web page check (plain HTTP)
    #[serde(default)]
    pub web: Option<WebCheck>,
    /// DNS check: ask this device to look up this name (empty = off)
    #[serde(default)]
    pub dns_name: String,
    /// The name the network gave it when first found (helps recognise it after an address change)
    #[serde(default)]
    pub auto_name: Option<String>,
    /// A "not seen lately" alert was sent
    #[serde(default)]
    pub departed: bool,

    // Live state (not saved)
    #[serde(skip)]
    pub status: Status,
    #[serde(skip)]
    pub latency: Option<f32>,
    #[serde(skip)]
    pub fails_in_row: u32,
    #[serde(skip)]
    pub last_check: Option<i64>,
    #[serde(skip)]
    pub down_since: Option<i64>,
    #[serde(skip)]
    pub slow: bool,
    #[serde(skip)]
    pub busy: bool,
    #[serde(skip)]
    pub perf: Option<crate::perf::Perf>,
    #[serde(skip)]
    pub perf_error: Option<String>,
    #[serde(skip)]
    pub perf_last: Option<i64>,
    #[serde(skip)]
    pub perf_busy: bool,
    #[serde(skip)]
    pub perf_history: VecDeque<crate::perf::PerfSample>,
    #[serde(skip)]
    pub counters: crate::perf::Counters,
    /// Active problems found in the stats: (key, severity, message)
    #[serde(skip)]
    pub problems: Vec<(String, Severity, String)>,
    #[serde(skip)]
    pub web_state: Option<Result<String, String>>,
    #[serde(skip)]
    pub dns_state: Option<Result<String, String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WebCheck {
    pub port: u16,
    pub path: String,
    #[serde(default)]
    pub expect: String,
}

fn public() -> String {
    "public".into()
}

fn two() -> u8 {
    2
}

fn yes() -> bool {
    true
}

impl Device {
    pub fn new(id: u64, name: String, ip: Ipv4Addr, kind: DeviceKind) -> Self {
        Device {
            id,
            name,
            ip,
            mac: None,
            maker: String::new(),
            kind,
            ping: true,
            arp: false,
            ports: vec![],
            paused: false,
            alerts: kind.alerts_by_default(),
            notes: String::new(),
            first_seen: now(),
            last_seen: None,
            map_pos: None,
            history: VecDeque::new(),
            checks: 0,
            ok_checks: 0,
            outages: 0,
            downtime_secs: 0,
            open_ports_seen: vec![],
            status: Status::Unknown,
            latency: None,
            fails_in_row: 0,
            last_check: None,
            down_since: None,
            slow: false,
            busy: false,
            group: String::new(),
            snmp: false,
            snmp_community: public(),
            snmp_version: 2,
            snmp_user: String::new(),
            snmp_auth: Default::default(),
            snmp_auth_pass: String::new(),
            snmp_priv: Default::default(),
            snmp_priv_pass: String::new(),
            web: None,
            dns_name: String::new(),
            auto_name: None,
            departed: false,
            perf: None,
            perf_error: None,
            perf_last: None,
            perf_busy: false,
            perf_history: VecDeque::new(),
            counters: Default::default(),
            problems: vec![],
            web_state: None,
            dns_state: None,
        }
    }

    /// How to sign in to this device's SNMP service.
    pub fn snmp_security(&self) -> crate::snmp::Security {
        if self.snmp_version == 3 {
            crate::snmp::Security::V3 {
                user: self.snmp_user.clone(),
                auth: self.snmp_auth,
                auth_pass: self.snmp_auth_pass.clone(),
                privp: if self.snmp_auth == crate::snmpv3::AuthProto::None { crate::snmpv3::PrivProto::None } else { self.snmp_priv },
                priv_pass: self.snmp_priv_pass.clone(),
            }
        } else {
            crate::snmp::Security::V2c(self.snmp_community.clone())
        }
    }

    pub fn uptime_pct(&self) -> Option<f32> {
        if self.checks == 0 {
            None
        } else {
            Some(self.ok_checks as f32 * 100.0 / self.checks as f32)
        }
    }

    pub fn latency_stats(&self) -> (Option<f32>, Option<f32>, Option<f32>) {
        let v: Vec<f32> = self.history.iter().filter_map(|s| s.ms).collect();
        if v.is_empty() {
            return (None, None, None);
        }
        let min = v.iter().cloned().fold(f32::MAX, f32::min);
        let max = v.iter().cloned().fold(0.0, f32::max);
        let avg = v.iter().sum::<f32>() / v.len() as f32;
        (Some(min), Some(avg), Some(max))
    }

    pub fn push_sample(&mut self, s: Sample) {
        self.history.push_back(s);
        while self.history.len() > HISTORY_LEN {
            self.history.pop_front();
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Info => "Info",
            Severity::Warning => "Warning",
            Severity::Critical => "Critical",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub id: u64,
    pub t: i64,
    pub severity: Severity,
    pub device_id: Option<u64>,
    pub device_name: String,
    pub message: String,
    #[serde(default)]
    pub acknowledged: bool,
    /// Alert type (see `alert_info`)
    #[serde(default)]
    pub kind: String,
    /// The person opened it to read more (used by alert tuning)
    #[serde(default)]
    pub opened: bool,
    /// Recorded while this alert type was muted
    #[serde(default)]
    pub muted: bool,
}

pub struct AlertInfo {
    pub title: &'static str,
    pub explain: &'static str,
    pub action: &'static str,
    /// MITRE ATT&CK technique, when the alert matches one
    pub mitre: &'static str,
}

/// MITRE ATT&CK technique name and tactics (the attacker's goal) for the IDs we use.
pub fn mitre_details(id: &str) -> Option<(&'static str, &'static str)> {
    Some(match id {
        "T1200" => ("Hardware Additions", "Initial Access"),
        "T1557" => ("Adversary-in-the-Middle", "Credential Access, Collection"),
        "T1557.002" => ("ARP Cache Poisoning", "Credential Access, Collection"),
        "T1133" => ("External Remote Services", "Initial Access, Persistence"),
        "T1554" => ("Compromise Host Software Binary", "Persistence"),
        "T1071" => ("Application Layer Protocol", "Command and Control"),
        "T1021" => ("Remote Services", "Lateral Movement"),
        _ => return None,
    })
}

/// "T1557.002 ARP Cache Poisoning (Credential Access, Collection)", or "" when untagged.
pub fn mitre_label(id: &str) -> String {
    match mitre_details(id) {
        Some((name, tactic)) => format!("{} {} ({})", id, name, tactic),
        None if id.is_empty() => String::new(),
        None => id.to_string(),
    }
}

impl Event {
    /// The alert type, working it out for older alerts that didn't record it.
    pub fn kind_or_guess(&self) -> &str {
        if self.kind.is_empty() {
            infer_kind(&self.message)
        } else {
            &self.kind
        }
    }
    /// MITRE ATT&CK technique for this alert, if it's a security alert.
    pub fn mitre(&self) -> &'static str {
        alert_info(self.kind_or_guess()).mitre
    }
}

pub const ALERT_KINDS: [&str; 19] = [
    "offline", "back-online", "slow", "port-closed", "new-device", "departed", "address-changed", "gateway-change", "arp-spoof", "new-open-port",
    "web-check", "dns-check", "nas-problem", "program-changed", "beacon", "lateral", "speed", "scan", "other",
];

/// Plain-language explanation and suggested action for each alert type.
pub fn alert_info(kind: &str) -> AlertInfo {
    let (title, explain, action, mitre) = match kind {
        "offline" => ("Device went offline", "The device stopped answering. It may be switched off, unplugged, out of Wi-Fi range, or stuck.", "Check that it has power and is connected. Restarting it often helps. If several devices dropped at once, check the router.", ""),
        "back-online" => ("Device back online", "A device that had stopped answering is answering again.", "Nothing to do. If it happens often, look at its Wi-Fi signal or power.", ""),
        "slow" => ("Slow responses", "The device keeps taking longer than your limit to answer. Busy Wi-Fi, distance from the router or a busy device can cause this.", "Move it closer to the router, or check whether something on it is using a lot of bandwidth.", ""),
        "port-closed" => ("Service stopped", "A service you asked to watch (like a web page or file sharing) stopped answering, even though the device may still be on.", "Open the device's settings or app and check that the service is running.", ""),
        "new-device" => ("New device joined", "Something your monitor hasn't seen before connected to your network.", "If you recognize it (a new phone, a guest), give it a name. If you don't, change your Wi-Fi password and check your router's device list.", "T1200"),
        "departed" => ("Device not seen lately", "A device that used to be on your network hasn't been seen for a day.", "If you got rid of it, remove it from the list. Otherwise check that it's switched on.", ""),
        "address-changed" => ("Device changed address", "A known device came back with a different network address or hardware address. Phones do this on purpose for privacy; it can also happen after a router restart.", "Usually nothing to do: names and settings were kept. If the device seems unfamiliar, check it.", ""),
        "gateway-change" => ("Router changed", "The address of your router (the way to the internet) changed. This happens with a new router or a different network, but can also mean something is pretending to be your router.", "If you didn't change anything, restart your router and check what's connected. Avoid entering passwords until it's explained.", "T1557"),
        "arp-spoof" => ("Possible router impersonation", "Another device on your network is answering for the router's address. This is how someone can secretly read traffic (\"ARP spoofing\").", "Unplug or disconnect unknown devices, restart the router, and change the Wi-Fi password. Use websites with https only until it's resolved.", "T1557.002"),
        "new-open-port" => ("New open port", "A device started offering a network service it didn't before (for example remote access or file sharing).", "If you just set something up, fine. Otherwise check the device's settings and turn off services you don't use.", "T1133"),
        "web-check" => ("Web page check", "A web page you asked to watch stopped loading or no longer shows what you expected.", "Open the page in a browser to see what's wrong.", ""),
        "dns-check" => ("Name lookup check", "A device you asked to watch couldn't look up web addresses (DNS). Websites won't load by name without it.", "Restart the router; if it keeps failing, try a different DNS server in the router's settings.", ""),
        "nas-problem" => ("Storage or system stats", "Your NAS or another device reported a problem: a disk, a storage pool, temperature, or a full volume.", "Open the Performance page for details. Replace failing disks soon and keep a backup.", ""),
        "program-changed" => ("Trusted program changed", "A program you trusted now has a different fingerprint. Updates do this, but so does tampering.", "If the program just updated, open it in Apps and trust the new version. If it didn't, scan the computer for malware.", "T1554"),
        "beacon" => ("Regular check-ins", "A program contacts the same internet address at very regular intervals. Many apps do this to check for updates or messages; malware does it to receive commands.", "If you know the program, it's probably fine. If you don't recognize it, look it up and consider removing it.", "T1071"),
        "lateral" => ("Reaching many home devices", "This computer contacted many devices at home on remote-control or file-sharing ports in a short time. Backups and network tools do this; so does malware spreading.", "If you weren't running a backup or network tool, check which program did it in Apps and scan the computer.", "T1021"),
        "speed" => ("Internet speed", "Speed test results.", "If speeds are much lower than your plan, restart the router and test again with a cable.", ""),
        "scan" => ("Network scan", "Your network was scanned for devices.", "Nothing to do.", ""),
        _ => ("Other", "A message from the monitor.", "Nothing to do.", ""),
    };
    AlertInfo { title, explain, action, mitre }
}

/// Work out the alert type of older-style messages.
pub fn infer_kind(message: &str) -> &'static str {
    let m = message.to_lowercase();
    if m.starts_with("stopped responding") {
        "offline"
    } else if m.starts_with("back online") {
        "back-online"
    } else if m.starts_with("responding slowly") {
        "slow"
    } else if m.contains("(port ") {
        "port-closed"
    } else if m.starts_with("new device joined") {
        "new-device"
    } else if m.starts_with("web page check") {
        "web-check"
    } else if m.starts_with("dns lookup check") {
        "dns-check"
    } else if m.starts_with("network scan") || m.starts_with("welcome") {
        "scan"
    } else if m.contains("storage") || m.contains("disk") || m.contains("hot (") || m.contains("processor busy") || m.contains("system problem") || m.contains("system stats") || m.starts_with("back to normal") || m.contains("health:") || m.contains("running hot") {
        "nas-problem"
    } else {
        "other"
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub interval_secs: u32,
    pub timeout_ms: u32,
    pub slow_ms: u32,
    pub fails_before_alert: u32,
    pub alert_new_devices: bool,
    pub scan_range: String,
    pub rescan_hours: u32,
    pub monitoring: bool,
    /// How often to read NAS / system stats (seconds)
    pub perf_interval_secs: u32,
    pub cpu_warn: u32,
    pub disk_warn: u32,
    pub temp_warn: u32,
    /// Limit for the NAS's own (system) temperature sensor, which runs warmer than disks
    pub sys_temp_warn: u32,
    /// What to do when something goes wrong
    pub sound: bool,
    pub flash: bool,
    pub quiet: bool,
    pub quiet_from: u32,
    pub quiet_to: u32,
    /// Look up which country internet destinations are in (sends addresses to ip-api.com)
    pub geo_lookup: bool,
    /// Alert types the person muted
    pub muted: Vec<String>,
    /// Alert when a device hasn't been seen for this many hours (0 = off)
    pub departed_hours: u32,
    /// Look for a new version on GitHub when the app starts (published copies only)
    pub check_updates: bool,
    /// Closing the window keeps HomeNet Watch watching in the background
    pub keep_running: bool,
    /// Pop-up notifications on this computer when the window isn't in front
    pub notify_desktop: bool,
    /// Phone and email alerts from this level up: 1 = warnings, 2 = critical only
    pub remote_min: u8,
    /// Quiet hours also hold back phone and email alerts
    pub quiet_remote: bool,
    pub ntfy_enabled: bool,
    pub ntfy_server: String,
    pub ntfy_topic: String,
    pub email_enabled: bool,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    #[serde(with = "crate::secret")]
    pub smtp_pass: String,
    pub email_from: String,
    pub email_to: String,
    /// Only routers, the internet, NAS boxes and network gear raise "slow" alerts
    pub slow_important_only: bool,
    /// Network map grouping: 0 none, 1 by type, 2 by room
    pub map_group: u8,
}

impl Settings {
    /// Inside quiet hours no sound or flashing (alerts are still recorded).
    pub fn quiet_now(&self) -> bool {
        if !self.quiet {
            return false;
        }
        use chrono::Timelike;
        let h = chrono::Local::now().hour();
        if self.quiet_from <= self.quiet_to {
            h >= self.quiet_from && h < self.quiet_to
        } else {
            h >= self.quiet_from || h < self.quiet_to
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            interval_secs: 30,
            timeout_ms: 1000,
            slow_ms: 150,
            fails_before_alert: 2,
            alert_new_devices: true,
            scan_range: crate::net::default_range(),
            rescan_hours: 6,
            monitoring: true,
            perf_interval_secs: 60,
            cpu_warn: 90,
            disk_warn: 90,
            temp_warn: 60,
            sys_temp_warn: 70,
            sound: true,
            flash: true,
            quiet: false,
            quiet_from: 22,
            quiet_to: 7,
            geo_lookup: false,
            muted: vec![],
            departed_hours: 24,
            check_updates: true,
            keep_running: true,
            notify_desktop: true,
            remote_min: 2,
            quiet_remote: true,
            ntfy_enabled: false,
            ntfy_server: "https://ntfy.sh".into(),
            ntfy_topic: String::new(),
            email_enabled: false,
            smtp_host: String::new(),
            smtp_port: 587,
            smtp_user: String::new(),
            smtp_pass: String::new(),
            email_from: String::new(),
            email_to: String::new(),
            slow_important_only: true,
            map_group: 0,
        }
    }
}

#[derive(Default, Clone)]
pub struct ScanState {
    pub running: bool,
    pub done: usize,
    pub total: usize,
    pub found: usize,
    pub new_found: usize,
    pub last_finished: Option<i64>,
    pub message: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Saved {
    pub devices: Vec<Device>,
    pub events: Vec<Event>,
    pub settings: Option<Settings>,
    pub next_id: u64,
    #[serde(default)]
    pub last_scan: Option<i64>,
    /// Router address and hardware address last time (to notice changes)
    #[serde(default)]
    pub last_gateway: Option<Ipv4Addr>,
    #[serde(default)]
    pub gateway_mac: Option<String>,
}

pub struct Shared {
    pub devices: Vec<Device>,
    pub events: VecDeque<Event>,
    pub settings: Settings,
    pub scan: ScanState,
    pub next_id: u64,
    pub local_ip: Option<Ipv4Addr>,
    pub gateway: Option<Ipv4Addr>,
    pub dirty: bool,
    pub persist: bool,
    pub check_now: bool,
    /// Set when a warning or critical alert arrives; the window flashes / beeps.
    pub attention: bool,
    /// The home's public address as the internet sees it
    pub external_ip: Option<String>,
    /// The window is visible and in front (desktop notifications are skipped then)
    pub window_active: bool,
    /// This computer's traffic, programs, destinations and speed tests
    pub traffic: crate::traffic::Traffic,
    pub last_gateway: Option<Ipv4Addr>,
    pub gateway_mac: Option<String>,
}

impl Shared {
    pub fn take_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn add_event(&mut self, severity: Severity, device: Option<&Device>, message: String) {
        let kind = infer_kind(&message);
        self.add_alert(kind, severity, device, message);
    }

    pub fn add_alert(&mut self, kind: &str, severity: Severity, device: Option<&Device>, message: String) {
        let id = self.take_id();
        let muted = self.settings.muted.iter().any(|k| k == kind);
        self.events.push_front(Event {
            id,
            t: now(),
            severity,
            device_id: device.map(|d| d.id),
            device_name: device.map(|d| d.name.clone()).unwrap_or_else(|| "System".into()),
            message,
            acknowledged: severity == Severity::Info || muted,
            kind: kind.to_string(),
            opened: false,
            muted,
        });
        if severity != Severity::Info && !muted {
            self.attention = true;
        }
        while self.events.len() > 500 {
            self.events.pop_back();
        }
        self.dirty = true;
    }

    pub fn device(&self, id: u64) -> Option<&Device> {
        self.devices.iter().find(|d| d.id == id)
    }
    pub fn device_mut(&mut self, id: u64) -> Option<&mut Device> {
        self.devices.iter_mut().find(|d| d.id == id)
    }

    pub fn unacked(&self) -> usize {
        self.events.iter().filter(|e| !e.acknowledged).count()
    }

    pub fn to_saved(&self) -> Saved {
        Saved {
            devices: self.devices.clone(),
            events: self.events.iter().cloned().collect(),
            settings: Some(self.settings.clone()),
            next_id: self.next_id,
            last_scan: self.scan.last_finished,
            last_gateway: self.last_gateway,
            gateway_mac: self.gateway_mac.clone(),
        }
    }
}

/// Where settings and history are kept:
/// Windows %APPDATA%, macOS ~/Library/Application Support, Linux ~/.config (or $XDG_CONFIG_HOME).
pub fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from).or(home);
    #[cfg(target_os = "macos")]
    let base = home.map(|h| h.join("Library").join("Application Support"));
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).or_else(|| home.map(|h| h.join(".config")));
    base.unwrap_or_else(|| PathBuf::from(".")).join("HomeNetWatch")
}

pub fn data_file() -> PathBuf {
    data_dir().join("homenet-watch.json")
}

pub fn load() -> Saved {
    std::fs::read_to_string(data_file())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(saved: &Saved) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir())?;
    let tmp = data_dir().join("homenet-watch.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(saved).unwrap_or_default())?;
    std::fs::rename(tmp, data_file())?;
    // Only the owner may read it (it can hold SNMP v3 passwords on Mac and Linux).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(data_file(), std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Guess what a device is from what it answers on.
pub fn guess_kind(ip: Ipv4Addr, gateway: Option<Ipv4Addr>, open: &[u16], maker: &str, name: &str) -> DeviceKind {
    if Some(ip) == gateway {
        return DeviceKind::Router;
    }
    let n = name.to_lowercase();
    let m = maker.to_lowercase();
    let has = |p: u16| open.contains(&p);
    let name_has = |w: &[&str]| w.iter().any(|x| n.contains(x));
    if name_has(&["iphone", "android", "pixel", "galaxy", "phone"]) {
        return DeviceKind::Phone;
    }
    if name_has(&["ipad", "tablet", "kindle", "fire-", "-tab", "tab-"]) {
        return DeviceKind::Tablet;
    }
    if name_has(&["printer", "officejet", "laserjet", "deskjet", "epson", "brother", "canon"]) || has(9100) || has(631) {
        return DeviceKind::Printer;
    }
    if name_has(&["nas", "diskstation", "synology", "qnap", "ugreen", "nasync", "dxp", "ugos", "truenas", "unraid"]) || m.contains("synology") || m.contains("qnap") || m.contains("ugreen") || has(5001) || has(9999) {
        return DeviceKind::Storage;
    }
    if name_has(&["tv", "roku", "chromecast", "firetv", "appletv", "apple-tv", "shield", "bravia"]) || has(8008) || has(8009) {
        return DeviceKind::Tv;
    }
    if name_has(&["switch", "eap", "access-point", "accesspoint", "mesh", "extender", "repeater", "deco", "eero", "orbi", "unifi", "uap-", "velop", "omada", "oc200", "oc300"]) && !n.contains("nintendo") {
        return DeviceKind::NetworkGear;
    }
    if name_has(&["xbox", "playstation", "ps4", "ps5", "nintendo"]) || m.contains("nintendo") || m.contains("sony") {
        return DeviceKind::Console;
    }
    if name_has(&["sonos", "echo", "homepod", "speaker", "soundbar", "nest-mini", "google-home"]) || m.contains("sonos") {
        return DeviceKind::Speaker;
    }
    if name_has(&["cam", "doorbell", "ring"]) || has(554) {
        return DeviceKind::Camera;
    }
    if name_has(&["hue", "plug", "bulb", "thermostat", "nest", "esp", "shelly", "tasmota", "hub", "fan", "sprinkler", "tuya", "myq", "tesla", "wallconnector", "cooktop", "oven", "fridge", "washer", "dryer", "lwip", "hs1", "hs2", "kasa", "wemo", "ecobee", "rachio", "garage", "sensor", "smart"])
        || m.contains("espressif")
        || m.contains("hue")
        || m.contains("nest")
        || has(1883)
        || has(8123)
    {
        return DeviceKind::SmartHome;
    }
    if name_has(&["macbook", "laptop", "notebook"]) {
        return DeviceKind::Laptop;
    }
    if has(62078) {
        return DeviceKind::Phone;
    }
    if has(5000) || has(5001) {
        return DeviceKind::Storage;
    }
    if has(3389) || has(445) || has(139) || has(22) || name_has(&["desktop", "pc", "imac", "mac", "server", "raspberry", "linux", "ubuntu", "workstation"]) || m.contains("raspberry") {
        return DeviceKind::Computer;
    }
    if m == "private address" {
        return DeviceKind::Phone;
    }
    DeviceKind::Unknown
}
