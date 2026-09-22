// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Typed OpenFOAM version identification and template compatibility.
//!
//! The reusable 2-D airfoil template was written against the OpenCFD
//! `v2306` documentation (`libs (forces)` short library names, blended
//! `omegaWallFunction`, `checkMesh -writeAllFields`, `simpleFoam
//! -postProcess`).  A probe therefore classifies the reported version instead
//! of treating any banner as usable: OpenCFD releases from the documentation
//! baseline are supported, older OpenCFD and Foundation releases up to 10 are
//! untested, and Foundation 11 or later is unsupported because `simpleFoam`
//! was replaced by the modular `foamRun` solvers.

use super::*;

/// OpenCFD release (`vYYMM`) the template dictionaries were written against.
pub const TEMPLATE_BASELINE_RELEASE: u32 = 2306;

/// Last OpenFOAM Foundation major release that still ships `simpleFoam`.
pub const FOUNDATION_LAST_RELEASE_WITH_SIMPLEFOAM: u32 = 10;

/// OpenFOAM distribution family inferred from a banner or directory name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenFoamDistribution {
    /// OpenCFD/ESI releases (`vYYMM`, openfoam.com).
    OpenCfd,
    /// OpenFOAM Foundation releases (integer major, openfoam.org).
    Foundation,
    /// The text did not identify a distribution family.
    Unknown,
}

impl OpenFoamDistribution {
    /// Human-readable family label for the setup card and reports.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::OpenCfd => "OpenCFD (openfoam.com)",
            Self::Foundation => "Foundation (openfoam.org)",
            Self::Unknown => "unknown distribution",
        }
    }
}

/// Structured version identity of one OpenFOAM installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenFoamVersion {
    /// Distribution family.
    pub distribution: OpenFoamDistribution,
    /// Version token as printed, for example `v2306`, `11` or `dev`.
    pub label: String,
    /// Numeric release: `YYMM` for OpenCFD, the major number for Foundation.
    /// `None` for development builds or unrecognised labels.
    pub release: Option<u32>,
}

impl OpenFoamVersion {
    /// Parse a utility banner, `foamVersion` output, an environment dump or
    /// a project directory name such as `OpenFOAM-v2306`.
    pub fn parse(text: &str) -> Option<Self> {
        let lower = text.to_ascii_lowercase();
        let hint = if lower.contains("openfoam.com") {
            OpenFoamDistribution::OpenCfd
        } else if lower.contains("openfoam.org") {
            OpenFoamDistribution::Foundation
        } else {
            OpenFoamDistribution::Unknown
        };
        let label = version_label(text)?;
        Some(Self::from_label(&label, hint))
    }

    /// Build the identity from a bare version label and an optional
    /// distribution hint taken from the surrounding text.
    pub fn from_label(label: &str, hint: OpenFoamDistribution) -> Self {
        let label = label.trim().trim_matches(|ch: char| ch == '(' || ch == ')');
        let digits = label.trim_start_matches(['v', 'V']);
        let numeric = digits
            .split('.')
            .next()
            .and_then(|token| token.parse::<u32>().ok());
        let looks_opencfd = label.starts_with(['v', 'V']) && digits.len() == 4 && numeric.is_some();
        let looks_foundation = !label.starts_with(['v', 'V'])
            && numeric.is_some_and(|value| value <= 99)
            && digits.chars().all(|ch| ch.is_ascii_digit() || ch == '.');
        // A bare `dev` label with no site hint is the Foundation development
        // branch; OpenCFD development builds print `openfoam.com` beside it.
        let unhinted_dev =
            label.eq_ignore_ascii_case("dev") && hint == OpenFoamDistribution::Unknown;
        let distribution = if looks_opencfd {
            OpenFoamDistribution::OpenCfd
        } else if looks_foundation || unhinted_dev {
            OpenFoamDistribution::Foundation
        } else {
            hint
        };
        Self {
            distribution,
            label: label.to_owned(),
            release: numeric,
        }
    }

    /// Conventional qualified name, for example `OpenFOAM-v2306`.
    pub fn qualified_name(&self) -> String {
        format!("OpenFOAM-{}", self.label)
    }

