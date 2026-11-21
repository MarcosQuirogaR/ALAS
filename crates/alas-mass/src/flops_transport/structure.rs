// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! FLOPS transport structural-group equations: wing (equations 10-17 and
//! 33-38 or the detailed factor of [`super::wing_bending`] with 39-41),
//! horizontal tail (46), vertical tail (50), fuselage (56-57), landing gear
//! (63-67), paint (68) and nacelles (69, 74) of NASA/TM-2017-219627 Vol. I.
//!
//! Every function takes SI inputs, converts to the published US customary
//! units with the exact factors in `alas-units`, evaluates the published
//! form and converts the result back to kilograms. The fighter/attack,
//! general-aviation and hybrid-wing-body branches of each equation are not
//! transport equations and are not translated.

use super::units::{ft, ft2, kg, lb};
use alas_units::{INCH, POUND_FORCE};

pub(super) mod gear_length;
pub mod pressurized_fuselage;

use pressurized_fuselage::{pressurized_fuselage_kg, PressurizedFuselageInputs};

/// FLOPS Table 1 wing constants for transport and hybrid-wing-body aircraft.
const A1: f64 = 8.80;
const A2: f64 = 6.25;
const A3: f64 = 0.68;
const A4: f64 = 0.34;
const A5: f64 = 0.60;
const A6: f64 = 0.035;
const A7: f64 = 1.50;

/// Where the wing equivalent bending material factor comes from.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum WingBendingFactor {
    /// Equation 10 from the trapezoidal planform; the pod inertia-relief
    /// factor is equation 38.
    Simplified,
    /// Equation 31 evaluated by [`super::wing_bending::detailed_bending_factor`];
    /// the pod inertia-relief factor is equation 39 with the engine-pod mass
    /// of equations 40-41.
    Detailed {
        /// Equivalent bending material factor `BT`.
        bt: f64,
        /// Engine inertia-relief factor `BTE` (equation 27).
        bte: f64,
        /// Mass of one engine pod including its nacelle, `WPOD`, kg.
        pod_mass_kg: f64,
    },
}

/// SI inputs to the wing equations.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct FlopsWingInputs {
    /// Design gross mass `DG`, kg.
    pub design_gross_mass_kg: f64,
    /// Reference wing area `SW`, m^2 (no glove: `SX = SW`).
    pub wing_area_m2: f64,
    /// Wing span `SPAN`, m.
    pub wing_span_m: f64,
    /// Taper ratio `TR`.
    pub taper_ratio: f64,
    /// Quarter-chord sweep `SWEEP`, degrees.
    pub quarter_chord_sweep_deg: f64,
    /// Weighted average thickness-to-chord ratio `TCA`.
    pub thickness_to_chord: f64,
    /// Total movable surface area `SFLAP`, m^2.
    pub movable_surface_area_m2: f64,
    /// Structural ultimate load factor `ULF`.
    pub ultimate_load_factor: f64,
    /// Composite utilization `FCOMP`, 0-1.
    pub composite_utilization: f64,
    /// Aeroelastic tailoring factor `FAERT`, 0-1.
    pub aeroelastic_tailoring: f64,
    /// Strut bracing factor `FSTRT`, 0-1.
    pub strut_bracing: f64,
    /// Fraction of load carried by the wing `PCTL`.
    pub wing_load_fraction: f64,
    /// Lower-cover service-life factor on the bending material `W1NIR`
    /// ([`service_life_bending_factor`]); 1.0 is the equation as published.
    pub bending_stress_factor: f64,
    /// Number of fuselages `NFUSE`, for the multiple-fuselage factor `CAYF`.
    pub fuselage_count: usize,
    /// Variable-sweep penalty `VARSWP`, 0-1.
    pub variable_sweep_penalty: f64,
    /// Number of wing-mounted engines `NEW`.
    pub wing_mounted_engine_count: usize,
    /// Bending-material factor source.
    pub bending: WingBendingFactor,
}

/// The wing mass terms, kg, and the factors that produced them.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct FlopsWingBreakdown {
    /// Equivalent bending material factor `BT`.
    pub bending_factor: f64,
    /// Propulsion pod inertia-relief factor `CAYE`.
    pub inertia_relief_factor: f64,
    /// Bending material `W1` after inertia relief, kg.
    pub bending_material_kg: f64,
    /// Shear material and control surfaces `W2`, kg.
    pub shear_and_control_kg: f64,
    /// Miscellaneous items `W3`, kg.
    pub miscellaneous_kg: f64,
    /// `W1 + W2 + W3`, kg.
    pub total_kg: f64,
}

