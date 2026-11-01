//! A key's profile: an id only the app understands, and the list the app offers.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A profile's id: any non-empty string. corundum stores and passes it; what the profile
/// may do is the app's to decide.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct ProfileId(String);

impl ProfileId {
    /// `s`, if it is not empty.
    pub fn new(s: &str) -> Result<ProfileId> {
        if s.is_empty() {
            Err(Error::InvalidProfile(s.to_owned()))
        } else {
            Ok(ProfileId(s.to_owned()))
        }
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProfileId({})", self.0)
    }
}

impl<'de> Deserialize<'de> for ProfileId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<ProfileId, D::Error> {
        let s = String::deserialize(d)?;
        ProfileId::new(&s).map_err(serde::de::Error::custom)
    }
}

/// A profile the app offers to choose from, when a key is added.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// What a key stores.
    pub id: ProfileId,
    /// What the user sees; never stored.
    pub label: String,
}

/// `Ok` if `id` is one of `profiles`; otherwise [`Error::UnknownProfile`], with the ids
/// there are.
pub fn check_profile(profiles: &[Profile], id: &ProfileId) -> Result<()> {
    if profiles.iter().any(|p| &p.id == id) {
        Ok(())
    } else {
        Err(Error::UnknownProfile {
            id: id.to_string(),
            known: profiles.iter().map(|p| p.id.clone()).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    fn id(s: &str) -> ProfileId {
        ProfileId::new(s).unwrap()
    }

    fn profiles() -> Vec<Profile> {
        vec![
            Profile {
                id: id("admin"),
                label: "Admin".into(),
            },
            Profile {
                id: id("editor"),
                label: "Editor".into(),
            },
        ]
    }

    #[test]
    fn empty_id_is_invalid() {
        assert!(matches!(ProfileId::new(""), Err(Error::InvalidProfile(_))));
        assert_eq!(id("room-1").as_str(), "room-1");
    }

    #[test]
    fn deserialize_validates() {
        assert!(serde_json::from_str::<ProfileId>("\"\"").is_err());
        assert!(serde_json::from_str::<ProfileId>("\"x\"").is_ok());
    }

    #[test]
    fn check_profile_lists_known_ids() {
        let p = profiles();
        assert!(check_profile(&p, &id("admin")).is_ok());
        let err = check_profile(&p, &id("x")).unwrap_err();
        assert!(err.to_string().contains("admin, editor"), "{err}");
        match err {
            Error::UnknownProfile { id: got, known } => {
                assert_eq!(got, "x");
                assert_eq!(known, vec![id("admin"), id("editor")]);
            }
            other => panic!("{other:?}"),
        }
    }
}
