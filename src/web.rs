//! Web requests (http and https) through Windows' own WinHTTP, so secure
//! connections work without bundling a TLS library.

#[derive(Default)]
pub struct Opts<'a> {
    pub method: &'a str,
    pub body: Option<&'a [u8]>,
    pub content_type: Option<&'a str>,
    pub timeout_ms: i32,
    /// Called with the number of bytes received so far (download progress).
    pub on_bytes: Option<&'a mut dyn FnMut(u64) -> bool>,
    /// Don't keep the body (speed tests), only count it.
    pub discard: bool,
}

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    #[allow(dead_code)] // used by the speed test (coming next)
    pub bytes: u64,
}

fn split_url(url: &str) -> Result<(bool, String, u16, String), String> {
    let (secure, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err("address must start with http:// or https://".into());
    };
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().map_err(|_| "bad port".to_string())?),
        None => (hostport.to_string(), if secure { 443 } else { 80 }),
    };
    Ok((secure, host, port, path))
}

pub fn get(url: &str, timeout_ms: i32) -> Result<Response, String> {
    request(url, Opts { method: "GET", timeout_ms, ..Default::default() })
}

#[cfg(windows)]
pub fn request(url: &str, mut o: Opts) -> Result<Response, String> {
    use windows_sys::Win32::Networking::WinHttp::*;
    let (secure, host, port, path) = split_url(url)?;
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let agent = w("HomeNetWatch/1.0");
    let whost = w(&host);
    let wpath = w(&path);
    let wmethod = w(if o.method.is_empty() { "GET" } else { o.method });
    unsafe {
        let session = WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, std::ptr::null(), std::ptr::null(), 0);
        let session = if session.is_null() {
            WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, std::ptr::null(), std::ptr::null(), 0)
        } else {
            session
        };
        if session.is_null() {
            return Err("could not start a web session".into());
        }
        let t = if o.timeout_ms > 0 { o.timeout_ms } else { 15000 };
        WinHttpSetTimeouts(session, t, t, t, t);
        let conn = WinHttpConnect(session, whost.as_ptr(), port, 0);
        if conn.is_null() {
            WinHttpCloseHandle(session);
            return Err("could not reach the server".into());
        }
        let req = WinHttpOpenRequest(conn, wmethod.as_ptr(), wpath.as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null(), if secure { WINHTTP_FLAG_SECURE } else { 0 });
        let close = || {
            WinHttpCloseHandle(conn);
            WinHttpCloseHandle(session);
        };
        if req.is_null() {
            close();
            return Err("could not prepare the request".into());
        }
        let headers = o.content_type.map(|c| w(&format!("Content-Type: {}\r\n", c)));
        let (hp, hl) = match &headers {
            Some(h) => (h.as_ptr(), (h.len() - 1) as u32),
            None => (std::ptr::null(), 0),
        };
        let (bp, bl) = match o.body {
            Some(b) => (b.as_ptr() as *const core::ffi::c_void, b.len() as u32),
            None => (std::ptr::null(), 0),
        };
        let ok = WinHttpSendRequest(req, hp, hl, bp, bl, bl, 0) != 0 && WinHttpReceiveResponse(req, std::ptr::null_mut()) != 0;
        if !ok {
            let err = std::io::Error::last_os_error();
            WinHttpCloseHandle(req);
            close();
            return Err(format!("no answer from {} ({})", host, err.raw_os_error().unwrap_or(0)));
        }
        let mut status: u32 = 0;
        let mut size = 4u32;
        WinHttpQueryHeaders(req, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, std::ptr::null(), &mut status as *mut u32 as *mut _, &mut size, std::ptr::null_mut());
        let mut body = Vec::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut total: u64 = 0;
        loop {
            let mut n = 0u32;
            if WinHttpReadData(req, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut n) == 0 || n == 0 {
                break;
            }
            total += n as u64;
            if !o.discard {
                body.extend_from_slice(&buf[..n as usize]);
                if body.len() > 256 * 1024 * 1024 {
                    break;
                }
            }
            if let Some(cb) = o.on_bytes.as_mut() {
                if !cb(total) {
                    break;
                }
            }
        }
        WinHttpCloseHandle(req);
        close();
        Ok(Response { status: status as u16, body, bytes: total })
    }
}

#[cfg(not(windows))]
pub fn request(url: &str, o: Opts) -> Result<Response, String> {
    let _ = split_url(url)?;
    crate::unix::curl(url, o)
}

/// This home's public (outside) address, as the internet sees it.
pub fn external_ip() -> Result<String, String> {
    for url in ["https://api.ipify.org", "https://ipv4.icanhazip.com"] {
        if let Ok(r) = get(url, 8000) {
            let t = String::from_utf8_lossy(&r.body).trim().to_string();
            if r.status == 200 && t.parse::<std::net::Ipv4Addr>().is_ok() {
                return Ok(t);
            }
        }
    }
    Err("couldn't find the outside address".into())
}

#[cfg(test)]
mod tests {
    // Needs the internet (a few kilobytes): cargo test -- --ignored
    #[test]
    #[ignore]
    fn speed_endpoints() {
        let mut got = 0u64;
        let mut cb = |n: u64| {
            got = n;
            true
        };
        let r = super::request(
            "https://speed.cloudflare.com/__down?bytes=50000",
            super::Opts { method: "GET", timeout_ms: 8000, on_bytes: Some(&mut cb), discard: true, ..Default::default() },
        );
        println!("down: {:?} bytes={}", r.as_ref().map(|r| (r.status, r.bytes)), got);
        assert!(r.is_ok());
        let body = vec![b'0'; 20000];
        let u = super::request(
            "https://speed.cloudflare.com/__up",
            super::Opts { method: "POST", body: Some(&body), content_type: Some("application/octet-stream"), timeout_ms: 8000, ..Default::default() },
        );
        println!("up: {:?}", u.as_ref().map(|r| r.status));
        assert!(u.is_ok());
    }
}
