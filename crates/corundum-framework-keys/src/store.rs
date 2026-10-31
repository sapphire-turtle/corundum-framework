//! The store: a directory of keys, one TOML file each.

use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::error::{Error, Result};
use crate::{Key, KeyHash, KeyId, Token};

/// A directory of keys.
///
/// The app chooses the directory; the store never assumes it is its only writer (another
/// process, or a sync, may change it), so every change reads the directory again first.
#[derive(Debug)]
pub struct KeyStore {
    dir: PathBuf,
    prefix: String,
    keys: Vec<Key>,
    /// The conflict copies the last read found, and their files.
    copies: Vec<(KeyId, PathBuf)>,
    /// Make the next write fail, to test what a failed write leaves.
    #[cfg(test)]
    fail_write: bool,
}

impl KeyStore {
    /// The store in `dir`, which is created if absent.
    pub fn open(dir: &Path) -> Result<KeyStore> {
        std::fs::create_dir_all(dir).map_err(|source| Error::Io {
            path: dir.to_owned(),
            source,
        })?;
        KeyStore::read(dir)
    }

    /// The store in `dir` as it is, without creating the directory: for a reader.
    pub(crate) fn read(dir: &Path) -> Result<KeyStore> {
        let mut store = KeyStore {
            dir: dir.to_owned(),
            prefix: String::new(),
            keys: Vec::new(),
            copies: Vec::new(),
            #[cfg(test)]
            fail_write: false,
        };
        store.reload()?;
        Ok(store)
    }

    /// Read the directory again.
    fn reload(&mut self) -> Result<()> {
        let io = |source| Error::Io {
            path: self.dir.clone(),
            source,
        };
        // Records by the file they were read from: `<id>.toml` is the key, any other file
        // with the same `id` is a copy a sync left when two hosts changed it at once.
        let mut keys: Vec<Key> = Vec::new();
        let mut copies: Vec<(Key, PathBuf)> = Vec::new();
        for entry in std::fs::read_dir(&self.dir).map_err(io)? {
            let path = entry.map_err(io)?.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            // Hidden files are a write in progress (`.<id>.toml.tmp`) or not ours.
            if name.starts_with('.') || !name.ends_with(".toml") {
                continue;
            }
            match read_key(&path) {
                Ok(key) if name == format!("{}.toml", key.id) => keys.push(key),
                Ok(copy) => copies.push((copy, path)),
                Err(message) => {
                    tracing::warn!("skipping the key file {}: {message}", path.display())
                }
            }
        }
        // A copy never lets a token in, but a retire in any copy holds: a retire made on one
        // host is never lost to a concurrent edit on another.
        for key in keys.iter_mut().filter(|k| !k.is_retired()) {
            key.retired_at = copies
                .iter()
                .filter(|(c, _)| c.id == key.id)
                .filter_map(|(c, _)| c.retired_at)
                .min();
        }
        keys.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
        self.keys = keys;
        self.copies = copies.into_iter().map(|(c, path)| (c.id, path)).collect();
        Ok(())
    }

