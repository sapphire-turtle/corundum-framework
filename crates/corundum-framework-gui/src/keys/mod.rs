//! API keys in Settings (feature `keys`): [`KeysPage`] lists an app's keys and adds,
//! retires, restores and rotates them, through a [`KeysBackend`].
//!
//! The page does not touch files itself: [`LocalKeys`] is the backend over a
//! [`KeyStore`] on this machine (sapphire keeps its keys in a synced workspace), and an app
//! whose GUI manages a remote server's keys writes one over that server's API (asteria).
//!
//! Every key names a [`Profile`] the app offers; the backend says which ones there are, and
//! the page asks again whenever it lists the keys, so profiles that come and go are seen.

mod page;

pub use corundum_framework_keys::{KeyId, KeyInfo, KeyStore, Profile, ProfileId, Token};
pub use page::KeysPage;

/// What the page asks of its backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeysRequest {
    /// Every key, retired ones included.
    List,
    /// The profiles a key may name.
    Profiles,
    /// Add a key.
    Add {
        /// Its name.
        name: String,
        /// Its note, if any.
        description: Option<String>,
        /// The profile it acts as; one of [`KeysAnswer::Profiles`].
        profile: ProfileId,
    },
    /// Retire a key.
    Retire(KeyId),
    /// Bring a retired key back.
    Restore(KeyId),
    /// Give a key a new token.
    Rotate(KeyId),
}

/// What a backend answers.
#[derive(Clone, Debug)]
pub enum KeysAnswer {
    /// The keys.
    List(Vec<KeyInfo>),
    /// The profiles a key may name.
    Profiles(Vec<Profile>),
    /// A key as it is after a retire or restore.
    Changed(KeyInfo),
    /// A key just added or rotated, and its token, to show once.
    WithToken(KeyInfo, Token),
    /// A request failed; the message is shown to the user.
    Failed(String),
}

/// Where the page's requests go. Answers may come later, through [`poll`](Self::poll),
/// so a backend over a network never blocks a frame.
pub trait KeysBackend {
    /// Start `request`.
    fn send(&mut self, request: KeysRequest);
    /// The answers that arrived since the last call.
    fn poll(&mut self) -> Vec<KeysAnswer>;
}

/// The backend over a [`KeyStore`] on this machine. It answers each request on the next
/// [`poll`](KeysBackend::poll): a store is a few small files.
pub struct LocalKeys {
    store: KeyStore,
    profiles: Box<dyn Fn() -> Vec<Profile>>,
    answers: Vec<KeysAnswer>,
}

impl LocalKeys {
    /// The backend over `store`. `profiles` gives the profiles the app offers; it is called
    /// each time they are asked for and before each add.
    pub fn new(store: KeyStore, profiles: impl Fn() -> Vec<Profile> + 'static) -> LocalKeys {
        LocalKeys {
            store,
            profiles: Box::new(profiles),
            answers: Vec::new(),
        }
    }
}

impl KeysBackend for LocalKeys {
    fn send(&mut self, request: KeysRequest) {
        let store = &mut self.store;
        let answer = match request {
            KeysRequest::List => {
                // Another process (the command line, a sync) may have changed the files.
                match KeyStore::open(store.dir()) {
                    Ok(fresh) => Ok(KeysAnswer::List(
                        fresh.list().iter().map(|k| k.info()).collect(),
                    )),
                    Err(err) => Err(err),
                }
            }
            KeysRequest::Profiles => Ok(KeysAnswer::Profiles((self.profiles)())),
            KeysRequest::Add {
                name,
                description,
                profile,
            } => corundum_framework_keys::check_profile(&(self.profiles)(), &profile)
                .and_then(|()| store.add(&name, description, profile))
                .map(|(k, t)| KeysAnswer::WithToken(k.info(), t)),
            KeysRequest::Retire(id) => store
                .retire(id.as_str())
                .map(|k| KeysAnswer::Changed(k.info())),
            KeysRequest::Restore(id) => store
                .restore(id.as_str())
                .map(|k| KeysAnswer::Changed(k.info())),
            KeysRequest::Rotate(id) => store
                .rotate(id.as_str())
                .map(|(k, t)| KeysAnswer::WithToken(k.info(), t)),
        };
        self.answers
            .push(answer.unwrap_or_else(|err| KeysAnswer::Failed(err.to_string())));
    }

    fn poll(&mut self) -> Vec<KeysAnswer> {
        std::mem::take(&mut self.answers)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    fn admin() -> Profile {
        Profile {
            id: ProfileId::new("admin").unwrap(),
            label: "Admin".into(),
        }
    }

    fn backend(dir: &std::path::Path) -> LocalKeys {
        LocalKeys::new(KeyStore::open(dir).unwrap(), || vec![admin()])
    }

    fn add(profile: &str) -> KeysRequest {
        KeysRequest::Add {
            name: "p".into(),
            description: None,
            profile: ProfileId::new(profile).unwrap(),
        }
    }

    #[test]
    fn local_keys_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = backend(dir.path());
        b.send(add("admin"));
        let Ok::<[KeysAnswer; 1], _>([KeysAnswer::WithToken(info, token)]) = b.poll().try_into()
        else {
            panic!("not a token");
        };
        assert_eq!(info.name, "p");
        assert_eq!(info.profile, admin().id);
        let fresh = KeyStore::open(dir.path()).unwrap();
        assert!(fresh.authenticate(token.expose()).is_some());

        b.send(KeysRequest::Retire(info.id.clone()));
        b.send(KeysRequest::List);
        let answers = b.poll();
        assert!(matches!(&answers[0], KeysAnswer::Changed(k) if k.is_retired()));
        assert!(matches!(&answers[1], KeysAnswer::List(l) if l.len() == 1));
    }

    #[test]
    fn local_keys_reports_failures() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = backend(dir.path());
        b.send(KeysRequest::Retire(KeyId::new("nope").unwrap()));
        assert!(matches!(b.poll().as_slice(), [KeysAnswer::Failed(_)]));
    }

    #[test]
    fn local_keys_answers_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = backend(dir.path());
        b.send(KeysRequest::Profiles);
        assert!(matches!(b.poll().as_slice(), [KeysAnswer::Profiles(v)] if *v == vec![admin()]));
    }

    #[test]
    fn local_keys_refuses_unknown_profile() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = backend(dir.path());
        b.send(add("nope"));
        assert!(matches!(b.poll().as_slice(), [KeysAnswer::Failed(_)]));
        assert!(KeyStore::open(dir.path()).unwrap().list().is_empty());
    }

    #[test]
    fn profile_removed_before_add_fails() {
        let dir = tempfile::tempdir().unwrap();
        let list = Rc::new(RefCell::new(vec![admin()]));
        let shared = list.clone();
        let mut b = LocalKeys::new(KeyStore::open(dir.path()).unwrap(), move || {
            shared.borrow().clone()
        });
        b.send(KeysRequest::Profiles);
        b.poll();
        list.borrow_mut().clear();
        b.send(add("admin"));
        assert!(matches!(b.poll().as_slice(), [KeysAnswer::Failed(_)]));
        assert!(KeyStore::open(dir.path()).unwrap().list().is_empty());
    }
}
