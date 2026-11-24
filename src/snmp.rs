//! A small SNMP v2c client (GET and GETBULK walks) over UDP, with just enough
//! BER encoding/decoding for reading device statistics.

use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::Duration;

pub type Oid = Vec<u32>;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i64),
    Str(Vec<u8>),
    Oid(Oid),
    Ip([u8; 4]),
    /// Counter32, Gauge32, TimeTicks, Counter64
    Num(u64),
    Null,
    /// noSuchObject / noSuchInstance / endOfMibView
    Missing,
    End,
}

impl Value {
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Int(i) if *i >= 0 => Some(*i as u64),
            Value::Num(n) => Some(*n),
            Value::Str(s) => String::from_utf8_lossy(s).trim().parse().ok(),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Num(n) => Some(*n as i64),
            Value::Str(s) => String::from_utf8_lossy(s).trim().parse().ok(),
            _ => None,
        }
    }
    pub fn as_string(&self) -> Option<String> {
        match self {
            Value::Str(s) => Some(String::from_utf8_lossy(s).trim_matches(char::from(0)).trim().to_string()),
            Value::Int(i) => Some(i.to_string()),
            Value::Num(n) => Some(n.to_string()),
            Value::Oid(o) => Some(oid_str(o)),
            _ => None,
        }
    }
    /// First number found in a value, e.g. "41 C/105 F" -> 41, "12.5 %" -> 12.5
    pub fn first_number(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Num(n) => Some(*n as f64),
            Value::Str(_) => {
                let s = self.as_string()?;
                let start = s.find(|c: char| c.is_ascii_digit() || c == '-')?;
                let num: String = s[start..].chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
                num.parse().ok()
            }
            _ => None,
        }
    }
    pub fn present(&self) -> bool {
        !matches!(self, Value::Missing | Value::End | Value::Null)
    }
}

pub fn oid(s: &str) -> Oid {
    s.trim_start_matches('.').split('.').filter_map(|p| p.parse().ok()).collect()
}

pub fn oid_str(o: &[u32]) -> String {
    o.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(".")
}

// ── BER encoding ───────────────────────────────────────────────

fn enc_len(len: usize, out: &mut Vec<u8>) {
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes = (len as u32).to_be_bytes();
        let skip = bytes.iter().take_while(|b| **b == 0).count();
        out.push(0x80 | (4 - skip) as u8);
        out.extend_from_slice(&bytes[skip..]);
    }
}

fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut v = vec![tag];
    enc_len(content.len(), &mut v);
    v.extend_from_slice(content);
    v
}

fn enc_int(i: i64) -> Vec<u8> {
    let bytes = i.to_be_bytes();
    let mut start = 0;
    while start < 7 {
        let b = bytes[start];
        let next = bytes[start + 1];
        if (b == 0 && next & 0x80 == 0) || (b == 0xFF && next & 0x80 != 0) {
            start += 1;
        } else {
            break;
        }
    }
    tlv(0x02, &bytes[start..])
}

fn enc_oid(o: &[u32]) -> Vec<u8> {
    let mut c = vec![];
    if o.len() >= 2 {
        c.push((o[0] * 40 + o[1]) as u8);
    }
    for &arc in o.iter().skip(2) {
        let mut stack = vec![(arc & 0x7F) as u8];
        let mut v = arc >> 7;
        while v > 0 {
            stack.push(((v & 0x7F) as u8) | 0x80);
            v >>= 7;
        }
        stack.reverse();
        c.extend(stack);
    }
    tlv(0x06, &c)
}

/// The PDU itself (GET / GETBULK with NULL values), as a complete TLV.
fn pdu(pdu_tag: u8, req_id: i32, a: i64, b: i64, oids: &[Oid]) -> Vec<u8> {
    let mut vbs = vec![];
    for o in oids {
        let mut vb = enc_oid(o);
        vb.extend_from_slice(&[0x05, 0x00]);
        vbs.extend(tlv(0x30, &vb));
    }
    let mut p = enc_int(req_id as i64);
    p.extend(enc_int(a));
    p.extend(enc_int(b));
    p.extend(tlv(0x30, &vbs));
    tlv(pdu_tag, &p)
}

