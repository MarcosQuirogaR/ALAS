// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use alas_aero::vlm::VlmGeometryCache;
use std::sync::{Arc, Mutex};

/// Assembly policy shared by the base probes and every rotated-tail probe.
pub(super) struct TrimAssembly<'a> {
    pub(super) analysis: &'a AnalysisConfig,
    cache: Option<&'a Arc<Mutex<VlmGeometryCache>>>,
    reference_compatibility: bool,
}

impl<'a> TrimAssembly<'a> {
    pub(super) fn new(
        analysis: &'a AnalysisConfig,
        cache: Option<&'a Arc<Mutex<VlmGeometryCache>>>,
    ) -> Self {
        Self {
            analysis,
            cache,
            reference_compatibility: false,
        }
    }

    /// The frozen reference lattice of [`assemble_reference_compatibility`],
    /// uncached.
    pub(super) fn reference_compatibility(analysis: &'a AnalysisConfig) -> Self {
        Self {
            analysis,
            cache: None,
            reference_compatibility: true,
        }
    }

    /// Whether trial incidences may first be iterated on one reference
    /// lattice (see `normal_rotation`): the cached product lattice only. The
    /// frozen compatibility form keeps its historical iteration.
    pub(super) fn rotates_normals(&self) -> bool {
        self.cache.is_some() && !self.reference_compatibility
    }

    /// The persistent companion of this assembly's cache for the family of
    /// stretched lattices, or `None` for an uncached assembly.
    pub(super) fn companion_cache(&self) -> Option<Arc<Mutex<VlmGeometryCache>>> {
        let mut cache = self
            .cache?
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Some(cache.companion())
    }

    pub(super) fn assemble<'p>(&self, airplane: &'p Airplane) -> Result<VlmSystem<'p>, VlmError> {
        if self.reference_compatibility {
            return assemble_reference_compatibility(airplane, self.analysis);
        }
        let Some(cache) = self.cache else {
            return assemble(airplane, self.analysis);
        };
        // The cache is a geometry memo keyed by the mesh; an entry left behind
        // by a panicking holder is rebuilt on a key mismatch, so recover the
        // guard rather than report a poisoned lock as a numerical failure.
        let mut cache = cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        VlmSystem::assemble_cached(
            airplane,
            resolution(self.analysis.spanwise_resolution),
            resolution(self.analysis.chordwise_resolution),
            &mut cache,
        )
    }
}

/// One low-side/high-side VLM probe: a level operating point (no sideslip, no
/// rotation) at `alpha_deg`, solved at the analysis mesh resolution.
pub(crate) fn probe(
    system: &VlmSystem<'_>,
    atmosphere: Atmosphere,
    velocity: f64,
    alpha_deg: f64,
) -> Result<VlmResult, VlmError> {
    let op_point = OperatingPoint::new(atmosphere, velocity, alpha_deg, 0.0, 0.0, 0.0, 0.0);
    system.solve(&op_point)
}

/// The mesh and factored influence matrix of `airplane` at the configured
/// resolution. Probes that share a geometry share one of these: the O(n^3)
/// factorization is paid once and each probe is an O(n^2) solve.
pub(crate) fn assemble<'a>(
    airplane: &'a Airplane,
    analysis: &AnalysisConfig,
) -> Result<VlmSystem<'a>, VlmError> {
    VlmSystem::assemble(
        airplane,
        resolution(analysis.spanwise_resolution),
        resolution(analysis.chordwise_resolution),
    )
}

/// [`assemble`] with the frozen reference lattice
/// ([`VlmSystem::assemble_reference_compatibility`]): the parity form's
/// mesh. For parity fixtures only.
pub(crate) fn assemble_reference_compatibility<'a>(
    airplane: &'a Airplane,
    analysis: &AnalysisConfig,
) -> Result<VlmSystem<'a>, VlmError> {
    VlmSystem::assemble_reference_compatibility(
        airplane,
        resolution(analysis.spanwise_resolution),
        resolution(analysis.chordwise_resolution),
    )
}

