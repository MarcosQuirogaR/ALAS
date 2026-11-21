// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Physically-anchored longitudinal neutral point across conditions.
//!
//! [`trim::neutral_point`] and [`trim::stability_and_trim`] in their
//! `reference_compatibility` form apply the tail dynamic-pressure efficiency
//! `eta` to the whole wing+tail VLM offset about the *geometric* quarter-MAC
//! point and shift the neutral point forward by a fuselage term whose strip
//! split, forebody coverage and afterbody ramp direction differ from
//! Multhopp's method, with no nacelle term and no compressibility. That form
//! is kept only to replay the frozen fixtures.
//!
//! This module reads each wing's own force/moment contribution off the VLM
//! panel data directly ([`surfaces::per_surface_contributions`]), applies
//! `eta` to the tail's own lift only ([`surfaces::combine`]), takes the
//! fuselage by Torenbeek's E-37/E-38 pair ([`fuselage::fuselage_terms`]),
//! adds a Torenbeek nacelle term ([`nacelles::nacelle_terms`]), and evaluates the whole
//! pipeline at a stated Mach through a Goethert/Prandtl-Glauert stretch of
//! the lifting-surface mesh ([`surfaces::goethert_stretch`]).
//!
//! [`neutral_point_conditions`] is the public surface for evaluating every
//! condition at once: clean low-speed, clean cruise, the flaps/slats-extended
//! condition of the declared devices, each with the dynamic-pressure-dependent
//! flexibility shift of a swept wing, and the Torenbeek propeller term,
//! reduced to the single most-forward (`critical`) station another module
//! forms the aft CG limit from. [`trim::neutral_point`]'s *product* path
//! (not its `reference_compatibility` twin) returns this module's clean
//! neutral point at its own fixed analysis condition; see its doc comment.

mod fuselage;
mod jet_inlet;
mod nacelles;
mod pipeline;
mod surfaces;
mod tunnel_ac;

use alas_aero::vlm::VlmError;
use alas_config::analysis::AnalysisConfig;
use alas_geom::aircraft::airplane::Airplane;

pub use fuselage::FuselageTerms;
pub use jet_inlet::{jet_inlet_shift_m, InletCondition, InletSlopes, JetInletSpec};
pub use nacelles::{nacelle_terms, NacelleTerm};
pub use surfaces::{
    beta_for_mach, combine, goethert_stretch, per_surface_contributions, SurfaceContribution,
};
pub use tunnel_ac::{
    tunnel_ac_correction, TUNNEL_AC_INTERCEPT, TUNNEL_AC_SLOPE_PER_TAN, TUNNEL_AC_SUMMARY_KEY,
    TUNNEL_AC_SWEEP_MAX_DEG, TUNNEL_AC_SWEEP_MIN_DEG, TUNNEL_AC_SWEEP_ZERO_DEG,
};

pub(crate) use pipeline::compressible::compressible_np_from_stretched_probes;
use pipeline::high_lift::high_lift_scaling;
pub use pipeline::high_lift::{extended_chord_lift, ExtendedChordLift};
pub(crate) use pipeline::product_neutral_point;
use pipeline::{clean_np_at_condition, clean_np_at_conditions_sharing_probe, HighLiftScaling};

/// The declared high-lift devices of an aircraft: spans as fractions of the
/// wing semi-span and the chord extension `c'/c - 1` each device adds when
/// deployed. Where an aircraft declares no device there is no extension and no
/// high-lift condition (`None` in [`NpConditionsInput::high_lift`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighLiftDevices {
    /// Inboard and outboard end of the trailing-edge flap run.
    pub flap_span: (f64, f64),
    /// The band the inboard aileron cuts out of the flap run, if any.
    pub flap_gap: Option<(f64, f64)>,
    /// Chord extension of the flap, `c'/c - 1` (Fowler action).
    pub flap_extension: f64,
    /// Flap chord over wing chord, `c_f/c`.
    pub flap_chord_fraction: f64,
    /// Flap deflection, degrees (cancels part of the gain, eq. G-19 below).
    pub flap_deflection_deg: f64,
    /// Inboard and outboard end of the leading-edge device run.
    pub slat_span: (f64, f64),
    /// Chord extension of the leading-edge device, `c'/c - 1`; zero for a
    /// Krueger flap, which adds no chord (Raymer Table 12.2).
    pub slat_extension: f64,
}

impl HighLiftDevices {
    /// The devices of a design on the declared control-surface runs of
    /// `controls`, with the chord extensions `extension` the design carries in
    /// its performance block; `None` when neither device extends the chord
    /// (hinged flaps, no slats), so there is no extended-chord condition.
    #[must_use]
    pub fn declared(
        extension: alas_config::HighLiftChordExtension,
        controls: &alas_config::control_surfaces::ControlSurfacesConfig,
    ) -> Option<Self> {
        if extension.flap <= 0.0 && extension.slat <= 0.0 {
            return None;
        }
        Some(Self {
            flap_span: (controls.flap_span_start_frac, controls.flap_span_end_frac),
            flap_gap: controls.flap_gap(),
            flap_extension: extension.flap.max(0.0),
            flap_chord_fraction: controls.flap_chord_fraction.max(0.0),
            flap_deflection_deg: extension.flap_landing_deg,
            slat_span: (controls.slat_span_start_frac, controls.slat_span_end_frac),
            slat_extension: extension.slat.max(0.0),
        })
    }
}

