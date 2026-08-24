// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/analysis/full_analysis.py
// Reference: alas @ rust-port-baseline.

//! High-fidelity aerodynamic, mass and stability analysis of a candidate design.
//!
//! [`FullAnalysis::run`] builds the 3-D aircraft geometry, calculates the
//! two-pass mass distribution (lumped first, then detailed cabin layout),
//! sets the moment reference to the true physical centre of gravity, runs a
//! fine-mesh vortex-lattice polar sweep, fits the drag polar, finds the
//! operating design point, evaluates neutral point and static margin, checks
//! typed model-derived CG and ground-reaction constraints, and solves the
//! trimmed cruise point.

use std::collections::HashMap;
use std::f64::consts::PI;

use alas_aero::analysis::{AeroAnalysis, PolarSweep, TrimPoint};
use alas_atmo::Atmosphere;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    run_mass_analysis_with_model_checked, MassBreakdown, MassCoordinateModel, MassCoordinates,
};
use alas_math::lstsq::least_squares;
use alas_opt::envelope::{assess_model_cg_envelope, check_cg_envelope};
use alas_payload::build::build_payload_layout;
use alas_payload::layout::PayloadLayout;
use alas_payload::oew::oew_and_cg;
use alas_stab::trim::{neutral_point, stability_and_trim};
use serde::{Deserialize, Serialize};

/// Operating conditions at the cruise design point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DesignPoint {
    /// Angle of attack, in degrees.
    pub alpha_deg: f64,
    /// Lift coefficient.
    pub cl: f64,
    /// Total drag coefficient.
    pub cd: f64,
    /// Lift-to-drag ratio.
    pub l_over_d: f64,
}

/// Genuinely trimmed cruise operating point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrimmedDesignPoint {
    /// Compressibility-corrected trim angle shown in summaries, in degrees.
    ///
    /// This is a presentation quantity. It is intentionally distinct from
    /// [`Self::geometric_body_alpha_deg`]: a 2-D section solver and an
    /// aircraft-attitude requirement must use the geometric VLM angle rather
    /// than this Prandtl-Glauert display correction.
    pub alpha_deg: f64,
    /// Geometric aircraft-body angle passed to the trimmed VLM solve, in
    /// degrees.
    ///
    /// Positive values raise the aircraft nose relative to the freestream.
    /// Root incidence, local washout, and induced angle are not included;
    /// consumers that need a local section condition must add them explicitly.
    pub geometric_body_alpha_deg: f64,
    /// Trimmed horizontal-stabilizer incidence, in degrees.
    pub trim_ih_deg: f64,
    /// Lift coefficient.
    pub cl: f64,
    /// Total drag coefficient.
    pub cd: f64,
    /// Lift-to-drag ratio.
    pub l_over_d: f64,
    /// Residual pitching moment (should be close to zero).
    pub cm_residual: f64,
}

/// Least-squares parabolic fit of the clean polar: `CD = CD0 + k * CL^2`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PolarFit {
    /// Zero-lift parasitic drag coefficient.
    pub cd0: f64,
    /// Induced drag factor `k`.
    pub k: f64,
    /// Oswald efficiency factor `e = 1 / (pi * AR * k)`.
    pub oswald_e: f64,
    /// Wing aspect ratio.
    pub aspect_ratio: f64,
}

