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

use alas_units::{FOOT, INCH, POUND_FORCE, POUND_MASS};

/// FLOPS Table 1 wing constants for transport and hybrid-wing-body aircraft.
const A1: f64 = 8.80;
const A2: f64 = 6.25;
const A3: f64 = 0.68;
const A4: f64 = 0.34;
const A5: f64 = 0.60;
const A6: f64 = 0.035;
const A7: f64 = 1.50;

fn ft(metres: f64) -> f64 {
    metres / FOOT
}

fn ft2(square_metres: f64) -> f64 {
    square_metres / (FOOT * FOOT)
}

fn lb(kilograms: f64) -> f64 {
    kilograms / POUND_MASS
}

fn kg(pounds: f64) -> f64 {
    pounds * POUND_MASS
}

/// Where the wing equivalent bending material factor comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
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
#[derive(Debug, Clone, Copy, PartialEq)]
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
#[derive(Debug, Clone, Copy, PartialEq)]
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
#[derive(Debug, Clone, Copy, PartialEq)]
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
}

/// The structural group, kg, with the wing terms kept separately.
#[derive(Debug, Clone, Copy, PartialEq)]
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
    );
    // Equation 56 multiplies by `NFUSE` linearly. The wing inputs carry the
    // single declared fuselage count (equation 34's `CAYF` reads the same
    // one), so the two equations cannot disagree about the architecture.
    let fuselage = fuselage_kg(
        inputs.fuselage_length_m,
        inputs.fuselage_width_m,
        inputs.fuselage_depth_m,
        inputs.scaled_fuselage_engines,
        inputs.military_cargo_floor,
        inputs.wing.fuselage_count.max(1),
    );
    let main_gear = main_gear_kg(
        inputs.design_landing_mass_kg,
        inputs.main_gear_oleo_length_m,
    );
    let nose_gear = nose_gear_kg(
        inputs.design_landing_mass_kg,
        inputs.nose_gear_oleo_length_m,
    );
    let nacelle = nacelle_kg(
        inputs.total_nacelles,
        inputs.nacelle_diameter_m,
        inputs.nacelle_length_m,
        inputs.rated_thrust_per_engine_n,
    );
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