/// Equation 10 with 11-17: the simplified equivalent bending material factor.
///
/// The grouping is the one the April 2018 errata to NASA/TM-2017-219627
/// Vol. I prints, not the main body's: the `EMS` power applies to the
/// planform ratio alone and `CAYL * TCA` divides the product. The errata's
/// base is `SPAN^2 / SW`, which differs from the aspect ratio `AR` only
/// through the glove and bat area `GLOV` of equation 9 (`AR = SPAN^2 /
/// (SW - GLOV)`). ALAS builds no glove, so the single `aspect_ratio`
/// argument is both quantities; a gloved planform would need them split.
pub fn simplified_bending_factor(
    aspect_ratio: f64,
    taper_ratio: f64,
    quarter_chord_sweep_deg: f64,
    thickness_to_chord: f64,
    aeroelastic_tailoring: f64,
    strut_bracing: f64,
) -> f64 {
    let tlam = quarter_chord_sweep_deg.to_radians().tan()
        - 2.0 * (1.0 - taper_ratio) / (aspect_ratio * (1.0 + taper_ratio));
    let slam = tlam / (1.0 + tlam * tlam).sqrt();
    let c4 = 1.0 - 0.5 * aeroelastic_tailoring;
    let c6 = 0.5 * aeroelastic_tailoring - 0.16 * strut_bracing;
    let caya = (aspect_ratio - 5.0).max(0.0);
    let cayl = (1.0 - slam * slam) * (1.0 + c6 * slam * slam + 0.03 * caya * c4 * slam);
    let ems = 1.0 - 0.25 * strut_bracing;
    // The April 2018 errata grouping: the power applies to the planform
    // ratio alone and the sweep and thickness factors divide it.
    0.215 * (0.37 + 0.7 * taper_ratio) * aspect_ratio.powf(ems) / (cayl * thickness_to_chord)
}

/// 1 g tensile stress limit of a riveted 2024-T3 lower cover on a short-range
/// airliner (more than 50 000 flights), Pa. Torenbeek, *Advanced Aircraft
/// Design* (2013), sec. 11.7.1, p. 350.
const LOWER_COVER_1G_STRESS_SHORT_RANGE_PA: f64 = 75.0e6;
/// 1 g tensile stress of an integrally machined lower cover on a long-range
/// airliner (about 25 000 flights), Pa. Same page.
const LOWER_COVER_1G_STRESS_LONG_RANGE_PA: f64 = 110.0e6;
/// Static tensile limit of the lower cover, Pa: 450 MPa for 2024-T3 less the
/// 5 % reserve for combined tension and shear. Same page.
const LOWER_COVER_STATIC_TENSILE_PA: f64 = 0.95 * 450.0e6;
/// Recommended compressive limit of a highly loaded 7075-T6 upper cover, Pa.
/// Same book, sec. 11.7.2, p. 351 (transports above about 50 t; a lighter
/// wing has a lower allowable, which makes the factor below slightly smaller).
const UPPER_COVER_COMPRESSIVE_PA: f64 = 420.0e6;
/// Standard inspection cut-out width `w_ic`, m. Same book, eq. 11.57, p. 341.
const INSPECTION_CUTOUT_WIDTH_M: f64 = 0.25;

/// Bending-material factor for the service life the lower cover is designed
/// to: the ratio of the mean root stress term of Torenbeek, *Advanced
/// Aircraft Design* (2013), eq. 11.83 p. 358,
/// `1/sigma_r = (R_ic/sigma_t + 1.25/sigma_c) / 2`, at the aircraft's haul
/// class to the same term at the long-range stress level.
///
/// `sigma_t = min(0.95 * 450 MPa, sigma_1g * n_ult)` with `sigma_1g` 75 MPa
/// for a short-range and 110 MPa for a long-range airliner (sec. 11.7.1,
/// p. 350); `R_ic = 1 + 2 w_ic b_st / S` (eq. 11.57) with the structural span
/// `b_st = b / cos(sweep of the 40 % chord line)` (eq. 11.15 and p. 358).
///
/// The FLOPS bending term carries no service-life input. Against the wing
/// weights of Beltramo et al., NASA CR-151970 (1977), Table 5.1, the FLOPS
/// wing reproduces the seven long-range aircraft (mean predicted/actual 1.00)
/// and is 11 % light on the seven short-range ones; with this factor the
/// short-range mean is 0.95 and the long-range one is unchanged. The long-range
/// level is therefore the reference at which the factor is one.
///
/// Class boundary. The source names the classes by design life in flights:
/// more than 50 000 (short range) and about 25 000 (long range). The haul
/// class stands for that life. In the check above the two groups are apart
/// on a regulatory measure of it, the flight-cycle implementation times of
/// 14 CFR 121.1107(a): F-28, BAC 1-11, DC-9 and 737 60 000 flights and 727
/// 45 000 (short range); DC-8 and DC-10 30 000, L-1011 27 000, 707 and 747
/// 15 000 (long range). A type between the groups (A300B2, 36 000) is not
/// placed by the source; a design goal near 50 000 flights sits with the
/// short-range level.
///
/// Better rule once the data exist: a sourced design service goal per
/// aircraft, with the 1 g stress interpolated between the two published
/// levels. No preset declares a sourced goal today. The short/medium-haul
/// presets that would need one are A220-300, A320-200, A320P2F, C919,
/// E195-E2, ATR72-600, C295, LJ60 and A400M; the long-haul ones (A340-300,
/// A350-900, A380-800, AVE, B747-400, B747-400F, B787-9, DC-10, DC-10-30F)
/// lack one too and stay at the long-range level.
///
/// The limit is a fatigue property of an aluminium cover, so only
/// `metallic_cover_fraction` of the bending material takes the factor.
/// Validity: conventional cantilever transport wings with aluminium-alloy
/// covers, the domain of the source section.
pub fn service_life_bending_factor(
    haul_class: alas_config::OperatingHaulClass,
    ultimate_load_factor: f64,
    wing_area_m2: f64,
    wing_span_m: f64,
    taper_ratio: f64,
    quarter_chord_sweep_deg: f64,
    metallic_cover_fraction: f64,
) -> f64 {
    let stress_1g = match haul_class {
        alas_config::OperatingHaulClass::LongHaul => return 1.0,
        alas_config::OperatingHaulClass::ShortMediumHaul => LOWER_COVER_1G_STRESS_SHORT_RANGE_PA,
    };
    let aspect_ratio = wing_span_m * wing_span_m / wing_area_m2;
    let elastic_axis_tan = quarter_chord_sweep_deg.to_radians().tan()
        - 4.0 / aspect_ratio * 0.15 * (1.0 - taper_ratio) / (1.0 + taper_ratio);
    let structural_span_m = wing_span_m * (1.0 + elastic_axis_tan * elastic_axis_tan).sqrt();
    let r_ic = 1.0 + 2.0 * INSPECTION_CUTOUT_WIDTH_M * structural_span_m / wing_area_m2;
    let root_stress_term = |stress_1g: f64| {
        let tensile = (stress_1g * ultimate_load_factor).min(LOWER_COVER_STATIC_TENSILE_PA);
        r_ic / tensile + 1.25 / UPPER_COVER_COMPRESSIVE_PA
    };
    let metallic =
        root_stress_term(stress_1g) / root_stress_term(LOWER_COVER_1G_STRESS_LONG_RANGE_PA);
    if !metallic.is_finite() {
        return 1.0;
    }
    1.0 + (metallic - 1.0) * metallic_cover_fraction.clamp(0.0, 1.0)
}

