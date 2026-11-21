// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/analysis_config.py

//! How finely the aerodynamics are evaluated, and where.
//!
//! Two fidelities live here, and the split is the point. The optimizer
//! evaluates thousands of candidates and needs an estimate it can afford; the
//! final analysis runs once on the winner and is what gets reported. Sharing
//! one resolution between them means either an optimizer that takes hours or
//! a reported cruise point that is wrong, and the second failure is silent,
//! which is why the fine resolutions are separate fields rather than a
//! multiplier someone remembers to raise.
//!
//! # Why the two meshes differ in cost and not in kind
//!
//! The chordwise resolution is the one that bites, and only the chordwise
//! one. `Wing::mesh_thin_surface` cuts `cosspace(0, 1, chordwise + 1)`
//! stations and samples the mean camber line at each, so at a resolution of
//! one the only stations are the leading and trailing edges, where every
//! mean line is zero, and the panel is the flat chord line. The camber is
//! not approximated coarsely; it is absent.
//!
//! Measured across four registered presets in an internal VLM
//! resolution-sensitivity study (cross-checked against
//! AeroSandbox 4.2.8 on identical geometry): at one chordwise panel the
//! trimmed cruise attitude is 1.1 to 4.1 degrees high depending on how much
//! camber the section carries, the induced-drag factor is wrong by -5 to
//! +35 %, and the lift-to-drag ratio by -14.4 to +2.9 %: the sign differs
//! between airframes, so no calibration constant can absorb it. Ranked over
//! neighbouring candidates the search then mis-orders them (Spearman 0.77
//! against a converged mesh) and under-predicts sized block fuel by about
//! 7 %. At eight chordwise panels the ranking is exact and the induced
//! factor is within 1.5 %; sixteen halves the residual attitude error again.
//! The four Hicks-Henne bump design variables produce *bit-identical* forces
//! at one panel, so a search run there cannot see a third of its own design
//! space.
//!
//! The spanwise resolution is a different animal: it is a subdivision
//! *multiplier* applied to a surface `AircraftBuilder` has already
//! subdivided, so a value of one already means 24 strips per semispan on the
//! main wing. Refining that cleanly (through `n_subdivisions`) moves the
//! trimmed attitude by 0.011 degrees and the induced factor by 0.4 % over a
//! twelve-fold range; it is converged. Refining it through *this*
//! multiplier instead re-applies a cosine spacing inside each existing strip
//! and destroys the answer; `validation::vlm_mesh_is_solvable` rejects
//! anything above two for that reason. So both spanwise fields default to
//! one: not to save time, but because that is the converged value.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// The product's tail dynamic-pressure ratio when none is stated: an empirical
/// value, not a computed one. NASA CR-2144 (Boeing 747 flight derivatives,
/// Tables IX-1 landing and IX-2 power approach): `CL_q^2 / (2 |Cm_q|)` gives
/// 0.746 and 0.701 of the wing area for the tail's effectiveness, an upper
/// bound because the wing adds to both derivatives, that is 0.80 and 0.75 of
/// the lattice isolated-tail slope. It also reproduces the tail increment of
/// NASA TN D-3797 (T-tail transport, Ames 40 x 80 ft): the model without its
/// tail has its neutral point at 0.44 of the mean chord (the lattice gives
/// 0.449), with it 0.715; the lattice with a free-stream tail adds 0.345, the
/// measurement 0.275, a ratio of 0.80. NACA RM A58H12 (747-class low tail) asks
/// for about 0.68. The DATCOM 4.4.1 / Roskam Eqs. 8.34-8.40 wing wake gives
/// 1.0 for every preset (the thin wake misses the tails) and no sourced
/// fuselage, nacelle or tail-body term closes the gap
/// (`alas_aero::analysis::physical_tail_efficiency` computes the wake value).
pub const EMPIRICAL_TAIL_EFFICIENCY: f64 = 0.78;

/// The efficiency the retained reference-compatibility comparison replays
/// (the frozen fixtures were taken at 0.90).
pub const REFERENCE_TAIL_EFFICIENCY: f64 = 0.90;