/// Conditions [`neutral_point_conditions`] evaluates.
///
/// Sources of the two non-clean effects:
///
/// - **High lift.** An extending device raises the lift slope of the wing with
///   the ratio of the extended to the original wing area (Raymer, *Aircraft
///   Design*, 6th ed., printed p. 413, PDF p. 447), the flap's gain cancelled
///   in part by its deflection: `c_l_alpha'/c_l_alpha = c'/c (1 - (c_f/c')
///   sin^2 delta_f)` (Torenbeek 1982, App. G, p. 537, eq. G-19). With `e =
///   c'/c - 1` the increments are `(e - (c_f/c) sin^2 delta_f) S_flap / S` and
///   the slat's `e S_slat / S`. The added lift acts at the quarter chord of the extended
///   section (see `high_lift_scaling`), so the wing's aerodynamic centre moves
///   with it. The downwash gradient at the tail keeps its clean value: USAF
///   DATCOM (1978, Sec. 4.4.1, pp. 4.4.1-2 and 4.4.1-8/-9) gives the flap's
///   effect on the downwash as an angle increment proportional to the flap
///   lift increment, a trim term. An aircraft without a declared extension has no
///   high-lift condition. Check against NASA CR-2144 (B-747 landing, CG 0.25 c):
///   measured slope ratio 1.16-1.18 and flap effect on the neutral point 0 to
///   -2 points MAC.
/// - **Flexibility.** A static aeroelastic correction computed from the
///   aircraft's own wing box ([`WingStiffness`]) and its VLM span load at the
///   condition's dynamic pressure: the streamwise incidence change of the swept
///   elastic axis `theta cos(L) - phi sin(L)`, `theta = int T/GJ dy`, `phi =
///   int M/EI dy` (Niu, *Airframe Structural Design*, 1st ed., 1988, sec. 3.2,
///   swept wing, pp. 31-33, eq. on p. 32) is added to the lattice and iterated
///   to a fixed point, and the flexible neutral point is formed from the
///   flexible `alpha` probes. The shift is flexible minus rigid. With no
///   stiffness supplied the wing is rigid (no stiffness is invented). Its
///   delta is checked against NASA CR-2144 (Heffley and Jewell 1972, Boeing 747
///   at 636,600 lb, CG 0.25 c, flexible, derivatives printed on pp. 220-221),
///   where `0.25 - Cm_alpha / CL_alpha` falls with dynamic pressure as `0.511 -
///   0.00396 q` (kPa) over 6-30 kPa; that trend is validation only. See
///   `aeroelastic` for the assumptions (no inertia relief, linear theory).
#[derive(Debug, Clone, PartialEq)]
pub struct NpConditionsInput {
    /// The clean low-speed evaluation Mach (typically an approach-speed
    /// order of magnitude, not a stall condition).
    pub low_speed_mach: f64,
    /// Altitude for the low-speed evaluation, m (sea level unless the
    /// caller has a specific low-speed condition).
    pub low_speed_altitude_m: f64,
    /// The clean cruise evaluation Mach.
    pub cruise_mach: f64,
    /// Altitude for the cruise evaluation, m.
    pub cruise_altitude_m: f64,
    /// The declared high-lift devices; `None` for no high-lift condition.
    pub high_lift: Option<HighLiftDevices>,
    /// Whether the dynamic-pressure-dependent flexibility shift is applied
    /// (where its regime holds).
    pub include_flexibility: bool,
    /// Tractor propeller of every podded nacelle, for the Torenbeek propeller
    /// term; `None` for jets and when the blade count is not declared.
    pub propeller: Option<PropellerSpec>,
    /// Engine state of each condition for the jet-inlet propulsive normal
    /// force (Roskam Part VI Eq. 8.111, [`jet_inlet_shift_m`]); `None` for
    /// propeller aircraft and when the engine cycle gives no state.
    pub jet_inlets: Option<JetInletSpec>,
    /// Stiffness of the main wing box for the static aeroelastic correction;
    /// `None` leaves the wing rigid (the correction needs the aircraft's own
    /// sized structure and is never assumed).
    pub wing_stiffness: Option<WingStiffness>,
    /// Aft-fuselage and horizontal-tail flexibility; `None` leaves the
    /// empennage support rigid. Used only together with `wing_stiffness`.
    pub tail_flexibility: Option<TailFlexibility>,
    /// `(Mach, altitude m)` of the maximum operating speed, where V_MO meets
    /// M_MO: the highest dynamic pressure of the en-route envelope, at which
    /// the static aeroelastic loss of stability is largest. `None`: the
    /// condition is not evaluated.
    pub max_operating: Option<(f64, f64)>,
}

/// Propeller data of the Torenbeek aerodynamic-centre shift (Synthesis of
/// Subsonic Airplane Design, Eq. E-42, printed p. 481, for windmilling tractor
/// propellers, after Perkins and Hage): `delta x_ac / c = -0.05 Sum B_p D_p^2
/// l_p / (S c CLa_wf)`, `l_p` positive ahead of 1/4 MAC. Power-on effects are
/// larger and not quantified by the source; the propeller plane is taken at
/// the nacelle nose station.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropellerSpec {
    /// Blades per propeller.
    pub blades: u32,
    /// Propeller diameter, m.
    pub diameter_m: f64,
}