/// Equations 33-38 (or 39 for a detailed factor) and 45: the transport wing
/// mass.
pub fn wing_mass(inputs: &FlopsWingInputs) -> FlopsWingBreakdown {
    let span_ft = ft(inputs.wing_span_m);
    let area_ft2 = ft2(inputs.wing_area_m2);
    let dg_lb = lb(inputs.design_gross_mass_kg);
    let aspect_ratio = span_ft * span_ft / area_ft2;
    let fcomp = inputs.composite_utilization;
    let faert = inputs.aeroelastic_tailoring;

    let (bt, caye) = match inputs.bending {
        WingBendingFactor::Simplified => (
            simplified_bending_factor(
                aspect_ratio,
                inputs.taper_ratio,
                inputs.quarter_chord_sweep_deg,
                inputs.thickness_to_chord,
                faert,
                inputs.strut_bracing,
            ),
            1.0 - 0.03 * inputs.wing_mounted_engine_count as f64,
        ),
        // The FLOPS source floors the pod relief at 0.84 (as read in NASA
        // Aviary's `wing_detailed.py`), the value it also assigns to a pod at
        // or beyond the tip.
        WingBendingFactor::Detailed {
            bt,
            bte,
            pod_mass_kg,
        } => (bt, (1.0 - bte / bt * lb(pod_mass_kg) / dg_lb).max(0.84)),
    };

    let cayf = if inputs.fuselage_count > 1 { 0.5 } else { 1.0 };
    let vfact = if inputs.variable_sweep_penalty > 0.0 {
        1.0 + inputs.variable_sweep_penalty
            * (0.96 / inputs.quarter_chord_sweep_deg.to_radians().cos() - 1.0)
    } else {
        1.0
    };
    let w1nir = A1
        * bt
        * (1.0 + (A2 / span_ft).sqrt())
        * inputs.ultimate_load_factor
        * span_ft
        * (1.0 - 0.4 * fcomp)
        * (1.0 - 0.1 * faert)
        * cayf
        * vfact
        * inputs.wing_load_fraction
        * inputs.bending_stress_factor
        / 1.0e6;
    let w2 =
        A3 * (1.0 - 0.17 * fcomp) * ft2(inputs.movable_surface_area_m2).powf(A4) * dg_lb.powf(A5);
    let w3 = A6 * (1.0 - 0.3 * fcomp) * area_ft2.powf(A7);
    let w1 = (dg_lb * caye * w1nir + w2 + w3) / (1.0 + w1nir) - w2 - w3;

    FlopsWingBreakdown {
        bending_factor: bt,
        inertia_relief_factor: caye,
        bending_material_kg: kg(w1),
        shear_and_control_kg: kg(w2),
        miscellaneous_kg: kg(w3),
        total_kg: kg(w1 + w2 + w3),
    }
}