/// The configuration's `i64` mesh resolution as the `usize` [`VlmSystem::assemble`]
/// takes: floored at one, as `alas-aero::analysis` does: a zero or negative
/// resolution is not a mesh, and both fields are documented multipliers of at
/// least one.
pub(crate) fn resolution(value: i64) -> usize {
    value.max(1) as usize
}

/// The main wing (or the first wing when none is named [`MAIN_WING_NAME`];
/// `None` with no wings): upstream's
/// `next((w ... if w.name == "Main Wing"), airplane.wings[0])`.
pub(crate) fn main_wing(airplane: &Airplane) -> Option<&Wing> {
    airplane
        .wings
        .iter()
        .find(|w| w.name == MAIN_WING_NAME)
        .or_else(|| airplane.wings.first())
}

/// The main wing's quarter-MAC aerodynamic-centre station, NaN with no wing.
pub(crate) fn main_wing_ac_x(airplane: &Airplane) -> f64 {
    main_wing(airplane).map_or(f64::NAN, |w| w.aerodynamic_center(AC_CHORD_FRACTION)[0])
}

/// The horizontal stabilizer, if the airplane has one named [`HSTAB_NAME`].
pub(crate) fn hstab(airplane: &Airplane) -> Option<&Wing> {
    airplane.wings.iter().find(|w| w.name == HSTAB_NAME)
}

/// A clone of `airplane` with every [`HSTAB_NAME`] section's twist set to
/// `twist_deg`: the rigid stabilizer perturbation, on a copy, about each
/// section's own spanwise axis (the frozen compatibility form). See the
/// module doc for why this is a clone rather than a mutate-and-restore.
pub(super) fn with_hstab_twist(airplane: &Airplane, twist_deg: f64) -> Airplane {
    let mut plane = airplane.clone();
    for wing in plane.wings.iter_mut().filter(|w| w.name == HSTAB_NAME) {
        for xsec in &mut wing.xsecs {
            xsec.twist = twist_deg;
        }
    }
    plane
}

/// [`with_hstab_twist`] with the incidence measured in the plane of symmetry
/// (`Wing::twist_about_pitch_axis`), the product stabilizer setting: each
/// chord stays in its `xz` plane, so the two halves never cross at the root.
pub(super) fn with_hstab_incidence(airplane: &Airplane, incidence_deg: f64) -> Airplane {
    let mut plane = with_hstab_twist(airplane, incidence_deg);
    for wing in plane.wings.iter_mut().filter(|w| w.name == HSTAB_NAME) {
        wing.twist_about_pitch_axis = true;
    }
    plane
}

/// Whether `airplane` already is [`with_hstab_incidence`] at `incidence_deg`:
/// every stabilizer section at that twist and, unless the surface already
/// twists about the pitch axis, no dihedral, where both axes coincide. The
/// base probes then serve as the incidence derivative's reference solve.
pub(super) fn is_at_hstab_incidence(airplane: &Airplane, incidence_deg: f64) -> bool {
    airplane
        .wings
        .iter()
        .filter(|w| w.name == HSTAB_NAME)
        .all(|wing| {
            wing.xsecs.iter().all(|xsec| xsec.twist == incidence_deg)
                && (wing.twist_about_pitch_axis
                    || wing
                        .xsecs
                        .windows(2)
                        .all(|pair| pair[1].xyz_le[2] == pair[0].xyz_le[2]))
        })
}

