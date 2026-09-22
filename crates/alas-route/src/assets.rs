// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/integration/assets.py
// Reference: alas @ rust-port-baseline.

//! The optional downloadable assets: where each one lives, and whether it is
//! there.
//!
//! Neither ships with the program. The enroute navigation data is published
//! under the GPL, and distributing it would impose that licence on anyone
//! redistributing this program. The Earth texture is public domain and is left
//! out only to keep the download small. Both are optional: without the
//! navigation data, routing falls back to a great circle, and without the
//! texture the globe renders untextured.
//!
//! # What is here and what is not
//!
//! The locations, the completeness check and the minimum sizes are here,
//! because the routing and reporting layers ask for them. The downloading is
//! not: fetching over HTTPS needs a client and a TLS stack, and this crate
//! computes distances. `alas-app` performs the transfers against the addresses
//! and floors stated here. A documented scope boundary, not a
//! `deviation-candidate`.
//!
//! The minimum sizes are the load-bearing part of that contract and are stated
//! here rather than left to the downloader. A truncated fix file parses
//! perfectly happily and produces a quietly wrong route, so a transfer that
//! does not clear the floor must not be moved into place at all. They are
//! conservative floors and not exact sizes, because the data is revised
//! periodically.

use std::path::{Path, PathBuf};

/// Where the navigation data is published.
///
/// This tracks a third party's default branch, so the data can change beneath
/// a design run that is meant to be reproducible. Pinning to a commit would fix
/// that; it is unpinned only because the mirror publishes no tags to pin to.
pub const NAVDATA_BASE_URL: &str =
    "https://raw.githubusercontent.com/mcantsin/x-plane-navdata/master";

/// Where the Earth texture is published.
pub const TEXTURE_URL: &str =
    "https://upload.wikimedia.org/wikipedia/commons/c/c4/Land_shallow_topo_2048.jpg";

/// The navigation-data directory, relative to the data root.
pub const NAVDATA_REL: &str = "alas/data/navdata";
/// The Earth texture, relative to the data root.
pub const TEXTURE_REL: &str = "alas/data/textures/earth_blue_marble.jpg";

/// The smallest a completed texture transfer may be.
pub const MIN_TEXTURE_BYTES: u64 = 200_000;

/// One navigation-data file and the size below which it is a truncated
/// transfer or an error page rather than data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavdataFile {
    /// Its name, in the published directory and on disk.
    pub name: &'static str,
    /// The floor a completed transfer must clear.
    pub min_bytes: u64,
    /// A SHA-256, lowercase hex, of the last content a maintainer reviewed
    /// and accepted for this file, or `None` when no review has pinned one
    /// yet, in which case `min_bytes` alone still gates a completed
    /// transfer, exactly as before this field existed.
    ///
    /// This is a **reviewed-content pin, not an official release checksum**:
    /// this mirror (`NAVDATA_BASE_URL` above) publishes no version tags, so a
    /// pinned value here can only mean "matches what a maintainer looked at
    /// and accepted on the stated review date," never "matches what X-Plane
    /// or the mirror's author intended to ship." A mismatch means the
    /// content changed since that review and needs a human to look at it
    /// again: an ordinary upstream edit to the tracked `master` branch
    /// produces the same symptom as tampering, so a mismatch must not be
    /// reported as proof of the latter.
    ///
    /// The three values below were pinned 2026-09-22 from a direct HTTPS
    /// fetch of each file at `NAVDATA_BASE_URL`, cross-checked against the
    /// byte sizes GitHub's contents API independently reports for the same
    /// commit, with the mirror's GPL-3.0 licence claim confirmed from its
    /// own `README.md` (matching `THIRD-PARTY-NOTICES.md`). Retained
    /// fetch logs, headers and hash evidence:
    /// `deliverables/alas-v1.2-dispatch/navdata-review/`. This mirror is a
    /// single-maintainer personal repository with no cryptographic release
    /// signature and no push since 2019; the pin only attests to content
    /// reviewed on that date, not to the publisher's identity or intent.
    ///
    /// To refresh a value after a legitimate upstream change: fetch the file
    /// over HTTPS, read and accept its content, then hash it (`sha256sum
    /// <file>` on Linux, `Get-FileHash <file> -Algorithm SHA256` on Windows),
    /// place the lowercase hex digest here, and update the review note
    /// above: the verification itself is already implemented and enforced
    /// by `alas_exec::download::download_files` whenever a spec carries a
    /// hash.
    pub expected_sha256: Option<&'static str>,
}

