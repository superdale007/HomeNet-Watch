//! What this computer itself is doing on the network, per operating system:
//! total bytes in/out, open internet connections and which program owns each.
//!
//! Windows: IP Helper (GetIfTable2, GetExtendedTcpTable) and, when running as
//! administrator, per-connection byte counts (TCP EStats).
//! Linux: /proc/net/dev, /proc/net/tcp and /proc/<pid>/fd.
//! macOS: `netstat -ibn` and `lsof`.

use std::net::{Ipv4Addr, SocketAddrV4};

#[derive(Clone, Debug)]
pub struct RawConn {
    pub pid: u32,
    pub local: SocketAddrV4,
    pub remote: SocketAddrV4,
}

#[derive(Clone, Debug, Default)]
pub struct ProcInfo {
    pub name: String,
    pub path: String,
    pub parent: Option<u32>,
}

// ───────────────────────────── Windows ─────────────────────────────
#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::NetworkManagement::IpHelper::*;

    pub fn iface_totals() -> Option<(u64, u64)> {
        unsafe {
            let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
            if GetIfTable2(&mut table) != 0 || table.is_null() {
                return None;
            }
            let n = (*table).NumEntries as usize;
            let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), n);
            let (mut rx, mut tx) = (0u64, 0u64);
            for r in rows {
                let flags = r.InterfaceAndOperStatusFlags._bitfield;
                let hardware = flags & 0x01 != 0;
                let filter = flags & 0x02 != 0;
                if hardware && !filter && r.OperStatus == 1 && r.Type != 24 {
                    rx += r.InOctets;
                    tx += r.OutOctets;
                }
            }
            FreeMibTable(table as *const _);
            Some((rx, tx))
        }
    }

    pub fn connections() -> Vec<RawConn> {
        let mut out = vec![];
        unsafe {
            let mut size: u32 = 0;
            GetExtendedTcpTable(std::ptr::null_mut(), &mut size, 0, 2, TCP_TABLE_OWNER_PID_ALL, 0);
            if size == 0 {
                return out;
            }
            let mut buf = vec![0u8; size as usize + 4096];
            size = buf.len() as u32;
            if GetExtendedTcpTable(buf.as_mut_ptr() as *mut _, &mut size, 0, 2, TCP_TABLE_OWNER_PID_ALL, 0) != 0 {
                return out;
            }
            let table = &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
            let rows = std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize);
            for r in rows {
                if r.dwState != 5 {
                    continue; // only ESTABLISHED
                }
                let port = |p: u32| u16::from_be((p & 0xFFFF) as u16);
                out.push(RawConn {
                    pid: r.dwOwningPid,
                    local: SocketAddrV4::new(Ipv4Addr::from(r.dwLocalAddr.to_ne_bytes()), port(r.dwLocalPort)),
                    remote: SocketAddrV4::new(Ipv4Addr::from(r.dwRemoteAddr.to_ne_bytes()), port(r.dwRemotePort)),
                });
            }
        }
        out
    }

    fn row(c: &RawConn) -> MIB_TCPROW_LH {
        MIB_TCPROW_LH {
            Anonymous: MIB_TCPROW_LH_0 { dwState: 5 },
            dwLocalAddr: u32::from_ne_bytes(c.local.ip().octets()),
            dwLocalPort: c.local.port().to_be() as u32,
            dwRemoteAddr: u32::from_ne_bytes(c.remote.ip().octets()),
            dwRemotePort: c.remote.port().to_be() as u32,
        }
    }

    /// Turn on byte counting for a connection. Only works as administrator.
    pub fn enable_bytes(c: &RawConn) -> bool {
        unsafe {
            let r = row(c);
            let rw = TCP_ESTATS_DATA_RW_v0 { EnableCollection: 1 };
            SetPerTcpConnectionEStats(&r, TcpConnectionEstatsData, &rw as *const _ as *const u8, 0, std::mem::size_of::<TCP_ESTATS_DATA_RW_v0>() as u32, 0) == 0
        }
    }

    /// (bytes in, bytes out) so far on this connection.
    pub fn conn_bytes(c: &RawConn) -> Option<(u64, u64)> {
        unsafe {
            let r = row(c);
            let mut rod: TCP_ESTATS_DATA_ROD_v0 = std::mem::zeroed();
            let rc = GetPerTcpConnectionEStats(
                &r,
                TcpConnectionEstatsData,
                std::ptr::null_mut(),
                0,
                0,
                std::ptr::null_mut(),
                0,
                0,
                &mut rod as *mut _ as *mut u8,
                0,
                std::mem::size_of::<TCP_ESTATS_DATA_ROD_v0>() as u32,
            );
            if rc == 0 {
                Some((rod.DataBytesIn, rod.DataBytesOut))
            } else {
                None
            }
        }
    }

    pub fn processes() -> std::collections::HashMap<u32, ProcInfo> {
        use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
        use windows_sys::Win32::System::Threading::*;
        let mut map = std::collections::HashMap::new();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap.is_null() || snap as isize == -1 {
                return map;
            }
            let mut e: PROCESSENTRY32W = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut e) != 0;
            while ok {
                let end = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
                let name = String::from_utf16_lossy(&e.szExeFile[..end]);
                let mut path = String::new();
                let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, e.th32ProcessID);
                if !h.is_null() {
                    let mut buf = [0u16; 1024];
                    let mut len = buf.len() as u32;
                    if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0 {
                        path = String::from_utf16_lossy(&buf[..len as usize]);
                    }
                    CloseHandle(h);
                }
                let name = match e.th32ProcessID {
                    0 => "System Idle".to_string(),
                    4 => "Windows (System)".to_string(),
                    _ => name,
                };
                map.insert(e.th32ProcessID, ProcInfo { name, path, parent: Some(e.th32ParentProcessID) });
                ok = Process32NextW(snap, &mut e) != 0;
            }
            CloseHandle(snap);
        }
        map
    }
}