/// Refine the closed-form trim estimate against the nonlinear VLM residuals.
///
/// The geometry solver is linear in circulation but the stabilizer incidence
/// changes the mesh geometry, so the two-point Jacobian is only a local model.
/// This Newton refinement is intentionally product-only; the explicit
/// compatibility API keeps the historical one-shot result and its fixtures.
///
/// `body_cm_per_deg` is the pitching-moment slope, per degree, of the airframe
/// the lattice does not carry (fuselage and nacelles, STAB-02), in the units of
/// the lattice's own `Cm`: the trim condition is
/// `Cm_lattice + body_cm_per_deg * alpha_deg = 0`. Pass `0.0` for the
/// wings-only trim.
#[allow(clippy::too_many_arguments)] // one named physical input each; a struct would only rename them
pub(super) fn refine_trim(
    airplane: &Airplane,
    assembly: &TrimAssembly<'_>,
    cl_target: f64,
    atmosphere: Atmosphere,
    velocity: f64,
    mut alpha_deg: f64,
    mut incidence_deg: f64,
    body_cm_per_deg: f64,
    jacobian_at_trim: bool,
) -> Result<RefinedTrim, VlmError> {
    const MAX_ITERATIONS: usize = 8;
    const RESIDUAL_TOLERANCE: f64 = 1.0e-7;
    const ALPHA_STEP_DEG: f64 = 0.1;
    const INCIDENCE_STEP_DEG: f64 = 0.1;
    const MAX_UPDATE_DEG: f64 = 5.0;

    let unsettled = |alpha_deg, incidence_deg| RefinedTrim {
        alpha_deg,
        incidence_deg,
        converged: false,
        jacobian: None,
    };
    if !alpha_deg.is_finite() || !incidence_deg.is_finite() || !cl_target.is_finite() {
        return Ok(unsettled(alpha_deg, incidence_deg));
    }
    if assembly.rotates_normals() && alas_aero::vlm::speed::normal_rotation_trim() {
        if let Some(settled) = super::normal_rotation::refine(
            airplane,
            assembly,
            cl_target,
            atmosphere,
            velocity,
            alpha_deg,
            incidence_deg,
            body_cm_per_deg,
            jacobian_at_trim,
        )? {
            return Ok(settled);
        }
    }
    let (start_alpha_deg, start_incidence_deg) = (alpha_deg, incidence_deg);
    // The Newton iteration stalled: the trim as a bracketed root, on the same
    // plane-of-symmetry incidence setting, with the Jacobian at that root on
    // the Newton steps when asked for.
    let bracketed = |last: (f64, f64)| -> Result<RefinedTrim, VlmError> {
        let lattice = |alpha_deg: f64, incidence_deg: f64| -> Result<(f64, f64), VlmError> {
            let plane = with_hstab_incidence(airplane, incidence_deg);
            let solved = probe(&assembly.assemble(&plane)?, atmosphere, velocity, alpha_deg)?;
            Ok((
                solved.cl_lift,
                solved.cm_pitch + body_cm_per_deg * alpha_deg,
            ))
        };
        let root = super::bracket::bracketed_trim::<VlmError>(
            lattice,
            cl_target,
            start_alpha_deg,
            start_incidence_deg,
            RESIDUAL_TOLERANCE,
        )?;
        let Some((alpha_deg, incidence_deg)) = root else {
            return Ok(unsettled(last.0, last.1));
        };
        let jacobian = if jacobian_at_trim {
            let (cl, cm) = lattice(alpha_deg, incidence_deg)?;
            let (cl_a, cm_a) = lattice(alpha_deg + ALPHA_STEP_DEG, incidence_deg)?;
            let (cl_i, cm_i) = lattice(alpha_deg, incidence_deg + INCIDENCE_STEP_DEG)?;
            // `lattice` includes the body slope in both moment probes; the
            // alpha row keeps it, as on the converged Newton path.
            Some([
                [
                    (cl_a - cl) / ALPHA_STEP_DEG,
                    (cl_i - cl) / INCIDENCE_STEP_DEG,
                ],
                [
                    (cm_a - cm) / ALPHA_STEP_DEG,
                    (cm_i - cm) / INCIDENCE_STEP_DEG,
                ],
            ])
            .filter(|rows: &[[f64; 2]; 2]| rows.iter().flatten().all(|v| v.is_finite()))
        } else {
            None
        };
        Ok(RefinedTrim {
            alpha_deg,
            incidence_deg,
            converged: true,
            jacobian,
        })
    };

    for _ in 0..MAX_ITERATIONS {
        let at_incidence = with_hstab_incidence(airplane, incidence_deg);
        let at_incidence = assembly.assemble(&at_incidence)?;
        let base = probe(&at_incidence, atmosphere, velocity, alpha_deg)?;
        let residual_cl = base.cl_lift - cl_target;
        let residual_cm = base.cm_pitch + body_cm_per_deg * alpha_deg;
        if residual_cl.is_finite()
            && residual_cm.is_finite()
            && residual_cl.abs().max(residual_cm.abs()) <= RESIDUAL_TOLERANCE
        {
            // The derivatives at the trimmed state itself, on the same steps
            // as the Newton iteration's: a consumer that moves the trim along
            // its own Jacobian (a lift change at fixed CG) then errs at second
            // order in the move, not at first order in the distance from the
            // probe points to the trim.
            let jacobian = if jacobian_at_trim {
                let alpha_probe = probe(
                    &at_incidence,
                    atmosphere,
                    velocity,
                    alpha_deg + ALPHA_STEP_DEG,
                )?;
                drop(at_incidence);
                let turned = with_hstab_incidence(airplane, incidence_deg + INCIDENCE_STEP_DEG);
                let incidence_probe = probe(
                    &assembly.assemble(&turned)?,
                    atmosphere,
                    velocity,
                    alpha_deg,
                )?;
                Some([
                    [
                        (alpha_probe.cl_lift - base.cl_lift) / ALPHA_STEP_DEG,
                        (incidence_probe.cl_lift - base.cl_lift) / INCIDENCE_STEP_DEG,
                    ],
                    [
                        (alpha_probe.cm_pitch - base.cm_pitch) / ALPHA_STEP_DEG + body_cm_per_deg,
                        (incidence_probe.cm_pitch - base.cm_pitch) / INCIDENCE_STEP_DEG,
                    ],
                ])
                .filter(|rows: &[[f64; 2]; 2]| rows.iter().flatten().all(|v| v.is_finite()))
            } else {
                None
            };
            return Ok(RefinedTrim {
                alpha_deg,
                incidence_deg,
                converged: true,
                jacobian,
            });
        }

        let alpha_probe = probe(
            &at_incidence,
            atmosphere,
            velocity,
            alpha_deg + ALPHA_STEP_DEG,
        )?;
        // Both results own their data; release this geometry's kernel before
        // updating the cache for the incidence probe.
        drop(at_incidence);
        let incidence_probe = with_hstab_incidence(airplane, incidence_deg + INCIDENCE_STEP_DEG);
        let incidence_probe = probe(
            &assembly.assemble(&incidence_probe)?,
            atmosphere,
            velocity,
            alpha_deg,
        )?;
        let jacobian = vec![
            vec![
                (alpha_probe.cl_lift - base.cl_lift) / ALPHA_STEP_DEG,
                (incidence_probe.cl_lift - base.cl_lift) / INCIDENCE_STEP_DEG,
            ],
            vec![
                (alpha_probe.cm_pitch - base.cm_pitch) / ALPHA_STEP_DEG + body_cm_per_deg,
                (incidence_probe.cm_pitch - base.cm_pitch) / INCIDENCE_STEP_DEG,
            ],
        ];
        let rhs = vec![vec![residual_cl], vec![residual_cm]];
        let Ok(update) = linalg::solve(&jacobian, &rhs) else {
            return bracketed((alpha_deg, incidence_deg));
        };
        let Some(delta_alpha) = update.first().and_then(|row| row.first()).copied() else {
            return bracketed((alpha_deg, incidence_deg));
        };
        let Some(delta_incidence) = update.get(1).and_then(|row| row.first()).copied() else {
            return bracketed((alpha_deg, incidence_deg));
        };
        if !delta_alpha.is_finite() || !delta_incidence.is_finite() {
            return bracketed((alpha_deg, incidence_deg));
        }

        let scale = 1.0_f64
            .min(MAX_UPDATE_DEG / delta_alpha.abs().max(1.0e-12))
            .min(MAX_UPDATE_DEG / delta_incidence.abs().max(1.0e-12));
        alpha_deg -= scale * delta_alpha;
        incidence_deg -= scale * delta_incidence;
        if !alpha_deg.is_finite() || !incidence_deg.is_finite() {
            return bracketed((start_alpha_deg, start_incidence_deg));
        }
    }

    bracketed((alpha_deg, incidence_deg))
}

