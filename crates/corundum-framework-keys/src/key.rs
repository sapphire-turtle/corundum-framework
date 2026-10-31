//! A key, its id and its token.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::KeyHash;
use crate::error::{Error, Result};

/// A key's id: ASCII letters, digits, `-` and `_`, so it can name the key's file.
///
/// New keys get a UUIDv7 in its simple form; imported keys keep theirs.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct KeyId(String);

impl KeyId {
    /// `s`, if it is a valid id.
    pub fn new(s: &str) -> Result<KeyId> {
        let valid = !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if valid {
            Ok(KeyId(s.to_owned()))
        } else {
            Err(Error::InvalidId(s.to_owned()))
        }
    }

    /// A new, unique id.
    pub fn generate() -> KeyId {
        KeyId(uuid::Uuid::now_v7().simple().to_string())
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KeyId({})", self.0)
    }
}

impl<'de> Deserialize<'de> for KeyId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<KeyId, D::Error> {
        let s = String::deserialize(d)?;
        KeyId::new(&s).map_err(serde::de::Error::custom)
    }
}

/// A token, shown once. Its `Debug` does not show it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Token(String);

impl Token {
    /// A new token: `prefix`, then 32 random bytes as unpadded base64url.
    pub fn generate(prefix: &str) -> Token {
        use base64::Engine as _;
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).expect("the system's random source");
        let body = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        Token(format!("{prefix}{body}"))
    }

    /// The token itself, to hand to the user once.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(…)")
    }
}

/// One key, as stored. Never holds the token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Key {
    /// Stable; the record's file is `<id>.toml`.
    pub id: KeyId,
    /// Unique among unretired keys when added.
    pub name: String,
    /// A note for the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Who the key acts for, when the app has users.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// The hash of its token.
    pub hash: KeyHash,
    /// When it was added.
    pub created_at: DateTime<Utc>,
    /// When its token was last replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotated_at: Option<DateTime<Utc>>,
    /// When it was retired, if it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired_at: Option<DateTime<Utc>>,
}

impl Key {
    /// Whether it is retired.
    pub fn is_retired(&self) -> bool {
        self.retired_at.is_some()
    }

    /// What a client may see of it: everything but the hash.
    pub fn info(&self) -> KeyInfo {
        KeyInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            subject: self.subject.clone(),
            created_at: self.created_at,
            rotated_at: self.rotated_at,
            retired_at: self.retired_at,
        }
    }
}

/// A key without its hash: what a client sees.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyInfo {
    /// See [`Key::id`].
    pub id: KeyId,
    /// See [`Key::name`].
    pub name: String,
    /// See [`Key::description`].
    #[serde(default)]
    pub description: Option<String>,
    /// See [`Key::subject`].
    #[serde(default)]
    pub subject: Option<String>,
    /// See [`Key::created_at`].
    pub created_at: DateTime<Utc>,
    /// See [`Key::rotated_at`].
    #[serde(default)]
    pub rotated_at: Option<DateTime<Utc>>,
    /// See [`Key::retired_at`].
    #[serde(default)]
    pub retired_at: Option<DateTime<Utc>>,
}

impl KeyInfo {
    /// Whether it is retired.
    pub fn is_retired(&self) -> bool {
        self.retired_at.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_id_validates() {
        assert_eq!(KeyId::new("a-b_C9").unwrap().as_str(), "a-b_C9");
        for bad in ["", "a/b", "a.b", "../x", "a b"] {
            assert!(
                matches!(KeyId::new(bad), Err(Error::InvalidId(_))),
                "{bad:?} accepted"
            );
        }
    }

    #[test]
    fn generated_ids_are_uuid_v7_simple() {
        let a = KeyId::generate();
        let b = KeyId::generate();
        assert_eq!(a.as_str().len(), 32);
        assert!(
            a.as_str()
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_eq!(&a.as_str()[12..13], "7", "version nibble");
        assert_ne!(a, b);
    }

    #[test]
    fn token_has_prefix_and_43_char_body() {
        let t = Token::generate("p-");
        let body = t.expose().strip_prefix("p-").unwrap();
        assert_eq!(body.len(), 43);
        assert!(
            body.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        assert_ne!(Token::generate("p-"), t);
    }

    #[test]
    fn token_debug_hides_it() {
        let t = Token::generate("");
        assert!(!format!("{t:?}").contains(t.expose()));
    }

    #[test]
    fn key_id_deserialize_validates() {
        assert!(serde_json::from_str::<KeyId>("\"a/b\"").is_err());
        assert!(serde_json::from_str::<KeyId>("\"ab\"").is_ok());
    }
}