/// Equation 46: transport horizontal-tail mass, kg.
pub fn horizontal_tail_kg(area_m2: f64, taper_ratio: f64, design_gross_mass_kg: f64) -> f64 {
    kg(0.53 * ft2(area_m2) * lb(design_gross_mass_kg).powf(0.2) * (taper_ratio + 0.5))
}

/// Equation 50: transport vertical-tail mass for `count` tails of
/// `area_per_tail_m2` each, kg.
pub fn vertical_tail_kg(
    area_per_tail_m2: f64,
    taper_ratio: f64,
    count: usize,
    design_gross_mass_kg: f64,
) -> f64 {
    kg(0.32
        * lb(design_gross_mass_kg).powf(0.3)
        * (taper_ratio + 0.5)
        * (count as f64).powf(0.7)
        * ft2(area_per_tail_m2).powf(0.85))
}

/// The fin mass factor of a tailplane carried by the fin.
///
/// FLOPS equation 50 has no term for where the tailplane sits. A fin that
/// carries it takes the tailplane's loads through its own box, which Torenbeek,
/// *Synthesis of Subsonic Airplane Design* (1982), Section 8.4.1, p. 282 prices
/// as `k_v = 1 + 0.15 (S_h h_h) / (S_v b_v)` for fin-mounted stabilizers, with
/// `k_v = 1` for a fuselage-mounted tailplane. `h_h` is the height of the
/// tailplane above the fin root and `b_v` the fin span (Fig. 9-20, p. 331), so a
/// T-tail has `h_h / b_v` of one. Neither Aviary validation case has a tailplane
/// on the fin, so both keep the published equation 50 result.
///
/// Published empennage masses keep the term. Roskam, *Airplane Design* Part V
/// (1985), Appendix A7, prints 6 T-tail and 9 conventional transport jets whose
/// fin taper Obert, *Aerodynamic Design of Transport Aircraft* (2009), Fig. 32.15
/// p. 422 plots (DC-9 0.68, 727 0.44, BAC-111 0.68, F-28 0.72; 737 0.29, 747
/// 0.30, A300 0.35, 707 0.36, DC-8 0.29). Over equations 46 + 50 at those tapers
/// the T-tail median stands 1.073 times the conventional one without the term
/// and 0.988 with it; a uniform error of 0.05 in the read tapers keeps that order.
pub fn fin_mounted_tailplane_factor(
    tailplane_area_m2: f64,
    tailplane_height_above_fin_root_m: f64,
    fin_area_m2: f64,
    fin_span_m: f64,
) -> f64 {
    if tailplane_height_above_fin_root_m <= 0.0 || fin_area_m2 <= 0.0 || fin_span_m <= 0.0 {
        return 1.0;
    }
    let height = tailplane_height_above_fin_root_m.min(fin_span_m);
    1.0 + 0.15 * (tailplane_area_m2 * height) / (fin_area_m2 * fin_span_m)
}

/// Equations 56-57: transport fuselage mass, kg. `fuselage_engines` is the
/// distributed-propulsion-scaled count `FNEF`.
pub fn fuselage_kg(
    length_m: f64,
    max_width_m: f64,
    max_depth_m: f64,
    fuselage_engines: f64,
    military_cargo_floor: f64,
    fuselage_count: usize,
) -> f64 {
    let dav_ft = (ft(max_width_m) + ft(max_depth_m)) / 2.0;
    kg(1.35
        * (ft(length_m) * dav_ft).powf(1.28)
        * (1.0 + 0.05 * fuselage_engines)
        * (1.0 + 0.38 * military_cargo_floor)
        * fuselage_count as f64)
}

/// Equation 63 for a land-based transport (`DFTE = 0`): main landing-gear
/// mass, kg, from the design landing mass and the extended oleo length.
pub fn main_gear_kg(design_landing_mass_kg: f64, oleo_length_m: f64) -> f64 {
    kg(0.0117 * lb(design_landing_mass_kg).powf(0.95) * (oleo_length_m / INCH).powf(0.43))
}

/// Equation 64 for a land-based transport (`DFTE = 0`, `CARBAS = 0`): nose
/// landing-gear mass, kg.
pub fn nose_gear_kg(design_landing_mass_kg: f64, oleo_length_m: f64) -> f64 {
    kg(0.048 * lb(design_landing_mass_kg).powf(0.67) * (oleo_length_m / INCH).powf(0.43))
}

/// Equation 66: the FLOPS estimate of the extended main-gear oleo length,
/// m. With wing-mounted engines it follows from the scaled nacelle diameter
/// `FNAC`, the wing dihedral, the outboard engine position and the fuselage
/// width; otherwise it is three quarters of the fuselage length read in
/// inches per foot of length, as FLOPS does.
pub fn main_gear_oleo_length_m(
    scaled_nacelle_diameter_m: f64,
    dihedral_deg: f64,
    outboard_engine_y_m: Option<f64>,
    fuselage_width_m: f64,
    fuselage_length_m: f64,
) -> f64 {
    let inches = match outboard_engine_y_m {
        Some(yee_m) => {
            12.0 * ft(scaled_nacelle_diameter_m)
                + (0.26 - dihedral_deg.to_radians().tan())
                    * (yee_m / INCH - 6.0 * ft(fuselage_width_m))
        }
        None => 0.75 * ft(fuselage_length_m),
    };
    inches * INCH
}