/// Complete aerodynamic, mass, and stability report of an aircraft design.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisReport {
    /// The input design vector.
    pub design: DesignVector,
    /// The built 3-D aircraft geometry.
    pub airplane: Airplane,
    /// Fine-resolution polar sweep.
    pub polar: PolarSweep,
    /// Un-trimmed operating point closest to cruise `CL_required`.
    pub design_point: DesignPoint,
    /// Parabolic polar fit.
    pub polar_fit: PolarFit,
    /// Aerodynamic static margin `(x_np - x_cg) / c_ref`.
    pub static_margin: f64,
    /// Neutral point longitudinal position, in meters.
    pub x_neutral_point: f64,
    /// Scalar geometry summary metrics.
    pub geometry_summary: HashMap<String, f64>,
    /// Breakdown of masses by component name, in kg.
    pub component_masses: HashMap<String, f64>,
    /// Centroid positions by component name, in meters `[x, y, z]`.
    pub mass_coordinates: HashMap<String, [f64; 3]>,
    /// Global mass-weighted center of gravity `[x, y, z]`, in meters.
    pub physical_cg: [f64; 3],
    /// Detailed cabin and cargo payload layout, if successfully constructed.
    pub payload_layout: Option<PayloadLayout>,
    /// Jointly trimmed cruise operating point, if trim solve converged.
    pub trimmed_design_point: Option<TrimmedDesignPoint>,
    /// True if the active model CG and gear constraints are compliant.
    ///
    /// Reference-compatibility analyses preserve the frozen Python Boolean;
    /// product analyses use the hard physical floor and typed gear checks.
    pub cg_envelope_ok: Option<bool>,
}

impl AnalysisReport {
    /// Extract the view required by mission vehicle generation.
    pub fn mission_view(&self) -> alas_mission::ReportView {
        alas_mission::ReportView {
            trimmed_l_over_d: self.trimmed_design_point.as_ref().map(|p| p.l_over_d),
            plain_l_over_d: Some(self.design_point.l_over_d),
            design_vector: serde_json::to_value(self.design).unwrap_or_default(),
            geometry_summary: serde_json::to_value(&self.geometry_summary).unwrap_or_default(),
            component_masses: serde_json::to_value(&self.component_masses).unwrap_or_default(),
        }
    }
}

/// Evaluates high-fidelity multidisciplinary analyses for candidate aircraft designs.
#[derive(Debug, Clone)]
pub struct FullAnalysis {
    /// Configuration governing geometry, requirements, analysis fidelity, and mass models.
    pub config: AlasConfig,
    reference_compatibility: bool,
}

impl FullAnalysis {
    /// Create a new analysis orchestrator for `config`.
    pub fn new(mut config: AlasConfig) -> Self {
        config.geometry.engine.apply_engine_spec();
        Self {
            config,
            reference_compatibility: false,
        }
    }

    /// Create a product analysis that preserves engine values supplied by an
    /// imported aircraft-data document.
    ///
    /// The ordinary [`Self::new`] constructor resolves the configured engine
    /// database entry for configuration-built aircraft. CPACS input has
    /// already supplied its own machine-readable engine values, so resolving
    /// the database again would silently replace that source data.
    pub fn new_preserving_engine_config(config: AlasConfig) -> Self {
        Self {
            config,
            reference_compatibility: false,
        }
    }

    /// Construct a full analysis that reproduces the frozen Python wing point.
    ///
    /// Product analyses use [`Self::new`]. This explicit compatibility path is
    /// reserved for the port's reference fixture and historical sensitivity
    /// artifacts, so the physical structural-centroid correction does not
    /// erase evidence of what the translated implementation did.
    pub fn new_reference_compatibility(mut config: AlasConfig) -> Self {
        config.geometry.engine.apply_engine_spec();
        Self {
            config,
            reference_compatibility: true,
        }
    }

    /// Execute the full analysis workflow on `design`.
    pub fn run(
        &self,
        design: &DesignVector,
        include_engines: bool,
    ) -> Result<AnalysisReport, String> {
        let builder = if self.reference_compatibility {
            AircraftBuilder::new_reference_compatibility(Some(self.config.geometry.clone()))
        } else {
            AircraftBuilder::new(Some(self.config.geometry.clone()))
        };
        let plane = builder
            .build(Some(design), include_engines)
            .map_err(|e| format!("geometry build error: {e:?}"))?;
        self.run_on_airplane(design, plane)
    }

