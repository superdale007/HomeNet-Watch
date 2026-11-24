//! Low-level network probes: ping, ARP, reverse name lookup, TCP port checks,
//! and finding the local address and router.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

/// Our own IPv4 address on the home network (no packets are sent).
pub fn local_ipv4() -> Option<Ipv4Addr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    match sock.local_addr().ok()? {
        SocketAddr::V4(a) => Some(*a.ip()),
        _ => None,
    }
}

/// The router (default gateway) address, if it can be found.
pub fn default_gateway() -> Option<Ipv4Addr> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::NetworkManagement::IpHelper::{GetBestRoute, MIB_IPFORWARDROW};
        unsafe {
            let mut row: MIB_IPFORWARDROW = std::mem::zeroed();
            let dest = u32::from_ne_bytes([8, 8, 8, 8]);
            if GetBestRoute(dest, 0, &mut row) == 0 && row.dwForwardNextHop != 0 {
                let ip = Ipv4Addr::from(row.dwForwardNextHop.to_ne_bytes());
                return Some(ip);
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        crate::unix::default_gateway().or_else(|| {
            local_ipv4().map(|ip| {
                let o = ip.octets();
                Ipv4Addr::new(o[0], o[1], o[2], 1)
            })
        })
    }
}

/// Send one ping. Returns the round trip time in milliseconds, or None.
pub fn ping(ip: Ipv4Addr, timeout_ms: u32) -> Option<f32> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::NetworkManagement::IpHelper::{
            IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY,
        };
        unsafe {
            let h = IcmpCreateFile();
            if h.is_null() || h as isize == -1 {
                return None;
            }
            let payload = *b"homenet-watch-probe-32-bytes-abc";
            let mut reply = vec![0u8; std::mem::size_of::<ICMP_ECHO_REPLY>() + payload.len() + 64];
            let start = Instant::now();
            let n = IcmpSendEcho(
                h,
                u32::from_ne_bytes(ip.octets()),
                payload.as_ptr() as *const _,
                payload.len() as u16,
                std::ptr::null(),
                reply.as_mut_ptr() as *mut _,
                reply.len() as u32,
                timeout_ms,
            );
            let elapsed = start.elapsed().as_secs_f32() * 1000.0;
            IcmpCloseHandle(h);
            if n == 0 {
                return None;
            }
            let r = &*(reply.as_ptr() as *const ICMP_ECHO_REPLY);
            if r.Status != 0 {
                return None;
            }
            // RoundTripTime is whole ms; use our own timer when it says 0.
            let rtt = r.RoundTripTime as f32;
            Some(if rtt < 1.0 { elapsed.min(1.0).max(0.2) } else { rtt })
        }
    }
    #[cfg(not(windows))]
    {
        crate::unix::ping(ip, timeout_ms)
    }
}

/// Ask the device for its hardware (MAC) address. Only works on the local network.
pub fn arp_mac(ip: Ipv4Addr) -> Option<String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::NetworkManagement::IpHelper::SendARP;
        unsafe {
            let mut mac = [0u8; 8];
            let mut len: u32 = 6;
            let r = SendARP(u32::from_ne_bytes(ip.octets()), 0, mac.as_mut_ptr() as *mut _, &mut len);
            if r == 0 && len >= 6 && mac[..6].iter().any(|b| *b != 0) {
                return Some(format_mac(&mac[..6]));
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        crate::unix::arp_mac(ip)
    }
}

pub(crate) fn format_mac(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02X}", x)).collect::<Vec<_>>().join(":")
}

/// Reverse name lookup (DNS / NetBIOS / mDNS where the system supports it).
pub fn reverse_name(ip: Ipv4Addr) -> Option<String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Networking::WinSock::{
            getnameinfo, WSAStartup, AF_INET, IN_ADDR, IN_ADDR_0, NI_NAMEREQD, SOCKADDR, SOCKADDR_IN, WSADATA,
        };
        unsafe {
            let mut wsa: WSADATA = std::mem::zeroed();
            WSAStartup(0x0202, &mut wsa);
            let sa = SOCKADDR_IN {
                sin_family: AF_INET,
                sin_port: 0,
                sin_addr: IN_ADDR { S_un: IN_ADDR_0 { S_addr: u32::from_ne_bytes(ip.octets()) } },
                sin_zero: [0; 8],
            };
            let mut host = [0u8; 256];
            let r = getnameinfo(
                &sa as *const SOCKADDR_IN as *const SOCKADDR,
                std::mem::size_of::<SOCKADDR_IN>() as i32,
                host.as_mut_ptr(),
                host.len() as u32,
                std::ptr::null_mut(),
                0,
                NI_NAMEREQD as i32,
            );
            if r != 0 {
                return None;
            }
            let end = host.iter().position(|b| *b == 0).unwrap_or(host.len());
            let name = String::from_utf8_lossy(&host[..end]).to_string();
            clean_name(&name, ip)
        }
    }
    #[cfg(not(windows))]
    {
        crate::unix::reverse_name(ip).and_then(|n| clean_name(&n, ip))
    }
}

fn clean_name(name: &str, ip: Ipv4Addr) -> Option<String> {
    let n = name.trim().trim_end_matches('.');
    if n.is_empty() || n == ip.to_string() {
        return None;
    }
    // Drop common home domain suffixes.
    let mut short = n.to_string();
    for suf in [".lan", ".local", ".home", ".localdomain", ".home.arpa", ".router", ".domain"] {
        if short.to_lowercase().ends_with(suf) {
            short.truncate(short.len() - suf.len());
        }
    }
    if short.is_empty() {
        None
    } else {
        Some(short)
    }
}

