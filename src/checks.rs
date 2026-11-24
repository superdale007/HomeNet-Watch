//! Extra checks beyond ping and ports: a web page check (plain HTTP)
//! and a DNS lookup check.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

/// Fetch a web page over plain HTTP. Ok((status code, ms)) when the page loads
/// with a 2xx/3xx status and contains `expect` (if given).
pub fn http_check(ip: Ipv4Addr, port: u16, path: &str, expect: &str, timeout_ms: u32) -> Result<(u16, f32), String> {
    let start = Instant::now();
    let to = Duration::from_millis(timeout_ms.max(500) as u64 * 3);
    let mut s = TcpStream::connect_timeout(&SocketAddr::V4(SocketAddrV4::new(ip, port)), to).map_err(|_| "no connection".to_string())?;
    s.set_read_timeout(Some(to)).ok();
    s.set_write_timeout(Some(to)).ok();
    let path = if path.starts_with('/') { path.to_string() } else { format!("/{}", path) };
    let req = format!("GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: HomeNetWatch/1.0\r\nAccept: */*\r\nConnection: close\r\n\r\n", path, ip);
    s.write_all(req.as_bytes()).map_err(|_| "could not send request".to_string())?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > 512 * 1024 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let ms = start.elapsed().as_secs_f32() * 1000.0;
    let text = String::from_utf8_lossy(&buf);
    let first = text.lines().next().unwrap_or("");
    if !first.starts_with("HTTP/") {
        return Err("not a web server (or it needs https)".into());
    }
    let code: u16 = first.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    if !(200..400).contains(&code) {
        return Err(format!("page answered with error {}", code));
    }
    if !expect.trim().is_empty() && !text.to_lowercase().contains(&expect.trim().to_lowercase()) {
        return Err(format!("page loaded but doesn't contain \"{}\"", expect.trim()));
    }
    Ok((code, ms))
}

/// Ask `server` to look up `name`. Ok((ms, first address)).
pub fn dns_check(server: Ipv4Addr, name: &str, timeout_ms: u32) -> Result<(f32, Option<Ipv4Addr>), String> {
    let name = name.trim().trim_end_matches('.');
    if name.is_empty() {
        return Err("no name to look up".into());
    }
    let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    sock.connect(SocketAddrV4::new(server, 53)).map_err(|e| e.to_string())?;
    sock.set_read_timeout(Some(Duration::from_millis(timeout_ms.max(500) as u64 * 2))).ok();
    let id: u16 = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(7) & 0xFFFF) as u16;
    let mut q = vec![];
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err("name not valid".into());
        }
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.extend_from_slice(&[0, 0, 1, 0, 1]);
    let start = Instant::now();
    let mut buf = [0u8; 1500];
    for _ in 0..2 {
        sock.send(&q).map_err(|e| e.to_string())?;
        let Ok(n) = sock.recv(&mut buf) else { continue };
        let ms = start.elapsed().as_secs_f32() * 1000.0;
        let r = &buf[..n];
        if n < 12 || r[0..2] != id.to_be_bytes() {
            continue;
        }
        let rcode = r[3] & 0x0F;
        let ancount = u16::from_be_bytes([r[6], r[7]]);
        if rcode == 3 {
            return Err(format!("\"{}\" doesn't exist", name));
        }
        if rcode != 0 {
            return Err(format!("lookup failed (code {})", rcode));
        }
        if ancount == 0 {
            return Err("no address in the answer".into());
        }
        // skip the question
        let mut p = 12;
        p = skip_name(r, p).ok_or("bad answer")? + 4;
        for _ in 0..ancount {
            p = skip_name(r, p).ok_or("bad answer")?;
            if p + 10 > n {
                break;
            }
            let ty = u16::from_be_bytes([r[p], r[p + 1]]);
            let len = u16::from_be_bytes([r[p + 8], r[p + 9]]) as usize;
            p += 10;
            if ty == 1 && len == 4 && p + 4 <= n {
                return Ok((ms, Some(Ipv4Addr::new(r[p], r[p + 1], r[p + 2], r[p + 3]))));
            }
            p += len;
        }
        return Ok((ms, None));
    }
    Err("the DNS server didn't answer".into())
}

fn skip_name(r: &[u8], mut p: usize) -> Option<usize> {
    loop {
        let l = *r.get(p)? as usize;
        if l == 0 {
            return Some(p + 1);
        }
        if l & 0xC0 == 0xC0 {
            return Some(p + 2);
        }
        p += 1 + l;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Needs a network: cargo test -- --ignored
    #[test]
    #[ignore]
    fn live_dns_and_web() {
        let r = dns_check(Ipv4Addr::new(1, 1, 1, 1), "example.com", 1000);
        println!("dns: {:?}", r);
        assert!(r.is_ok());
        let w = http_check(Ipv4Addr::new(10, 10, 10, 1), 80, "/", "", 1000);
        println!("web: {:?}", w);
        println!("nas web: {:?}", http_check(Ipv4Addr::new(10, 10, 10, 155), 5000, "/", "", 1000));
    }
}