    /// Execute the same analysis workflow on an aircraft supplied by a CPACS
    /// interchange document.
    ///
    /// The aerodynamic, mass, stability, and feasibility formulas remain the
    /// same as [`Self::run`]. Only geometry ownership changes: the caller has
    /// already decoded and validated the aircraft data.
    pub fn run_on_airplane(
        &self,
        design: &DesignVector,
        airplane: Airplane,
    ) -> Result<AnalysisReport, String> {
        let req = &self.config.requirements;
        let mut plane = airplane;

        // First pass with lumped payload to determine OEW and approximate CG.
        let coordinate_model = if self.reference_compatibility {
            MassCoordinateModel::ReferenceCompatibility
        } else {
            MassCoordinateModel::StructuralWingbox(&self.config.structures)
        };
        let (masses_init, coords_init, _cg_init) = run_mass_analysis_with_model_checked(
            &plane,
            req,
            &self.config.geometry,
            &self.config.cabin,
            &self.config.control_surfaces,
            Some(&self.config.mass_model),
            None,
            coordinate_model,
        )
        .map_err(|error| format!("mass-coordinate error: {error}"))?;

        // Second pass: build detailed interior layout and recompute mass breakdown and CG.
        let (oew, x_oew) = oew_and_cg(&masses_init, &coords_init);
        let payload_layout = build_payload_layout(&plane, &self.config, oew, x_oew).ok();
        let layout_summary =
            payload_layout
                .as_ref()
                .map(|l| alas_mass::breakdown::PayloadLayoutSummary {
                    total_mass: l.total_mass,
                    cg_x: l.cg_x,
                    cg_y: l.cg_y,
                });

        let (masses, coords, cg) = run_mass_analysis_with_model_checked(
            &plane,
            req,
            &self.config.geometry,
            &self.config.cabin,
            &self.config.control_surfaces,
            Some(&self.config.mass_model),
            layout_summary.as_ref(),
            coordinate_model,
        )
        .map_err(|error| format!("mass-coordinate error: {error}"))?;

        // Anchor the aerodynamic moment reference to the actual physical CG.
        plane.xyz_ref[0] = cg[0];

        // Fine resolution configuration for reporting.
        let mut fine_analysis = self.config.analysis.clone();
        fine_analysis.spanwise_resolution = fine_analysis.fine_spanwise_resolution;
        fine_analysis.chordwise_resolution = fine_analysis.fine_chordwise_resolution;

        let aero = AeroAnalysis::new(
            &plane,
            design.sweep_deg,
            Some(self.config.geometry.clone()),
            Some(self.config.drag_model.clone()),
            Some(fine_analysis.clone()),
        );

        let polar = aero
            .run_sweep(req.cruise_mach, req.cruise_altitude_m)
            .map_err(|e| format!("polar sweep error: {e:?}"))?;

        let design_point = self.compute_design_point(&plane, &polar);
        let polar_fit = self.fit_polar(&plane, &polar);

        let (x_np, sm, _) = neutral_point(&plane, &fine_analysis)
            .map_err(|e| format!("neutral point error: {e:?}"))?;

        let cg_envelope_ok = if self.reference_compatibility {
            let env = check_cg_envelope(
                &plane,
                &masses,
                &coords,
                cg[0],
                x_np,
                plane.c_ref,
                &self.config,
            );
            Some(!env.violation)
        } else {
            let env = assess_model_cg_envelope(
                &plane,
                &masses,
                &coords,
                cg[0],
                x_np,
                plane.c_ref,
                &self.config,
            )
            .map_err(|error| format!("model CG assessment error: {error}"))?;
            Some(env.hard_constraints_pass())
        };

        // Trimmed cruise operating point.
        let trimmed_design_point = self.compute_trimmed_design_point(&plane, &aero, &fine_analysis);

        Ok(AnalysisReport {
            design: *design,
            airplane: plane.clone(),
            polar,
            design_point,
            polar_fit,
            static_margin: sm,
            x_neutral_point: x_np,
            geometry_summary: self.geometry_summary(&plane, design),
            component_masses: breakdown_to_map(&masses),
            mass_coordinates: coordinates_to_map(&coords),
            physical_cg: cg,
            payload_layout,
            trimmed_design_point,
            cg_envelope_ok,
        })
    }