/// Structural stiffness of the main wing's load-carrying box, from the sized
/// wing box (`alas_struct::ground_deflection::box_bending_stiffness_nm2` and
/// `box_torsional_stiffness_nm2`), the input of the static aeroelastic
/// correction. Stations run from the box root (the wing-body junction) to the
/// tip, SI. Inboard of `y_m[0]` the wing is rigid.
#[derive(Debug, Clone, PartialEq)]
pub struct WingStiffness {
    /// Semispan stations, m, strictly increasing.
    pub y_m: Vec<f64>,
    /// Spanwise-frame bending stiffness `EI_n cos^3 L`, N m^2 (`w'' = M_x / EI`).
    pub ei_nm2: Vec<f64>,
    /// Torsional rigidity `G J` of the section normal to the elastic axis, N m^2.
    pub gj_nm2: Vec<f64>,
    /// Chord fraction of the elastic axis (mean of the outermost spar
    /// fractions).
    pub box_axis_fraction: f64,
    /// Mass the semi-wing carries on its elastic axis, whose inertia relieves
    /// the aerodynamic load when the aircraft accelerates with the lift;
    /// `None` takes no relief.
    pub inertia_relief: Option<InertiaRelief>,
}

/// Wing-carried mass for the inertia relief of the aeroelastic loads. With the
/// aircraft in equilibrium the load factor is `L / (m g)` and the carried mass
/// `dm` pulls with `-(L / m) dm`; the mass is taken to act on the elastic axis,
/// so it relieves bending and, through the axis sweep, torque.
#[derive(Debug, Clone, PartialEq)]
pub struct InertiaRelief {
    /// Running mass of one semi-wing at [`WingStiffness::y_m`], kg/m.
    pub carried_kg_m: Vec<f64>,
    /// `(spanwise station m, kg)` point masses of one semi-wing.
    pub point_masses_kg: Vec<(f64, f64)>,
    /// Aircraft mass the relief is referred to, kg.
    pub aircraft_mass_kg: f64,
}

impl WingStiffness {
    /// Whether the table is a usable, finite, positive stiffness description.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        let n = self.y_m.len();
        n >= 2
            && self.ei_nm2.len() == n
            && self.gj_nm2.len() == n
            && self.box_axis_fraction.is_finite()
            && self.y_m.iter().all(|y| y.is_finite())
            && self.y_m.windows(2).all(|p| p[1] > p[0])
            && self.ei_nm2.iter().all(|v| v.is_finite() && *v > 0.0)
            && self.gj_nm2.iter().all(|v| v.is_finite() && *v > 0.0)
            && self.inertia_relief.as_ref().is_none_or(|r| {
                r.carried_kg_m.len() == n
                    && r.carried_kg_m.iter().all(|m| m.is_finite() && *m >= 0.0)
                    && r.aircraft_mass_kg.is_finite()
                    && r.aircraft_mass_kg > 0.0
            })
    }
}

/// Flexibility of the empennage support for the static aeroelastic correction:
/// the aft fuselage as a bending beam and the horizontal-tail box.
///
/// Under the tail load the aft fuselage bends and rotates the tail nose-down by
/// `int M/EI_f dx`, which cuts the tail's incidence change with `alpha`
/// (Obert, *Aerodynamic Design of Transport Aircraft*, 2009, p. 396); the tail
/// box twists and bends like the wing's.
#[derive(Debug, Clone, PartialEq)]
pub struct TailFlexibility {
    /// Stations from the wing-body junction to the tail root, m (geometry
    /// axes, increasing aft).
    pub fuselage_x_m: Vec<f64>,
    /// Fuselage bending stiffness `EI_f` at those stations, N m^2.
    pub fuselage_ei_nm2: Vec<f64>,
    /// Stiffness of one horizontal-tail semispan, as the wing's.
    pub stabilizer: WingStiffness,
}

impl TailFlexibility {
    /// Whether the table is usable.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        let n = self.fuselage_x_m.len();
        n >= 2
            && self.fuselage_ei_nm2.len() == n
            && self.fuselage_x_m.windows(2).all(|p| p[1] > p[0])
            && self
                .fuselage_ei_nm2
                .iter()
                .all(|v| v.is_finite() && *v > 0.0)
            && self.stabilizer.is_usable()
    }
}

/// The static aeroelastic correction of one condition (see
/// [`NpConditionsInput::wing_stiffness`]).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct ElasticNpReport {
    /// Flexible minus rigid clean neutral point, geometry axes, m (negative:
    /// forward). Zero when not converged.
    pub shift_m: f64,
    /// Dynamic pressure of the condition, Pa.
    pub dynamic_pressure_pa: f64,
    /// Whether the aeroelastic fixed point converged. `false` reports static
    /// divergence (or an elastic incidence beyond linear theory) at this `q`.
    pub converged: bool,
    /// Iterations of the slower of the two alpha probes.
    pub iterations: usize,
    /// Last change of the elastic incidence, rad.
    pub residual_rad: f64,
    /// Elastic streamwise incidence at the wing tip of the high-alpha probe,
    /// degrees (negative: washout).
    pub tip_incidence_deg: f64,
}

impl Default for NpConditionsInput {
    fn default() -> Self {
        Self {
            low_speed_mach: 0.2,
            low_speed_altitude_m: 0.0,
            cruise_mach: 0.78,
            cruise_altitude_m: 11_000.0,
            high_lift: None,
            include_flexibility: true,
            propeller: None,
            jet_inlets: None,
            wing_stiffness: None,
            tail_flexibility: None,
            max_operating: None,
        }
    }
}