#[cfg(test)]
fn build(community: &str, pdu_tag: u8, req_id: i32, a: i64, b: i64, oids: &[Oid]) -> Vec<u8> {
    build_v2c(community, &pdu(pdu_tag, req_id, a, b, oids))
}

fn build_v2c(community: &str, pdu: &[u8]) -> Vec<u8> {
    let mut msg = enc_int(1); // version 2c
    msg.extend(tlv(0x04, community.as_bytes()));
    msg.extend_from_slice(pdu);
    tlv(0x30, &msg)
}

// ── BER decoding ───────────────────────────────────────────────

struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn tlv(&mut self) -> Option<(u8, &'a [u8])> {
        let tag = *self.b.get(self.p)?;
        self.p += 1;
        let first = *self.b.get(self.p)?;
        self.p += 1;
        let len = if first & 0x80 == 0 {
            first as usize
        } else {
            let n = (first & 0x7F) as usize;
            if n == 0 || n > 4 {
                return None;
            }
            let mut l = 0usize;
            for _ in 0..n {
                l = (l << 8) | *self.b.get(self.p)? as usize;
                self.p += 1;
            }
            l
        };
        let end = self.p.checked_add(len)?;
        let content = self.b.get(self.p..end)?;
        self.p = end;
        Some((tag, content))
    }
}

fn dec_int(c: &[u8]) -> i64 {
    let mut v: i64 = if c.first().map(|b| b & 0x80 != 0).unwrap_or(false) { -1 } else { 0 };
    for b in c.iter().take(8) {
        v = (v << 8) | *b as i64;
    }
    v
}

fn dec_uint(c: &[u8]) -> u64 {
    let mut v: u64 = 0;
    for b in c.iter().skip_while(|b| **b == 0).take(8) {
        v = (v << 8) | *b as u64;
    }
    v
}

fn dec_oid(c: &[u8]) -> Oid {
    let mut o = vec![];
    if let Some(f) = c.first() {
        o.push((*f / 40) as u32);
        o.push((*f % 40) as u32);
    }
    let mut v: u32 = 0;
    for b in c.iter().skip(1) {
        v = (v << 7) | (b & 0x7F) as u32;
        if b & 0x80 == 0 {
            o.push(v);
            v = 0;
        }
    }
    o
}

/// v2c reply: check it answers our request and return its values.
fn parse_response(buf: &[u8], want_id: i32) -> Result<Vec<(Oid, Value)>, String> {
    let mut top = Rd { b: buf, p: 0 };
    let (_, msg) = top.tlv().ok_or("bad reply")?;
    let mut m = Rd { b: msg, p: 0 };
    m.tlv().ok_or("bad reply")?; // version
    m.tlv().ok_or("bad reply")?; // community
    let (tag, pdu) = m.tlv().ok_or("bad reply")?;
    if tag != 0xA2 {
        return Err("unexpected reply".into());
    }
    decode_pdu(pdu, Some(want_id))
}

/// Values inside a Response or Report PDU (content after the tag/length).
fn decode_pdu(pdu: &[u8], want_id: Option<i32>) -> Result<Vec<(Oid, Value)>, String> {
    let mut p = Rd { b: pdu, p: 0 };
    let (_, id) = p.tlv().ok_or("bad reply")?;
    if let Some(w) = want_id {
        if dec_int(id) as i32 != w {
            return Err("reply for another request".into());
        }
    }
    let (_, es) = p.tlv().ok_or("bad reply")?;
    p.tlv().ok_or("bad reply")?;
    let err = dec_int(es);
    if err != 0 {
        return Err(format!("device refused the request (error {})", err));
    }
    let (_, vbl) = p.tlv().ok_or("bad reply")?;
    let mut l = Rd { b: vbl, p: 0 };
    let mut out = vec![];
    while l.p < vbl.len() {
        let (_, vb) = l.tlv().ok_or("bad reply")?;
        let mut r = Rd { b: vb, p: 0 };
        let (_, o) = r.tlv().ok_or("bad reply")?;
        let (t, c) = r.tlv().ok_or("bad reply")?;
        let v = match t {
            0x02 => Value::Int(dec_int(c)),
            0x04 => Value::Str(c.to_vec()),
            0x06 => Value::Oid(dec_oid(c)),
            0x40 if c.len() == 4 => Value::Ip([c[0], c[1], c[2], c[3]]),
            0x41 | 0x42 | 0x43 | 0x46 => Value::Num(dec_uint(c)),
            0x44 => Value::Str(c.to_vec()),
            0x05 => Value::Null,
            0x80 | 0x81 => Value::Missing,
            0x82 => Value::End,
            _ => Value::Null,
        };
        out.push((dec_oid(o), v));
    }
    Ok(out)
}

