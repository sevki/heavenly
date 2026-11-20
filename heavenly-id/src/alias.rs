//! Human-readable aliases.

use alloc::string::{String, ToString};
use core::fmt;

use crate::IdError;

/// A human-readable, registry-managed name such as `dc:lhr:01`,
/// `earth.surface.gb-lon.01` or `sat:example:42/module-2/rack-1`.
///
/// An alias names a location but is not its identity: it can be renamed or
/// pointed at a different [`heavenly_core::LocationId`] without changing
/// any ID. Aliases are not coordinates either. Nothing about a location's
/// position is derived from its alias.
///
/// Syntax: 1–128 characters of lowercase ASCII letters, digits, `-` and
/// `_`, in segments separated by `:`, `.` or `/`. Segments are non-empty,
/// and the alias must start with a letter.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Alias(String);

impl Alias {
    pub fn new(s: &str) -> Result<Alias, IdError> {
        let bad = |reason| IdError::InvalidAlias {
            alias: s.to_string(),
            reason,
        };
        if s.is_empty() || s.len() > 128 {
            return Err(bad("must be 1 to 128 characters"));
        }
        if !s.as_bytes()[0].is_ascii_lowercase() {
            return Err(bad("must start with a lowercase letter"));
        }
        let mut prev_sep = false;
        for c in s.bytes() {
            let sep = matches!(c, b':' | b'.' | b'/');
            if sep && prev_sep {
                return Err(bad("empty segment"));
            }
            if !(sep || c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_') {
                return Err(bad(
                    "allowed characters are a-z, 0-9, '-', '_' and the separators ':', '.', '/'",
                ));
            }
            prev_sep = sep;
        }
        if prev_sep {
            return Err(bad("must not end with a separator"));
        }
        Ok(Alias(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The segments between separators.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split([':', '.', '/'])
    }
}

impl fmt::Display for Alias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl core::str::FromStr for Alias {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Alias, IdError> {
        Alias::new(s)
    }
}