/// Every intermediate term [`neutral_point_conditions`] and
/// [`clean_np_at_condition`] compute, for reporting and cross-checking.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct NpDiagnostics {
    /// The main wing's own implied aerodynamic centre from the two-alpha
    /// VLM probe (`eta`-independent; not the geometric quarter-MAC point),
    /// geometry axes, m.
    pub x_wing_alone_ac: f64,
    /// The per-surface neutral point before any fuselage/nacelle
    /// correction (F1), geometry axes, m.
    pub x_np_surfaces: f64,
    /// The Mach-consistent total lift-curve slope this evaluation used,
    /// per rad.
    pub cl_alpha_total: f64,
    /// The main wing's share of the total lift-curve slope (its `dL` over the
    /// summed `dL` of all lifting surfaces), the factor that converts a
    /// wing-body aerodynamic-centre shift into a neutral-point shift.
    pub wing_lift_fraction: f64,
    /// The VLM-implied downwash gradient at the tail
    /// (`1 - a_t,in_presence / a_t,isolated`), used by the fuselage
    /// afterbody factor; `0.0` with no tail.
    pub deps_dalpha_vlm: f64,
    /// Primary and cross-check fuselage terms (F2).
    pub fuselage: FuselageTerms,
    /// Every nacelle's own Torenbeek term (F3); empty with no podded
    /// fuselage.
    pub nacelle_terms: Vec<NacelleTerm>,
    /// Sum of [`Self::nacelle_terms`], m (negative: forward).
    pub nacelle_shift_m: f64,
    /// Torenbeek (1982, E-33/E-34, p. 479) wing-body lift interference factor
    /// `K_I` applied to the wing slope (`1.0` when not applied: no fuselage,
    /// `b_f/b >= 0.2` or the fuselage correction disabled).
    pub wing_body_lift_factor: f64,
    /// Station of the extra wing-body lift `(K_I - 1) a_w` (exposed-root
    /// mid-chord), geometry axes, m; NaN when `K_I` is not applied.
    pub x_wing_body_lift_m: f64,
}

/// The full set of conditions [`neutral_point_conditions`] evaluates, plus
/// the single most-forward (`critical`) station another module forms the aft
/// CG limit from.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct NeutralPointConditions {
    /// Clean, rigid neutral point at [`NpConditionsInput::low_speed_mach`].
    pub clean_low_speed: f64,
    /// Clean, rigid neutral point at [`NpConditionsInput::cruise_mach`].
    pub cruise: f64,
    /// Neutral point at the low-speed condition with the flaps and slats
    /// extended (wing slope raised by the extended-area ratio, see
    /// [`NpConditionsInput`]).
    pub high_lift: f64,
    /// The wing slope scale the high-lift condition used (extended over
    /// original wing area).
    pub high_lift_slope_scale: f64,
    /// `(flexible, rigid)` cruise neutral point: the dynamic-pressure
    /// aeroelastic shift (see [`NpConditionsInput`]) applied to the
    /// cruise condition, else both entries are the rigid cruise value.
    pub elastic_band: (f64, f64),
    /// Aeroelastic fixed-point report of the cruise condition (`None` when
    /// flexibility is off or no wing stiffness was supplied).
    pub elastic_cruise: Option<ElasticNpReport>,
    /// The same at the low-speed condition.
    pub elastic_low_speed: Option<ElasticNpReport>,
    /// `(flexible, rigid)` clean neutral point at the maximum operating speed
    /// ([`NpConditionsInput::max_operating`]); `None` when not evaluated.
    #[serde(default)]
    pub max_operating: Option<(f64, f64)>,
    /// Aeroelastic report of that condition.
    #[serde(default)]
    pub elastic_max_operating: Option<ElasticNpReport>,
    /// `min` over the clean low-speed, flexible cruise, flexible maximum
    /// operating speed and high-lift
    /// conditions: the station a critical-condition aft CG limit should be
    /// formed from. High-speed flight is in general the dominant condition
    /// for the aft limit of a transonic airliner, through compressibility and
    /// aeroelasticity (Torenbeek, *Advanced Aircraft Design*, 2013, sec. 6.3.1,
    /// p. 167), and the stability requirements run to V_MO/M_MO, so the
    /// flexible condition is taken there and not only at the cruise point.
    pub critical: f64,
    /// Diagnostics from whichever condition produced [`Self::critical`].
    pub diagnostics: NpDiagnostics,
}

