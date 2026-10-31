//! The hash a record keeps of its token.

use std::fmt;
use std::str::FromStr;

use sha2::{Digest, Sha256};

const BLAKE3: &str = "blake3";
const SHA256: &str = "sha256";

/// The hash of a token, with its algorithm.
///
/// New keys are hashed with BLAKE3; SHA-256 is read for keys imported from elsewhere
/// (sapphire's external devices). A random 256-bit token needs no slow hash. On disk it is
/// `"blake3:<hex>"` or `"sha256:<hex>"`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KeyHash {
    /// BLAKE3 of the token.
    Blake3([u8; 32]),
    /// SHA-256 of the token.
    Sha256([u8; 32]),
}

impl KeyHash {
    /// The BLAKE3 hash of `token`.
    pub fn of_blake3(token: &str) -> KeyHash {
        KeyHash::Blake3(*blake3::hash(token.as_bytes()).as_bytes())
    }

    /// Whether `token` hashes to this, compared in constant time.
    pub fn matches(&self, token: &str) -> bool {
        let (stored, presented) = match self {
            KeyHash::Blake3(h) => (h, *blake3::hash(token.as_bytes()).as_bytes()),
            KeyHash::Sha256(h) => (h, Sha256::digest(token.as_bytes()).into()),
        };
        stored
            .iter()
            .zip(presented.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }

    fn parts(&self) -> (&'static str, &[u8; 32]) {
        match self {
            KeyHash::Blake3(h) => (BLAKE3, h),
            KeyHash::Sha256(h) => (SHA256, h),
        }
    }
}

impl fmt::Debug for KeyHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KeyHash({self})")
    }
}

impl fmt::Display for KeyHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (algorithm, bytes) = self.parts();
        f.write_str(algorithm)?;
        f.write_str(":")?;
        bytes.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

/// Not `"blake3:<64 hex>"` or `"sha256:<64 hex>"`.
#[derive(Debug, thiserror::Error)]
#[error("{0:?} is not a key hash (blake3:<hex> or sha256:<hex>)")]
pub struct InvalidHash(String);

impl FromStr for KeyHash {
    type Err = InvalidHash;

    fn from_str(s: &str) -> Result<KeyHash, InvalidHash> {
        let invalid = || InvalidHash(s.to_owned());
        let (algorithm, hex) = s.split_once(':').ok_or_else(invalid)?;
        // `from_str_radix` alone would take a sign, as in `+f`.
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(hex.get(2 * i..2 * i + 2).ok_or_else(invalid)?, 16)
                .map_err(|_| invalid())?;
        }
        match algorithm {
            BLAKE3 => Ok(KeyHash::Blake3(bytes)),
            SHA256 => Ok(KeyHash::Sha256(bytes)),
            _ => Err(invalid()),
        }
    }
}

impl serde::Serialize for KeyHash {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for KeyHash {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<KeyHash, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn blake3_round_trips() {
        let h = KeyHash::of_blake3("t");
        let s = h.to_string();
        assert!(s.starts_with("blake3:"), "{s}");
        assert_eq!(s.parse::<KeyHash>().unwrap(), h);
        assert!(h.matches("t"));
        assert!(!h.matches("u"));
    }

    #[test]
    fn sha256_hex_is_accepted() {
        let s = format!("sha256:{}", hex(&Sha256::digest(b"t")));
        let h: KeyHash = s.parse().unwrap();
        assert!(h.matches("t"));
        assert!(!h.matches("u"));
        assert_eq!(h.to_string(), s);
    }

    #[test]
    fn unknown_or_bare_hashes_are_refused() {
        let bare = hex(&Sha256::digest(b"t"));
        for s in [
            "md5:00",
            bare.as_str(),
            "blake3:zz",
            "blake3:",
            &format!("blake3:+{}", "f".repeat(63)),
            "sha256:00",
        ] {
            assert!(s.parse::<KeyHash>().is_err(), "{s} parsed");
        }
    }
}