    /// Compatibility of this version with the versioned airfoil template.
    pub fn assess(&self) -> OpenFoamVersionAssessment {
        match (self.distribution, self.release) {
            (OpenFoamDistribution::OpenCfd, Some(release))
                if release >= TEMPLATE_BASELINE_RELEASE =>
            {
                OpenFoamVersionAssessment::new(
                    OpenFoamSupportLevel::Supported,
                    format!(
                        "OpenCFD {} is at or after the v{TEMPLATE_BASELINE_RELEASE} template baseline.",
                        self.label
                    ),
                )
            }
            (OpenFoamDistribution::OpenCfd, Some(_)) => OpenFoamVersionAssessment::new(
                OpenFoamSupportLevel::Untested,
                format!(
                    "OpenCFD {} predates the v{TEMPLATE_BASELINE_RELEASE} template baseline; dictionary keywords were not exercised on it.",
                    self.label
                ),
            ),
            (OpenFoamDistribution::Foundation, Some(release))
                if release > FOUNDATION_LAST_RELEASE_WITH_SIMPLEFOAM =>
            {
                OpenFoamVersionAssessment::new(
                    OpenFoamSupportLevel::Unsupported,
                    format!(
                        "OpenFOAM Foundation {} replaced simpleFoam with the modular foamRun solvers; the template requires simpleFoam.",
                        self.label
                    ),
                )
            }
            (OpenFoamDistribution::Foundation, _) => OpenFoamVersionAssessment::new(
                OpenFoamSupportLevel::Untested,
                format!(
                    "OpenFOAM Foundation {} ships simpleFoam, but the template dictionaries were validated only against OpenCFD v{TEMPLATE_BASELINE_RELEASE}.",
                    self.label
                ),
            ),
            _ => OpenFoamVersionAssessment::new(
                OpenFoamSupportLevel::Untested,
                format!(
                    "Version label {} could not be mapped to a known distribution release.",
                    self.label
                ),
            ),
        }
    }
}

/// Compatibility class of a probed version with the versioned template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenFoamSupportLevel {
    /// The template dictionaries target this release line.
    Supported,
    /// Expected to run, but not exercised; results need their own evidence.
    Untested,
    /// A required utility or dictionary contract is known to be absent.
    Unsupported,
}

impl OpenFoamSupportLevel {
    /// Stable label for the setup card, logs and reports.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Supported => "Supported",
            Self::Untested => "Untested",
            Self::Unsupported => "Unsupported",
        }
    }
}

/// Version compatibility with an actionable reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenFoamVersionAssessment {
    /// Compatibility class.
    pub level: OpenFoamSupportLevel,
    /// Why the class was assigned, suitable for the setup card.
    pub reason: String,
}

impl OpenFoamVersionAssessment {
    fn new(level: OpenFoamSupportLevel, reason: String) -> Self {
        Self { level, reason }
    }

    /// Assessment used when no version could be identified.
    pub fn unknown() -> Self {
        Self::new(
            OpenFoamSupportLevel::Untested,
            "No OpenFOAM version was identified; compatibility with the template could not be assessed."
                .to_owned(),
        )
    }

    /// Assess an optional parsed version.
    pub fn from_version(version: Option<&OpenFoamVersion>) -> Self {
        version.map_or_else(Self::unknown, OpenFoamVersion::assess)
    }
}

impl Default for OpenFoamVersionAssessment {
    fn default() -> Self {
        Self::unknown()
    }
}

/// Extract the bare version label from banner-like text.
///
/// Recognised forms, in priority order: `OpenFOAM-v2306`, `OpenFOAM-11`,
/// `Version:  v2306`, `Version: 11`, `WM_PROJECT_VERSION=v2306`, and the
/// parenthesised `(2306)` release printed after an OpenCFD `Using:` line.
fn version_label(text: &str) -> Option<String> {
    for token in text.split_whitespace() {
        let token = token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-');
        if let Some(rest) = token.strip_prefix("OpenFOAM-") {
            if is_version_token(rest) {
                return Some(rest.to_owned());
            }
        }
    }
    for line in text.lines() {
        let trimmed = line.trim();
        for prefix in ["Version:", "WM_PROJECT_VERSION="] {
            if let Some(index) = trimmed.find(prefix) {
                let candidate = trimmed[index + prefix.len()..]
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.');
                if is_version_token(candidate) {
                    return Some(candidate.to_owned());
                }
            }
        }
    }
    text.split_whitespace().find_map(|token| {
        let inner = token.strip_prefix('(')?.strip_suffix(')')?;
        (inner.len() == 4 && inner.chars().all(|ch| ch.is_ascii_digit()))
            .then(|| format!("v{inner}"))
    })
}

fn is_version_token(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if token.eq_ignore_ascii_case("dev") {
        return true;
    }
    let digits = token.trim_start_matches(['v', 'V']);
    !digits.is_empty()
        && digits.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        && digits.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
}

