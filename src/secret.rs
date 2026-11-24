//! Keep passwords (SNMP v3) out of plain sight in the saved settings.
//! Windows: encrypted for the signed-in Windows account (DPAPI), so the file is
//! useless on another account or computer. macOS / Linux: stored in the
//! settings file, which is made readable only by its owner.
//!
//! Use on a String field with `#[serde(default, with = "crate::secret")]`.

use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer>(v: &str, s: S) -> Result<S::Ok, S::Error> {
    if v.is_empty() {
        return s.serialize_str("");
    }
    s.serialize_str(&protect(v))
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let raw = String::deserialize(d)?;
    Ok(unprotect(&raw).unwrap_or_default())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

#[cfg(windows)]
fn protect(v: &str) -> String {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    let mut data = v.as_bytes().to_vec();
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_mut_ptr() };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    unsafe {
        if CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out) == 0 {
            return String::new(); // never fall back to plain text on Windows
        }
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as _);
        format!("dpapi:{}", hex(&bytes))
    }
}

#[cfg(not(windows))]
fn protect(v: &str) -> String {
    format!("plain:{}", v)
}

fn unprotect(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return Some(String::new());
    }
    if let Some(p) = raw.strip_prefix("plain:") {
        return Some(p.to_string());
    }
    if let Some(h) = raw.strip_prefix("dpapi:") {
        return dpapi_open(&unhex(h)?);
    }
    None
}

#[cfg(windows)]
fn dpapi_open(bytes: &[u8]) -> Option<String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    let mut data = bytes.to_vec();
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_mut_ptr() };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    unsafe {
        if CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out) == 0 {
            return None;
        }
        let plain = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as _);
        String::from_utf8(plain).ok()
    }
}

#[cfg(not(windows))]
fn dpapi_open(_bytes: &[u8]) -> Option<String> {
    None // encrypted on a Windows computer; must be entered again here
}

#[cfg(test)]
mod tests {
    #[test]
    fn roundtrip() {
        let p = super::protect("s3cret pass");
        assert!(!p.contains("s3cret") || p.starts_with("plain:"));
        assert_eq!(super::unprotect(&p).as_deref(), Some("s3cret pass"));
        assert_eq!(super::unprotect("").as_deref(), Some(""));
    }
}
