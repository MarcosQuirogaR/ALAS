// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Result types of the centre-of-gravity envelope assessment.

use alas_config::{CgEnvelopeEvidence, CgEnvelopeSource};

/// Result of comparing one analyzed point with public CG reference evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanningCgStatus {
    /// No public planning curve was evaluated.
    #[default]
    NotEvaluated,
    /// The point lies between both published planning limits.
    WithinPublishedLimits,
    /// The point is forward of the published planning limit.
    ForwardLimitViolation,
    /// The point is aft of the published planning limit.
    AftLimitViolation,
    /// A forward limit exists at this mass, but the source omits an aft limit.
    AftLimitNotPublished,
}

/// Provenance and result of the public planning-envelope comparison.
///
/// This assessment is intentionally separate from the model-derived CG and
/// landing-gear check. A public planning curve is preliminary design evidence;
/// the actual aircraft WBM remains the operational authority.
#[derive(Debug, Clone, PartialEq)]
pub struct CgEnvelopeAssessment {
    /// Kind of source evidence registered for the selected preset.
    pub evidence: CgEnvelopeEvidence,
    /// Outcome of the planning-curve comparison, if one was possible.
    pub planning_status: PlanningCgStatus,
    /// Analyzed aircraft mass, in kilograms.
    pub mass_kg: Option<f64>,
    /// Analyzed longitudinal CG in the manufacturer planning frame, in percent MAC.
    pub cg_pct_mac: Option<f64>,
    /// Interpolated public planning forward limit, in percent MAC.
    pub forward_limit_pct_mac: Option<f64>,
    /// Interpolated public planning aft limit, in percent MAC.
    pub aft_limit_pct_mac: Option<f64>,
    /// Exact source location of the planning curve, when one is registered.
    pub source: Option<CgEnvelopeSource>,
    /// Document that controls actual-aircraft dispatch and loading.
    pub controlling_document: Option<&'static str>,
    /// How far the built model's own mean-aerodynamic-chord leading edge sits
    /// aft of the published planning one, in metres, positive aft.
    ///
    /// The comparison this assessment makes is only like for like when the
    /// two references describe the same chord at the same station: the mass
    /// stations summed into [`Self::cg_pct_mac`] are placed on the **model's**
    /// geometry, while the percentage they are expressed as is referred to the
    /// **manufacturer's** published leading edge and chord. A non-zero offset
    /// here is a systematic shift of every reported percentage and is
    /// therefore reported beside the verdict rather than absorbed into it.
    ///
    /// [`None`] when no planning envelope is registered or the built geometry
    /// carries no usable chord.
    pub model_mac_leading_edge_offset_m: Option<f64>,
    /// Built model mean aerodynamic chord less the published planning chord,
    /// in metres.
    ///
    /// Reported for the same reason as
    /// [`Self::model_mac_leading_edge_offset_m`]: a percentage of one chord
    /// compared against a limit stated as a percentage of a different chord
    /// is a scale error on top of the datum offset.
    pub model_mac_length_difference_m: Option<f64>,
    /// The published planning mean aerodynamic chord the percentages above are
    /// referred to, in metres, when one is registered.
    pub published_mac_chord_m: Option<f64>,
    /// Every named loading state, potato extreme and
    /// fuel-vector extreme this run evaluated, compared against the
    /// published planning envelope's in-flight curve, reporting the
    /// smallest margin and which point produced it. `None` when no
    /// planning envelope is registered for this preset, or the loading
    /// states/potato/fuel-vector evidence needed to sweep it was
    /// unavailable.
    pub flight_curve_comparison: Option<PlanningCurveComparison>,
    /// Same sweep as [`Self::flight_curve_comparison`], against the
    /// published envelope's on-ground curve.
    pub ground_curve_comparison: Option<PlanningCurveComparison>,
}