/// Infer a version from the last component of a project directory such as
/// `C:\OpenFOAM\OpenFOAM-v2306` or `/usr/lib/openfoam/openfoam2306`.
pub fn version_from_directory(path: &Path) -> Option<OpenFoamVersion> {
    let name = path.file_name()?.to_str()?;
    if let Some(version) = OpenFoamVersion::parse(name) {
        return Some(version);
    }
    let lower = name.to_ascii_lowercase();
    let digits = lower
        .strip_prefix("openfoam")?
        .trim_start_matches(['-', '_']);
    if digits.len() == 4 && digits.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(OpenFoamVersion::from_label(
            &format!("v{digits}"),
            OpenFoamDistribution::OpenCfd,
        ));
    }
    if !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(OpenFoamVersion::from_label(
            digits,
            OpenFoamDistribution::Foundation,
        ));
    }
    None
}

#[cfg(test)]
// Tests assert on values they parsed from fixtures built here, so a failed
// expect is the assertion failing rather than a library invariant breaking.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn opencfd_banner_is_supported_from_the_template_baseline() {
        let banner =
            "Using: OpenFOAM-v2306 (2306) - visit www.openfoam.com\nBuild: _7bd4ed7a-20230626\n";
        let version = OpenFoamVersion::parse(banner).expect("version");
        assert_eq!(version.distribution, OpenFoamDistribution::OpenCfd);
        assert_eq!(version.label, "v2306");
        assert_eq!(version.release, Some(2306));
        assert_eq!(version.qualified_name(), "OpenFOAM-v2306");
        assert_eq!(version.assess().level, OpenFoamSupportLevel::Supported);
    }

    #[test]
    fn older_opencfd_release_is_untested_not_unsupported() {
        let version = OpenFoamVersion::parse("Version:  v2012\n").expect("version");
        assert_eq!(version.distribution, OpenFoamDistribution::OpenCfd);
        assert_eq!(version.assess().level, OpenFoamSupportLevel::Untested);
    }

    #[test]
    fn foundation_eleven_is_unsupported_because_simplefoam_was_removed() {
        let banner = "| Website: https://openfoam.org\n| Version:  11\n";
        let version = OpenFoamVersion::parse(banner).expect("version");
        assert_eq!(version.distribution, OpenFoamDistribution::Foundation);
        assert_eq!(version.release, Some(11));
        let assessment = version.assess();
        assert_eq!(assessment.level, OpenFoamSupportLevel::Unsupported);
        assert!(assessment.reason.contains("foamRun"));
    }

    #[test]
    fn foundation_ten_and_dev_are_untested() {
        let ten = OpenFoamVersion::parse("OpenFOAM-10").expect("version");
        assert_eq!(ten.distribution, OpenFoamDistribution::Foundation);
        assert_eq!(ten.assess().level, OpenFoamSupportLevel::Untested);
        let dev = OpenFoamVersion::parse("OpenFOAM-dev").expect("version");
        assert_eq!(dev.release, None);
        assert_eq!(dev.assess().level, OpenFoamSupportLevel::Untested);
    }

    #[test]
    fn environment_dump_and_directory_names_identify_the_release() {
        let env = OpenFoamVersion::parse(
            "WM_PROJECT_VERSION=v2406\nWM_PROJECT_DIR=/usr/lib/openfoam/openfoam2406\n",
        )
        .expect("version");
        assert_eq!(env.release, Some(2406));
        assert_eq!(env.distribution, OpenFoamDistribution::OpenCfd);
        let native = version_from_directory(Path::new(r"C:\OpenFOAM\OpenFOAM-v2312")).expect("dir");
        assert_eq!(native.label, "v2312");
        let debian =
            version_from_directory(Path::new("/usr/lib/openfoam/openfoam2306")).expect("dir");
        assert_eq!(debian.release, Some(2306));
        assert_eq!(debian.distribution, OpenFoamDistribution::OpenCfd);
        let foundation = version_from_directory(Path::new("/opt/openfoam9")).expect("dir");
        assert_eq!(foundation.distribution, OpenFoamDistribution::Foundation);
        assert_eq!(foundation.release, Some(9));
    }

    #[test]
    fn usage_text_without_a_version_yields_the_unknown_assessment() {
        assert!(OpenFoamVersion::parse("Usage: simpleFoam [OPTIONS]\n  -case <dir>\n").is_none());
        let assessment = OpenFoamVersionAssessment::from_version(None);
        assert_eq!(assessment.level, OpenFoamSupportLevel::Untested);
        assert!(assessment.reason.contains("could not be assessed"));
    }
}