    fn write(&mut self, key: &Key) -> Result<()> {
        let text = toml::to_string(key).map_err(|e| Error::Parse {
            path: self.file(&key.id),
            message: e.to_string(),
        })?;
        #[cfg(test)]
        if self.fail_write {
            return Err(Error::Io {
                path: self.file(&key.id),
                source: std::io::Error::other("a write made to fail"),
            });
        }
        // Unique, so two writers of one key never rename each other's half-written file.
        let tmp = self
            .dir
            .join(format!(".{}.{}.toml.tmp", key.id, KeyId::generate()));
        let io = |path: &Path| {
            let path = path.to_owned();
            move |source| Error::Io { path, source }
        };
        let written = std::fs::File::create(&tmp).and_then(|mut f| {
            use std::io::Write as _;
            f.write_all(text.as_bytes())?;
            f.sync_all()
        });
        if let Err(err) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(io(&tmp)(err));
        }
        let file = self.file(&key.id);
        std::fs::rename(&tmp, &file).map_err(io(&file))?;
        self.reload()
    }

    fn file(&self, id: &KeyId) -> PathBuf {
        self.dir.join(format!("{id}.toml"))
    }

    /// Change the key `selector` names, after reading the directory again.
    fn change(&mut self, selector: &str, f: impl FnOnce(&mut Key)) -> Result<Key> {
        self.reload()?;
        let mut key = self.resolve(selector)?.clone();
        f(&mut key);
        // The user decided on the key as it stands now, which settles the conflicts this
        // read found. The key is written first: a failed write leaves the copies, and a
        // retire in them, where they were.
        let seen: Vec<PathBuf> = self
            .copies
            .iter()
            .filter(|(id, _)| *id == key.id)
            .map(|(_, path)| path.clone())
            .collect();
        self.write(&key)?;
        for path in seen {
            match std::fs::remove_file(&path) {
                Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
                    return Err(Error::Io { path, source: err });
                }
                _ => {}
            }
        }
        self.reload()?;
        Ok(key)
    }

    /// Start new tokens with `prefix`, so a leaked token says what it is.
    pub fn token_prefix(mut self, prefix: &str) -> KeyStore {
        self.prefix = prefix.to_owned();
        self
    }

    /// The directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Every key, retired ones included, oldest first.
    pub fn list(&self) -> &[Key] {
        &self.keys
    }

    /// The key with this id or, failing that, the one unretired key with this name.
    ///
    /// A name that more than one unretired key has (two hosts added it at once), or that
    /// only retired keys have, more than one of them, is ambiguous: give the id.
    pub fn resolve(&self, selector: &str) -> Result<&Key> {
        if let Some(key) = self.keys.iter().find(|k| k.id.as_str() == selector) {
            return Ok(key);
        }
        let named: Vec<&Key> = self.keys.iter().filter(|k| k.name == selector).collect();
        let live: Vec<&Key> = named.iter().copied().filter(|k| !k.is_retired()).collect();
        match (live.as_slice(), named.as_slice()) {
            ([key], _) => Ok(key),
            ([], [key]) => Ok(key),
            ([], []) => Err(Error::NotFound(selector.to_owned())),
            _ => Err(Error::Ambiguous(selector.to_owned())),
        }
    }

    /// Add a key named `name`; its token is returned, once.
    pub fn add(
        &mut self,
        name: &str,
        description: Option<String>,
        subject: Option<String>,
    ) -> Result<(Key, Token)> {
        self.reload()?;
        if self.keys.iter().any(|k| k.name == name && !k.is_retired()) {
            return Err(Error::NameTaken(name.to_owned()));
        }
        let token = Token::generate(&self.prefix);
        let key = Key {
            id: KeyId::generate(),
            name: name.to_owned(),
            description,
            subject,
            hash: KeyHash::of_blake3(token.expose()),
            created_at: Utc::now(),
            rotated_at: None,
            retired_at: None,
        };
        self.write(&key)?;
        Ok((key, token))
    }

    /// Retire a key: its token stops working, its record stays.
    pub fn retire(&mut self, selector: &str) -> Result<Key> {
        self.change(selector, |k| {
            k.retired_at.get_or_insert_with(Utc::now);
        })
    }

    /// Bring a retired key back.
    pub fn restore(&mut self, selector: &str) -> Result<Key> {
        self.change(selector, |k| k.retired_at = None)
    }

    /// Give a key a new token, keeping its id; the old token stops working.
    pub fn rotate(&mut self, selector: &str) -> Result<(Key, Token)> {
        let token = Token::generate(&self.prefix);
        let key = self.change(selector, |k| {
            k.hash = KeyHash::of_blake3(token.expose());
            k.rotated_at = Some(Utc::now());
        })?;
        Ok((key, token))
    }

    /// Take a record made elsewhere as it is, its hash included.
    ///
    /// An id the store already has is refused.
    pub fn import(&mut self, key: Key) -> Result<Key> {
        self.reload()?;
        // Ignoring case: on macOS and Windows `ABC.toml` would overwrite `abc.toml`.
        let taken = |id: &KeyId| id.as_str().eq_ignore_ascii_case(key.id.as_str());
        if self.keys.iter().any(|k| taken(&k.id)) {
            return Err(Error::IdTaken(key.id.to_string()));
        }
        self.write(&key)?;
        Ok(key)
    }

    /// The unretired key `token` belongs to.
    ///
    /// Every key is compared, so the time taken does not say which one matched.
    pub fn authenticate(&self, token: &str) -> Option<&Key> {
        if token.is_empty() {
            return None;
        }
        let mut found = None;
        for key in &self.keys {
            if key.hash.matches(token) && !key.is_retired() {
                found = Some(key);
            }
        }
        found
    }
}