/// The full corrected neutral point: clean low-speed, clean cruise, and the
/// flaps/slats-extended condition, each with the flexibility shift at its own
/// dynamic pressure where it applies, and the most-forward (`critical`)
/// station across them. See [`NpConditionsInput`] for every condition's source
/// and [`NeutralPointConditions`] for the fields.
///
/// # Errors
///
/// See [`VlmError`].
pub fn neutral_point_conditions(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    input: &NpConditionsInput,
) -> Result<NeutralPointConditions, VlmError> {
    // The low-speed and high-lift conditions share Mach, altitude and probe
    // angles, differing only in the added wing slope; the underlying VLM
    // probe (stretched-mesh assembly, alpha pair, isolated-tail downwash
    // solve) is therefore identical between them and is paid for once.
    let airplane = &*crate::trim::with_product_tail(airplane, analysis);
    let scaling = input
        .high_lift
        .as_ref()
        .map_or(HighLiftScaling::NONE, |devices| {
            high_lift_scaling(airplane, devices)
        });
    let ((low, low_diag), (high_lift, high_lift_diag)) = clean_np_at_conditions_sharing_probe(
        airplane,
        analysis,
        input.low_speed_mach,
        input.low_speed_altitude_m,
        HighLiftScaling::NONE,
        scaling,
    )?;
    let (cruise, cruise_diag) = clean_np_at_condition(
        airplane,
        analysis,
        input.cruise_mach,
        input.cruise_altitude_m,
        HighLiftScaling::NONE,
    )?;

    // Torenbeek E-42 propeller term, divided by the wing-fuselage slope
    // (CLa_wf = total slope x wing lift share, as the E-41 nacelle term).
    let propeller_shift = |diag: &NpDiagnostics| -> f64 {
        let cl_alpha = diag.cl_alpha_total * diag.wing_lift_fraction;
        let Some(spec) = input.propeller else {
            return 0.0;
        };
        if !cl_alpha.is_finite() || cl_alpha.abs() < 0.1 {
            return 0.0;
        }
        // Arms to 1/4 of the wing's own (lofted) MAC, as eq. E-41's.
        let x_c4_mac = crate::trim::main_wing(airplane)
            .map(|wing| wing.aerodynamic_center(0.25)[0])
            .filter(|x| x.is_finite())
            .unwrap_or(airplane.xyz_ref[0]);
        let sum: f64 = airplane
            .fuselages
            .iter()
            .skip(1)
            .filter_map(|pod| pod.xsecs.first())
            .map(|first| {
                f64::from(spec.blades)
                    * spec.diameter_m
                    * spec.diameter_m
                    * (x_c4_mac - first.xyz_c[0])
            })
            .sum();
        -0.05 * sum / (airplane.s_ref.max(1.0) * cl_alpha)
    };
    // Roskam Part VI Eq. 8.111 jet-inlet normal force at the engine state of
    // each condition (CS 25.175) and the condition's own neutral point.
    let inlet_shift =
        |x_np: f64, diag: &NpDiagnostics, pick: fn(&JetInletSpec) -> Option<InletCondition>| {
            input.jet_inlets.as_ref().and_then(pick).map_or(0.0, |condition| {
                let slopes = InletSlopes {
                    cl_alpha_total: diag.cl_alpha_total,
                    cl_alpha_wb: diag.cl_alpha_total * diag.wing_lift_fraction,
                    deps_dalpha: diag.deps_dalpha_vlm,
                };
                jet_inlet_shift_m(airplane, condition, x_np, slopes)
            })
        };
    let low = low + propeller_shift(&low_diag);
    let low = low + inlet_shift(low, &low_diag, |s| Some(s.climb));
    let high_lift = high_lift + propeller_shift(&high_lift_diag);
    let high_lift = high_lift + inlet_shift(high_lift, &high_lift_diag, |s| Some(s.approach));
    let cruise = cruise + propeller_shift(&cruise_diag);
    let cruise = cruise + inlet_shift(cruise, &cruise_diag, |s| Some(s.cruise));

    // Static aeroelastic shift of the clean neutral point at each condition's
    // own dynamic pressure, from the wing box's own stiffness; none without it.
    let elastic_at = |mach: f64, altitude_m: f64| -> Result<Option<ElasticNpReport>, VlmError> {
        match (&input.wing_stiffness, input.include_flexibility) {
            (Some(stiffness), true) => pipeline::elastic_np_shift(
                airplane,
                analysis,
                mach,
                altitude_m,
                stiffness,
                input.tail_flexibility.as_ref(),
            )
            .map(Some),
            _ => Ok(None),
        }
    };
    let elastic_low = elastic_at(input.low_speed_mach, input.low_speed_altitude_m)?;
    let elastic_cruise = elastic_at(input.cruise_mach, input.cruise_altitude_m)?;
    let shifted =
        |x_np: f64, report: &Option<ElasticNpReport>| x_np + report.map_or(0.0, |r| r.shift_m);
    let low_flexible = shifted(low, &elastic_low);
    let high_lift_flexible = shifted(high_lift, &elastic_low);
    let cruise_flexible = shifted(cruise, &elastic_cruise);
    // The maximum operating speed: the rigid neutral point at its Mach number
    // and the aeroelastic shift at its dynamic pressure.
    let max_operating = match input.max_operating {
        Some((mach, altitude_m)) => {
            let (rigid, diag) =
                clean_np_at_condition(airplane, analysis, mach, altitude_m, HighLiftScaling::NONE)?;
            let rigid = rigid + propeller_shift(&diag);
            let rigid = rigid + inlet_shift(rigid, &diag, |s| s.max_operating);
            let elastic = elastic_at(mach, altitude_m)?;
            Some((shifted(rigid, &elastic), rigid, elastic, diag))
        }
        None => None,
    };

    let mut candidates = vec![
        (low_flexible, &low_diag),
        (cruise_flexible, &cruise_diag),
        (high_lift_flexible, &high_lift_diag),
    ];
    if let Some((flexible, _, _, diag)) = &max_operating {
        candidates.push((*flexible, diag));
    }
    let (critical, critical_diag) = candidates
        .iter()
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(x, d)| (*x, (*d).clone()))
        .unwrap_or((low, low_diag.clone()));

    Ok(NeutralPointConditions {
        clean_low_speed: low,
        cruise,
        high_lift,
        high_lift_slope_scale: scaling.slope_scale(),
        elastic_band: (cruise_flexible, cruise),
        elastic_cruise,
        elastic_low_speed: elastic_low,
        max_operating: max_operating
            .as_ref()
            .map(|(flexible, rigid, _, _)| (*flexible, *rigid)),
        elastic_max_operating: max_operating
            .as_ref()
            .and_then(|(_, _, elastic, _)| *elastic),
        critical,
        diagnostics: critical_diag,
    })
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn probe_fuselage() -> Fuselage {
        let station = |x: f64, r: f64| {
            FuselageXSec::new([x, 0.0, 0.0], Some(r), None, None, DEFAULT_SHAPE)
                .expect("radius alone is valid")
        };
        Fuselage::new(
            "Fuselage",
            vec![
                station(0.0, 0.6),
                station(3.0, 1.6),
                station(16.0, 1.6),
                station(22.0, 0.7),
            ],
        )
    }

    fn probe_airplane() -> Airplane {
        probe_airplane_with_tail_scale(1.0)
    }

    /// The probe with both horizontal-tail chords scaled by `tail_scale`, so
    /// the tail area and tail volume scale by the same factor.
    fn probe_airplane_with_tail_scale(tail_scale: f64) -> Airplane {
        let main = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([8.0, 0.0, 0.0], 4.0, 0.0, naca("naca0012")),
                WingXSec::new([9.5, 12.0, 0.0], 1.5, 0.0, naca("naca0012")),
            ],
            true,
        );
        let hstab = Wing::new(
            "Horizontal Stabilizer",
            vec![
                WingXSec::new([20.0, 0.0, 0.0], 1.8 * tail_scale, -2.0, naca("naca0012")),
                WingXSec::new([20.6, 4.0, 0.0], 1.0 * tail_scale, -2.0, naca("naca0012")),
            ],
            true,
        );
        let s_ref = main.reference_area();
        let b_ref = main.reference_span();
        let c_ref = main.mean_aerodynamic_chord();
        Airplane {
            name: "Probe".to_owned(),
            xyz_ref: [9.0, 0.0, 0.0],
            wings: vec![main, hstab],
            fuselages: vec![probe_fuselage()],
            s_ref,
            c_ref,
            b_ref,
        }
    }

    #[test]
    fn neutral_point_conditions_reports_a_critical_no_more_aft_than_every_rigid_condition() {
        let plane = probe_airplane();
        let analysis = AnalysisConfig::default();
        let input = NpConditionsInput::default();
        let result = neutral_point_conditions(&plane, &analysis, &input)
            .expect("the probe meshes and solves");
        assert!(result.critical <= result.clean_low_speed + 1e-9);
        assert!(result.critical <= result.cruise + 1e-9);
        assert!(result.critical <= result.high_lift + 1e-9);
        assert!(result.elastic_band.0 <= result.elastic_band.1 + 1e-12);
    }

    /// the tail's dynamic-pressure ratio scales the
    /// tail's lift and moment, not the downwash the wing induces at it: the
    /// downwash gradient the lattice reports, which the fuselage afterbody term
    /// reads, is that of the free-stream tail, and the tail's slope is the
    /// ratio times the free-stream one.
    #[test]
    fn a_tail_dynamic_pressure_ratio_scales_the_tail_slope_and_leaves_the_downwash() {
        let analysis = AnalysisConfig::default();
        // One condition only: the diagnostics are those of the critical
        // condition, so with distinct cruise and low-speed conditions the two
        // runs could report different ones.
        let defaults = NpConditionsInput::default();
        let input = NpConditionsInput {
            cruise_mach: defaults.low_speed_mach,
            cruise_altitude_m: defaults.low_speed_altitude_m,
            high_lift: None,
            include_flexibility: false,
            ..defaults
        };
        let free = probe_airplane_with_tail_scale(1.0);
        let mut weak = free.clone();
        weak.set_horizontal_tail_dynamic_pressure_ratio(0.8);
        // A tail at free-stream dynamic pressure, then at 0.8 (a plane that
        // carries a ratio keeps it; one that does not gets the analysis').
        let free_stream = AnalysisConfig {
            tail_efficiency: Some(1.0),
            ..analysis.clone()
        };
        let a = neutral_point_conditions(&free, &free_stream, &input).expect("solves");
        let b = neutral_point_conditions(&weak, &analysis, &input).expect("solves");
        assert!(
            (a.diagnostics.deps_dalpha_vlm - b.diagnostics.deps_dalpha_vlm).abs() < 1.0e-4,
            "downwash {} vs {}",
            a.diagnostics.deps_dalpha_vlm,
            b.diagnostics.deps_dalpha_vlm
        );
        // The tail share of the total slope falls with the ratio.
        assert!(b.diagnostics.wing_lift_fraction > a.diagnostics.wing_lift_fraction);
        assert!(b.diagnostics.cl_alpha_total < a.diagnostics.cl_alpha_total);
    }

    /// Physical properties of the neutral point, in metres aft of the nose
    /// tip: a tailed aircraft has its neutral point aft of the wing-alone
    /// aerodynamic centre (the tail adds a stabilizing, aft-acting lift-curve
    /// slope; Raymer, Aircraft Design: A Conceptual Approach, 6th ed.,
    /// section 16.3), and a larger tail moves it further aft (Etkin and Reid,
    /// Dynamics of Flight, 3rd ed., section 2.5: the tail contribution grows
    /// with tail volume).
    #[test]
    fn the_neutral_point_is_aft_of_the_wing_alone_centre_and_grows_with_tail_volume() {
        let analysis = AnalysisConfig::default();
        let input = NpConditionsInput::default();
        let mut previous: Option<f64> = None;
        for tail_scale in [0.5, 1.0, 1.5, 2.0] {
            let plane = probe_airplane_with_tail_scale(tail_scale);
            let result = neutral_point_conditions(&plane, &analysis, &input)
                .expect("the probe meshes and solves");
            assert!(
                result.clean_low_speed.is_finite()
                    && result.diagnostics.x_wing_alone_ac.is_finite()
            );
            // The wing-alone centre is the reference; the fuselage term can
            // move the wing-body centre forward, so the claim is on the
            // tailed neutral point against the wing alone.
            assert!(
                result.clean_low_speed > result.diagnostics.x_wing_alone_ac,
                "tail scale {tail_scale}: neutral point {:.3} m not aft of the wing-alone \
                 centre {:.3} m",
                result.clean_low_speed,
                result.diagnostics.x_wing_alone_ac
            );
            if let Some(before) = previous {
                assert!(
                    result.clean_low_speed > before,
                    "tail scale {tail_scale}: neutral point {:.3} m did not move aft of {before:.3} m",
                    result.clean_low_speed
                );
            }
            previous = Some(result.clean_low_speed);
        }
    }

    fn transport_devices() -> HighLiftDevices {
        HighLiftDevices {
            flap_span: (0.10, 0.62),
            flap_gap: None,
            flap_extension: 0.275,
            flap_chord_fraction: 0.25,
            flap_deflection_deg: 35.0,
            slat_span: (0.08, 0.95),
            slat_extension: 0.075,
        }
    }

    #[test]
    fn declared_extending_devices_raise_the_wing_slope_and_the_neutral_point_stays_finite() {
        // Raymer p. 413: the lift slope rises with the extended-area ratio;
        // the added lift acts at the extended section's quarter chord, and the
        // tail loses slope through the larger downwash.
        let plane = probe_airplane();
        let analysis = AnalysisConfig::default();
        let result = neutral_point_conditions(
            &plane,
            &analysis,
            &NpConditionsInput {
                high_lift: Some(transport_devices()),
                ..NpConditionsInput::default()
            },
        )
        .expect("the probe meshes and solves");
        assert!(result.high_lift_slope_scale > 1.0);
        assert!(result.high_lift.is_finite());
        // Extended-chord quarter-chord placement: the tail loss outweighs the
        // aft-moving wing AC, so the NP moves forward, by a few per cent MAC
        // (handbook 1-3 %, CR-2144 flaps-down ~0; here the probe carries the
        // heaviest transport devices, slope scale 1.23).
        let shift = (result.high_lift - result.clean_low_speed) / plane.c_ref;
        assert!(
            (-0.10..0.0).contains(&shift),
            "high-lift NP shift {shift} MAC"
        );
        // No declared device: no extension, no high-lift condition.
        let bare = neutral_point_conditions(&plane, &analysis, &NpConditionsInput::default())
            .expect("the probe meshes and solves");
        assert!((bare.high_lift_slope_scale - 1.0).abs() < 1e-12);
        assert!((bare.high_lift - bare.clean_low_speed).abs() < 1e-9);
    }

    #[test]
    fn a_krueger_only_leading_edge_adds_no_chord() {
        let plane = probe_airplane();
        let analysis = AnalysisConfig::default();
        let no_slat = HighLiftDevices {
            slat_extension: 0.0,
            ..transport_devices()
        };
        let result = neutral_point_conditions(
            &plane,
            &analysis,
            &NpConditionsInput {
                high_lift: Some(no_slat),
                ..NpConditionsInput::default()
            },
        )
        .expect("solves");
        let with_slat = neutral_point_conditions(
            &plane,
            &analysis,
            &NpConditionsInput {
                high_lift: Some(transport_devices()),
                ..NpConditionsInput::default()
            },
        )
        .expect("solves");
        assert!(result.high_lift_slope_scale < with_slat.high_lift_slope_scale);
    }

    fn swept_probe_airplane() -> Airplane {
        let mut plane = probe_airplane();
        let naca = naca("naca0012");
        plane.wings[0] = Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([8.0, 0.0, 0.0], 4.0, 0.0, naca.clone()),
                WingXSec::new([15.5, 12.0, 0.0], 1.5, 0.0, naca),
            ],
            true,
        );
        plane
    }

    /// Wing-box stiffness of the 12 m semispan probe wing: tapering `EI`, `GJ`
    /// scaled by `scale` (a test fixture, not an aircraft).
    fn probe_stiffness(scale: f64, gj_nm2: f64) -> WingStiffness {
        let y_m: Vec<f64> = (0..=6).map(|i| 1.0 + 11.0 * f64::from(i) / 6.0).collect();
        let taper = |y: f64| (1.0 - 0.85 * (y - 1.0) / 11.0).powi(3);
        WingStiffness {
            ei_nm2: y_m.iter().map(|&y| scale * 4.0e7 * taper(y)).collect(),
            gj_nm2: y_m.iter().map(|&y| scale * gj_nm2 * taper(y)).collect(),
            y_m,
            box_axis_fraction: 0.4,
            inertia_relief: None,
        }
    }

    fn shift_of(plane: &Airplane, scale: f64, gj_nm2: f64, mach: f64) -> ElasticNpReport {
        let conditions = neutral_point_conditions(
            plane,
            &AnalysisConfig {
                spanwise_resolution: 12,
                ..AnalysisConfig::default()
            },
            &NpConditionsInput {
                cruise_mach: mach,
                wing_stiffness: Some(probe_stiffness(scale, gj_nm2)),
                ..NpConditionsInput::default()
            },
        )
        .expect("solves");
        conditions.elastic_cruise.expect("a stiffness was supplied")
    }

    #[test]
    fn aeroelastic_shift_is_physical_zero_when_rigid_monotonic_and_forward_when_swept_aft() {
        let swept = swept_probe_airplane();
        let report = shift_of(&swept, 1.0, 1.0e12, 0.78);
        assert!(report.converged, "{report:?}");
        // Bending only (rigid in torsion): aft-swept washout at the tip.
        assert!(report.tip_incidence_deg < 0.0, "{report:?}");
        // Forward (negative x) for an aft-swept wing.
        assert!(report.shift_m < 0.0, "{report:?}");
        // Infinite stiffness: no shift (and no incidence).
        let stiff = shift_of(&swept, 1.0e6, 1.0e12, 0.78);
        assert!(
            stiff.shift_m.abs() < 1.0e-4 * report.shift_m.abs(),
            "{stiff:?}"
        );
        // Monotonic in dynamic pressure (Mach at fixed altitude) and in
        // flexibility (a softer wing is the same as a higher q).
        let slow = shift_of(&swept, 1.0, 1.0e12, 0.4);
        assert!(slow.dynamic_pressure_pa < report.dynamic_pressure_pa);
        assert!(slow.shift_m.abs() < report.shift_m.abs());
        let soft = shift_of(&swept, 0.5, 1.0e12, 0.78);
        assert!(
            soft.converged && soft.shift_m.abs() > report.shift_m.abs(),
            "{soft:?} vs {report:?}"
        );
        // Without a stiffness or with flexibility off the wing is rigid.
        let analysis = AnalysisConfig::default();
        for input in [
            NpConditionsInput::default(),
            NpConditionsInput {
                include_flexibility: false,
                wing_stiffness: Some(probe_stiffness(1.0, 1.5e7)),
                ..NpConditionsInput::default()
            },
        ] {
            let rigid = neutral_point_conditions(&swept, &analysis, &input).expect("solves");
            assert!((rigid.elastic_band.0 - rigid.elastic_band.1).abs() < 1e-12);
            assert!(rigid.elastic_cruise.is_none());
        }
        // The nearly straight probe wing shifts far less than the swept one.
        let straight = shift_of(&probe_airplane(), 1.0, 1.0e12, 0.78);
        assert!(
            straight.shift_m.abs() < 0.5 * report.shift_m.abs(),
            "{straight:?}"
        );
        // A torsionally soft wing with the elastic axis aft of the lift line
        // is statically divergent at this q: reported, not silently shifted.
        let soft_torsion = shift_of(&probe_airplane(), 0.01, 1.5e7, 0.78);
        assert!(!soft_torsion.converged, "{soft_torsion:?}");
        // A bending-soft swept wing whose washout leaves the small-angle range
        // (0.173 rad) is reported not converged, however stable the iteration.
        let beyond = shift_of(&swept, 0.002, 1.0e12, 0.78);
        assert!(!beyond.converged && beyond.shift_m == 0.0, "{beyond:?}");
        assert!(soft_torsion.shift_m == 0.0);
    }

    #[test]
    fn inertia_relief_lowers_the_aeroelastic_shift_of_a_swept_wing() {
        let swept = swept_probe_airplane();
        let run = |relief: Option<InertiaRelief>| {
            let mut stiffness = probe_stiffness(1.0, 1.0e12);
            stiffness.inertia_relief = relief;
            neutral_point_conditions(
                &swept,
                &AnalysisConfig {
                    spanwise_resolution: 12,
                    ..AnalysisConfig::default()
                },
                &NpConditionsInput {
                    wing_stiffness: Some(stiffness),
                    ..NpConditionsInput::default()
                },
            )
            .expect("solves")
            .elastic_cruise
            .expect("a stiffness was supplied")
        };
        let rigid_mass = run(None);
        let n = probe_stiffness(1.0, 1.0e12).y_m.len();
        let relieved = run(Some(InertiaRelief {
            carried_kg_m: vec![400.0; n],
            point_masses_kg: vec![(4.0, 3000.0)],
            aircraft_mass_kg: 40_000.0,
        }));
        assert!(relieved.converged && rigid_mass.converged);
        assert!(relieved.shift_m < 0.0);
        assert!(relieved.shift_m.abs() < rigid_mass.shift_m.abs());
        assert!(relieved.tip_incidence_deg.abs() < rigid_mass.tip_incidence_deg.abs());
    }

    #[test]
    fn aft_fuselage_and_stabilizer_flexibility_move_the_neutral_point_forward() {
        let plane = swept_probe_airplane();
        let tail = |ei_f: f64, box_scale: f64, gj_scale: f64| TailFlexibility {
            fuselage_x_m: (0..=10).map(|i| 12.0 + 0.8 * f64::from(i)).collect(),
            fuselage_ei_nm2: vec![ei_f; 11],
            stabilizer: WingStiffness {
                y_m: (0..=4).map(|i| 0.0 + f64::from(i)).collect(),
                ei_nm2: vec![box_scale * 2.0e6; 5],
                gj_nm2: vec![gj_scale * 1.0e6; 5],
                box_axis_fraction: 0.4,
                inertia_relief: None,
            },
        };
        let run = |t: TailFlexibility| {
            neutral_point_conditions(
                &plane,
                &AnalysisConfig {
                    spanwise_resolution: 12,
                    ..AnalysisConfig::default()
                },
                &NpConditionsInput {
                    // A wing too stiff to deform isolates the empennage.
                    wing_stiffness: Some(probe_stiffness(1.0e6, 1.0e12)),
                    tail_flexibility: Some(t),
                    ..NpConditionsInput::default()
                },
            )
            .expect("solves")
            .elastic_cruise
            .expect("a stiffness was supplied")
        };
        let soft = run(tail(2.0e9, 1.0e6, 1.0e6));
        let stiffer = run(tail(2.0e10, 1.0e6, 1.0e6));
        let rigid_tail = run(tail(5.0e14, 1.0e9, 1.0e9));
        assert!(soft.converged && stiffer.converged && rigid_tail.converged);
        assert!(
            soft.shift_m < 0.0,
            "{soft:?} rigid {rigid_tail:?} stiffer {stiffer:?}"
        );
        assert!(soft.shift_m < stiffer.shift_m, "{soft:?} {stiffer:?}");
        assert!(rigid_tail.shift_m.abs() < 1.0e-3 * soft.shift_m.abs());
        // A bending-soft stabilizer box (torsionally stiff) alone also moves it forward.
        let box_only = run(tail(5.0e14, 5.0, 1.0e12));
        assert!(
            box_only.converged && box_only.shift_m < rigid_tail.shift_m,
            "{box_only:?}"
        );
    }
}