// ── client ─────────────────────────────────────────────────────

/// Plain-language meaning of an SNMP v3 refusal ("report").
fn report_text(oid: &str) -> String {
    let n = oid.strip_prefix(USM_STATS).and_then(|r| r.trim_start_matches('.').split('.').next()).and_then(|x| x.parse::<u32>().ok());
    match n {
        Some(1) => "the device wants a different security level (check the authentication and privacy settings)".into(),
        Some(3) => "the device doesn't know this SNMP v3 user name".into(),
        Some(5) => "wrong authentication password or protocol".into(),
        Some(6) => "wrong privacy (encryption) password or protocol".into(),
        _ => format!("the device refused the request ({})", oid),
    }
}

use crate::snmpv3::{AuthProto, PrivProto};

/// How to sign in to a device's SNMP service.
#[derive(Clone, Debug)]
pub enum Security {
    /// v1/v2c: a shared "community" word, sent unencrypted
    V2c(String),
    /// v3: a user name, with optional authentication and privacy (encryption)
    V3 { user: String, auth: AuthProto, auth_pass: String, privp: PrivProto, priv_pass: String },
}

struct Engine {
    id: Vec<u8>,
    boots: u32,
    time: u32,
    at: std::time::Instant,
    auth_key: Vec<u8>,
    priv_key: Vec<u8>,
}

enum V3Reply {
    Data(Vec<(Oid, Value)>),
    /// The agent answered with a Report (e.g. our clock is off): which counter, and its engine info
    Report(Oid, Vec<u8>, u32, u32),
}

const USM_STATS: &str = "1.3.6.1.6.3.15.1.1";

pub struct Client {
    sock: UdpSocket,
    sec: Security,
    next_id: i32,
    engine: Option<Engine>,
    salt: u64,
}

