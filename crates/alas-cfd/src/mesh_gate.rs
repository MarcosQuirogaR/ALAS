// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Enforcement of the declared mesh-quality contract against measured values.
//!
//! [`MeshQualityThresholds`] has always published numbers — `70 deg`
//! non-orthogonality, skewness limits, an aspect-ratio ceiling — while the only
//! thing the runner actually tested was `checkMesh`'s own `Mesh OK` verdict.
//! Nothing compared a parsed value against a declared limit, so a mesh could
//! exceed a published maximum and still be accepted.
//!
//! That gap is closed here, in the direction that does not weaken anything: the
//! declared numbers are now **enforced** on the quantities this crate actually
//! measures, and every check this crate cannot measure is reported explicitly as
//! [`MeshCheckStatus::NotMeasured`] rather than quietly counted as a pass.
//!
//! A consequence, recorded rather than avoided: the **fine preset does not meet
//! the declared contract.** `G3-fine-p404` and `P1-fine-preltol001` both mesh to
//! a maximum face non-orthogonality of `71.3691 deg` against the declared
//! `70 deg` — one face out of 436 389, but one face is a violation of a maximum.
//! `checkMesh` calls that a warning and passes the mesh; ALAS's own declared
//! limit does not. Where the two disagree, the stricter one is applied, and the
//! numerical-solver verdict is reported separately so a case is never silently
//! greened by either.

use serde::{Deserialize, Serialize};

use super::mesh::{BoundaryPatchReport, MeshQualityThresholds};
use super::result_types::MeshQuality;

/// Outcome of one declared mesh-quality check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshCheckStatus {
    /// The measured value satisfies the declared limit.
    Passed,
    /// The measured value violates the declared limit.
    Failed,
    /// This crate does not measure the quantity, so nothing is claimed.
    ///
    /// Never treated as a pass: an unmeasured check is missing evidence.
    NotMeasured,
}

impl MeshCheckStatus {
    /// Stable label for reports and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "FAILED",
            Self::NotMeasured => "not measured",
        }
    }
}

/// One declared limit, the value measured against it, and where that came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshQualityCheck {
    /// Declared quantity, matching the `MeshQualityThresholds` field name.
    pub name: String,
    /// Verdict for this check.
    pub status: MeshCheckStatus,
    /// Declared limit, when the check is numeric.
    pub limit: Option<f64>,
    /// Value actually measured, when one was available.
    pub measured: Option<f64>,
    /// Where the measured value came from, or why there is none.
    pub provenance: String,
}

/// Aggregate standing of the declared mesh contract.
///
/// Three states, not two.  "No check failed" and "every declared check was
/// measured and passed" are different claims, and collapsing them lets an
/// unproven contract read as a complete one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshQualificationStanding {
    /// Every declared check was measured and satisfied.
    FullyQualified,
    /// No measured check failed, but at least one declared check could not be
    /// measured, so the contract is **not** fully demonstrated.
    #[default]
    PassedOnMeasuredChecksOnly,
    /// At least one measured check violates its declared limit.
    Failed,
}

impl MeshQualificationStanding {
    /// Stable label for reports and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FullyQualified => "fully qualified",
            Self::PassedOnMeasuredChecksOnly => "passed on measured checks only (unproven)",
            Self::Failed => "FAILED",
        }
    }
}

/// Whether the converted mesh meets the declared contract.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MeshQualification {
    /// `true` when no **measured** declared check failed.
    ///
    /// This is deliberately NOT a statement that the contract is fully
    /// demonstrated — read [`Self::standing`] for that.  It is the value the
    /// combined outcome uses, so a mesh that violates a measured limit fails,
    /// while a mesh with unmeasurable checks is not blocked by evidence this
    /// crate has no way to produce.
    pub passed: bool,
    /// The three-state aggregate, which distinguishes "unproven" from "passed".
    #[serde(default)]
    pub standing: MeshQualificationStanding,
    /// Every declared check, in declaration order.
    pub checks: Vec<MeshQualityCheck>,
    /// Faces `checkMesh` reported past its own severe-non-orthogonality line.
    #[serde(default)]
    pub severely_non_orthogonal_faces: Option<u64>,
    /// `severely_non_orthogonal_faces` divided by the **cell** count.
    ///
    /// This is a face-per-cell ratio, **not** the fraction of mesh faces: the
    /// face total is not parsed from `checkMesh` output, and dividing a face
    /// count by a cell count would be a category error if presented as a
    /// fraction.  Named for what it is.  Prevalence either way, not influence:
    /// it says nothing about whether the outlier affects the integrated loads.
    #[serde(default)]
    pub severely_non_orthogonal_faces_per_cell: Option<f64>,
}