/// The result of [`refine_trim`]: the attitude, whether it met the residual
/// tolerance, and, when asked for and converged, `[[dCL/dalpha, dCL/di_h],
/// [dCm/dalpha, dCm/di_h]]` per degree at that attitude, on the lattice's own
/// moment (the body slope included).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RefinedTrim {
    pub(super) alpha_deg: f64,
    pub(super) incidence_deg: f64,
    pub(super) converged: bool,
    pub(super) jacobian: Option<[[f64; 2]; 2]>,
}

/// The airplane the product stability functions solve: `airplane` with the
/// horizontal tail at the dynamic-pressure ratio of a stated
/// `analysis.tail_efficiency` unless the caller already gave it one (the
/// pipelines that build the aircraft from a configuration do, from the stated
/// value or from the geometry), so the trim, the drag and the neutral point carry
/// one tail effectiveness whichever entry point is used.
pub(crate) fn with_product_tail<'a>(
    airplane: &'a Airplane,
    analysis: &AnalysisConfig,
) -> std::borrow::Cow<'a, Airplane> {
    match analysis.tail_efficiency {
        Some(stated)
            if airplane.horizontal_tail_dynamic_pressure_ratio() == 1.0 && stated != 1.0 =>
        {
            let mut tailed = airplane.clone();
            tailed.set_horizontal_tail_dynamic_pressure_ratio(stated);
            std::borrow::Cow::Owned(tailed)
        }
        _ => std::borrow::Cow::Borrowed(airplane),
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap or
// expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
    use alas_geom::aircraft::wing::WingXSec;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    /// A short circular fuselage spanning past the wing trailing edge, so
    /// [`fuselage_cm_alpha`] reaches both its fore-body and after-body
    /// branches: what the VLM-fed functions need present (upstream's
    /// `fuselages[0]` would `IndexError` on an airplane with none).
    fn probe_fuselage() -> Fuselage {
        let station = |x: f64, r: f64| {
            FuselageXSec::new([x, 0.0, 0.0], Some(r), None, None, DEFAULT_SHAPE)
                .expect("radius alone is valid")
        };
        Fuselage::new(
            "Fuselage",
            vec![
                station(0.0, 0.5),
                station(2.0, 1.5),
                station(10.0, 1.5),
                station(18.0, 0.8),
            ],
        )
    }

    /// A small probe: a symmetric main wing, a circular fuselage, and
    /// optionally a horizontal stabilizer aft of the wing.
    fn probe_airplane(with_hstab: bool) -> Airplane {
        let main = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 3.0, 0.0, naca("naca0012")),
                WingXSec::new([0.0, 8.0, 0.0], 3.0, 0.0, naca("naca0012")),
            ],
            true,
        );
        let mut wings = vec![main];
        if with_hstab {
            wings.push(Wing::new(
                HSTAB_NAME,
                vec![
                    WingXSec::new([15.0, 0.0, 0.0], 1.5, -2.0, naca("naca0012")),
                    WingXSec::new([15.0, 3.0, 0.0], 1.5, -2.0, naca("naca0012")),
                ],
                true,
            ));
        }
        let s_ref = wings[0].reference_area();
        let b_ref = wings[0].reference_span();
        let c_ref = wings[0].mean_aerodynamic_chord();
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [1.0, 0.0, 0.0],
            wings,
            fuselages: vec![probe_fuselage()],
            s_ref,
            c_ref,
            b_ref,
        }
    }

    #[test]
    fn munk_factor_clamps_to_the_tabulated_range() {
        assert_eq!(munk_apparent_mass_factor(2.0), 0.77);
        assert_eq!(munk_apparent_mass_factor(4.0), 0.77);
        assert_eq!(munk_apparent_mass_factor(20.0), 0.98);
        assert_eq!(munk_apparent_mass_factor(100.0), 0.98);
    }

    #[test]
    fn munk_factor_interpolates_between_table_points() {
        // Fineness 9 sits halfway between the 8 (0.91) and 10 (0.94) rows.
        assert!((munk_apparent_mass_factor(9.0) - 0.925).abs() < 1e-15);
    }

    #[test]
    fn fuselage_cm_alpha_is_zero_for_a_single_station_body() {
        // The `len(xs) < 2` guard: a one-station fuselage has no area to
        // integrate over.
        let mut plane = probe_airplane(true);
        plane.fuselages = vec![Fuselage::new(
            "Fuselage",
            vec![
                FuselageXSec::new([0.0, 0.0, 0.0], Some(2.0), None, None, DEFAULT_SHAPE)
                    .expect("radius alone is valid"),
            ],
        )];
        assert_eq!(fuselage_cm_alpha(&plane, 5.5), 0.0);
    }

    #[test]
    fn static_margin_is_nan_when_the_two_probes_carry_the_same_lift() {
        // Coincident probe alphas make dCL exactly zero, reaching the
        // degeneracy guard: the branch no real geometry reaches.
        let plane = probe_airplane(true);
        let analysis = AnalysisConfig {
            autobalance_alpha_low_deg: 2.0,
            autobalance_alpha_high_deg: 2.0,
            ..Default::default()
        };
        let sm = static_margin(&plane, &analysis).expect("the probe meshes and solves");
        assert!(sm.is_nan(), "sm={sm}");
    }

    #[test]
    fn autobalance_leaves_the_cg_unshifted_when_the_static_margin_is_nan() {
        let mut plane = probe_airplane(true);
        let before = plane.xyz_ref[0];
        let analysis = AnalysisConfig {
            autobalance_alpha_low_deg: 2.0,
            autobalance_alpha_high_deg: 2.0,
            ..Default::default()
        };
        let sm = autobalance(&mut plane, 0.1, &analysis).expect("the probe meshes and solves");
        assert!(sm.is_nan(), "sm={sm}");
        assert_eq!(plane.xyz_ref[0], before);
    }

    #[test]
    fn autobalance_shifts_the_cg_aft_toward_a_lower_target_margin() {
        // A stable probe has a positive static margin; asking for a smaller
        // one moves the CG aft (a positive shift), by SM being measured in
        // fractions of the chord.
        let mut plane = probe_airplane(true);
        let before = plane.xyz_ref[0];
        let analysis = AnalysisConfig::default();
        let sm_before =
            autobalance(&mut plane, 0.05, &analysis).expect("the probe meshes and solves");
        assert!(sm_before > 0.05, "sm_before={sm_before}");
        let expected = before + (sm_before - 0.05) * plane.c_ref;
        assert!(
            (plane.xyz_ref[0] - expected).abs() < 1e-12,
            "xyz_ref_x={} expected={expected}",
            plane.xyz_ref[0]
        );
    }

    #[test]
    fn stability_and_trim_degrades_to_pure_alpha_without_a_stabilizer() {
        let plane = probe_airplane(false);
        let analysis = AnalysisConfig::default();
        let result = stability_and_trim(&plane, &analysis, 0.4, 0.3, 0.0)
            .expect("the probe meshes and solves");
        assert!(result.trim_ih_deg.is_nan(), "i_h={}", result.trim_ih_deg);
        assert_eq!(result.cl_ih, 0.0);
        assert_eq!(result.cm_ih, 0.0);
        assert!(result.trim_alpha_deg.is_finite());
    }

    #[test]
    fn stability_and_trim_leaves_the_stabilizer_twist_unchanged() {
        // The perturbation is a clone, so the caller's aircraft comes back as
        // it went in: the property that keeps repeated evaluations from
        // drifting onto altered geometry.
        let plane = probe_airplane(true);
        let before = plane.clone();
        let analysis = AnalysisConfig::default();
        stability_and_trim(&plane, &analysis, 0.4, 0.3, 0.0).expect("the probe meshes and solves");
        assert_eq!(plane.wings, before.wings);
    }

    fn assert_trim_bits_equal(expected: StabilityTrimResult, actual: StabilityTrimResult) {
        let values = |result: StabilityTrimResult| {
            [
                result.x_np,
                result.static_margin,
                result.cl_alpha,
                result.cm_alpha,
                result.trim_alpha_deg,
                result.trim_ih_deg,
                result.cl_ih,
                result.cm_ih,
            ]
            .map(f64::to_bits)
        };
        assert_eq!(actual.converged, expected.converged);
        assert_eq!(values(actual), values(expected));
    }

    /// The Schur factorization update differs from the ordinary LU by
    /// rounding only; trim quantities agree to far better than 1e-9.
    fn assert_trim_close(expected: StabilityTrimResult, actual: StabilityTrimResult) {
        let values = |result: StabilityTrimResult| {
            [
                result.x_np,
                result.static_margin,
                result.cl_alpha,
                result.cm_alpha,
                result.trim_alpha_deg,
                result.trim_ih_deg,
                result.cl_ih,
                result.cm_ih,
            ]
        };
        assert_eq!(actual.converged, expected.converged);
        for (a, e) in values(actual).into_iter().zip(values(expected)) {
            assert!((a - e).abs() <= 1.0e-9 * e.abs().max(1.0e-6), "{a} vs {e}");
        }
    }

    #[test]
    fn cached_trim_with_the_schur_update_matches_fresh_assemblies_to_rounding() {
        cached_trim_across_lift_targets(
            VlmGeometryCache::with_schur_update(true),
            assert_trim_close,
        );
    }

    #[test]
    fn cached_trim_matches_fresh_assemblies_across_lift_targets_and_cg_changes() {
        // The ordinary LU: bitwise equal to the fresh assemblies.
        cached_trim_across_lift_targets(
            VlmGeometryCache::with_schur_update(false),
            assert_trim_bits_equal,
        );
    }

    fn cached_trim_across_lift_targets(
        cache: VlmGeometryCache,
        compare: fn(StabilityTrimResult, StabilityTrimResult),
    ) {
        let mut plane = probe_airplane(true);
        plane.wings[0].xsecs[0].airfoil = naca("naca2412");
        plane.wings[0].xsecs[1].airfoil = naca("naca2412");
        plane.wings[0].xsecs[1].xyz_le = [3.0, 8.0, 0.6];
        let analysis = AnalysisConfig {
            spanwise_resolution: 2,
            chordwise_resolution: 3,
            ..Default::default()
        };
        let cache = Arc::new(Mutex::new(cache));
        for (cg_x, cl_target, mach, altitude_m) in [
            (1.0, 0.25, 0.3, 0.0),
            (1.3, 0.45, 0.7, 10_000.0),
            (0.8, 0.65, 0.2, 1_500.0),
            (1.0, 0.25, 0.3, 0.0),
        ] {
            plane.xyz_ref[0] = cg_x;
            let expected = stability_and_trim(&plane, &analysis, cl_target, mach, altitude_m)
                .expect("fresh trim solve");
            let actual = stability_and_trim_with_cache(
                &plane, &analysis, cl_target, mach, altitude_m, &cache,
            )
            .expect("cached trim solve");
            compare(expected, actual);
        }
    }

    #[test]
    fn cached_trim_invalidates_geometry_and_mesh_and_preserves_missing_tail_fallback() {
        let cache = Arc::new(Mutex::new(VlmGeometryCache::with_schur_update(false)));
        for (with_tail, spanwise, chordwise, wing_x) in [
            (true, 1, 2, 0.0),
            (true, 2, 3, 0.0),
            (true, 2, 3, 1.0),
            (false, 2, 3, 1.0),
        ] {
            let mut plane = probe_airplane(with_tail);
            plane.wings[0].xsecs[0].xyz_le[0] = wing_x;
            let analysis = AnalysisConfig {
                spanwise_resolution: spanwise,
                chordwise_resolution: chordwise,
                ..Default::default()
            };
            let expected =
                stability_and_trim(&plane, &analysis, 0.4, 0.3, 0.0).expect("fresh trim solve");
            let actual = stability_and_trim_with_cache(&plane, &analysis, 0.4, 0.3, 0.0, &cache)
                .expect("cached trim solve");
            assert_trim_bits_equal(expected, actual);
        }
    }

    #[test]
    fn with_hstab_twist_perturbs_the_copy_and_not_the_source() {
        let plane = probe_airplane(true);
        let perturbed = with_hstab_twist(&plane, 5.0);
        for xsec in &perturbed.wings[1].xsecs {
            assert_eq!(xsec.twist, 5.0);
        }
        for xsec in &plane.wings[1].xsecs {
            assert_eq!(xsec.twist, -2.0);
        }
    }

    #[test]
    fn tail_volume_coefficients_report_none_for_missing_surfaces() {
        let main_only = probe_airplane(false);
        let (vh, vv) = tail_volume_coefficients(&main_only);
        assert!(vh.is_none());
        assert!(vv.is_none());

        let with_hstab = probe_airplane(true);
        let (vh, vv) = tail_volume_coefficients(&with_hstab);
        assert!(vh.is_some_and(|v| v > 0.0), "Vh={vh:?}");
        assert!(vv.is_none());
    }

    #[test]
    fn tail_volume_uses_projected_stabilizer_area() {
        let mut plane = probe_airplane(true);
        // Introduce a nonzero stabilizer dihedral so unfolded and reference
        // planform areas are observably different.
        plane.wings[1].xsecs[1].xyz_le[2] = 1.0;
        let (vh, _) = tail_volume_coefficients(&plane);
        let hstab = &plane.wings[1];
        let x_wing_ac = plane.wings[0].aerodynamic_center(AC_CHORD_FRACTION)[0];
        let l_h = (hstab.aerodynamic_center(AC_CHORD_FRACTION)[0] - x_wing_ac).max(0.0);
        let expected = hstab.reference_area() * l_h / (plane.s_ref * plane.c_ref);
        assert!((vh.expect("horizontal tail volume") - expected).abs() < 1e-12);
        assert!((hstab.reference_area() - hstab.unfolded_area()).abs() > 1e-6);
    }

    #[test]
    fn geometry_queries_without_wings_or_fuselage_return_values_not_panics() {
        // No body contributes no fuselage moment; a body with no wing to
        // place the downwash against is NaN; no wings means no tail volumes.
        let mut no_fuselage = probe_airplane(false);
        let mut no_wings = no_fuselage.clone();
        no_fuselage.fuselages.clear();
        no_wings.wings.clear();
        assert_eq!(fuselage_cm_alpha(&no_fuselage, 5.0), 0.0);
        assert!(fuselage_cm_alpha(&no_wings, 5.0).is_nan());
        assert!(main_wing_ac_x(&no_wings).is_nan());
        assert_eq!(tail_volume_coefficients(&no_wings), (None, None));
    }
}