/// Settings for the vortex-lattice polar sweeps and the stability probes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct AnalysisConfig {
    /// Wall-clock deadline for final native AVL sweeps, in seconds.
    #[serde(default = "default_avl_timeout_s")]
    #[config(
        label = "AVL reporting timeout",
        unit = "s",
        help = "Wall-clock deadline for final AVL sweeps; independent of the optimizer candidate budget and aerodynamic mesh."
    )]
    pub avl_timeout_s: f64,

    /// Lowest angle of attack in the final polar sweep.
    #[config(
        label = "Polar sweep: alpha minimum",
        unit = "deg",
        help = "Lowest angle of attack evaluated in the final high-fidelity polar sweep."
    )]
    pub sweep_alpha_min_deg: f64,

    /// Highest angle of attack in the final polar sweep.
    #[config(
        label = "Polar sweep: alpha maximum",
        unit = "deg",
        help = "Highest angle of attack evaluated in the final high-fidelity polar sweep."
    )]
    pub sweep_alpha_max_deg: f64,

    /// How many angles the final polar sweep evaluates.
    #[config(
        label = "Polar sweep: number of points",
        help = "How many angle-of-attack points to evaluate between the min/max alpha. More points = smoother drag polar, slower analysis. Part of the Fidelity preset."
    )]
    pub sweep_n_points: i64,

    /// Spanwise panel multiplier for the in-loop estimate.
    #[config(
        label = "VLM spanwise panel resolution",
        help = "Multiplier on each surface's built-in spanwise vortex-lattice panel count. Leave at 1; values above 2 are rejected. Part of the Fidelity preset.",
        long_help = "Multiplier on each surface's built-in spanwise panel subdivision for the vortex-lattice solver. Leave at 1: the geometry builder has already subdivided every surface (24 strips per semispan on the main wing), and that is converged, refining it further moves the trimmed cruise attitude by 0.01 deg. Values above 2 are rejected, because this multiplier re-applies a cosine spacing inside each existing strip and the induced drag then stops converging. Part of the Fidelity preset."
    )]
    pub spanwise_resolution: i64,

    /// Chordwise panel multiplier for the in-loop estimate.
    #[config(
        label = "VLM chordwise panel resolution",
        help = "Chordwise vortex-lattice panels per strip for the fast in-loop estimate (a count, not a multiplier). 1 reduces the section to a flat plate; 8 is converged.",
        long_help = "Number of chordwise panels per strip for the vortex-lattice solver, used by the fast in-loop estimate the optimizer ranks candidates with; the final reported analysis uses fine_chordwise_resolution instead. This is a literal panel count, not a multiplier, and nothing else in the pipeline sets one. At 1 the mesh samples the camber line only at the leading and trailing edges, where it is zero, so the section becomes a flat plate: cruise attitude comes out 1-4 deg high, L/D wrong by -14 to +3 percent, and the four airfoil bump design variables have no effect at all. 8 ranks candidates identically to a converged mesh. Higher = finer mesh, slower. Part of the Fidelity preset."
    )]
    pub chordwise_resolution: i64,

    /// Spanwise panel multiplier for the once-per-run final analysis.
    #[config(
        label = "Fine VLM spanwise resolution (final analysis)",
        help = "Spanwise panel resolution for the once-per-run final analysis only (polar, trimmed cruise, neutral point). Leave at 1: the span is already converged.",
        long_help = "Spanwise panel resolution used ONLY for the once-per-run final/reported analysis (drag polar, trimmed cruise point, neutral point), not the optimizer loop. Leave at 1 for the same reason as the in-loop field: the span is already converged, so raising this doubles the panel count to change the answer by about 1 percent. Spend the panels on fine_chordwise_resolution instead."
    )]
    pub fine_spanwise_resolution: i64,

    /// Chordwise panel multiplier for the once-per-run final analysis.
    #[config(
        label = "Fine VLM chordwise resolution (final analysis)",
        help = "Chordwise panels for the once-per-run final analysis. A supercritical section needs about 8 to resolve its camber; convergence is first order, so 16 is better.",
        long_help = "Chordwise panel resolution for the once-per-run final/reported analysis. A supercritical section needs roughly 8 chordwise panels before the VLM resolves its camber line at all, and the convergence is first-order in panel count: 8 still leaves the reported cruise attitude about 1 deg high on a supercritical wing, 16 about 0.5 deg. Kept above the in-loop value so the REPORTED cruise alpha and L/D are the more trustworthy of the two, at a cost paid once per run."
    )]
    pub fine_chordwise_resolution: i64,

    /// Lower angle of the two-point in-loop lift-slope probe.
    #[config(
        label = "Fast-probe alpha (low)",
        unit = "deg",
        help = "Lower of a two-point alpha pair used for a fast in-loop lift-slope/stability estimate (not the full sweep)."
    )]
    pub probe_alpha_low_deg: f64,

    /// Upper angle of the two-point in-loop lift-slope probe.
    #[config(
        label = "Fast-probe alpha (high)",
        unit = "deg",
        help = "Higher of the two-point fast-probe alpha pair."
    )]
    pub probe_alpha_high_deg: f64,

    /// Stabilizer incidence perturbation used by the trim solve.
    #[config(
        label = "Trim-solve h-stab incidence probe delta",
        unit = "deg",
        help = "Small horizontal-stabilizer incidence perturbation [deg] used to estimate dCL/di_h and dCm/di_h for the closed-form trim solve. 0.5-2 deg is typical.",
        long_help = "Small horizontal-stabilizer incidence perturbation used to estimate dCL/di_h and dCm/di_h for the closed-form longitudinal trim solve (alpha, tail incidence) run inside the optimizer loop and the final full analysis. Smaller values are more locally linear but noisier; 0.5-2 deg is typical for a small-perturbation VLM probe. See methods.md Sec 9f."
    )]
    pub trim_incidence_probe_delta_deg: f64,

    /// Airspeed the autobalance probe runs at.
    #[config(
        label = "Autobalance probe airspeed",
        unit = "m/s",
        help = "Airspeed used for the autobalance neutral-point probe (finds the CG that gives the target static margin)."
    )]
    pub autobalance_velocity_m_s: f64,

    /// Lower angle of the autobalance probe pair.
    #[config(
        label = "Autobalance probe alpha (low)",
        unit = "deg",
        help = "Lower alpha of the two-point pair used to estimate the Cm-Cl slope during autobalance."
    )]
    pub autobalance_alpha_low_deg: f64,

    /// Upper angle of the autobalance probe pair.
    #[config(
        label = "Autobalance probe alpha (high)",
        unit = "deg",
        help = "Higher alpha of the two-point pair used to estimate the Cm-Cl slope during autobalance."
    )]
    pub autobalance_alpha_high_deg: f64,

    /// Ratio of the tail's local dynamic pressure to freestream; unset, the
    /// empirical [`EMPIRICAL_TAIL_EFFICIENCY`].
    #[serde(default)]
    #[config(
        label = "Tail dynamic-pressure efficiency (eta_t)",
        help = "Ratio of the tail's dynamic pressure to the free stream's (dimensionless); tail lift and moment scale with it. Unset: empirical 0.78 (NASA CR-2144, TN D-3797).",
        long_help = "Ratio of the tail's dynamic pressure to the free stream's: the vortex lattice carries it on the tail, so its lift, moment and induced drag are this ratio times those it would have in the free stream at the same angles, in the trim, the drag table and the neutral point alike (the downwash gradient is not changed). Leave unset for the empirical default 0.78, which the report labels as such: NASA CR-2144's Boeing 747 flight derivatives bound it (0.75 to 0.80 of the lattice isolated tail slope) and it reproduces the tail increment of NASA TN D-3797 (measured 0.275 of the mean chord, the lattice with a free-stream tail 0.345, a ratio of 0.80). It is not a wake loss: the DATCOM 4.4.1 wing wake (Roskam Part VI Eqs. 8.34 to 8.40) gives 1 for these tails, and no sourced fuselage, nacelle or tail-body term closes the difference. Lower it to make the tail less effective (NP moves forward, more tail incidence to trim). The retained reference-compatibility comparison keeps 0.90."
    )]
    pub tail_efficiency: Option<f64>,

    /// Whether the fuselage's destabilising contribution is included.
    #[config(
        label = "Include fuselage destabilising effect",
        help = "Whether to include the geometry-driven fuselage (Munk/Multhopp) destabilising contribution, which moves the neutral point forward."
    )]
    pub include_fuselage_stability: bool,

    /// Whether the trim attitude is solved on the Goethert-stretched lattice.
    #[serde(default = "default_compressible_trim", skip_serializing_if = "is_true")]
    #[config(
        label = "Compressible trim (Goethert stretch)",
        help = "Solve trim and the polar on the Goethert/Prandtl-Glauert stretched lattice at the cruise Mach.",
        long_help = "Solve the cruise trim attitude (body angle of attack, stabilizer incidence, lift and moment slopes, neutral point) on the Goethert/Prandtl-Glauert stretched lattice at the cruise Mach, the same transform the neutral-point conditions use. The polar sweep (lift, pitching moment, induced drag) is solved on the same lattice at the true angle of attack, with the number of sweep points raised by about the lift-slope ratio (up to 1.3 times at high Mach) so the lift points keep the incompressible spacing. The reported trimmed cruise point and polar are on the stretched lattice, while the mission and sizing drag table still trims on the incompressible one, so the reported trimmed L/D and the drag the mission flies differ by the compressibility effect (a follow-up changes the table). Off keeps the incompressible lattice: the trim understates the lift slope by about 30 % at Mach 0.78 and overstates the body angle by 1 to 1.5 degrees, and the polar's angle axis is relabelled by the whole-wing Prandtl-Glauert factor, which overstates its lift slope by 3 to 13 % against the stretched lattice (Mach 0.44 to 0.85). It is a second, coarser compressibility model, kept as an opt-out."
    )]
    pub compressible_trim: bool,

    /// Search-model switch, never read from or written to a configuration
    /// file: the drag table's trim nodes take the stabilizer incidence as a
    /// rotation of the tail's panel normals on the lattice of the design
    /// trim (`VlmSystem::solve_with_surface_pitch`, Katz and Plotkin ch. 12
    /// small-angle boundary condition) instead of rebuilding the lattice at
    /// every trial incidence. The screening and refinement models set it;
    /// every delivered and verified analysis keeps the exact lattice.
    #[serde(skip)]
    #[config(skip)]
    pub rotated_tail_nodes: bool,

    /// Lower lift-coefficient bound of the drag-polar fit window.
    #[config(
        label = "Drag-polar fit window: CL minimum",
        help = "Lower CL bound of the window used to fit the parabolic drag polar (CD = CD0 + k*CL^2). Points outside the window are excluded so stall/pre-stall regions don't bias the fit."
    )]
    pub polar_fit_cl_min: f64,

    /// Upper lift-coefficient bound of the drag-polar fit window.
    #[config(
        label = "Drag-polar fit window: CL maximum",
        help = "Upper CL bound of the primary drag-polar fit window."
    )]
    pub polar_fit_cl_max: f64,

    /// Lower bound of the wider fallback fit window.
    #[config(
        label = "Drag-polar fit fallback window: CL minimum",
        help = "Wider fallback lower CL bound used when the primary fit window captures fewer than 3 points."
    )]
    pub polar_fit_cl_min_fallback: f64,

    /// Upper bound of the wider fallback fit window.
    #[config(
        label = "Drag-polar fit fallback window: CL maximum",
        help = "Wider fallback upper CL bound used when the primary fit window captures fewer than 3 points."
    )]
    pub polar_fit_cl_max_fallback: f64,
}

