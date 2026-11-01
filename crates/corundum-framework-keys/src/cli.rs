//! The subcommand that manages a store from a terminal (feature `clap`).

use std::io::Write;

use crate::error::Result;
use crate::{Key, KeyStore, Profile, ProfileId, Token, check_profile};

/// Manage API keys. Mount it under any name; it works on the files, so it needs no
/// running server.
#[derive(Clone, Debug, clap::Subcommand)]
pub enum KeyCommand {
    /// List the keys.
    List {
        /// Include retired keys.
        #[arg(long)]
        all: bool,
    },
    /// Add a key and print its token, once.
    Add {
        /// A name for the client that will use it.
        name: String,
        /// A note for yourself.
        #[arg(long)]
        description: Option<String>,
        /// The profile the key acts as; one the app offers.
        #[arg(long)]
        profile: String,
    },
    /// Retire a key: its token stops working.
    Retire {
        /// Its id or name.
        selector: String,
    },
    /// Bring a retired key back.
    Restore {
        /// Its id or name.
        selector: String,
    },
    /// Give a key a new token and print it, once; the old one stops working.
    Rotate {
        /// Its id or name.
        selector: String,
    },
}

impl KeyCommand {
    /// Carry out the command on `store`, printing to stdout and stderr; the exit code.
    /// `profiles` are the ones the app offers: `add` takes one of them, `list` shows their
    /// labels.
    pub fn run(self, store: &mut KeyStore, profiles: &[Profile]) -> Result<i32> {
        self.run_to(
            store,
            profiles,
            &mut std::io::stdout(),
            &mut std::io::stderr(),
        )
    }

    /// As [`run`](Self::run), printing to `out` and `err`. The token alone goes to `out`,
    /// so `token=$(app api-key add pendant --profile admin)` works.
    pub fn run_to(
        self,
        store: &mut KeyStore,
        profiles: &[Profile],
        out: &mut dyn Write,
        err: &mut dyn Write,
    ) -> Result<i32> {
        match self {
            KeyCommand::List { all } => {
                for key in store.list().iter().filter(|k| all || !k.is_retired()) {
                    print(out, &line(key, profiles))?;
                }
            }
            KeyCommand::Add {
                name,
                description,
                profile,
            } => {
                let profile = ProfileId::new(&profile)?;
                check_profile(profiles, &profile)?;
                let (key, token) = store.add(&name, description, profile)?;
                show_token(out, err, &key, &token)?;
            }
            KeyCommand::Retire { selector } => {
                let key = store.retire(&selector)?;
                print(err, &format!("retired {} ({})", key.name, key.id))?;
            }
            KeyCommand::Restore { selector } => {
                let key = store.restore(&selector)?;
                print(err, &format!("restored {} ({})", key.name, key.id))?;
            }
            KeyCommand::Rotate { selector } => {
                let (key, token) = store.rotate(&selector)?;
                show_token(out, err, &key, &token)?;
            }
        }
        Ok(0)
    }
}

/// `id  name  profile  created  [retired]`; the profile by its label, or
/// `<id> (unknown)`.
fn line(key: &Key, profiles: &[Profile]) -> String {
    let profile = match profiles.iter().find(|p| p.id == key.profile) {
        Some(p) => p.label.clone(),
        None => format!("{} (unknown)", key.profile),
    };
    let mut line = format!(
        "{}  {}  {}  {}",
        key.id,
        key.name,
        profile,
        key.created_at.format("%Y-%m-%d %H:%M")
    );
    if key.is_retired() {
        line.push_str("  retired");
    }
    line
}

fn show_token(out: &mut dyn Write, err: &mut dyn Write, key: &Key, token: &Token) -> Result<()> {
    print(
        err,
        &format!(
            "the token for {} ({}) follows; it is shown only once",
            key.name, key.id
        ),
    )?;
    print(out, token.expose())
}