/// The three files a complete navigation-data set has.
pub static NAVDATA_FILES: [NavdataFile; 3] = [
    NavdataFile {
        name: "earth_fix.dat",
        min_bytes: 1_000_000,
        expected_sha256: Some("ae60780b7bab8f09f93c3396349d63b4c3b471c27206bd8bb87baa21a3c15ed6"),
    },
    NavdataFile {
        name: "earth_awy.dat",
        min_bytes: 1_000_000,
        expected_sha256: Some("eb2c2671c2e29618c8769677e00c05ccfcd976697632cb74dfb34bfb8be96e24"),
    },
    NavdataFile {
        name: "earth_nav.dat",
        min_bytes: 500_000,
        expected_sha256: Some("1968bbe7bc24a189f15a6df2bfdff4b3502856bc8487f025b86c6cbfd81b0fae"),
    },
];

/// Whether an optional asset is installed, and what to tell the operator if it
/// is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetStatus {
    /// Whether it can be used.
    pub available: bool,
    /// What to show on the external-tools page.
    pub detail: String,
}

/// Where to download one navigation-data file from.
pub fn navdata_file_url(file: &NavdataFile) -> String {
    format!("{NAVDATA_BASE_URL}/{}", file.name)
}

/// Whether one navigation-data file is a complete-looking regular file.
///
/// Existence alone is not enough here: a killed download leaves a file behind
/// and the parsers would otherwise accept it as a valid, silently incomplete
/// navigation graph.
pub fn navdata_file_is_usable(navdata_dir: &Path, file: &NavdataFile) -> bool {
    navdata_dir
        .join(file.name)
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() >= file.min_bytes)
}

/// Whether a directory holds a complete navigation-data set.
pub fn navdata_status(navdata_dir: &Path) -> AssetStatus {
    let valid = NAVDATA_FILES
        .iter()
        .filter(|file| navdata_file_is_usable(navdata_dir, file))
        .count();
    let present = NAVDATA_FILES
        .iter()
        .filter(|file| navdata_dir.join(file.name).is_file())
        .count();
    let where_it_is = navdata_dir.display();
    if valid == NAVDATA_FILES.len() {
        AssetStatus {
            available: true,
            detail: format!("valid at {where_it_is}"),
        }
    } else {
        AssetStatus {
            available: false,
            detail: format!(
                "incomplete ({valid}/{} valid files; {present}/{} present) at {where_it_is}",
                NAVDATA_FILES.len(),
                NAVDATA_FILES.len(),
            ),
        }
    }
}

/// Whether the Earth texture is installed.
pub fn texture_status(texture_path: &Path) -> AssetStatus {
    let where_it_is = texture_path.display();
    if texture_path
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() >= MIN_TEXTURE_BYTES)
    {
        AssetStatus {
            available: true,
            detail: format!("valid at {where_it_is}"),
        }
    } else {
        AssetStatus {
            available: false,
            detail: format!("missing or incomplete at {where_it_is}"),
        }
    }
}

/// The navigation-data directory under a data root.
pub fn navdata_dir(data_root: &Path) -> PathBuf {
    data_root.join(NAVDATA_REL)
}

/// The Earth texture's path under a data root.
pub fn texture_path(data_root: &Path) -> PathBuf {
    data_root.join(TEXTURE_REL)
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_with_nothing_in_it_reports_how_much_is_missing() {
        let status = navdata_status(Path::new("no/such/directory"));
        assert!(!status.available);
        assert!(status.detail.contains("0/3"), "{}", status.detail);
    }

    #[test]
    fn every_navdata_file_has_a_floor_a_truncated_transfer_would_fail() {
        // The floors are what stop a half-written fix file being parsed into a
        // quietly wrong route, so a file listed without one is a gap.
        for file in &NAVDATA_FILES {
            assert!(file.min_bytes > 0, "{} has no size floor", file.name);
            assert!(navdata_file_url(file).ends_with(file.name));
        }
    }

    #[test]
    fn files_below_the_floor_are_not_reported_as_available() {
        let directory =
            std::env::temp_dir().join(format!("alas-navdata-short-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("test navdata directory");
        for file in &NAVDATA_FILES {
            std::fs::write(directory.join(file.name), b"short").expect("short test file");
        }
        let status = navdata_status(&directory);
        assert!(!status.available);
        assert!(status.detail.contains("0/3 valid"), "{}", status.detail);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn the_two_files_the_router_reads_are_both_in_the_set() {
        // `navdata::NavdataGraph` reads these two by name; a set that stopped
        // carrying one would download an incomplete pair and report success.
        let names: Vec<&str> = NAVDATA_FILES.iter().map(|file| file.name).collect();
        assert!(names.contains(&crate::navdata::FIX_FILE));
        assert!(names.contains(&crate::navdata::AIRWAY_FILE));
    }
}