fn read_key(path: &Path) -> std::result::Result<Key, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    toml::from_str(&text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, KeyStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = KeyStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn add_then_authenticate() {
        let (dir, mut s) = store();
        let (key, token) = s.add("pendant", Some("desk".into()), None).unwrap();
        assert_eq!(s.authenticate(token.expose()).unwrap().id, key.id);
        // and from a fresh read of the directory
        let again = KeyStore::open(dir.path()).unwrap();
        assert_eq!(again.authenticate(token.expose()).unwrap().name, "pendant");
        assert_eq!(again.list()[0].description.as_deref(), Some("desk"));
        assert!(dir.path().join(format!("{}.toml", key.id)).is_file());
    }

    #[test]
    fn retired_is_refused_and_restore_accepts() {
        let (_dir, mut s) = store();
        let (_, token) = s.add("p", None, None).unwrap();
        assert!(s.retire("p").unwrap().is_retired());
        assert!(s.authenticate(token.expose()).is_none());
        assert!(!s.restore("p").unwrap().is_retired());
        assert!(s.authenticate(token.expose()).is_some());
    }

    #[test]
    fn rotate_keeps_id_and_replaces_token() {
        let (_dir, mut s) = store();
        let (key, old) = s.add("p", None, None).unwrap();
        let (rotated, new) = s.rotate(key.id.as_str()).unwrap();
        assert_eq!(rotated.id, key.id);
        assert!(rotated.rotated_at.is_some());
        assert!(s.authenticate(old.expose()).is_none());
        assert_eq!(s.authenticate(new.expose()).unwrap().id, key.id);
    }

    #[test]
    fn import_keeps_hash_and_refuses_existing_id() {
        use sha2::{Digest, Sha256};
        let (_dir, mut s) = store();
        let hex: String = Sha256::digest(b"old")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let key = Key {
            id: KeyId::new("abc123").unwrap(),
            name: "imported".into(),
            description: None,
            subject: None,
            hash: format!("sha256:{hex}").parse().unwrap(),
            created_at: Utc::now(),
            rotated_at: None,
            retired_at: None,
        };
        s.import(key.clone()).unwrap();
        assert_eq!(s.authenticate("old").unwrap().id.as_str(), "abc123");
        assert!(matches!(s.import(key), Err(Error::IdTaken(_))));
    }

    #[test]
    fn bad_file_is_skipped() {
        let (dir, mut s) = store();
        let (_, token) = s.add("p", None, None).unwrap();
        std::fs::write(dir.path().join("garbage.toml"), "not = [toml").unwrap();
        let again = KeyStore::open(dir.path()).unwrap();
        assert_eq!(again.list().len(), 1);
        assert!(again.authenticate(token.expose()).is_some());
    }

    #[test]
    fn temp_files_are_ignored() {
        let (dir, mut s) = store();
        let (key, _) = s.add("p", None, None).unwrap();
        let text = std::fs::read_to_string(dir.path().join(format!("{}.toml", key.id))).unwrap();
        std::fs::write(dir.path().join(".other.toml.tmp"), &text).unwrap();
        std::fs::write(dir.path().join(".other.toml"), &text).unwrap();
        assert_eq!(KeyStore::open(dir.path()).unwrap().list().len(), 1);
    }

    #[test]
    fn prefix_is_hashed() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = KeyStore::open(dir.path()).unwrap().token_prefix("p-");
        let (_, token) = s.add("k", None, None).unwrap();
        assert!(token.expose().starts_with("p-"));
        assert!(s.authenticate(token.expose()).is_some());
        assert!(s.authenticate(&token.expose()[2..]).is_none());
    }

    #[test]
    fn add_refuses_taken_name() {
        let (_dir, mut s) = store();
        s.add("p", None, None).unwrap();
        assert!(matches!(s.add("p", None, None), Err(Error::NameTaken(_))));
        s.retire("p").unwrap();
        s.add("p", None, None).unwrap();
    }

    #[test]
    fn same_name_twice_on_disk_is_ambiguous() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = KeyStore::open(dir.path()).unwrap();
        let mut b = KeyStore::open(dir.path()).unwrap();
        let (ka, _) = a.add("pendant", None, None).unwrap();
        // `b` writes from its own view, as a second host would before syncing.
        std::fs::remove_file(dir.path().join(format!("{}.toml", ka.id))).unwrap();
        let (kb, _) = b.add("pendant", None, None).unwrap();
        a.import(ka.clone()).unwrap();
        let s = KeyStore::open(dir.path()).unwrap();
        assert!(matches!(s.resolve("pendant"), Err(Error::Ambiguous(_))));
        assert_eq!(s.resolve(ka.id.as_str()).unwrap().id, ka.id);
        assert_eq!(s.resolve(kb.id.as_str()).unwrap().id, kb.id);
    }

    #[test]
    fn empty_token_is_refused() {
        let (_dir, mut s) = store();
        s.add("p", None, None).unwrap();
        assert!(s.authenticate("").is_none());
    }

    #[test]
    fn unknown_selector_is_not_found() {
        let (_dir, mut s) = store();
        assert!(matches!(s.retire("nope"), Err(Error::NotFound(_))));
    }

    /// Write `key` to `file` in `dir`, as a sync would.
    fn plant(dir: &Path, file: &str, key: &Key) {
        std::fs::write(dir.join(file), toml::to_string(key).unwrap()).unwrap();
    }

    #[test]
    fn copy_alone_never_accepts() {
        let (dir, mut s) = store();
        let (key, token) = s.add("p", None, None).unwrap();
        let file = dir.path().join(format!("{}.toml", key.id));
        std::fs::rename(
            &file,
            dir.path().join(format!("{}.conflict-r-1.toml", key.id)),
        )
        .unwrap();
        let s = KeyStore::open(dir.path()).unwrap();
        assert!(s.list().is_empty());
        assert!(s.authenticate(token.expose()).is_none());
    }

    #[test]
    fn retired_copy_retires_the_key() {
        let (dir, mut s) = store();
        let (key, token) = s.add("p", None, None).unwrap();
        let mut copy = key.clone();
        copy.retired_at = Some(Utc::now());
        plant(dir.path(), &format!("{}.conflict-r-1.toml", key.id), &copy);
        let s = KeyStore::open(dir.path()).unwrap();
        assert!(s.authenticate(token.expose()).is_none());
        assert_eq!(s.list().len(), 1);
        assert!(s.list()[0].is_retired());
    }

    #[test]
    fn unretired_copy_does_not_change_the_key() {
        let (dir, mut s) = store();
        let (key, token) = s.add("p", None, None).unwrap();
        let other = Token::generate("");
        let mut copy = key.clone();
        copy.hash = KeyHash::of_blake3(other.expose());
        plant(dir.path(), &format!("{}.conflict-r-1.toml", key.id), &copy);
        let s = KeyStore::open(dir.path()).unwrap();
        assert_eq!(s.list().len(), 1);
        assert!(s.authenticate(token.expose()).is_some());
        assert!(s.authenticate(other.expose()).is_none());
    }

    #[test]
    fn copy_under_a_different_name_groups_by_id() {
        let (dir, mut s) = store();
        let (key, token) = s.add("p", None, None).unwrap();
        let mut copy = key.clone();
        copy.retired_at = Some(Utc::now());
        plant(dir.path(), "B.conflict-r-1.toml", &copy);
        let s = KeyStore::open(dir.path()).unwrap();
        assert_eq!(s.list().len(), 1);
        assert_eq!(s.list()[0].id, key.id);
        assert!(s.authenticate(token.expose()).is_none());
    }

    #[test]
    fn restore_clears_a_retired_copy() {
        let (dir, mut s) = store();
        let (key, token) = s.add("p", None, None).unwrap();
        let mut copy = key.clone();
        copy.retired_at = Some(Utc::now());
        plant(dir.path(), &format!("{}.conflict-r-1.toml", key.id), &copy);
        let mut s = KeyStore::open(dir.path()).unwrap();
        assert!(!s.restore(key.id.as_str()).unwrap().is_retired());
        let s = KeyStore::open(dir.path()).unwrap();
        assert!(s.authenticate(token.expose()).is_some());
    }

    #[test]
    fn failed_write_keeps_the_retired_copy() {
        let (dir, mut s) = store();
        let (key, token) = s.add("p", None, None).unwrap();
        let mut copy = key.clone();
        copy.retired_at = Some(Utc::now());
        plant(dir.path(), &format!("{}.conflict-r-1.toml", key.id), &copy);
        let mut s = KeyStore::open(dir.path()).unwrap();
        s.fail_write = true;
        assert!(s.rotate(key.id.as_str()).is_err());
        let s = KeyStore::open(dir.path()).unwrap();
        assert!(s.list()[0].is_retired(), "the retire was lost");
        assert!(s.authenticate(token.expose()).is_none());
    }

    #[test]
    fn a_leftover_tmp_does_not_block_a_write() {
        let (dir, mut s) = store();
        let (key, _) = s.add("p", None, None).unwrap();
        // Another writer's temporary file of the same key, still there.
        std::fs::create_dir(dir.path().join(format!(".{}.toml.tmp", key.id))).unwrap();
        s.retire("p").unwrap();
    }

    #[test]
    fn import_refuses_an_id_differing_in_case() {
        let (_dir, mut s) = store();
        let mut key = s.add("p", None, None).unwrap().0;
        key.id = KeyId::new(&key.id.as_str().to_uppercase()).unwrap();
        key.name = "q".into();
        assert!(matches!(s.import(key), Err(Error::IdTaken(_))));
    }
}