impl MeshQualification {
    /// Checks that failed, for a status line.
    pub fn failures(&self) -> Vec<&MeshQualityCheck> {
        self.checks
            .iter()
            .filter(|check| check.status == MeshCheckStatus::Failed)
            .collect()
    }

    /// Checks this crate could not evaluate.
    pub fn unmeasured(&self) -> Vec<&MeshQualityCheck> {
        self.checks
            .iter()
            .filter(|check| check.status == MeshCheckStatus::NotMeasured)
            .collect()
    }

    /// One-line summary naming every failure with its measured value.
    pub fn summary(&self) -> String {
        let failures = self.failures();
        if failures.is_empty() {
            let unmeasured = self.unmeasured();
            return if unmeasured.is_empty() {
                "Mesh qualification FULLY QUALIFIED: every declared limit was measured and satisfied."
                    .to_owned()
            } else {
                format!(
                    "Mesh qualification UNPROVEN: every measured limit passed, but {} declared check(s) could NOT be measured and are therefore not demonstrated: {}. This is not a complete qualification of the declared contract.",
                    unmeasured.len(),
                    unmeasured
                        .iter()
                        .map(|check| check.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
        }
        format!(
            "Mesh qualification FAILED against the declared contract: {}.",
            failures
                .iter()
                .map(|check| {
                    match (check.measured, check.limit) {
                        (Some(measured), Some(limit)) => {
                            format!(
                                "{} {measured:.6} exceeds the declared {limit:.6}",
                                check.name
                            )
                        }
                        _ => format!("{} {}", check.name, check.provenance),
                    }
                })
                .collect::<Vec<_>>()
                .join("; ")
        )
    }
}

/// Evaluate the declared thresholds against what was actually measured.
///
/// `boundary` is the converted mesh's own patch report, when it can be read.
/// Supplying it turns `required_patches` and `reject_nonzero_unknown_patches`
/// from unmeasured declarations into measured checks; omitting it leaves them
/// honestly unmeasured rather than assumed.
pub fn qualify_mesh_with_boundary(
    thresholds: &MeshQualityThresholds,
    quality: &MeshQuality,
    boundary: Option<&BoundaryPatchReport>,
) -> MeshQualification {
    let mut checks = Vec::new();
    let distribution_max = |field: &str| {
        quality
            .distributions
            .iter()
            .find(|entry| entry.field.eq_ignore_ascii_case(field))
            .map(|entry| (entry.max, entry.source.clone(), entry.sample_count))
    };

    // checkMesh's own verdict, which was the only check the runner ever ran.
    checks.push(MeshQualityCheck {
        name: "require_check_mesh_ok".to_owned(),
        status: if !thresholds.require_check_mesh_ok {
            MeshCheckStatus::NotMeasured
        } else if quality.passed {
            MeshCheckStatus::Passed
        } else {
            MeshCheckStatus::Failed
        },
        limit: None,
        measured: None,
        provenance: if thresholds.require_check_mesh_ok {
            "checkMesh reported Mesh OK with no failed checks".to_owned()
        } else {
            "not required by the declared contract".to_owned()
        },
    });

    // The declared maximum, applied to the measured maximum.  This is the check
    // whose absence let a 71.37 deg face through a 70 deg contract.
    checks.push(numeric_check(
        "max_non_orthogonality_deg",
        thresholds.max_non_orthogonality_deg,
        quality.max_non_orthogonality_deg,
        "checkMesh `Mesh non-orthogonality Max`",
    ));

    // `checkMesh` prints one overall skewness maximum; it does not separate
    // internal from boundary faces in the text this crate parses.  The stricter
    // declared limit is therefore the only defensible one to apply, and the
    // boundary limit is reported as unmeasured rather than assumed satisfied.
    checks.push(numeric_check(
        "max_internal_skewness",
        thresholds.max_internal_skewness,
        quality.max_skewness,
        "checkMesh `Max skewness` (overall; internal and boundary faces are not separated in the parsed output, so the stricter internal limit is applied)",
    ));
    checks.push(match (quality.max_boundary_skewness, quality.skewness_faces_in_error) {
        // BEST evidence: the exact boundary-face maximum, read from the written
        // `skewness` field.  `checkMesh`'s headline `Max skewness` is the
        // INTERNAL maximum and cannot stand in for it.
        (Some(max), _) => MeshQualityCheck {
            name: "max_boundary_skewness".to_owned(),
            status: if max.is_finite() && max <= thresholds.max_boundary_skewness {
                MeshCheckStatus::Passed
            } else {
                MeshCheckStatus::Failed
            },
            limit: Some(thresholds.max_boundary_skewness),
            measured: Some(max),
            provenance: format!(
                "largest boundary-face skewness (dimensionless) over the written `skewness` boundaryField{}; empty patches carry no faces and are excluded",
                quality
                    .max_boundary_skewness_patch
                    .as_ref()
                    .map_or_else(String::new, |patch| format!(", on patch `{patch}`"))
            ),
        },
        _ => match quality.skewness_faces_in_error {
        // `checkMesh -meshQuality` applies the internal and boundary skewness
        // limits to their own face sets and counts the faces in error.  That is
        // the only boundary-skewness evidence this toolchain produces: with it
        // the check is measured, without it it stays unproven rather than
        // assumed.
        Some(0) => MeshQualityCheck {
            name: "max_boundary_skewness".to_owned(),
            status: MeshCheckStatus::Passed,
            limit: Some(thresholds.max_boundary_skewness),
            measured: Some(0.0),
            provenance: format!(
                "checkMesh -meshQuality: 0 faces with skewness > {:.0} (internal) or {:.0} (boundary); zero is unambiguous for both limits",
                thresholds.max_internal_skewness, thresholds.max_boundary_skewness
            ),
        },
        Some(faces) => MeshQualityCheck {
            name: "max_boundary_skewness".to_owned(),
            status: MeshCheckStatus::Failed,
            limit: Some(thresholds.max_boundary_skewness),
            measured: Some(faces as f64),
            provenance: format!(
                "checkMesh -meshQuality: {faces} face(s) exceed the internal or boundary skewness limit; the report does not attribute them, so both are treated as violated"
            ),
        },
        None => MeshQualityCheck {
            name: "max_boundary_skewness".to_owned(),
            status: MeshCheckStatus::NotMeasured,
            limit: Some(thresholds.max_boundary_skewness),
            measured: None,
            provenance: "no written `skewness` field and checkMesh was not run with -meshQuality: the boundary limit is not demonstrated".to_owned(),
        },
        },
    });

    let aspect = distribution_max("aspectRatio");
    checks.push(match aspect {
        Some((max, source, count)) => numeric_check(
            "max_aspect_ratio",
            thresholds.max_aspect_ratio,
            Some(max),
            &format!("native cellAspectRatio field, {count} cells, {source}"),
        ),
        None => MeshQualityCheck {
            name: "max_aspect_ratio".to_owned(),
            status: MeshCheckStatus::NotMeasured,
            limit: Some(thresholds.max_aspect_ratio),
            measured: None,
            provenance: "no native aspectRatio field was written for this case".to_owned(),
        },
    });

    checks.push(
        match (
            thresholds.require_positive_cell_volumes,
            quality.min_volume_m3,
        ) {
            (false, _) => MeshQualityCheck {
                name: "require_positive_cell_volumes".to_owned(),
                status: MeshCheckStatus::NotMeasured,
                limit: None,
                measured: quality.min_volume_m3,
                provenance: "not required by the declared contract".to_owned(),
            },
            (true, Some(min)) => MeshQualityCheck {
                name: "require_positive_cell_volumes".to_owned(),
                status: if min > 0.0 {
                    MeshCheckStatus::Passed
                } else {
                    MeshCheckStatus::Failed
                },
                limit: Some(0.0),
                measured: Some(min),
                provenance: "checkMesh `Min volume`".to_owned(),
            },
            (true, None) => MeshQualityCheck {
                name: "require_positive_cell_volumes".to_owned(),
                status: MeshCheckStatus::NotMeasured,
                limit: Some(0.0),
                measured: None,
                provenance: "checkMesh did not report a minimum cell volume".to_owned(),
            },
        },
    );

    // The patch contract IS measurable from the converted mesh's own boundary
    // file, so it is measured rather than declared unmeasured.  Only when that
    // file could not be read does this stay unproven.
    checks.push(match boundary {
        Some(report) => {
            let mismatched = thresholds
                .required_patches
                .iter()
                .filter(|(name, expected)| {
                    report
                        .patches
                        .iter()
                        .find(|patch| patch.name == **name)
                        .is_none_or(|patch| patch.patch_type != **expected)
                })
                .map(|(name, expected)| format!("{name} must be `{expected}`"))
                .collect::<Vec<_>>();
            let populated_unknown = report
                .patches
                .iter()
                .filter(|patch| {
                    !thresholds.required_patches.contains_key(&patch.name)
                        && patch.n_faces.is_some_and(|faces| faces > 0)
                })
                .map(|patch| patch.name.clone())
                .collect::<Vec<_>>();
            let rejected_unknown =
                thresholds.reject_nonzero_unknown_patches && !populated_unknown.is_empty();
            MeshQualityCheck {
                name: "required_patches".to_owned(),
                status: if mismatched.is_empty() && !rejected_unknown {
                    MeshCheckStatus::Passed
                } else {
                    MeshCheckStatus::Failed
                },
                limit: None,
                measured: Some(report.patches.len() as f64),
                provenance: if mismatched.is_empty() && !rejected_unknown {
                    format!(
                        "all {} declared patches present with the declared type in constant/polyMesh/boundary, and no populated patch outside the contract",
                        thresholds.required_patches.len()
                    )
                } else {
                    let mut reason = Vec::new();
                    if !mismatched.is_empty() {
                        reason.push(format!("missing or mistyped: {}", mismatched.join(", ")));
                    }
                    if rejected_unknown {
                        reason.push(format!(
                            "populated patch(es) outside the contract: {}",
                            populated_unknown.join(", ")
                        ));
                    }
                    reason.join("; ")
                },
            }
        }
        None => MeshQualityCheck {
            name: "required_patches".to_owned(),
            status: MeshCheckStatus::NotMeasured,
            limit: None,
            measured: None,
            provenance: "constant/polyMesh/boundary was not available to this gate".to_owned(),
        },
    });

    let passed = !checks
        .iter()
        .any(|check| check.status == MeshCheckStatus::Failed);
    let standing = if !passed {
        MeshQualificationStanding::Failed
    } else if checks
        .iter()
        .any(|check| check.status == MeshCheckStatus::NotMeasured)
    {
        MeshQualificationStanding::PassedOnMeasuredChecksOnly
    } else {
        MeshQualificationStanding::FullyQualified
    };
    let faces_per_cell = match (quality.severely_non_orthogonal_faces, quality.cells) {
        (Some(faces), Some(cells)) if cells > 0 => Some(faces as f64 / cells as f64),
        _ => None,
    };
    MeshQualification {
        passed,
        standing,
        checks,
        severely_non_orthogonal_faces: quality.severely_non_orthogonal_faces,
        severely_non_orthogonal_faces_per_cell: faces_per_cell,
    }
}

/// [`qualify_mesh_with_boundary`] without the converted boundary file.
pub fn qualify_mesh(
    thresholds: &MeshQualityThresholds,
    quality: &MeshQuality,
) -> MeshQualification {
    qualify_mesh_with_boundary(thresholds, quality, None)
}

fn numeric_check(
    name: &str,
    limit: f64,
    measured: Option<f64>,
    provenance: &str,
) -> MeshQualityCheck {
    MeshQualityCheck {
        name: name.to_owned(),
        status: match measured {
            Some(value) if value.is_finite() && value <= limit => MeshCheckStatus::Passed,
            Some(_) => MeshCheckStatus::Failed,
            None => MeshCheckStatus::NotMeasured,
        },
        limit: Some(limit),
        measured,
        provenance: match measured {
            Some(_) => provenance.to_owned(),
            None => format!("not available: {provenance}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::BoundaryPatch;
    use crate::result_types::ScalarDistribution;

    fn measured(max_non_ortho: f64, severe: Option<u64>) -> MeshQuality {
        MeshQuality {
            passed: true,
            cells: Some(436_389),
            max_non_orthogonality_deg: Some(max_non_ortho),
            max_skewness: Some(1.813_801_981_2),
            min_volume_m3: Some(5.280_502_174_39e-11),
            severely_non_orthogonal_faces: severe,
            max_boundary_skewness: Some(0.419_765_853_1),
            max_boundary_skewness_patch: Some("farField".to_owned()),
            skewness_faces_in_error: Some(0),
            non_orthogonality_faces_in_error: Some(0),
            ..MeshQuality::default()
        }
    }

    /// `G3-fine-p404` and `P1-fine-preltol001`, verbatim: `checkMesh` passes the
    /// mesh, ALAS's own declared `70 deg` maximum does not.  The stricter one
    /// applies, and the count and fraction travel with the verdict.
    #[test]
    fn the_declared_maximum_is_enforced_even_when_check_mesh_passes() {
        let thresholds = MeshQualityThresholds::template_defaults();
        let qualification = qualify_mesh(&thresholds, &measured(71.369_124_925_5, Some(1)));
        assert!(!qualification.passed);
        let failures = qualification.failures();
        assert_eq!(failures.len(), 1, "{:?}", qualification.checks);
        assert_eq!(failures[0].name, "max_non_orthogonality_deg");
        assert_eq!(failures[0].limit, Some(70.0));
        assert_eq!(qualification.severely_non_orthogonal_faces, Some(1));
        let fraction = qualification
            .severely_non_orthogonal_faces_per_cell
            .unwrap_or_else(|| panic!("fraction"));
        assert!((fraction - 1.0 / 436_389.0).abs() < 1.0e-15);
        assert!(qualification.summary().contains("FAILED"));
        assert!(qualification.summary().contains("71.369"));
    }

    /// The medium preset, which does meet the contract.
    #[test]
    fn a_mesh_inside_every_measured_limit_qualifies() {
        let thresholds = MeshQualityThresholds::template_defaults();
        let qualification = qualify_mesh(&thresholds, &measured(46.185_144_557_9, None));
        assert!(qualification.passed, "{:?}", qualification.failures());
        assert_eq!(qualification.severely_non_orthogonal_faces, None);
    }

    /// A declared limit this crate cannot measure must never be silently
    /// counted as satisfied.
    #[test]
    fn unmeasured_checks_are_reported_rather_than_assumed() {
        let thresholds = MeshQualityThresholds::template_defaults();
        let qualification = qualify_mesh(&thresholds, &measured(46.0, None));
        let unmeasured = qualification
            .unmeasured()
            .iter()
            .map(|check| check.name.clone())
            .collect::<Vec<_>>();
        assert!(
            !unmeasured.contains(&"max_boundary_skewness".to_owned()),
            "boundary skewness is measurable with checkMesh -meshQuality: {unmeasured:?}"
        );
        // No native aspectRatio field in this fixture.
        assert!(unmeasured.contains(&"max_aspect_ratio".to_owned()));
        assert!(
            qualification.summary().contains("UNPROVEN"),
            "{}",
            qualification.summary()
        );
        assert_eq!(
            qualification.standing,
            MeshQualificationStanding::PassedOnMeasuredChecksOnly
        );
        // `passed` stays true so the combined outcome is not blocked by
        // evidence this crate cannot produce, but the standing says plainly
        // that the contract is not fully demonstrated.
        assert!(qualification.passed);
    }

    /// The EXACT boundary maximum is preferred over the faces-in-error count,
    /// and `checkMesh`'s headline `Max skewness` must not stand in for it: that
    /// number is the internal maximum.  Measured on `S6-medium-p404`, internal
    /// `0.5948238075` versus boundary `0.4197658531` — different quantities,
    /// different declared limits (4 and 20).
    #[test]
    fn the_boundary_skewness_maximum_is_read_from_the_boundary_faces() {
        let thresholds = MeshQualityThresholds::template_defaults();
        let find = |q: &MeshQualification| {
            q.checks
                .iter()
                .find(|check| check.name == "max_boundary_skewness")
                .cloned()
                .unwrap_or_else(|| panic!("check missing"))
        };
        let clean = qualify_mesh(&thresholds, &measured(46.0, None));
        let check = find(&clean);
        assert_eq!(check.status, MeshCheckStatus::Passed);
        assert_eq!(check.measured, Some(0.419_765_853_1));
        assert!(
            check.provenance.contains("boundary-face skewness"),
            "{}",
            check.provenance
        );
        assert!(
            check.provenance.contains("farField"),
            "{}",
            check.provenance
        );

        // A boundary face past the declared limit fails, even with zero faces
        // reported in error by a run that did not use -meshQuality.
        let mut bad = measured(46.0, None);
        bad.max_boundary_skewness = Some(20.000_001);
        bad.skewness_faces_in_error = None;
        let bad = qualify_mesh(&thresholds, &bad);
        assert_eq!(find(&bad).status, MeshCheckStatus::Failed);
        assert_eq!(bad.standing, MeshQualificationStanding::Failed);

        // Neither source available: unproven, never a default pass.
        let mut blind = measured(46.0, None);
        blind.max_boundary_skewness = None;
        blind.max_boundary_skewness_patch = None;
        blind.skewness_faces_in_error = None;
        let blind = qualify_mesh(&thresholds, &blind);
        assert_eq!(find(&blind).status, MeshCheckStatus::NotMeasured);
        assert_ne!(blind.standing, MeshQualificationStanding::FullyQualified);
    }

    /// `max_boundary_skewness` is MEASURED when `checkMesh -meshQuality` ran,
    /// and honestly unproven when it did not.  Never assumed either way.
    #[test]
    fn boundary_skewness_is_measured_from_the_mesh_quality_pass() {
        let thresholds = MeshQualityThresholds::template_defaults();
        let find = |qualification: &MeshQualification| {
            qualification
                .checks
                .iter()
                .find(|check| check.name == "max_boundary_skewness")
                .cloned()
                .unwrap_or_else(|| panic!("check missing"))
        };

        // Measured and satisfied: zero faces in error is unambiguous for the
        // internal and the boundary limit alike.
        let clean = qualify_mesh(&thresholds, &measured(46.0, None));
        assert_eq!(find(&clean).status, MeshCheckStatus::Passed);

        // Measured and violated.  The report does not say which limit, so both
        // are treated as violated rather than guessed at.
        let mut dirty = measured(46.0, None);
        dirty.max_boundary_skewness = None;
        dirty.max_boundary_skewness_patch = None;
        dirty.skewness_faces_in_error = Some(3);
        let dirty = qualify_mesh(&thresholds, &dirty);
        assert_eq!(find(&dirty).status, MeshCheckStatus::Failed);
        assert!(!dirty.passed);
        assert_eq!(dirty.standing, MeshQualificationStanding::Failed);

        // Not run: unproven, and NOT a default pass.
        let mut legacy = measured(46.0, None);
        legacy.max_boundary_skewness = None;
        legacy.max_boundary_skewness_patch = None;
        legacy.skewness_faces_in_error = None;
        let legacy = qualify_mesh(&thresholds, &legacy);
        assert_eq!(find(&legacy).status, MeshCheckStatus::NotMeasured);
        assert_ne!(legacy.standing, MeshQualificationStanding::FullyQualified);
    }

    /// With every declared limit measurable, a clean mesh can finally reach
    /// `FullyQualified` — which nothing could before boundary skewness and the
    /// patch contract became real measurements.
    #[test]
    fn a_completely_measured_clean_mesh_is_fully_qualified() {
        let thresholds = MeshQualityThresholds::template_defaults();
        let mut quality = measured(46.0, None);
        quality.distributions = vec![ScalarDistribution {
            field: "aspectRatio".to_owned(),
            label: "aspect ratio".to_owned(),
            unit: "-".to_owned(),
            source: "0/aspectRatio".to_owned(),
            sample_count: 436_389,
            min: 1.0,
            mean: 12.0,
            max: 282.708_980_277,
            percentiles: Vec::new(),
            values: Vec::new(),
        }];
        let boundary = BoundaryPatchReport {
            patches: thresholds
                .required_patches
                .iter()
                .map(|(name, kind)| BoundaryPatch {
                    name: name.clone(),
                    patch_type: kind.clone(),
                    physical_type: None,
                    n_faces: Some(100),
                })
                .collect(),
            required_types: thresholds.required_patches.clone(),
            updated: false,
            unknown_patches: Vec::new(),
        };
        let qualification = qualify_mesh_with_boundary(&thresholds, &quality, Some(&boundary));
        assert!(
            qualification.unmeasured().is_empty(),
            "{:?}",
            qualification.unmeasured()
        );
        assert_eq!(
            qualification.standing,
            MeshQualificationStanding::FullyQualified
        );
        assert!(qualification.summary().contains("FULLY QUALIFIED"));
    }
}
