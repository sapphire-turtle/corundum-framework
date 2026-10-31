//! The check a server makes on every request.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::{KeyInfo, KeyStore};

/// How long a success is trusted without reading the store again. A retire takes effect
/// within this long (plus, in a synced directory, the sync's delay).
pub const CACHE_TTL: Duration = Duration::from_secs(30);

/// The answer to a presented token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The token belongs to this unretired key.
    Accepted(KeyInfo),
    /// It does not.
    Refused,
}

/// Checks tokens against the store in a directory.
///
/// A success is kept for the cache's lifetime, by the token's hash (the token itself is
/// never kept); anything else reads the directory again, so a key added by the command
/// line, the page or a sync is accepted on its first use.
pub struct KeyVerifier {
    dir: PathBuf,
    ttl: Duration,
    cache: Mutex<HashMap<[u8; 32], (KeyInfo, Instant)>>,
}

impl KeyVerifier {
    /// A verifier for the store in `dir`.
    pub fn new(dir: &Path) -> KeyVerifier {
        KeyVerifier {
            dir: dir.to_owned(),
            ttl: CACHE_TTL,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Trust a success for `ttl` instead of [`CACHE_TTL`].
    pub fn cache_ttl(mut self, ttl: Duration) -> KeyVerifier {
        self.ttl = ttl;
        self
    }

    /// The verdict on `token`. Reads files on a cache miss.
    pub fn verify(&self, token: &str) -> Verdict {
        let key = *blake3::hash(token.as_bytes()).as_bytes();
        if let Some((info, at)) = self.cache.lock().expect("key cache").get(&key)
            && at.elapsed() < self.ttl
        {
            return Verdict::Accepted(info.clone());
        }
        let found = match KeyStore::read(&self.dir) {
            Ok(store) => store.authenticate(token).map(|k| k.info()),
            Err(err) => {
                tracing::warn!("could not read the keys: {err}");
                None
            }
        };
        let mut cache = self.cache.lock().expect("key cache");
        match found {
            Some(info) => {
                cache.insert(key, (info.clone(), Instant::now()));
                Verdict::Accepted(info)
            }
            None => {
                cache.remove(&key);
                Verdict::Refused
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_added_after_creation_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let v = KeyVerifier::new(dir.path());
        let (_, token) = KeyStore::open(dir.path())
            .unwrap()
            .add("p", None, None)
            .unwrap();
        match v.verify(token.expose()) {
            Verdict::Accepted(info) => assert_eq!(info.name, "p"),
            Verdict::Refused => panic!("refused"),
        }
        assert_eq!(v.verify("wrong"), Verdict::Refused);
    }

    #[test]
    fn retire_is_seen_after_ttl() {
        let dir = tempfile::tempdir().unwrap();
        let v = KeyVerifier::new(dir.path()).cache_ttl(Duration::from_secs(1));
        let mut store = KeyStore::open(dir.path()).unwrap();
        let (_, token) = store.add("p", None, None).unwrap();
        assert!(matches!(v.verify(token.expose()), Verdict::Accepted(_)));
        store.retire("p").unwrap();
        assert!(matches!(v.verify(token.expose()), Verdict::Accepted(_)));
        std::thread::sleep(Duration::from_millis(1200));
        assert_eq!(v.verify(token.expose()), Verdict::Refused);
    }

    #[test]
    fn missing_dir_refuses() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("keys");
        let v = KeyVerifier::new(&gone);
        let (_, token) = KeyStore::open(&gone).unwrap().add("p", None, None).unwrap();
        std::fs::remove_dir_all(&gone).unwrap();
        // `open` would create it again; the verifier must not, and must refuse.
        assert_eq!(v.verify(token.expose()), Verdict::Refused);
        assert!(!gone.exists());
    }
}