impl Client {
    pub fn new(ip: Ipv4Addr, sec: Security, timeout_ms: u64) -> Result<Self, String> {
        let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        sock.connect(SocketAddrV4::new(ip, 161)).map_err(|e| e.to_string())?;
        sock.set_read_timeout(Some(Duration::from_millis(timeout_ms))).map_err(|e| e.to_string())?;
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1);
        let seed = ((nanos as u64 & 0x3FFF_FFFF) as i32).max(1);
        Ok(Client { sock, sec, next_id: seed, engine: None, salt: nanos as u64 })
    }

    fn next_id(&mut self) -> i32 {
        self.next_id = self.next_id.wrapping_add(1) & 0x3FFF_FFFF;
        self.next_id.max(1)
    }

    /// Send `pkt` and wait for an answer that `parse` accepts (two tries).
    fn exchange<T>(&self, pkt: &[u8], mut parse: impl FnMut(&[u8]) -> Result<T, String>) -> Result<T, String> {
        let mut buf = vec![0u8; 65535];
        for _attempt in 0..2 {
            self.sock.send(pkt).map_err(|e| e.to_string())?;
            loop {
                match self.sock.recv(&mut buf) {
                    Ok(n) => match parse(&buf[..n]) {
                        Err(e) if e == "reply for another request" => continue,
                        other => return other,
                    },
                    Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => break,
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
        Err(match self.sec {
            Security::V2c(_) => "no answer (is SNMP turned on, and is the community name right?)".into(),
            Security::V3 { .. } => "no answer (is SNMP v3 turned on for this device?)".into(),
        })
    }

    fn request(&mut self, tag: u8, a: i64, b: i64, oids: &[Oid]) -> Result<Vec<(Oid, Value)>, String> {
        if let Security::V2c(community) = self.sec.clone() {
            let id = self.next_id();
            let pkt = build_v2c(&community, &pdu(tag, id, a, b, oids));
            return self.exchange(&pkt, |r| parse_response(r, id));
        }
        // SNMP v3
        if self.engine.is_none() {
            self.discover()?;
        }
        for _ in 0..3 {
            let id = self.next_id();
            let pkt = self.build_v3(&pdu(tag, id, a, b, oids), id)?;
            let reply = self.exchange(&pkt, |r| self.parse_v3(r, id))?;
            match reply {
                V3Reply::Data(v) => return Ok(v),
                V3Reply::Report(oid, eid, boots, time) => {
                    let which = oid_str(&oid);
                    if which.starts_with(&format!("{}.2", USM_STATS)) {
                        // not in time window: take the device's clock and try again
                        if let Some(e) = self.engine.as_mut() {
                            e.boots = boots;
                            e.time = time;
                            e.at = std::time::Instant::now();
                        }
                        continue;
                    }
                    if which.starts_with(&format!("{}.4", USM_STATS)) {
                        let _ = eid;
                        self.engine = None;
                        self.discover()?;
                        continue;
                    }
                    return Err(report_text(&which));
                }
            }
        }
        Err("the device kept refusing the request".into())
    }

    /// Ask the device for its engine ID, boot count and clock (RFC 3414 4).
    fn discover(&mut self) -> Result<(), String> {
        let id = self.next_id();
        let pkt = self.build_v3(&pdu(0xA0, id, 0, 0, &[]), id)?;
        let reply = self.exchange(&pkt, |r| self.parse_v3(r, id))?;
        let (eid, boots, time) = match reply {
            V3Reply::Report(_, eid, b, t) => (eid, b, t),
            V3Reply::Data(_) => return Err("the device didn't share its SNMP v3 engine ID".into()),
        };
        if eid.is_empty() {
            return Err("the device didn't share its SNMP v3 engine ID".into());
        }
        let Security::V3 { auth, auth_pass, priv_pass, .. } = &self.sec else { return Ok(()) };
        self.engine = Some(Engine {
            auth_key: auth.localize(auth_pass, &eid),
            priv_key: auth.localize(priv_pass, &eid),
            id: eid,
            boots,
            time,
            at: std::time::Instant::now(),
        });
        Ok(())
    }

    fn build_v3(&mut self, pdu: &[u8], msg_id: i32) -> Result<Vec<u8>, String> {
        let Security::V3 { user, auth, privp, .. } = &self.sec else { return Err("not v3".into()) };
        let (user, auth, privp) = (user.clone(), *auth, *privp);
        let (eid, boots, time, akey, pkey) = match &self.engine {
            Some(e) => (e.id.clone(), e.boots, e.time.saturating_add(e.at.elapsed().as_secs() as u32), e.auth_key.clone(), e.priv_key.clone()),
            None => (vec![], 0, 0, vec![], vec![]),
        };
        let discovering = self.engine.is_none();
        let use_auth = !discovering && auth != AuthProto::None;
        let use_priv = use_auth && privp != PrivProto::None;
        let mut scoped = tlv(0x04, &eid);
        scoped.extend(tlv(0x04, b""));
        scoped.extend_from_slice(pdu);
        let scoped = tlv(0x30, &scoped);
        let (data, priv_params) = if use_priv {
            self.salt = self.salt.wrapping_add(1);
            let (c, salt) = privp.encrypt(&pkey, boots, time, self.salt, &scoped)?;
            (tlv(0x04, &c), salt)
        } else {
            (scoped, vec![])
        };
        let flags: u8 = 0x04 | if use_auth { 0x01 } else { 0 } | if use_priv { 0x02 } else { 0 };
        let mut header = enc_int(msg_id as i64);
        header.extend(enc_int(65507));
        header.extend(tlv(0x04, &[flags]));
        header.extend(enc_int(3));
        let header = tlv(0x30, &header);
        let user_b = if discovering { vec![] } else { user.into_bytes() };
        let make = |auth_params: &[u8]| {
            let mut usm = tlv(0x04, &eid);
            usm.extend(enc_int(boots as i64));
            usm.extend(enc_int(time as i64));
            usm.extend(tlv(0x04, &user_b));
            usm.extend(tlv(0x04, auth_params));
            usm.extend(tlv(0x04, &priv_params));
            let mut msg = enc_int(3);
            msg.extend_from_slice(&header);
            msg.extend(tlv(0x04, &tlv(0x30, &usm)));
            msg.extend_from_slice(&data);
            tlv(0x30, &msg)
        };
        if use_auth {
            let unsigned = make(&vec![0u8; auth.mac_len()]);
            let sig = auth.sign(&akey, &unsigned);
            Ok(make(&sig))
        } else {
            Ok(make(&[]))
        }
    }

    fn parse_v3(&self, buf: &[u8], want_id: i32) -> Result<V3Reply, String> {
        let bad = || "bad reply".to_string();
        let mut top = Rd { b: buf, p: 0 };
        let (_, msg) = top.tlv().ok_or_else(bad)?;
        let mut m = Rd { b: msg, p: 0 };
        let (_, ver) = m.tlv().ok_or_else(bad)?;
        if dec_int(ver) != 3 {
            return Err("reply for another request".into());
        }
        let (_, header) = m.tlv().ok_or_else(bad)?;
        let mut h = Rd { b: header, p: 0 };
        let (_, mid) = h.tlv().ok_or_else(bad)?;
        if dec_int(mid) as i32 != want_id {
            return Err("reply for another request".into());
        }
        h.tlv().ok_or_else(bad)?; // max size
        let (_, fl) = h.tlv().ok_or_else(bad)?;
        let flags = *fl.first().unwrap_or(&0);
        let (_, secp) = m.tlv().ok_or_else(bad)?;
        let mut sp = Rd { b: secp, p: 0 };
        let (_, usm) = sp.tlv().ok_or_else(bad)?;
        let mut u = Rd { b: usm, p: 0 };
        let (_, eid) = u.tlv().ok_or_else(bad)?;
        let (_, boots) = u.tlv().ok_or_else(bad)?;
        let (_, time) = u.tlv().ok_or_else(bad)?;
        u.tlv().ok_or_else(bad)?; // user name
        let (_, authp) = u.tlv().ok_or_else(bad)?;
        let (_, privp_params) = u.tlv().ok_or_else(bad)?;
        let (dtag, data) = m.tlv().ok_or_else(bad)?;
        let (boots, time) = (dec_int(boots) as u32, dec_int(time) as u32);

        // Check the device's signature when it signed the reply.
        if flags & 0x01 != 0 {
            if let (Some(e), Security::V3 { auth, .. }) = (&self.engine, &self.sec) {
                let off = authp.as_ptr() as usize - buf.as_ptr() as usize;
                let mut zeroed = buf.to_vec();
                for b in &mut zeroed[off..off + authp.len()] {
                    *b = 0;
                }
                if auth.sign(&e.auth_key, &zeroed) != authp {
                    return Err("the device's answer failed the signature check (wrong authentication password or protocol?)".into());
                }
            }
        }
        let scoped: Vec<u8> = if flags & 0x02 != 0 && dtag == 0x04 {
            let (Some(e), Security::V3 { privp, .. }) = (&self.engine, &self.sec) else { return Err(bad()) };
            privp.decrypt(&e.priv_key, boots, time, privp_params, data)?
        } else {
            tlv(dtag, data)
        };
        let mut sr = Rd { b: &scoped, p: 0 };
        let (_, sc) = sr.tlv().ok_or("couldn't read the answer (wrong privacy password or protocol?)")?;
        let mut s = Rd { b: sc, p: 0 };
        s.tlv().ok_or_else(bad)?; // context engine ID
        s.tlv().ok_or_else(bad)?; // context name
        let (ptag, pdu_c) = s.tlv().ok_or_else(bad)?;
        match ptag {
            0xA2 => Ok(V3Reply::Data(decode_pdu(pdu_c, Some(want_id))?)),
            0xA8 => {
                let vbs = decode_pdu(pdu_c, None)?;
                let oid = vbs.first().map(|v| v.0.clone()).unwrap_or_default();
                Ok(V3Reply::Report(oid, eid.to_vec(), boots, time))
            }
            _ => Err("unexpected reply".into()),
        }
    }

    /// GET several values at once. Missing ones come back as Value::Missing.
    pub fn get(&mut self, oids: &[Oid]) -> Result<Vec<Value>, String> {
        let r = self.request(0xA0, 0, 0, oids)?;
        Ok(oids
            .iter()
            .map(|o| r.iter().find(|(ro, _)| ro == o).map(|(_, v)| v.clone()).unwrap_or(Value::Missing))
            .collect())
    }

    pub fn get_one(&mut self, o: &str) -> Option<Value> {
        self.get(&[oid(o)]).ok()?.into_iter().next().filter(|v| v.present())
    }

    /// Walk a subtree with GETBULK. Returns (oid suffix after base, value).
    pub fn walk(&mut self, base: &str, limit: usize) -> Vec<(Oid, Value)> {
        let base = oid(base);
        let mut cur = base.clone();
        let mut out = vec![];
        'outer: while out.len() < limit {
            let Ok(r) = self.request(0xA5, 0, 25, &[cur.clone()]) else { break };
            if r.is_empty() {
                break;
            }
            for (o, v) in r {
                if !o.starts_with(&base) || matches!(v, Value::End) || o <= cur {
                    break 'outer;
                }
                cur = o.clone();
                out.push((o[base.len()..].to_vec(), v));
            }
        }
        out
    }

    /// Walk a table column and key it by the row index (the part after the column OID).
    pub fn column(&mut self, base: &str) -> Vec<(Oid, Value)> {
        self.walk(base, 400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_oid() {
        let o = oid("1.3.6.1.4.1.6574.2.1.1.6.300");
        let e = enc_oid(&o);
        assert_eq!(dec_oid(&e[2..]), o);
    }
    #[test]
    fn ints() {
        for i in [0i64, 1, 127, 128, 255, 256, -1, -129, 70000] {
            let e = enc_int(i);
            assert_eq!(dec_int(&e[2..]), i);
        }
    }
}

#[cfg(test)]
mod reply_tests {
    use super::*;
    /// A hand-built GetResponse with an integer, a string and a Counter64 parses back.
    #[test]
    fn parse_get_response() {
        let mut vbs = vec![];
        for (o, v) in [
            ("1.3.6.1.4.1.6574.1.2.0", vec![0x02, 0x01, 41]),
            ("1.3.6.1.2.1.1.5.0", tlv(0x04, b"DiskStation")),
            ("1.3.6.1.4.1.6574.3.1.1.5.0", tlv(0x46, &[0x01, 0x00, 0x00, 0x00, 0x00])),
        ] {
            let mut vb = enc_oid(&oid(o));
            vb.extend(v);
            vbs.extend(tlv(0x30, &vb));
        }
        let mut pdu = enc_int(77);
        pdu.extend(enc_int(0));
        pdu.extend(enc_int(0));
        pdu.extend(tlv(0x30, &vbs));
        let mut msg = enc_int(1);
        msg.extend(tlv(0x04, b"public"));
        msg.extend(tlv(0xA2, &pdu));
        let pkt = tlv(0x30, &msg);
        let r = parse_response(&pkt, 77).unwrap();
        assert_eq!(r[0].1, Value::Int(41));
        assert_eq!(r[1].1.as_string().unwrap(), "DiskStation");
        assert_eq!(r[2].1, Value::Num(1u64 << 32));
    }
}
