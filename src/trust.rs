//! Who made a program: its digital signature (Windows Authenticode, macOS
//! code signing) and its SHA-256 fingerprint for pinning.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub enum SigStatus {
    #[default]
    Unknown,
    /// Signed and the signature checks out
    Signed,
    /// Part of the operating system (signed through the system's own catalog)
    System,
    /// No signature
    Unsigned,
    /// Has a signature, but it doesn't check out (changed or broken)
    Invalid,
    /// This system doesn't sign programs (most Linux)
    NotApplicable,
}

impl SigStatus {
    pub fn label(&self) -> &'static str {
        match self {
            SigStatus::Unknown => "Not checked yet",
            SigStatus::Signed => "Signed",
            SigStatus::System => "Part of the operating system",
            SigStatus::Unsigned => "Not signed",
            SigStatus::Invalid => "Signature broken",
            SigStatus::NotApplicable => "Signatures not used on this system",
        }
    }
}

/// Check a program's signature. Returns (status, signer name).
pub fn signature(path: &str) -> (SigStatus, Option<String>) {
    if path.is_empty() {
        return (SigStatus::Unknown, None);
    }
    imp(path)
}

#[cfg(windows)]
fn imp(path: &str) -> (SigStatus, Option<String>) {
    use windows_sys::Win32::Security::Cryptography::{CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE};
    use windows_sys::Win32::Security::WinTrust::*;
    let wpath: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: wpath.as_ptr(),
            hFile: std::ptr::null_mut(),
            pgKnownSubject: std::ptr::null_mut(),
        };
        let mut data: WINTRUST_DATA = std::mem::zeroed();
        data.cbStruct = std::mem::size_of::<WINTRUST_DATA>() as u32;
        data.dwUIChoice = WTD_UI_NONE;
        data.fdwRevocationChecks = WTD_REVOKE_NONE;
        data.dwUnionChoice = WTD_CHOICE_FILE;
        data.Anonymous.pFile = &mut file;
        data.dwStateAction = WTD_STATEACTION_VERIFY;
        data.dwProvFlags = WTD_CACHE_ONLY_URL_RETRIEVAL | WTD_REVOCATION_CHECK_NONE;
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let rc = WinVerifyTrust(std::ptr::null_mut(), &mut action, &mut data as *mut _ as *mut _);
        let mut signer = None;
        if !data.hWVTStateData.is_null() {
            let prov = WTHelperProvDataFromStateData(data.hWVTStateData);
            if !prov.is_null() {
                let sgnr = WTHelperGetProvSignerFromChain(prov, 0, 0, 0);
                if !sgnr.is_null() && (*sgnr).csCertChain > 0 && !(*sgnr).pasCertChain.is_null() {
                    let cert = (*(*sgnr).pasCertChain).pCert;
                    if !cert.is_null() {
                        let mut buf = [0u16; 256];
                        let n = CertGetNameStringW(cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, std::ptr::null(), buf.as_mut_ptr(), buf.len() as u32);
                        if n > 1 {
                            signer = Some(String::from_utf16_lossy(&buf[..n as usize - 1]));
                        }
                    }
                }
            }
        }
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        WinVerifyTrust(std::ptr::null_mut(), &mut action, &mut data as *mut _ as *mut _);
        const TRUST_E_NOSIGNATURE: i32 = 0x800B0100u32 as i32;
        const TRUST_E_SUBJECT_FORM_UNKNOWN: i32 = 0x800B0003u32 as i32;
        const TRUST_E_PROVIDER_UNKNOWN: i32 = 0x800B0001u32 as i32;
        match rc {
            0 => (SigStatus::Signed, signer),
            TRUST_E_NOSIGNATURE | TRUST_E_SUBJECT_FORM_UNKNOWN | TRUST_E_PROVIDER_UNKNOWN => {
                // Many built-in Windows programs are signed through a system catalog
                // rather than inside the file itself.
                let windir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()).to_lowercase();
                if path.to_lowercase().starts_with(&windir) {
                    (SigStatus::System, Some("Microsoft Windows".into()))
                } else {
                    (SigStatus::Unsigned, None)
                }
            }
            _ => (SigStatus::Invalid, signer),
        }
    }
}

#[cfg(target_os = "macos")]
fn imp(path: &str) -> (SigStatus, Option<String>) {
    use std::process::{Command, Stdio};
    let info = Command::new("codesign").args(["-dv", "--verbose=2", path]).stdin(Stdio::null()).stdout(Stdio::null()).output();
    let Ok(info) = info else { return (SigStatus::Unknown, None) };
    let text = String::from_utf8_lossy(&info.stderr).to_string();
    if text.contains("not signed at all") {
        return (SigStatus::Unsigned, None);
    }
    let signer = text.lines().find_map(|l| l.strip_prefix("Authority=")).map(|s| s.trim().to_string());
    let valid = Command::new("codesign").args(["--verify", path]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
    if !valid {
        return (SigStatus::Invalid, signer);
    }
    if path.starts_with("/System/") || path.starts_with("/usr/") || path.starts_with("/sbin/") || path.starts_with("/bin/") {
        return (SigStatus::System, signer.or(Some("Apple".into())));
    }
    (SigStatus::Signed, signer)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn imp(path: &str) -> (SigStatus, Option<String>) {
    // Linux programs usually aren't signed; system packages are checked by the
    // package manager instead. The fingerprint pin still works.
    if path.starts_with("/usr/") || path.starts_with("/bin/") || path.starts_with("/sbin/") || path.starts_with("/lib") {
        (SigStatus::System, Some("System package".into()))
    } else {
        (SigStatus::NotApplicable, None)
    }
}

/// Does this program live somewhere programs usually shouldn't run from?
pub fn odd_location(path: &str) -> Option<&'static str> {
    let p = path.to_lowercase().replace('\\', "/");
    if p.contains("/temp/") || p.contains("/tmp/") {
        Some("runs from a temporary folder")
    } else if p.contains("/downloads/") {
        Some("runs from the Downloads folder")
    } else if p.contains("/appdata/local/temp") {
        Some("runs from a temporary folder")
    } else if p.contains("/users/public/") {
        Some("runs from the shared Public folder")
    } else {
        None
    }
}

#[cfg(all(test, windows))]
mod tests {
    // Needs real files: cargo test -- --ignored
    #[test]
    #[ignore]
    fn live_signatures() {
        let t = std::time::Instant::now();
        for p in [r"C:\Windows\System32\notepad.exe", r"C:\Windows\explorer.exe", r"C:\Program Files\Google\Drive File Stream\launch.bat"] {
            println!("{} -> {:?} in {:?}", p, super::signature(p), t.elapsed());
        }
        if let Ok(exe) = std::env::current_exe() {
            println!("self -> {:?}", super::signature(&exe.to_string_lossy()));
        }
        for e in std::fs::read_dir(r"C:\Program Files\Google\Drive File Stream").into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                for f in std::fs::read_dir(&p).into_iter().flatten().flatten() {
                    if f.path().extension().map(|x| x == "exe").unwrap_or(false) {
                        let s = f.path().to_string_lossy().to_string();
                        let t = std::time::Instant::now();
                        println!("{} -> {:?} in {:?}; sha in {:?}", s, super::signature(&s), t.elapsed(), { let t2 = std::time::Instant::now(); let _ = crate::sha256::file(&s); t2.elapsed() });
                        return;
                    }
                }
            }
        }
    }
}
