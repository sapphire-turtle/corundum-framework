//! Workspace-relative paths.

use std::fmt;

use crate::FsError;

/// A path inside the workspace: `/` between segments, the root `""`.
///
/// It serializes as its string, and deserializing one runs [`WsPath::parse`].
///
/// It is checked when it is made, so one that exists is already normalized: not absolute,
/// no `.`, `..` or empty segment, no `\` or NUL (and on Windows no `:`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WsPath(String);

impl WsPath {
    /// The workspace's root.
    pub fn root() -> WsPath {
        WsPath(String::new())
    }

    /// Checks `s` and makes it a path.
    pub fn parse(s: &str) -> Result<WsPath, FsError> {
        if s.is_empty() {
            return Ok(WsPath::root());
        }
        for segment in s.split('/') {
            check_name(s, segment)?;
        }
        Ok(WsPath(s.to_owned()))
    }

    /// The path as given, `/` between segments.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is the root.
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// Its segments, from the root down; none for the root.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/').filter(|s| !s.is_empty())
    }

    /// Its last segment; `None` for the root.
    pub fn name(&self) -> Option<&str> {
        self.segments().last()
    }

    /// The folder it is in; `None` for the root.
    pub fn parent(&self) -> Option<WsPath> {
        if self.is_root() {
            return None;
        }
        Some(match self.0.rfind('/') {
            Some(i) => WsPath(self.0[..i].to_owned()),
            None => WsPath::root(),
        })
    }

    /// The entry `name` in this folder. `name` is one segment.
    pub fn join(&self, name: &str) -> Result<WsPath, FsError> {
        check_name(name, name)?;
        Ok(if self.is_root() {
            WsPath(name.to_owned())
        } else {
            WsPath(format!("{}/{name}", self.0))
        })
    }

    /// Every path from the first segment down to this one: `a`, `a/b`, `a/b/c` for `a/b/c`.
    pub fn ancestors_and_self(&self) -> impl Iterator<Item = WsPath> + '_ {
        self.0
            .match_indices('/')
            .map(|(i, _)| i)
            .chain((!self.is_root()).then_some(self.0.len()))
            .map(|end| WsPath(self.0[..end].to_owned()))
    }

    /// Whether a segment starts with `.`: such entries are always hidden.
    pub fn has_dot_segment(&self) -> bool {
        self.segments().any(|s| s.starts_with('.'))
    }
}

impl fmt::Display for WsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl serde::Serialize for WsPath {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for WsPath {
    /// Deserializes a string, checked by [`WsPath::parse`].
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<WsPath, D::Error> {
        let s = String::deserialize(d)?;
        WsPath::parse(&s).map_err(serde::de::Error::custom)
    }
}

fn check_name(path: &str, name: &str) -> Result<(), FsError> {
    let why = if name.is_empty() {
        "an empty segment (or a leading or trailing `/`)"
    } else if name == "." || name == ".." {
        "a `.` or `..` segment"
    } else if name.contains('/') {
        "a `/` in a name"
    } else if name.contains('\\') {
        "a `\\`"
    } else if name.contains('\0') {
        "a NUL"
    } else if cfg!(windows) && name.contains(':') {
        "a `:`"
    } else {
        return Ok(());
    };
    Err(FsError::InvalidPath(format!("{path:?} has {why}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FsError;
    use corundum_framework_fs_api::codes;

    fn p(s: &str) -> WsPath {
        WsPath::parse(s).unwrap()
    }

    #[test]
    fn accepts_normal_paths() {
        assert!(p("").is_root());
        assert_eq!(WsPath::root(), p(""));
        let path = p("a/b c/d.txt");
        assert_eq!(path.segments().collect::<Vec<_>>(), ["a", "b c", "d.txt"]);
        assert_eq!(path.name(), Some("d.txt"));
        assert_eq!(path.parent(), Some(p("a/b c")));
        assert_eq!(p("a").parent(), Some(WsPath::root()));
        assert_eq!(WsPath::root().parent(), None);
        assert_eq!(WsPath::root().name(), None);
        assert_eq!(path.as_str(), "a/b c/d.txt");
    }

    #[test]
    fn refuses_bad_paths() {
        let mut bad = vec![
            "/a", "a/", "a//b", ".", "a/./b", "..", "a/../b", "a\\b", "a\0b",
        ];
        if cfg!(windows) {
            bad.extend(["C:", "a:b"]);
        }
        for s in bad {
            assert!(
                matches!(WsPath::parse(s), Err(FsError::InvalidPath(_))),
                "{s:?} was accepted"
            );
        }
    }

    #[test]
    fn join_checks_the_name() {
        assert_eq!(WsPath::root().join("a").unwrap(), p("a"));
        assert_eq!(p("a").join("b").unwrap(), p("a/b"));
        for name in ["x/y", "..", ""] {
            assert!(
                matches!(p("a").join(name), Err(FsError::InvalidPath(_))),
                "{name:?} was joined"
            );
        }
    }

    #[test]
    fn dot_segments_are_found() {
        assert!(p(".asteria/trash").has_dot_segment());
        assert!(p("a/.git").has_dot_segment());
        assert!(!p("a.b/c").has_dot_segment());
        assert!(!WsPath::root().has_dot_segment());
    }

    #[test]
    fn ancestors_and_self_go_down() {
        let got: Vec<WsPath> = p("a/b/c").ancestors_and_self().collect();
        assert_eq!(got, [p("a"), p("a/b"), p("a/b/c")]);
        assert_eq!(WsPath::root().ancestors_and_self().count(), 0);
    }

    #[test]
    fn error_codes_match_the_api() {
        let io = || std::io::Error::other("x");
        let cases = [
            (FsError::InvalidPath("x".into()), codes::INVALID_PATH),
            (FsError::NotFound("x".into()), codes::NOT_FOUND),
            (FsError::TooLarge("x".into()), codes::TOO_LARGE),
            (FsError::InvalidParams("x".into()), codes::INVALID_PARAMS),
            (FsError::MethodNotFound("x".into()), codes::METHOD_NOT_FOUND),
            (
                FsError::Io {
                    path: "x".into(),
                    source: io(),
                },
                codes::INTERNAL_ERROR,
            ),
        ];
        for (e, code) in cases {
            assert_eq!(e.code(), code, "{e}");
        }
    }
}