// ───────────────────────────── Linux ─────────────────────────────
#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::collections::HashMap;

    pub fn iface_totals() -> Option<(u64, u64)> {
        let text = std::fs::read_to_string("/proc/net/dev").ok()?;
        let (mut rx, mut tx) = (0u64, 0u64);
        for line in text.lines().skip(2) {
            let Some((name, rest)) = line.split_once(':') else { continue };
            let name = name.trim();
            if name == "lo" || ["docker", "veth", "br-", "virbr", "vnet", "tun", "tap", "wg", "zt"].iter().any(|p| name.starts_with(p)) {
                continue;
            }
            let f: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            if f.len() >= 9 {
                rx += f[0];
                tx += f[8];
            }
        }
        Some((rx, tx))
    }

    fn hex_addr(s: &str) -> Option<SocketAddrV4> {
        let (a, p) = s.split_once(':')?;
        let ip = u32::from_str_radix(a, 16).ok()?;
        let port = u16::from_str_radix(p, 16).ok()?;
        Some(SocketAddrV4::new(Ipv4Addr::from(ip.to_le_bytes()), port))
    }

    pub fn connections() -> Vec<RawConn> {
        // socket inode -> pid (only for processes we're allowed to look at)
        let mut owner: HashMap<String, u32> = HashMap::new();
        if let Ok(dir) = std::fs::read_dir("/proc") {
            for e in dir.flatten() {
                let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
                let Ok(fds) = std::fs::read_dir(e.path().join("fd")) else { continue };
                for fd in fds.flatten() {
                    if let Ok(link) = std::fs::read_link(fd.path()) {
                        let l = link.to_string_lossy();
                        if let Some(inode) = l.strip_prefix("socket:[").and_then(|x| x.strip_suffix(']')) {
                            owner.insert(inode.to_string(), pid);
                        }
                    }
                }
            }
        }
        let mut out = vec![];
        let Ok(text) = std::fs::read_to_string("/proc/net/tcp") else { return out };
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 10 || f[3] != "01" {
                continue;
            }
            let (Some(local), Some(remote)) = (hex_addr(f[1]), hex_addr(f[2])) else { continue };
            let pid = owner.get(f[9]).cloned().unwrap_or(0);
            out.push(RawConn { pid, local, remote });
        }
        out
    }

    pub fn enable_bytes(_c: &RawConn) -> bool {
        false
    }
    pub fn conn_bytes(_c: &RawConn) -> Option<(u64, u64)> {
        None
    }

    pub fn processes() -> HashMap<u32, ProcInfo> {
        let mut map = HashMap::new();
        if let Ok(dir) = std::fs::read_dir("/proc") {
            for e in dir.flatten() {
                let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
                let name = std::fs::read_to_string(e.path().join("comm")).unwrap_or_default().trim().to_string();
                let path = std::fs::read_link(e.path().join("exe")).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                let parent = std::fs::read_to_string(e.path().join("stat"))
                    .ok()
                    .and_then(|s| s.rsplit_once(')').map(|(_, r)| r.split_whitespace().nth(1).and_then(|x| x.parse().ok())))
                    .flatten();
                map.insert(pid, ProcInfo { name, path, parent });
            }
        }
        map
    }
}

