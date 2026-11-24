//! SNMP v3 User-based Security Model (RFC 3414, RFC 3826, RFC 7860):
//! password-to-key, message signing (HMAC) and privacy (DES-CBC, AES-128-CFB).
//! Uses the RustCrypto libraries for the cryptography itself.

use hmac::{Hmac, Mac};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum AuthProto {
    #[default]
    None,
    Md5,
    Sha1,
    Sha256,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum PrivProto {
    #[default]
    None,
    Des,
    Aes128,
}

impl AuthProto {
    pub const ALL: [AuthProto; 4] = [AuthProto::None, AuthProto::Md5, AuthProto::Sha1, AuthProto::Sha256];
    pub fn label(self) -> &'static str {
        match self {
            AuthProto::None => "None",
            AuthProto::Md5 => "MD5",
            AuthProto::Sha1 => "SHA-1",
            AuthProto::Sha256 => "SHA-256",
        }
    }
    /// Length of the signature carried in the message.
    pub fn mac_len(self) -> usize {
        match self {
            AuthProto::None => 0,
            AuthProto::Md5 | AuthProto::Sha1 => 12,
            AuthProto::Sha256 => 24,
        }
    }
    fn hash(self, data: &[u8]) -> Vec<u8> {
        use sha2::Digest;
        match self {
            AuthProto::None => vec![],
            AuthProto::Md5 => md5::Md5::digest(data).to_vec(),
            AuthProto::Sha1 => sha1::Sha1::digest(data).to_vec(),
            AuthProto::Sha256 => sha2::Sha256::digest(data).to_vec(),
        }
    }
    fn hash_stream(self, password: &[u8]) -> Vec<u8> {
        use sha2::Digest;
        // Hash 1 MiB of the password repeated (RFC 3414 A.2).
        let mut buf = Vec::with_capacity(1_048_576);
        while buf.len() < 1_048_576 {
            let take = (1_048_576 - buf.len()).min(password.len());
            buf.extend_from_slice(&password[..take]);
        }
        match self {
            AuthProto::None => vec![],
            AuthProto::Md5 => md5::Md5::digest(&buf).to_vec(),
            AuthProto::Sha1 => sha1::Sha1::digest(&buf).to_vec(),
            AuthProto::Sha256 => sha2::Sha256::digest(&buf).to_vec(),
        }
    }

    /// Localized key for this agent: H(Ku || engineID || Ku).
    pub fn localize(self, password: &str, engine_id: &[u8]) -> Vec<u8> {
        if self == AuthProto::None || password.is_empty() {
            return vec![];
        }
        let ku = self.hash_stream(password.as_bytes());
        let mut m = ku.clone();
        m.extend_from_slice(engine_id);
        m.extend_from_slice(&ku);
        self.hash(&m)
    }

    /// Signature over a whole message (whose signature field is zeros).
    pub fn sign(self, key: &[u8], msg: &[u8]) -> Vec<u8> {
        let full = match self {
            AuthProto::None => return vec![],
            AuthProto::Md5 => {
                let mut m = <Hmac<md5::Md5> as Mac>::new_from_slice(key).expect("key");
                m.update(msg);
                m.finalize().into_bytes().to_vec()
            }
            AuthProto::Sha1 => {
                let mut m = <Hmac<sha1::Sha1> as Mac>::new_from_slice(key).expect("key");
                m.update(msg);
                m.finalize().into_bytes().to_vec()
            }
            AuthProto::Sha256 => {
                let mut m = <Hmac<sha2::Sha256> as Mac>::new_from_slice(key).expect("key");
                m.update(msg);
                m.finalize().into_bytes().to_vec()
            }
        };
        full[..self.mac_len()].to_vec()
    }
}

