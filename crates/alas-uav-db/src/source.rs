// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Data sources and the redistribution policy attached to each.
//!
//! Legal cleanliness is data, not folklore: every [`Source`] records its
//! licence, the dates its terms and robots.txt were last read, and a
//! [`Redistribution`] class. A bundle that ships in the repository may cite
//! only sources whose class permits it (see `Database::validate_for_shipping`).

use serde::{Deserialize, Serialize};

use crate::date::IsoDate;
use crate::error::{DbError, DbResult};
use crate::ids::SourceId;

/// What kind of publisher a source is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Manufacturer datasheet or product page.
    Manufacturer,
    /// Retailer product page.
    Retailer,
    /// Peer-reviewed or institutional measurement set (for example UIUC).
    Academic,
    /// Competition organiser document or table.
    Organiser,
    /// Crowd-sourced or vendor-neutral test database (for example Tyto).
    TestDatabase,
    /// Team-owned logs, bench data or coupon tests.
    Team,
    /// Entered by the end user (overlay).
    User,
}

/// May the facts cited from this source be shipped in the public repository?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Redistribution {
    /// Individual facts (Kv, mass, ...) with citation, as our own curation
    /// (plan section 7.1 rank 1). Raw tables, images and datasheets are never
    /// copied.
    FactsWithCitation,
    /// Written permission is on file for the stated scope.
    PermissionGranted,
    /// Use only through a user-side fetch; never bundled.
    FetchOnDemandOnly,
    /// Terms forbid copying (for example eCalc); manual cross-check only.
    Forbidden,
    /// Not yet reviewed; treated like [`Self::FetchOnDemandOnly`].
    Unreviewed,
    /// The user's own data, never shipped by ALAS.
    UserOwned,
}

impl Redistribution {
    /// Whether a public bundle may cite this source.
    pub fn allows_shipping(self) -> bool {
        matches!(self, Self::FactsWithCitation | Self::PermissionGranted)
    }
}

/// A cited publication, page or dataset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// Stable identifier referenced by every field.
    pub id: SourceId,
    /// Canonical https URL, or a descriptive locator for non-web sources.
    pub url: String,
    /// Publisher class.
    pub kind: SourceKind,
    /// Licence text or "none stated".
    pub licence: String,
    /// Redistribution class used by the shipping gate.
    pub redistribution: Redistribution,
    /// Date the terms of service were last read by a human.
    #[serde(default)]
    pub tos_checked_at: Option<IsoDate>,
    /// Date robots.txt was last read.
    #[serde(default)]
    pub robots_checked_at: Option<IsoDate>,
    /// `sha256:<64 hex>` of the retrieved document, when one was kept.
    #[serde(default)]
    pub content_hash: Option<String>,
    /// Citation text (publisher, title, edition or page, retrieval date).
    pub citation: String,
}

impl Source {
    /// Check required text and the hash format.
    pub fn validate(&self) -> DbResult<()> {
        if self.url.trim().is_empty() || self.citation.trim().is_empty() {
            return Err(DbError::new(format!(
                "source '{}' needs a url and a citation",
                self.id.as_str()
            )));
        }
        if let Some(hash) = &self.content_hash {
            let ok = hash.strip_prefix("sha256:").is_some_and(|hex| {
                hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            });
            if !ok {
                return Err(DbError::new(format!(
                    "source '{}' content_hash must be sha256:<64 lowercase hex>",
                    self.id.as_str()
                )));
            }
        }
        let web = matches!(self.kind, SourceKind::Manufacturer | SourceKind::Retailer);
        if web && !self.url.starts_with("https://") {
            return Err(DbError::new(format!(
                "source '{}' must cite an https URL",
                self.id.as_str()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(hash: Option<&str>) -> Result<Source, DbError> {
        Ok(Source {
            id: SourceId::new("tmotor-at2814")?,
            url: "https://example.org/p".into(),
            kind: SourceKind::Manufacturer,
            licence: "none stated".into(),
            redistribution: Redistribution::FactsWithCitation,
            tos_checked_at: None,
            robots_checked_at: None,
            content_hash: hash.map(str::to_owned),
            citation: "Example, product page".into(),
        })
    }

    #[test]
    fn hash_format_is_enforced() -> Result<(), DbError> {
        assert!(source(None)?.validate().is_ok());
        let good = format!("sha256:{}", "ab".repeat(32));
        assert!(source(Some(&good))?.validate().is_ok());
        assert!(source(Some("sha256:xyz"))?.validate().is_err());
        Ok(())
    }

    #[test]
    fn only_clean_classes_may_ship() {
        assert!(Redistribution::FactsWithCitation.allows_shipping());
        assert!(Redistribution::PermissionGranted.allows_shipping());
        assert!(!Redistribution::FetchOnDemandOnly.allows_shipping());
        assert!(!Redistribution::Forbidden.allows_shipping());
        assert!(!Redistribution::Unreviewed.allows_shipping());
    }
}