fn print(to: &mut dyn Write, text: &str) -> Result<()> {
    writeln!(to, "{text}").map_err(|source| crate::Error::Io {
        path: "<output>".into(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profiles() -> Vec<Profile> {
        ["admin", "editor"]
            .iter()
            .map(|id| Profile {
                id: ProfileId::new(id).unwrap(),
                label: id[..1].to_uppercase() + &id[1..],
            })
            .collect()
    }

    fn run(store: &mut KeyStore, c: KeyCommand) -> (Result<i32>, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let r = c.run_to(store, &profiles(), &mut out, &mut err);
        (
            r,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    fn add(name: &str) -> KeyCommand {
        KeyCommand::Add {
            name: name.into(),
            description: None,
            profile: "admin".into(),
        }
    }

    fn add_as(name: &str, profile: &str) -> KeyCommand {
        KeyCommand::Add {
            name: name.into(),
            description: None,
            profile: profile.into(),
        }
    }

    #[test]
    fn add_with_unknown_profile_fails() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = KeyStore::open(dir.path()).unwrap();
        let (r, out, _) = run(&mut s, add_as("p", "nope"));
        let err = r.unwrap_err();
        assert!(
            matches!(err, crate::Error::UnknownProfile { .. }),
            "{err:?}"
        );
        assert!(err.to_string().contains("admin, editor"), "{err}");
        assert!(out.is_empty());
        assert!(KeyStore::open(dir.path()).unwrap().list().is_empty());
    }

    #[test]
    fn add_with_empty_profile_fails() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = KeyStore::open(dir.path()).unwrap();
        let (r, _, _) = run(&mut s, add_as("p", ""));
        assert!(matches!(r, Err(crate::Error::InvalidProfile(_))));
    }

    #[test]
    fn list_shows_profile_label_or_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = KeyStore::open(dir.path()).unwrap();
        s.add("a", None, ProfileId::new("admin").unwrap()).unwrap();
        s.add("b", None, ProfileId::new("gone").unwrap()).unwrap();
        let (_, out, _) = run(&mut s, KeyCommand::List { all: false });
        let line_of = |name: &str| {
            out.lines()
                .find(|l| l.contains(&format!("  {name}  ")))
                .unwrap_or_else(|| panic!("no line for {name}: {out}"))
                .to_owned()
        };
        assert!(line_of("a").contains("Admin"), "{out}");
        assert!(line_of("b").contains("gone (unknown)"), "{out}");
    }

    #[test]
    fn add_prints_only_the_token() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = KeyStore::open(dir.path()).unwrap();
        let (r, out, err) = run(&mut s, add("p"));
        assert_eq!(r.unwrap(), 0);
        let token = out.strip_suffix('\n').unwrap();
        assert!(!token.contains('\n'));
        assert!(s.authenticate(token).is_some());
        assert!(err.contains("only once"), "{err}");
    }

    #[test]
    fn list_hides_retired_unless_all() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = KeyStore::open(dir.path()).unwrap();
        s.add("live", None, crate::ProfileId::new("p").unwrap())
            .unwrap();
        s.add("gone", None, crate::ProfileId::new("p").unwrap())
            .unwrap();
        s.retire("gone").unwrap();
        let (_, out, _) = run(&mut s, KeyCommand::List { all: false });
        assert!(out.contains("live") && !out.contains("gone"), "{out}");
        let (_, out, _) = run(&mut s, KeyCommand::List { all: true });
        assert!(out.contains("live") && out.contains("gone") && out.contains("retired"));
    }

    #[test]
    fn rotate_prints_the_new_token() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = KeyStore::open(dir.path()).unwrap();
        let (_, old) = s
            .add("p", None, crate::ProfileId::new("p").unwrap())
            .unwrap();
        let (r, out, _) = run(
            &mut s,
            KeyCommand::Rotate {
                selector: "p".into(),
            },
        );
        assert_eq!(r.unwrap(), 0);
        let new = out.trim_end();
        assert_ne!(new, old.expose());
        assert!(s.authenticate(new).is_some());
    }

    #[test]
    fn unknown_selector_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = KeyStore::open(dir.path()).unwrap();
        let (r, _, _) = run(
            &mut s,
            KeyCommand::Retire {
                selector: "nope".into(),
            },
        );
        assert!(matches!(r, Err(crate::Error::NotFound(_))));
    }
}