impl PrivProto {
    pub const ALL: [PrivProto; 3] = [PrivProto::None, PrivProto::Des, PrivProto::Aes128];
    pub fn label(self) -> &'static str {
        match self {
            PrivProto::None => "None",
            PrivProto::Des => "DES",
            PrivProto::Aes128 => "AES-128",
        }
    }

    /// Encrypt a scoped PDU. Returns (ciphertext, privacy parameters/salt).
    pub fn encrypt(self, key: &[u8], boots: u32, time: u32, salt_counter: u64, plain: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
        match self {
            PrivProto::None => Ok((plain.to_vec(), vec![])),
            PrivProto::Des => {
                use cbc::cipher::{block_padding::NoPadding, BlockEncryptMut, KeyIvInit};
                if key.len() < 16 {
                    return Err("privacy key too short".into());
                }
                let mut salt = boots.to_be_bytes().to_vec();
                salt.extend_from_slice(&(salt_counter as u32).to_be_bytes());
                let iv: Vec<u8> = key[8..16].iter().zip(&salt).map(|(a, b)| a ^ b).collect();
                let mut buf = plain.to_vec();
                while buf.len() % 8 != 0 {
                    buf.push(0);
                }
                let n = buf.len();
                let enc = cbc::Encryptor::<des::Des>::new_from_slices(&key[..8], &iv).map_err(|e| e.to_string())?;
                enc.encrypt_padded_mut::<NoPadding>(&mut buf, n).map_err(|e| e.to_string())?;
                Ok((buf, salt))
            }
            PrivProto::Aes128 => {
                use cfb_mode::cipher::{AsyncStreamCipher, KeyIvInit};
                if key.len() < 16 {
                    return Err("privacy key too short".into());
                }
                let salt = salt_counter.to_be_bytes().to_vec();
                let mut iv = boots.to_be_bytes().to_vec();
                iv.extend_from_slice(&time.to_be_bytes());
                iv.extend_from_slice(&salt);
                let mut buf = plain.to_vec();
                cfb_mode::Encryptor::<aes::Aes128>::new_from_slices(&key[..16], &iv).map_err(|e| e.to_string())?.encrypt(&mut buf);
                Ok((buf, salt))
            }
        }
    }

    pub fn decrypt(self, key: &[u8], boots: u32, time: u32, salt: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
        match self {
            PrivProto::None => Ok(data.to_vec()),
            PrivProto::Des => {
                use cbc::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
                if key.len() < 16 || salt.len() != 8 || data.len() % 8 != 0 {
                    return Err("couldn't decrypt the answer".into());
                }
                let iv: Vec<u8> = key[8..16].iter().zip(salt).map(|(a, b)| a ^ b).collect();
                let mut buf = data.to_vec();
                let dec = cbc::Decryptor::<des::Des>::new_from_slices(&key[..8], &iv).map_err(|e| e.to_string())?;
                dec.decrypt_padded_mut::<NoPadding>(&mut buf).map_err(|e| e.to_string())?;
                Ok(buf)
            }
            PrivProto::Aes128 => {
                use cfb_mode::cipher::{AsyncStreamCipher, KeyIvInit};
                if key.len() < 16 || salt.len() != 8 {
                    return Err("couldn't decrypt the answer".into());
                }
                let mut iv = boots.to_be_bytes().to_vec();
                iv.extend_from_slice(&time.to_be_bytes());
                iv.extend_from_slice(salt);
                let mut buf = data.to_vec();
                cfb_mode::Decryptor::<aes::Aes128>::new_from_slices(&key[..16], &iv).map_err(|e| e.to_string())?.decrypt(&mut buf);
                Ok(buf)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{:02x}", x)).collect()
    }

    // RFC 3414 appendix A.3: password "maplesyrup", engine ID 00..02
    const ENGINE: [u8; 12] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2];

    #[test]
    fn md5_key_localization() {
        assert_eq!(hex(&AuthProto::Md5.localize("maplesyrup", &ENGINE)), "526f5eed9fcce26f8964c2930787d82b");
    }

    #[test]
    fn sha1_key_localization() {
        assert_eq!(hex(&AuthProto::Sha1.localize("maplesyrup", &ENGINE)), "6695febc9288e36282235fc7151f128497b38f3f");
    }

    #[test]
    fn privacy_roundtrip() {
        let key = AuthProto::Sha1.localize("privpassword", &ENGINE);
        let msg = b"scoped pdu bytes that are not a multiple of eight".to_vec();
        for p in [PrivProto::Des, PrivProto::Aes128] {
            let (c, salt) = p.encrypt(&key, 7, 1234, 42, &msg).unwrap();
            assert_ne!(c[..msg.len()], msg[..]);
            let back = p.decrypt(&key, 7, 1234, &salt, &c).unwrap();
            assert_eq!(&back[..msg.len()], &msg[..]);
        }
    }

    #[test]
    fn signature_lengths() {
        let k = AuthProto::Sha256.localize("maplesyrup", &ENGINE);
        assert_eq!(k.len(), 32);
        assert_eq!(AuthProto::Sha256.sign(&k, b"x").len(), 24);
        assert_eq!(AuthProto::Md5.sign(&AuthProto::Md5.localize("maplesyrup", &ENGINE), b"x").len(), 12);
    }
}