    /// Required cruise lift coefficient based on weight and dynamic pressure.
    pub fn cruise_cl(&self, plane: &Airplane) -> f64 {
        let req = &self.config.requirements;
        let atmo = Atmosphere::new(req.cruise_altitude_m);
        let v = req.cruise_mach * atmo.speed_of_sound();
        let q = 0.5 * atmo.density() * v * v;
        req.required_cruise_cl(q, plane.s_ref)
    }

    fn compute_design_point(&self, plane: &Airplane, polar: &PolarSweep) -> DesignPoint {
        let cl_target = self.cruise_cl(plane);
        let mut best_idx = 0usize;
        let mut min_diff = f64::INFINITY;
        for (i, &cl) in polar.cl.iter().enumerate() {
            let diff = (cl - cl_target).abs();
            if diff < min_diff {
                min_diff = diff;
                best_idx = i;
            }
        }
        DesignPoint {
            alpha_deg: polar.alpha_deg[best_idx],
            cl: polar.cl[best_idx],
            cd: polar.cd[best_idx],
            l_over_d: polar.l_over_d[best_idx],
        }
    }

    fn fit_polar(&self, plane: &Airplane, polar: &PolarSweep) -> PolarFit {
        let cfg = &self.config.analysis;
        let mut selected_indices: Vec<usize> = polar
            .cl
            .iter()
            .enumerate()
            .filter(|(_, &cl)| cl > cfg.polar_fit_cl_min && cl < cfg.polar_fit_cl_max)
            .map(|(i, _)| i)
            .collect();

        if selected_indices.len() < 3 {
            selected_indices = polar
                .cl
                .iter()
                .enumerate()
                .filter(|(_, &cl)| {
                    cl > cfg.polar_fit_cl_min_fallback && cl < cfg.polar_fit_cl_max_fallback
                })
                .map(|(i, _)| i)
                .collect();
        }

        let (cd0, k) = if selected_indices.len() >= 2 {
            let a_mat: Vec<Vec<f64>> = selected_indices
                .iter()
                .map(|&i| vec![1.0, polar.cl[i].powi(2)])
                .collect();
            let b_vec: Vec<f64> = selected_indices.iter().map(|&i| polar.cd[i]).collect();
            match least_squares(&a_mat, &b_vec) {
                Ok(sol) => (sol[0], sol[1]),
                Err(_) => (0.02, 0.04),
            }
        } else {
            (0.02, 0.04)
        };

        let ar = plane
            .wings
            .first()
            .map(|w| w.aspect_ratio())
            .unwrap_or(10.0);
        let oswald_e = if k > 0.0 {
            1.0 / (PI * ar * k)
        } else {
            f64::NAN
        };

        PolarFit {
            cd0,
            k,
            oswald_e,
            aspect_ratio: ar,
        }
    }

    fn compute_trimmed_design_point(
        &self,
        plane: &Airplane,
        aero: &AeroAnalysis,
        fine_analysis: &alas_config::AnalysisConfig,
    ) -> Option<TrimmedDesignPoint> {
        let req = &self.config.requirements;
        let cl_target = self.cruise_cl(plane);
        let trim = stability_and_trim(
            plane,
            fine_analysis,
            cl_target,
            req.cruise_mach,
            req.cruise_altitude_m,
        )
        .ok()?;

        let trim_point = TrimPoint {
            trim_alpha_deg: trim.trim_alpha_deg,
            trim_ih_deg: trim.trim_ih_deg,
            cl_alpha: trim.cl_alpha,
        };

        let trim_perf = aero
            .trimmed_performance(&trim_point, req.cruise_mach, req.cruise_altitude_m)
            .ok()?;

        Some(TrimmedDesignPoint {
            alpha_deg: trim_perf.alpha_deg,
            geometric_body_alpha_deg: trim.trim_alpha_deg,
            trim_ih_deg: trim_perf.incidence_deg,
            cl: trim_perf.cl,
            cd: trim_perf.cd,
            l_over_d: trim_perf.l_over_d,
            cm_residual: trim_perf.cm_residual,
        })
    }