/// Equation 67: the FLOPS estimate of the nose-gear oleo length, m.
pub fn nose_gear_oleo_length_m(main_gear_oleo_length_m: f64) -> f64 {
    0.7 * main_gear_oleo_length_m
}

/// Equation 68: paint mass, kg, from the area density and the total wetted
/// area it covers.
pub fn paint_kg(area_density_kg_m2: f64, wetted_area_m2: f64) -> f64 {
    area_density_kg_m2 * wetted_area_m2
}

/// Equation 69: transport nacelle mass for every nacelle, kg. `total_nacelles`
/// is `TNAC` of equation 74 (the engine count plus one half for a
/// centre-mounted engine).
pub fn nacelle_kg(
    total_nacelles: f64,
    diameter_m: f64,
    length_m: f64,
    rated_thrust_per_engine_n: f64,
) -> f64 {
    kg(0.25
        * total_nacelles
        * ft(diameter_m)
        * ft(length_m)
        * (rated_thrust_per_engine_n / POUND_FORCE).powf(0.36))
}

/// SI inputs to the complete structural group.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct FlopsStructureInputs {
    /// The wing equations' inputs.
    pub wing: FlopsWingInputs,
    /// Horizontal-tail theoretical area `SHT`, m^2.
    pub horizontal_tail_area_m2: f64,
    /// Horizontal-tail taper ratio `TRHT`.
    pub horizontal_tail_taper_ratio: f64,
    /// Vertical-tail theoretical area per tail `SVT`, m^2.
    pub vertical_tail_area_m2: f64,
    /// Vertical-tail taper ratio `TRVT`.
    pub vertical_tail_taper_ratio: f64,
    /// Number of vertical tails `NVERT`.
    pub vertical_tail_count: usize,
    /// Fin mass factor for a tailplane the fin carries, `k_v` of Torenbeek,
    /// *Synthesis of Subsonic Airplane Design* (1982), Section 8.4.1, p. 282.
    /// One for a fuselage-mounted tailplane; see
    /// [`fin_mounted_tailplane_factor`].
    pub fin_mounted_tailplane_factor: f64,
    /// Total fuselage length `XL`, m.
    pub fuselage_length_m: f64,
    /// Maximum fuselage width `WF`, m.
    pub fuselage_width_m: f64,
    /// Maximum fuselage depth `DF`, m.
    pub fuselage_depth_m: f64,
    /// Distributed-propulsion-scaled fuselage-mounted engine count `FNEF`.
    pub scaled_fuselage_engines: f64,
    /// Military cargo floor factor `CARGF`.
    pub military_cargo_floor: f64,
    /// Fractional fuselage increment for a large main-deck cargo door, 0 for
    /// none ([`super::military_cargo::MAIN_DECK_CARGO_DOOR_FUSELAGE_FRACTION`]),
    /// applied to whichever fuselage equation prices the aircraft.
    pub main_deck_cargo_door_fraction: f64,
    /// Design landing mass `WLDG`, kg.
    pub design_landing_mass_kg: f64,
    /// Extended main-gear oleo length `XMLG`, m.
    pub main_gear_oleo_length_m: f64,
    /// Extended nose-gear oleo length `XNLG`, m.
    pub nose_gear_oleo_length_m: f64,
    /// Total nacelles `TNAC` (equation 74).
    pub total_nacelles: f64,
    /// Average nacelle diameter `DNAC`, m.
    pub nacelle_diameter_m: f64,
    /// Average nacelle length `XNAC`, m.
    pub nacelle_length_m: f64,
    /// Rated thrust per engine `THRUST`, N.
    pub rated_thrust_per_engine_n: f64,
    /// Paint area density `WPAINT`, kg/m^2.
    pub paint_area_density_kg_m2: f64,
    /// Total painted wetted area, m^2.
    pub painted_wetted_area_m2: f64,
    /// Nacelle mass for every nacelle, kg, when the FLOPS thrust-based
    /// equation 69 does not apply.
    ///
    /// A turboprop has no rated thrust, so equation 69 has no argument. The
    /// shaft-power group of [`super::turboprop`] evaluates the nacelle from
    /// the NASA GASP area-density relation instead and injects the result
    /// here, so the nacelle still reaches the structural total exactly once
    /// and from exactly one method.
    pub nacelle_mass_override_kg: Option<f64>,
    /// The pressure-bending fuselage inputs, when the aircraft belongs to the
    /// regional turboprop class whose fuselage equation 56 does not describe
    /// ([`pressurized_fuselage`]). `None` evaluates equation 56.
    pub pressurized_fuselage: Option<PressurizedFuselageInputs>,
}