/// Is a TCP port open? Returns connect time in ms.
pub fn tcp_open(ip: Ipv4Addr, port: u16, timeout_ms: u32) -> Option<f32> {
    let addr = SocketAddr::V4(SocketAddrV4::new(ip, port));
    let start = Instant::now();
    TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms as u64))
        .ok()
        .map(|_| start.elapsed().as_secs_f32() * 1000.0)
}

/// Parse a scan range: "192.168.1.0/24", "192.168.1.1-254" or "192.168.1.10-192.168.1.50".
pub fn parse_range(text: &str) -> Result<Vec<Ipv4Addr>, String> {
    let t = text.trim();
    if let Some((base, bits)) = t.split_once('/') {
        let ip: Ipv4Addr = base.trim().parse().map_err(|_| "Address not understood".to_string())?;
        let bits: u32 = bits.trim().parse().map_err(|_| "Prefix not understood".to_string())?;
        if !(22..=30).contains(&bits) {
            return Err("Use a prefix between /22 and /30".into());
        }
        let mask = u32::MAX << (32 - bits);
        let net = u32::from(ip) & mask;
        let size = 1u32 << (32 - bits);
        return Ok((1..size - 1).map(|i| Ipv4Addr::from(net + i)).collect());
    }
    if let Some((a, b)) = t.split_once('-') {
        let start: Ipv4Addr = a.trim().parse().map_err(|_| "Start address not understood".to_string())?;
        let end: Ipv4Addr = if b.contains('.') {
            b.trim().parse().map_err(|_| "End address not understood".to_string())?
        } else {
            let last: u8 = b.trim().parse().map_err(|_| "End number not understood".to_string())?;
            let o = start.octets();
            Ipv4Addr::new(o[0], o[1], o[2], last)
        };
        let (s, e) = (u32::from(start), u32::from(end));
        if e < s {
            return Err("End is before start".into());
        }
        if e - s > 1024 {
            return Err("Range too large (max 1024 addresses)".into());
        }
        return Ok((s..=e).map(Ipv4Addr::from).collect());
    }
    let ip: Ipv4Addr = t.parse().map_err(|_| "Range not understood".to_string())?;
    Ok(vec![ip])
}

pub fn default_range() -> String {
    match local_ipv4() {
        Some(ip) => {
            let o = ip.octets();
            format!("{}.{}.{}.0/24", o[0], o[1], o[2])
        }
        None => "192.168.1.0/24".into(),
    }
}

/// A few well-known hardware maker prefixes. Phones and tablets often use
/// private (randomised) addresses, which we can also spot.
pub fn maker_from_mac(mac: &str) -> String {
    let up = mac.to_uppercase();
    let prefix: String = up.chars().take(8).collect();
    let table: &[(&str, &str)] = &[
        ("B8:27:EB", "Raspberry Pi"),
        ("DC:A6:32", "Raspberry Pi"),
        ("E4:5F:01", "Raspberry Pi"),
        ("00:50:56", "VMware (virtual)"),
        ("00:0C:29", "VMware (virtual)"),
        ("08:00:27", "VirtualBox (virtual)"),
        ("00:15:5D", "Hyper-V (virtual)"),
        ("00:17:88", "Philips Hue"),
        ("18:B4:30", "Nest"),
        ("00:1A:11", "Google"),
        ("F4:F5:D8", "Google"),
        ("3C:5A:B4", "Google"),
        ("44:65:0D", "Amazon"),
        ("FC:65:DE", "Amazon"),
        ("00:03:93", "Apple"),
        ("00:1B:63", "Apple"),
        ("00:11:32", "Synology"),
        ("00:08:9B", "QNAP"),
        ("24:5E:BE", "QNAP"),
        ("00:24:E4", "Withings"),
        ("B0:BE:76", "TP-Link"),
        ("50:C7:BF", "TP-Link"),
        ("00:1D:0F", "TP-Link"),
        ("00:09:5B", "Netgear"),
        ("00:14:6C", "Netgear"),
        ("00:18:0A", "Cisco Meraki"),
        ("00:1E:C2", "Apple"),
        ("00:80:77", "Brother"),
        ("00:1B:A9", "Brother"),
        ("00:00:48", "Epson"),
        ("00:26:AB", "Epson"),
        ("00:1E:0B", "HP"),
        ("00:25:B3", "HP"),
        ("00:04:4B", "NVIDIA"),
        ("00:D9:D1", "Sony"),
        ("00:1F:A7", "Sony"),
        ("7C:BB:8A", "Nintendo"),
        ("00:1F:32", "Nintendo"),
        ("00:22:48", "Microsoft"),
        ("00:50:F2", "Microsoft"),
        ("00:0D:3A", "Microsoft"),
        ("00:12:FB", "Samsung"),
        ("00:15:99", "Samsung"),
        ("00:0E:58", "Sonos"),
        ("5C:AA:FD", "Sonos"),
        ("94:9F:3E", "Sonos"),
        ("00:04:20", "Logitech"),
        ("24:0A:C4", "Espressif (smart device)"),
        ("30:AE:A4", "Espressif (smart device)"),
        ("84:F3:EB", "Espressif (smart device)"),
        ("A4:CF:12", "Espressif (smart device)"),
        ("2C:F4:32", "Espressif (smart device)"),
    ];
    for (p, name) in table {
        if prefix == *p {
            return name.to_string();
        }
    }
    // Locally administered bit set: the device is using a private address.
    if let Some(c) = up.chars().nth(1) {
        if matches!(c, '2' | '6' | 'A' | 'E') {
            return "Private address".into();
        }
    }
    String::new()
}