    fn geometry_summary(&self, plane: &Airplane, design: &DesignVector) -> HashMap<String, f64> {
        let mut map = HashMap::new();
        if let Some(wing) = plane.wings.first() {
            map.insert("span_m".to_owned(), wing.span());
            map.insert("wing_area_m2".to_owned(), wing.area());
            map.insert("projected_span_m".to_owned(), wing.projected_span());
            map.insert("projected_wing_area_m2".to_owned(), wing.projected_area());
            map.insert("aspect_ratio".to_owned(), wing.aspect_ratio());
            map.insert(
                "mean_aerodynamic_chord_m".to_owned(),
                wing.mean_aerodynamic_chord(),
            );
        }
        map.insert(
            "taper_ratio".to_owned(),
            design.tip_chord_m / design.root_chord_m,
        );
        let wing = &self.config.geometry.wing;
        let legacy_break_span_m = wing.break_span_fraction * design.span_m / 2.0;
        let (
            break_span_m,
            break_span_fraction,
            inboard_sweep_deg,
            outboard_sweep_deg,
            break_leading_edge_x_offset_m,
        ) = if self.reference_compatibility {
            (
                legacy_break_span_m,
                wing.break_span_fraction,
                design.sweep_deg,
                design.sweep_deg - wing.outboard_sweep_decrement_deg,
                legacy_break_span_m * design.sweep_deg.to_radians().tan(),
            )
        } else if let Ok(planform) = wing.transport_planform(design) {
            (
                planform.kink.y_m,
                planform.kink.span_fraction,
                planform.inboard_le_sweep_deg,
                planform.outboard_le_sweep_deg,
                planform.kink.leading_edge_x_m,
            )
        } else {
            (
                legacy_break_span_m,
                wing.break_span_fraction,
                design.sweep_deg,
                design.sweep_deg - wing.outboard_sweep_decrement_deg,
                legacy_break_span_m * design.sweep_deg.to_radians().tan(),
            )
        };
        map.insert("root_chord_m".to_owned(), design.root_chord_m);
        map.insert("break_chord_m".to_owned(), design.break_chord_m);
        map.insert("tip_chord_m".to_owned(), design.tip_chord_m);
        map.insert("break_span_m".to_owned(), break_span_m);
        map.insert("break_span_fraction".to_owned(), break_span_fraction);
        map.insert("inboard_sweep_deg".to_owned(), inboard_sweep_deg);
        map.insert("outboard_sweep_deg".to_owned(), outboard_sweep_deg);
        map.insert(
            "break_leading_edge_x_offset_m".to_owned(),
            break_leading_edge_x_offset_m,
        );
        map.insert("sweep_deg".to_owned(), design.sweep_deg);
        map.insert("fuselage_length_m".to_owned(), design.fuselage_length_m);
        if plane.wings.len() > 1 {
            map.insert("h_stab_area_m2".to_owned(), plane.wings[1].area());
        }
        if plane.wings.len() > 2 {
            map.insert("v_stab_area_m2".to_owned(), plane.wings[2].area());
        }
        map
    }
}

fn breakdown_to_map(mb: &MassBreakdown) -> HashMap<String, f64> {
    mb.as_pairs()
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

fn coordinates_to_map(mc: &MassCoordinates) -> HashMap<String, [f64; 3]> {
    mc.as_pairs()
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}