/// The structural group, kg, with the wing terms kept separately.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct FlopsStructureBreakdown {
    /// Wing terms and factors.
    pub wing: FlopsWingBreakdown,
    /// Horizontal tail `WHT`, kg.
    pub horizontal_tail_kg: f64,
    /// Vertical tail(s) `WVT`, kg.
    pub vertical_tail_kg: f64,
    /// Fuselage `WFUSE`, kg.
    pub fuselage_kg: f64,
    /// Main landing gear `WLGM`, kg.
    pub main_gear_kg: f64,
    /// Nose landing gear `WLGN`, kg.
    pub nose_gear_kg: f64,
    /// Nacelles `WNAC`, kg.
    pub nacelle_kg: f64,
    /// Paint `WTPNT`, kg.
    pub paint_kg: f64,
    /// Equation 136 without fins and canards: the structural group, kg.
    pub total_kg: f64,
}

/// Equations 10-71 and 136: the transport structural group.
pub fn estimate_flops_structure(inputs: &FlopsStructureInputs) -> FlopsStructureBreakdown {
    let dg_kg = inputs.wing.design_gross_mass_kg;
    let wing = wing_mass(&inputs.wing);
    let horizontal_tail = horizontal_tail_kg(
        inputs.horizontal_tail_area_m2,
        inputs.horizontal_tail_taper_ratio,
        dg_kg,
    );
    let vertical_tail = vertical_tail_kg(
        inputs.vertical_tail_area_m2,
        inputs.vertical_tail_taper_ratio,
        inputs.vertical_tail_count,
        dg_kg,
    ) * inputs.fin_mounted_tailplane_factor;
    // Equation 56 multiplies by `NFUSE` linearly. The wing inputs carry the
    // single declared fuselage count (equation 34's `CAYF` reads the same
    // one), so the two equations cannot disagree about the architecture.
    let fuselage = match inputs.pressurized_fuselage.as_ref() {
        Some(pressurized) => pressurized_fuselage_kg(pressurized, wing.total_kg),
        None => fuselage_kg(
            inputs.fuselage_length_m,
            inputs.fuselage_width_m,
            inputs.fuselage_depth_m,
            inputs.scaled_fuselage_engines,
            inputs.military_cargo_floor,
            inputs.wing.fuselage_count.max(1),
        ),
    } * (1.0 + inputs.main_deck_cargo_door_fraction);
    let main_gear = main_gear_kg(
        inputs.design_landing_mass_kg,
        inputs.main_gear_oleo_length_m,
    );
    let nose_gear = nose_gear_kg(
        inputs.design_landing_mass_kg,
        inputs.nose_gear_oleo_length_m,
    );
    let nacelle = inputs.nacelle_mass_override_kg.unwrap_or_else(|| {
        nacelle_kg(
            inputs.total_nacelles,
            inputs.nacelle_diameter_m,
            inputs.nacelle_length_m,
            inputs.rated_thrust_per_engine_n,
        )
    });
    let paint = paint_kg(
        inputs.paint_area_density_kg_m2,
        inputs.painted_wetted_area_m2,
    );
    FlopsStructureBreakdown {
        wing,
        horizontal_tail_kg: horizontal_tail,
        vertical_tail_kg: vertical_tail,
        fuselage_kg: fuselage,
        main_gear_kg: main_gear,
        nose_gear_kg: nose_gear,
        nacelle_kg: nacelle,
        paint_kg: paint,
        total_kg: wing.total_kg
            + horizontal_tail
            + vertical_tail
            + fuselage
            + main_gear
            + nose_gear
            + nacelle
            + paint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_units::{FOOT, POUND_MASS};

    /// A 737-800-class wing: 125 ft^2 movable area is about a third of the
    /// 1,341 ft^2 reference area; the published FLOPS defaults elsewhere.
    fn narrowbody_wing() -> FlopsWingInputs {
        FlopsWingInputs {
            design_gross_mass_kg: 79_000.0,
            wing_area_m2: 124.6,
            wing_span_m: 34.3,
            taper_ratio: 0.28,
            quarter_chord_sweep_deg: 25.0,
            thickness_to_chord: 0.115,
            movable_surface_area_m2: 41.5,
            ultimate_load_factor: 3.75,
            composite_utilization: 0.0,
            aeroelastic_tailoring: 0.0,
            strut_bracing: 0.0,
            wing_load_fraction: 1.0,
            bending_stress_factor: 1.0,
            fuselage_count: 1,
            variable_sweep_penalty: 0.0,
            wing_mounted_engine_count: 2,
            bending: WingBendingFactor::Simplified,
        }
    }

    #[test]
    fn the_simplified_bending_factor_reproduces_equations_10_to_17_by_hand() {
        // AR 9.44, TR 0.28, sweep 25 deg, t/c 0.115, no tailoring or strut.
        let tlam = 25.0_f64.to_radians().tan() - 2.0 * 0.72 / (9.44 * 1.28);
        let slam = tlam / (1.0 + tlam * tlam).sqrt();
        let caya = 9.44 - 5.0;
        let cayl = (1.0 - slam * slam) * (1.0 + 0.03 * caya * slam);
        let expected = 0.215 * (0.37 + 0.7 * 0.28) * 9.44 / (cayl * 0.115);
        let bt = simplified_bending_factor(9.44, 0.28, 25.0, 0.115, 0.0, 0.0);
        assert!((bt - expected).abs() < 1e-12, "{bt} vs {expected}");
    }

    #[test]
    fn a_narrowbody_wing_lands_in_the_published_weight_class() {
        // Published group statements put a narrowbody transport wing at
        // 9-11 percent of its maximum takeoff weight (B737-200 9.2%,
        // B727-200 10.4%, DC-9-30 10.5%, Roskam Part V); the hand
        // evaluation of equations 10-37 for these inputs is 16,503 lb.
        let inputs = narrowbody_wing();
        let wing = wing_mass(&inputs);
        let total_lb = wing.total_kg / POUND_MASS;
        let fraction = wing.total_kg / inputs.design_gross_mass_kg;
        assert!(
            (0.08..=0.12).contains(&fraction),
            "wing {total_lb:.0} lb, {fraction:.3} of DG"
        );
        assert!((total_lb - 16_503.0).abs() < 60.0, "wing {total_lb:.0} lb");
        assert!(wing.bending_material_kg > 0.0);
        assert!(wing.shear_and_control_kg > 0.0);
        assert!(wing.miscellaneous_kg > 0.0);
        assert!((wing.inertia_relief_factor - 0.94).abs() < 1e-12);
        let sum = wing.bending_material_kg + wing.shear_and_control_kg + wing.miscellaneous_kg;
        assert!((sum - wing.total_kg).abs() < 1e-9);
    }

    #[test]
    fn the_service_life_factor_follows_the_published_stress_levels() {
        use alas_config::OperatingHaulClass::{LongHaul, ShortMediumHaul};
        // A320-class planform: 122.6 m2, 34.1 m span, taper 0.24, 25 deg.
        let factor = |haul, ulf, metallic| {
            service_life_bending_factor(haul, ulf, 122.6, 34.1, 0.24, 25.0, metallic)
        };
        assert_eq!(factor(LongHaul, 3.75, 1.0), 1.0);
        // b_st = 34.1 / cos(22.6 deg) = 36.9 m, R_ic = 1 + 0.5 * 36.9 / 122.6 = 1.151;
        // (1.151/281.25 + 1.25/420) / (1.151/412.5 + 1.25/420) = 1.226.
        let short = factor(ShortMediumHaul, 3.75, 1.0);
        assert!((short - 1.226).abs() < 2e-3, "{short}");
        // Composite covers carry no aluminium fatigue limit.
        assert_eq!(factor(ShortMediumHaul, 3.75, 0.0), 1.0);
        assert!((factor(ShortMediumHaul, 3.75, 0.5) - (1.0 + 0.5 * (short - 1.0))).abs() < 1e-12);
        // Above n_ult = 5.7 the static limit governs both classes.
        assert!((factor(ShortMediumHaul, 6.0, 1.0) - 1.0).abs() < 1e-12);
        // The factor scales the bending material and nothing else.
        let mut inputs = narrowbody_wing();
        let base = wing_mass(&inputs);
        inputs.bending_stress_factor = short;
        let heavier = wing_mass(&inputs);
        assert!(heavier.bending_material_kg > base.bending_material_kg);
        assert_eq!(heavier.shear_and_control_kg, base.shear_and_control_kg);
        assert_eq!(heavier.miscellaneous_kg, base.miscellaneous_kg);
    }

    #[test]
    fn composites_and_engines_relieve_the_wing_as_the_equations_say() {
        let metallic = wing_mass(&narrowbody_wing());
        let mut composite_inputs = narrowbody_wing();
        composite_inputs.composite_utilization = 1.0;
        let composite = wing_mass(&composite_inputs);
        assert!(composite.total_kg < metallic.total_kg);
        assert!((composite.miscellaneous_kg / metallic.miscellaneous_kg - 0.7).abs() < 1e-9);
        assert!(
            (composite.shear_and_control_kg / metallic.shear_and_control_kg - 0.83).abs() < 1e-9
        );

        // Hand evaluation of NASA/TM-2017-219627 Eqs. 33-37 in US units for
        // FCOMP = 0, 0.5, 1: the factors are (1 - 0.4F), (1 - 0.17F)
        // and (1 - 0.3F) on W1NIR, W2 and W3, and the total is
        // (DG CAYE W1NIR + W2 + W3) / (1 + W1NIR). Constants are written out
        // here, not read from the module.
        let ft = 1.0 / 0.3048;
        let lb = 1.0 / 0.453_592_37;
        let (span, area, dg): (f64, f64, f64) = (34.3 * ft, 124.6 * ft * ft, 79_000.0 * lb);
        let bt = simplified_bending_factor(span * span / area, 0.28, 25.0, 0.115, 0.0, 0.0);
        let caye = 1.0 - 0.03 * 2.0;
        let mut previous = f64::INFINITY;
        for fcomp in [0.0_f64, 0.5, 1.0] {
            let w1nir =
                8.80 * bt * (1.0 + (6.25 / span).sqrt()) * 3.75 * span * (1.0 - 0.4 * fcomp)
                    / 1.0e6;
            let w2 = 0.68 * (1.0 - 0.17 * fcomp) * (41.5 * ft * ft).powf(0.34) * dg.powf(0.60);
            let w3 = 0.035 * (1.0 - 0.3 * fcomp) * area.powf(1.50);
            let expected_kg = (dg * caye * w1nir + w2 + w3) / (1.0 + w1nir) / lb;
            let mut inputs = narrowbody_wing();
            inputs.composite_utilization = fcomp;
            let got = wing_mass(&inputs).total_kg;
            assert!(
                (got - expected_kg).abs() < 1e-9 * expected_kg,
                "FCOMP {fcomp}: {got} vs hand {expected_kg}"
            );
            assert!(got < previous, "wing mass must fall as FCOMP rises");
            previous = got;
        }

        let mut clean_wing = narrowbody_wing();
        clean_wing.wing_mounted_engine_count = 0;
        let clean = wing_mass(&clean_wing);
        assert!(clean.bending_material_kg > metallic.bending_material_kg);
    }

    #[test]
    fn the_tail_fuselage_and_gear_equations_match_hand_evaluations() {
        let dg_kg = 79_000.0;
        let dg_lb = dg_kg / POUND_MASS;
        let ht = horizontal_tail_kg(32.8, 0.3, dg_kg) / POUND_MASS;
        let sht = 32.8 / (FOOT * FOOT);
        assert!((ht - 0.53 * sht * dg_lb.powf(0.2) * 0.8).abs() < 1e-9);

        // A tailplane on the fin raises the fin by Torenbeek's k_v, 1.15 for a
        // T-tail of equal tail areas, and a fuselage-mounted one leaves it.
        assert!((fin_mounted_tailplane_factor(26.4, 5.0, 26.4, 5.0) - 1.15).abs() < 1e-12);
        assert_eq!(fin_mounted_tailplane_factor(26.4, -0.5, 26.4, 5.0), 1.0);
        let vt = vertical_tail_kg(26.4, 0.3, 1, dg_kg) / POUND_MASS;
        let svt = 26.4 / (FOOT * FOOT);
        assert!((vt - 0.32 * dg_lb.powf(0.3) * 0.8 * svt.powf(0.85)).abs() < 1e-9);

        let fuse = fuselage_kg(38.0, 3.76, 4.01, 0.0, 0.0, 1) / POUND_MASS;
        let dav = (3.76 / FOOT + 4.01 / FOOT) / 2.0;
        assert!((fuse - 1.35 * (38.0 / FOOT * dav).powf(1.28)).abs() < 1e-9);

        let wldg_kg = 66_000.0;
        let main = main_gear_kg(wldg_kg, 2.0) / POUND_MASS;
        assert!(
            (main - 0.0117 * (wldg_kg / POUND_MASS).powf(0.95) * (2.0 / INCH).powf(0.43)).abs()
                < 1e-9
        );
        let nose = nose_gear_kg(wldg_kg, 1.4) / POUND_MASS;
        assert!(
            (nose - 0.048 * (wldg_kg / POUND_MASS).powf(0.67) * (1.4 / INCH).powf(0.43)).abs()
                < 1e-9
        );
    }

    #[test]
    fn the_gear_length_estimate_follows_equations_66_and_67() {
        // Wing engines: 12 FNAC + (0.26 - tan DIH)(YEE - 6 WF), in inches.
        let xmlg = main_gear_oleo_length_m(2.0, 6.0, Some(5.0), 3.76, 38.0);
        let expected_in = 12.0 * (2.0 / FOOT)
            + (0.26 - 6.0_f64.to_radians().tan()) * (5.0 / INCH - 6.0 * (3.76 / FOOT));
        assert!((xmlg / INCH - expected_in).abs() < 1e-9);
        assert!((nose_gear_oleo_length_m(xmlg) - 0.7 * xmlg).abs() < 1e-12);
        // No wing engines: 0.75 XL, feet read as inches.
        let rear = main_gear_oleo_length_m(2.0, 6.0, None, 3.76, 38.0);
        assert!((rear / INCH - 0.75 * 38.0 / FOOT).abs() < 1e-9);
    }

    #[test]
    fn the_nacelle_equation_scales_with_thrust_to_the_0_36_power() {
        let one = nacelle_kg(2.0, 2.0, 4.3, 100_000.0);
        let doubled = nacelle_kg(2.0, 2.0, 4.3, 200_000.0);
        assert!((doubled / one - 2.0_f64.powf(0.36)).abs() < 1e-12);
        assert_eq!(paint_kg(0.0, 500.0), 0.0);
        assert!((paint_kg(0.25, 500.0) - 125.0).abs() < 1e-12);
    }
}