impl AnalysisConfig {
    /// Restore the vortex-lattice mesh the frozen Python implementation used.
    ///
    /// The product defaults deliberately differ (see the module doc): the
    /// reference evaluates its search at one chordwise panel, which samples
    /// the mean camber line only at the leading and trailing edges (where
    /// every mean line is zero) and spends its reported-analysis budget
    /// spanwise, on a surface the builder has already converged.
    ///
    /// Every reference-compatibility replay calls this. A fixture pinned
    /// against the Python model is evidence about the *port* only if this
    /// side meshes the way the reference did; inheriting the product default
    /// would quietly re-point those fixtures at a different aerodynamic
    /// model. The four mesh values are literals here rather than a second
    /// `Default` impl so that changing the product mesh cannot move them.
    pub fn restore_reference_mesh(&mut self) {
        self.spanwise_resolution = 1;
        self.chordwise_resolution = 1;
        self.fine_spanwise_resolution = 2;
        self.fine_chordwise_resolution = 8;
    }
}

fn default_avl_timeout_s() -> f64 {
    600.0
}

fn default_compressible_trim() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl AnalysisConfig {
    /// The dynamic-pressure ratio the horizontal stabilizer is solved with in
    /// the lattice (`Wing::dynamic_pressure_ratio`): the tail efficiency of the
    /// product, `1.0` for the retained reference-compatibility comparison,
    /// whose own neutral-point formula applies it.
    #[must_use]
    pub fn tail_dynamic_pressure_ratio(&self, reference_compatibility: bool) -> Option<f64> {
        if reference_compatibility {
            Some(1.0)
        } else {
            self.tail_efficiency
        }
    }

    /// The ratio the neutral point weights the tail by: the stated
    /// `tail_efficiency`, else `carried`, the ratio the lattice's horizontal
    /// stabilizer already holds (`Airplane::horizontal_tail_dynamic_pressure_ratio`,
    /// set from the geometry by `alas_aero::analysis::apply_tail_efficiency`).
    #[must_use]
    pub fn tail_efficiency_or(&self, carried: f64) -> f64 {
        self.tail_efficiency.unwrap_or(carried)
    }

    /// The tail efficiency the retained reference-compatibility neutral-point
    /// formula applies: always [`REFERENCE_TAIL_EFFICIENCY`], the value the
    /// frozen fixtures were taken at, whatever the product setting is.
    #[must_use]
    pub fn reference_tail_efficiency(&self) -> f64 {
        REFERENCE_TAIL_EFFICIENCY
    }
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            avl_timeout_s: default_avl_timeout_s(),
            sweep_alpha_min_deg: -4.0,
            sweep_alpha_max_deg: 10.0,
            sweep_n_points: 15,
            // See the module doc for the measurements these four come from.
            spanwise_resolution: 1,
            chordwise_resolution: 8,
            fine_spanwise_resolution: 1,
            fine_chordwise_resolution: 16,
            probe_alpha_low_deg: 2.0,
            probe_alpha_high_deg: 3.0,
            trim_incidence_probe_delta_deg: 1.0,
            autobalance_velocity_m_s: 250.0,
            autobalance_alpha_low_deg: 0.0,
            autobalance_alpha_high_deg: 2.0,
            tail_efficiency: None,
            include_fuselage_stability: true,
            compressible_trim: true,
            rotated_tail_nodes: false,
            polar_fit_cl_min: 0.3,
            polar_fit_cl_max: 0.6,
            polar_fit_cl_min_fallback: 0.1,
            polar_fit_cl_max_fallback: 0.8,
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn avl_timeout_roundtrips_defaults_and_is_exposed_in_schema() {
        let config = AnalysisConfig {
            avl_timeout_s: 1234.5,
            ..AnalysisConfig::default()
        };
        let mut value = serde_json::to_value(&config).unwrap();
        assert_eq!(
            serde_json::from_value::<AnalysisConfig>(value.clone())
                .unwrap()
                .avl_timeout_s,
            1234.5
        );
        value.as_object_mut().unwrap().remove("avl_timeout_s");
        assert_eq!(
            serde_json::from_value::<AnalysisConfig>(value)
                .unwrap()
                .avl_timeout_s,
            600.0
        );
        assert!(config.schema().field("avl_timeout_s").is_some());
        for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let mut full = crate::AlasConfig::default();
            full.analysis.avl_timeout_s = invalid;
            assert!(crate::validation::validate(&full)
                .iter()
                .any(|issue| issue.field_path == "analysis.avl_timeout_s"));
        }
    }

    #[test]
    fn the_reported_analysis_is_at_least_as_fine_as_the_in_loop_one() {
        // The whole reason both exist is that the reported numbers are worth
        // more compute than the search is. A fine resolution below the coarse
        // one would mean the reported cruise point is the less trustworthy of
        // the two, which nothing downstream would notice.
        let config = AnalysisConfig::default();
        assert!(config.fine_spanwise_resolution >= config.spanwise_resolution);
        assert!(config.fine_chordwise_resolution >= config.chordwise_resolution);
    }

    #[test]
    fn every_probe_pair_and_window_is_ordered_low_before_high() {
        let config = AnalysisConfig::default();
        assert!(config.sweep_alpha_min_deg < config.sweep_alpha_max_deg);
        assert!(config.probe_alpha_low_deg < config.probe_alpha_high_deg);
        assert!(config.autobalance_alpha_low_deg < config.autobalance_alpha_high_deg);
        assert!(config.polar_fit_cl_min < config.polar_fit_cl_max);
        assert!(config.polar_fit_cl_min_fallback < config.polar_fit_cl_max_fallback);
    }

    #[test]
    fn the_fallback_fit_window_is_wider_than_the_primary_one() {
        // It exists to catch the case where the primary window is too narrow
        // to hold three points, so a fallback inside the primary would never
        // help.
        let config = AnalysisConfig::default();
        assert!(config.polar_fit_cl_min_fallback < config.polar_fit_cl_min);
        assert!(config.polar_fit_cl_max_fallback > config.polar_fit_cl_max);
    }

    #[test]
    fn panel_resolutions_reach_the_form_as_whole_numbers() {
        let schema = AnalysisConfig::default().schema();
        match &schema.field("sweep_n_points").unwrap().entry {
            crate::Entry::Leaf(leaf) => assert_eq!(leaf.kind, crate::Kind::Int),
            crate::Entry::Node(_) => panic!("a panel count is not a group"),
        }
    }
}