// ───────────────────────────── macOS ─────────────────────────────
#[cfg(all(unix, not(target_os = "linux")))]
mod imp {
    use super::*;
    use std::collections::HashMap;
    use std::process::{Command, Stdio};

    fn run(cmd: &str, args: &[&str]) -> String {
        Command::new(cmd)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default()
    }

    pub fn iface_totals() -> Option<(u64, u64)> {
        // Name Mtu Network Address Ipkts Ierrs Ibytes Opkts Oerrs Obytes Coll
        let text = run("netstat", &["-ibn"]);
        let (mut rx, mut tx) = (0u64, 0u64);
        let mut any = false;
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 10 || !f[2].starts_with("<Link#") {
                continue;
            }
            let n = f[0];
            if ["lo", "utun", "awdl", "llw", "bridge", "gif", "stf", "anpi", "ap", "XHC"].iter().any(|p| n.starts_with(p)) {
                continue;
            }
            // with a hardware address there are 11 columns, without one 10
            let off = if f.len() >= 11 { 0 } else { 1 };
            if let (Ok(i), Ok(o)) = (f[6 - off].parse::<u64>(), f[9 - off].parse::<u64>()) {
                rx += i;
                tx += o;
                any = true;
            }
        }
        any.then_some((rx, tx))
    }

    pub fn connections() -> Vec<RawConn> {
        let text = run("lsof", &["-nP", "-iTCP", "-sTCP:ESTABLISHED", "-Fpn"]);
        let mut out = vec![];
        let mut pid = 0u32;
        for line in text.lines() {
            if let Some(p) = line.strip_prefix('p') {
                pid = p.parse().unwrap_or(0);
            } else if let Some(n) = line.strip_prefix('n') {
                // 192.168.1.5:50123->17.253.1.2:443
                if let Some((l, r)) = n.split_once("->") {
                    if let (Ok(local), Ok(remote)) = (l.parse::<SocketAddrV4>(), r.parse::<SocketAddrV4>()) {
                        out.push(RawConn { pid, local, remote });
                    }
                }
            }
        }
        out
    }

    pub fn enable_bytes(_c: &RawConn) -> bool {
        false
    }
    pub fn conn_bytes(_c: &RawConn) -> Option<(u64, u64)> {
        None
    }

    pub fn processes() -> HashMap<u32, ProcInfo> {
        let text = run("ps", &["-axo", "pid=,ppid=,comm="]);
        let mut map = HashMap::new();
        for line in text.lines() {
            let mut it = line.split_whitespace();
            let (Some(pid), Some(ppid)) = (it.next().and_then(|x| x.parse().ok()), it.next().and_then(|x| x.parse().ok())) else { continue };
            let path: String = it.collect::<Vec<_>>().join(" ");
            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
            map.insert(pid, ProcInfo { name, path, parent: Some(ppid) });
        }
        map
    }
}

pub use imp::*;

/// Is this an address on the home network (or this computer)?
pub fn is_local(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() || ip.is_multicast() || ip.octets()[0] == 100 && ip.octets()[1] & 0xC0 == 64
}
