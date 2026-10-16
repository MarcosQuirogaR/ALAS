// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Stable lowercase-slug identifiers for components and sources.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::DbError;

fn validate_slug(kind: &str, text: &str) -> Result<(), DbError> {
    let ok = !text.is_empty()
        && text.len() <= 96
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.');
    if ok {
        Ok(())
    } else {
        Err(DbError::new(format!(
            "{kind} id '{text}' is not a lowercase slug (a-z, 0-9, '-', '.'; at most 96 bytes)"
        )))
    }
}

macro_rules! slug_id {
    ($(#[$doc:meta])* $name:ident, $kind:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Validate and wrap a slug.
            pub fn new(text: &str) -> Result<Self, DbError> {
                validate_slug($kind, text)?;
                Ok(Self(text.to_owned()))
            }

            /// The slug text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                Self::new(&text).map_err(serde::de::Error::custom)
            }
        }
    };
}

slug_id!(
    /// Identifier of a [`crate::Source`].
    SourceId,
    "source"
);
slug_id!(
    /// Identifier of a component (stable across spec revisions).
    ComponentId,
    "component"
);

/// Lowercase a free-text name into a slug fragment (`[a-z0-9-]`, no runs).
pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_reject_uppercase_and_spaces() {
        assert!(ComponentId::new("tmotor-at2814-kv900").is_ok());
        assert!(ComponentId::new("T-Motor").is_err());
        assert!(SourceId::new("a b").is_err());
        assert!(SourceId::new("").is_err());
    }

    #[test]
    fn slugify_collapses_separators() {
        assert_eq!(slugify("T-MOTOR AT2814 / KV900"), "t-motor-at2814-kv900");
        assert_eq!(slugify("  --x"), "x");
    }
}