/// The worst (smallest) margin one curve of a published
/// planning envelope reported across every point swept -- every named
/// loading state ([`alas_opt::ModelCgLoadingState`]), every potato extreme
/// ([`crate::feasibility::OperationalEnvelopeAssessment::potato_pct_mac`])
/// and every fuel-vector extreme
/// ([`crate::feasibility::OperationalEnvelopeAssessment::fuel_vector_checks`]) --
/// plus the label of whichever point produced it.
///
/// Margin is positive inside the published band and negative outside it:
/// `min(cg_pct_mac - forward_pct_mac, aft_pct_mac - cg_pct_mac)` when an aft
/// limit is published at that point's mass, else just the forward margin.
/// This is preliminary design evidence, not the actual aircraft WBM (see
/// [`CgEnvelopeAssessment`]'s own module doc).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanningCurveComparison {
    /// Smallest margin found across every swept point, percent MAC.
    pub worst_margin_pct_mac: f64,
    /// Stable label of the point that produced [`Self::worst_margin_pct_mac`]
    /// (a loading-state label, `"potato extreme"` or `"fuel-vector extreme"`).
    pub worst_point_label: String,
    /// That point's own analyzed mass, kilograms.
    pub worst_point_mass_kg: f64,
    /// That point's own analyzed centre of gravity, percent MAC (this
    /// envelope's own manufacturer frame).
    pub worst_point_cg_pct_mac: f64,
    /// Number of points actually swept (loading states plus potato and
    /// fuel-vector extremes whose mass fell inside the published table).
    pub points_swept: usize,
}

/// Datum offset, as a fraction of the published chord, beyond which the model
/// and published MAC references are reported as disagreeing.
///
/// Below it the shift is smaller than the rounding of the published vertices
/// themselves; above it the reported percentage moves by an amount comparable
/// with the width of the envelope's own margins.
const MAC_DATUM_AGREEMENT_FRACTION: f64 = 0.01;

/// The same, for the chord length. A chord scale error shifts every reported
/// percentage proportionally rather than uniformly, so it is allowed a wider
/// band before it is called a disagreement.
const MAC_LENGTH_AGREEMENT_FRACTION: f64 = 0.02;

impl CgEnvelopeAssessment {
    /// Whether the model and published MAC references disagree by enough to
    /// make the reported percentage a frame artefact rather than a measured
    /// position.
    ///
    /// This does not change a verdict. A reported violation stays a reported
    /// violation; this says whether the number that produced it is referred to
    /// the chord its limit is stated in.
    pub fn mac_references_disagree(&self) -> bool {
        let Some(chord_m) = self
            .published_mac_chord_m
            .filter(|chord| chord.is_finite() && *chord > 0.0)
        else {
            return false;
        };
        self.model_mac_leading_edge_offset_m
            .is_some_and(|offset| offset.abs() > MAC_DATUM_AGREEMENT_FRACTION * chord_m)
            || self
                .model_mac_length_difference_m
                .is_some_and(|difference| {
                    difference.abs() > MAC_LENGTH_AGREEMENT_FRACTION * chord_m
                })
    }

    /// How far the reported percentage is shifted by the datum offset alone,
    /// in percent MAC of the published chord, when both are known.
    ///
    /// A positive value means the model's chord starts aft of the published
    /// one, so an unshifted model station reads *lower* than it should; the
    /// sign is stated rather than left for the reader to derive.
    pub fn mac_datum_shift_pct_mac(&self) -> Option<f64> {
        let chord_m = self
            .published_mac_chord_m
            .filter(|chord| chord.is_finite() && *chord > 0.0)?;
        let offset_m = self.model_mac_leading_edge_offset_m?;
        offset_m.is_finite().then(|| 100.0 * offset_m / chord_m)
    }
}

impl Default for CgEnvelopeAssessment {
    fn default() -> Self {
        Self {
            evidence: CgEnvelopeEvidence::Unknown,
            planning_status: PlanningCgStatus::NotEvaluated,
            mass_kg: None,
            cg_pct_mac: None,
            forward_limit_pct_mac: None,
            aft_limit_pct_mac: None,
            source: None,
            controlling_document: None,
            model_mac_leading_edge_offset_m: None,
            model_mac_length_difference_m: None,
            published_mac_chord_m: None,
            flight_curve_comparison: None,
            ground_curve_comparison: None,
        }
    }
}
