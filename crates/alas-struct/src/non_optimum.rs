// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Non-optimum (non-ideal) weight of the wing box, item by item.
//!
//! The sized box of [`crate::sizing`] is an ideal, strength-sized structure:
//! continuously tapered, without joints, cut-outs, attachments or aeroelastic
//! stiffening. A built wing carries more. Torenbeek, *Advanced Aircraft Design*
//! (2013), section 11.5 (book p. 339-344), gives each source of that extra
//! weight its own equation; this module evaluates them one by one on the sized
//! box. There is no lumped factor: every term has its own driver, so it moves
//! with the design variable that causes it, which is the book's own argument
//! against the factor-on-ideal-weight form (p. 339).
//!
//! All masses are kilograms for the **complete wing** (both semi-wings), SI
//! throughout; forces are never used (Torenbeek's weights in newtons are
//! divided by `g`, so `W_ref = 200 N` is `200 / 9.80665 kg`).
//!
//! | item | equation | driver |
//! |---|---|---|
//! | joints, splices, fasteners, non-taper | 11.55 | box surface area, built-up 1 mm smeared |
//! | web fail safety and damage tolerance | 11.56 | 15 % of the web (shear) material |
//! | access cut-outs of the lower cover | 11.57 | cut-out width over box width, on the lower cover |
//! | engine pylon attachments | 11.58 | wing-mounted powerplant mass and count |
//! | main-gear support | 11.59 | landing mass, only if the gear is on the wing |
//! | wing-fuselage attachment | 11.61 | ultimate load factor times gross mass |
//! | torsional stiffness | 11.62 | dive dynamic pressure, span, thickness, sweep (Al-alloy) |
//! | paint, sealant, fasteners, jacking fittings | 11.6.7 | 10 % of the secondary wing weight |
//!
//! # What is not here, and why
//!
//! - *Minimum gauge* is not a separate item: the sized box already enforces
//!   `t_skin_min_m`, `t_web_min_m` and the cap minimum, which are heavier than
//!   Torenbeek's 0.8 mm sheet, and the book folds the non-taper cost into
//!   Eq. 11.55.
//! - *Rib-to-cover attachment* is part of the rib mass of the sized box and of
//!   the smeared fastener increment of Eq. 11.55; the book has no separate
//!   term.
//! - *Dynamic gust over-swing* (11.5.5) has no equation in the book: it is
//!   argued to be inside the torsional-stiffness penalty.
//! - *Composite wings.* Torenbeek gives no composite-specific values. Eq. 11.55
//!   uses the book's 20 % bonded reduction of `delta_nid`; Eq. 11.62 is stated
//!   for an Al-alloy box and is not applied to a composite box. Every other
//!   item is applied unchanged. These are declared assumptions.

use alas_geom::wing_structure::WingStructureGeometry;

use crate::sizing::WingboxSizing;

/// Number of items [`evaluate`] returns.
pub const ITEM_COUNT: usize = 8;

/// Standard gravity used to turn Torenbeek's newton constants into kilograms.
const STANDARD_GRAVITY_M_S2: f64 = 9.806_65;

/// Smeared thickness increment of a built-up (fastened) box, m (Eq. 11.55).
const DELTA_NID_BUILT_UP_M: f64 = 1.0e-3;
/// Reduction of `delta_nid` for a bonded structure, Eq. 11.55: 20 %.
const BONDED_FACTOR: f64 = 0.8;
/// Reduction of `delta_nid` for an integrally machined structure: 50 %.
const INTEGRAL_FACTOR: f64 = 0.5;
/// Fail-safe and damage-tolerance penalty on the shear material, Eq. 11.56.
const FAIL_SAFE_WEB_FRACTION: f64 = 0.15;
/// Standard inspection cut-out width, m: 10 in (Eq. 11.57).
const INSPECTION_CUTOUT_WIDTH_M: f64 = 0.25;
/// Pylon attachment coefficient, Eq. 11.58.
const PYLON_COEFFICIENT: f64 = 0.015;
/// Pylon attachment engine-count coefficient, Eq. 11.58.
const PYLON_ENGINE_COUNT_COEFFICIENT: f64 = 0.2;
/// Gear support fraction of the landing mass, Eq. 11.59.
const GEAR_SUPPORT_FRACTION: f64 = 0.006;
/// Wing-fuselage attachment coefficient, Eq. 11.61.
const FUSELAGE_ATTACHMENT_COEFFICIENT: f64 = 0.0003;
/// Torsional-stiffness calibration force, N, "typical" when no in-house data
/// exist (Eq. 11.62 text, book p. 344).
const STIFFNESS_REFERENCE_FORCE_N: f64 = 200.0;
/// Reference dynamic pressure of Eq. 11.62, Pa.
const STIFFNESS_REFERENCE_DYNAMIC_PRESSURE_PA: f64 = 30.0e3;
/// Reference span of Eq. 11.62, m.
const STIFFNESS_REFERENCE_SPAN_M: f64 = 50.0;
/// Miscellaneous items as a fraction of the secondary wing weight, 11.6.7.
const MISCELLANEOUS_FRACTION: f64 = 0.10;
/// Largest normal Mach number of Eq. 11.62: the equation is subsonic.
const MAX_NORMAL_MACH: f64 = 0.95;

/// How the box covers are manufactured, which scales Eq. 11.55.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Construction {
    /// Fabricated, built-up with high-strength fasteners: `delta_nid = 1 mm`.
    BuiltUp,
    /// Adhesively bonded: 20 % less.
    Bonded,
    /// Integrally machined: 50 % less.
    IntegrallyMachined,
}

impl Construction {
    /// Smeared non-ideal thickness increment, m.
    pub fn delta_nid_m(self) -> f64 {
        DELTA_NID_BUILT_UP_M
            * match self {
                Self::BuiltUp => 1.0,
                Self::Bonded => BONDED_FACTOR,
                Self::IntegrallyMachined => INTEGRAL_FACTOR,
            }
    }
}

/// The quantities of a sized box the penalties are driven by, complete wing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxQuantities {
    /// Skin (cover) mass, kg.
    pub skin_kg: f64,
    /// Spar-cap (cover flange) mass, kg.
    pub caps_kg: f64,
    /// Spar-web mass, kg.
    pub webs_kg: f64,
    /// Mass of the extra lower-flange area added by the fatigue limit, kg.
    pub lower_extra_kg: f64,
    /// Plan-projected area of ONE cover between the outermost spars, m^2.
    pub cover_plan_area_m2: f64,
    /// Projected area of all spar webs, `integral h dy`, m^2.
    pub web_area_m2: f64,
    /// Length of the elastic axis, root to tip, both wings, m (Torenbeek's
    /// structural span `b_st`).
    pub structural_span_m: f64,
}

impl BoxQuantities {
    /// Read the quantities off a sized box. `cap_density_kg_m3` converts the
    /// extra lower-flange area to mass.
    pub fn from_sizing(
        wsg: &WingStructureGeometry,
        sizing: &WingboxSizing,
        cap_density_kg_m3: f64,
    ) -> Self {
        let y = &sizing.y_stations;
        let n = y.len();
        let (front, rear) = crate::sizing::box_chord_band(wsg);
        let band = (rear - front).max(0.0);
        let integrate = |f: &dyn Fn(usize) -> f64| -> f64 {
            (0..n.saturating_sub(1))
                .map(|j| 0.5 * (f(j) + f(j + 1)) * (y[j + 1] - y[j]))
                .sum()
        };
        let cover_plan = integrate(&|j| band * sizing.chord[j]);
        let webs = integrate(&|j| sizing.spars.iter().map(|s| s.h[j]).sum::<f64>());
        let axis = 0.5 * (front + rear);
        let axis_x: Vec<f64> = sizing
            .eta_stations
            .iter()
            .zip(&sizing.chord)
            .map(|(&eta, &c)| wsg.x_le(eta) + axis * c)
            .collect();
        let axis_length: f64 = (0..n.saturating_sub(1))
            .map(|j| (y[j + 1] - y[j]).hypot(axis_x[j + 1] - axis_x[j]))
            .sum();
        let lower_extra_semi: f64 = sizing
            .spars
            .iter()
            .map(|s| integrate_with(y, |j| s.lower_extra_area[j]) * cap_density_kg_m3)
            .sum();
        let mb = sizing.mass_breakdown_kg;
        Self {
            skin_kg: 2.0 * mb.skin,
            caps_kg: 2.0 * mb.spar_caps,
            webs_kg: 2.0 * mb.spar_webs,
            lower_extra_kg: 2.0 * lower_extra_semi,
            cover_plan_area_m2: 2.0 * cover_plan,
            web_area_m2: 2.0 * webs,
            structural_span_m: 2.0 * axis_length,
        }
    }

    /// Mass of the lower cover, kg: half of the skin and of the (symmetric)
    /// spar-cap material plus the lower-only fatigue area. The covers are the
    /// bending material of this box, so skin plus caps is Torenbeek's "ideal
    /// lower panel".
    pub fn lower_cover_kg(&self) -> f64 {
        0.5 * (self.skin_kg + self.caps_kg) + self.lower_extra_kg
    }
}

fn integrate_with(y: &[f64], f: impl Fn(usize) -> f64) -> f64 {
    (0..y.len().saturating_sub(1))
        .map(|j| 0.5 * (f(j) + f(j + 1)) * (y[j + 1] - y[j]))
        .sum()
}

/// Everything the penalties are evaluated from. SI; complete wing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NonOptimumInputs {
    /// The sized box.
    pub box_quantities: BoxQuantities,
    /// Density of the box covers, kg/m^3.
    pub cover_density_kg_m3: f64,
    /// Cover manufacturing class (scales Eq. 11.55).
    pub construction: Construction,
    /// Whether the box is a composite (Eq. 11.62 is Al-alloy only).
    pub composite: bool,
    /// Gross wing planform area `S`, m^2.
    pub wing_area_m2: f64,
    /// Wing span `b`, m.
    pub span_m: f64,
    /// Ultimate load factor `n_ult`.
    pub ultimate_load_factor: f64,
    /// Design gross mass `m_MTO`, kg.
    pub design_gross_mass_kg: f64,
    /// Landing mass `m_ML`, kg.
    pub landing_mass_kg: f64,
    /// Mass of the wing-mounted powerplant (engines, nacelles, reversers,
    /// pylons), all engines, kg.
    pub wing_mounted_powerplant_kg: f64,
    /// Number of wing-mounted engines, all wings.
    pub wing_mounted_engines: usize,
    /// Share of the main-gear load carried by legs attached to the wing, 0-1:
    /// 0 for fuselage-mounted gear, 1 when every leg is on the wing.
    pub wing_gear_load_fraction: f64,
    /// Dynamic pressure at the design dive speed, Pa.
    pub dive_dynamic_pressure_pa: f64,
    /// Mach number at the design dive speed.
    pub dive_mach: f64,
    /// Leading-edge sweep, rad.
    pub le_sweep_rad: f64,
    /// Mid-chord sweep, rad.
    pub mid_chord_sweep_rad: f64,
    /// Thickness ratio of the chord just inside the aileron.
    pub aileron_root_thickness_ratio: f64,
    /// Mass of the secondary wing structure the miscellaneous items scale
    /// with (movable surfaces and high-lift devices), kg.
    pub secondary_mass_kg: f64,
}

/// One evaluated penalty.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NonOptimumItem {
    /// Stable item name.
    pub name: &'static str,
    /// Complete-wing mass, kg. Zero when the item does not apply.
    pub mass_kg: f64,
    /// Source with equation and page.
    pub source: &'static str,
    /// What it covers and when it applies.
    pub applicability: &'static str,
    /// Declared relative uncertainty (1 = 100 %): a judgement from the
    /// scatter the source quotes, not a measured value.
    pub relative_uncertainty: f64,
}

fn positive(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

/// Evaluate every penalty. A non-finite or non-physical driver zeroes its own
/// item (never produces NaN); the order matches [`ITEM_COUNT`] and the module
/// table.
pub fn evaluate(inputs: &NonOptimumInputs) -> [NonOptimumItem; ITEM_COUNT] {
    let q = &inputs.box_quantities;
    let rho = positive(inputs.cover_density_kg_m3);

    // Eq. 11.55: smeared thickness on the covers (two) and the webs.
    let joints = inputs.construction.delta_nid_m()
        * rho
        * positive(2.0 * q.cover_plan_area_m2 + q.web_area_m2);

    // Eq. 11.56: 15 % on the web (shear) material.
    let fail_safe = FAIL_SAFE_WEB_FRACTION * positive(q.webs_kg);

    // Eq. 11.57: cut-out width over the mean box width, on the lower cover.
    // The box width is the plan area of one cover over the structural span,
    // which is Torenbeek's 50 % of the mean chord when the box is half the
    // chord, so `R_ic - 1 = 2 w b_st / S` is recovered exactly in that case.
    let cutout_ratio = if q.cover_plan_area_m2 > 0.0 {
        INSPECTION_CUTOUT_WIDTH_M * positive(q.structural_span_m) / q.cover_plan_area_m2
    } else {
        0.0
    };
    let cutouts = cutout_ratio * positive(q.lower_cover_kg());

    // Eq. 11.58.
    let pylons = PYLON_COEFFICIENT
        * (1.0 + PYLON_ENGINE_COUNT_COEFFICIENT * inputs.wing_mounted_engines as f64)
        * positive(inputs.wing_mounted_powerplant_kg);

    // Eq. 11.59 is the provision for the landing load absorbed by wing-mounted
    // landing gears and is omitted when the legs attach to the fuselage
    // (11.60 text): it is charged on the landing mass carried by wing legs.
    let gear = GEAR_SUPPORT_FRACTION
        * positive(inputs.landing_mass_kg)
        * if inputs.wing_gear_load_fraction.is_finite() {
            inputs.wing_gear_load_fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };

    // Eq. 11.61.
    let fuselage = FUSELAGE_ATTACHMENT_COEFFICIENT
        * positive(inputs.ultimate_load_factor)
        * positive(inputs.design_gross_mass_kg);

    // Eq. 11.62, Al-alloy box only.
    let stiffness = if inputs.composite {
        0.0
    } else {
        let mach_n =
            (inputs.dive_mach * inputs.mid_chord_sweep_rad.cos()).clamp(0.0, MAX_NORMAL_MACH);
        let span_term =
            positive(inputs.span_m) * inputs.le_sweep_rad.cos() / STIFFNESS_REFERENCE_SPAN_M;
        STIFFNESS_REFERENCE_FORCE_N / STANDARD_GRAVITY_M_S2
            * positive(inputs.dive_dynamic_pressure_pa)
            / STIFFNESS_REFERENCE_DYNAMIC_PRESSURE_PA
            * span_term.powi(3)
            / positive(inputs.aileron_root_thickness_ratio)
                .max(1.0e-3)
                .powi(2)
            * (1.0 - inputs.mid_chord_sweep_rad.sin()).max(0.0)
            / (1.0 - mach_n * mach_n).sqrt()
    };

    // 11.6.7.
    let miscellaneous = MISCELLANEOUS_FRACTION * positive(inputs.secondary_mass_kg);

    [
        NonOptimumItem {
            name: "joints_splices_fasteners",
            mass_kg: joints,
            source: "Torenbeek, Advanced Aircraft Design, 2013, Eq. 11.55 (book p. 340): delta_nid rho Sum S_box, delta_nid = 1 mm built-up, -20 % bonded, -50 % integrally machined",
            applicability: "production joints, splices, non-taper steps and the fastener increment of the box covers and webs, smeared over the box surface (cover plan area x2 plus web projected area, from the sized box); covers 5-10 % of box weight per the book; composite boxes use the bonded value (declared assumption)",
            relative_uncertainty: 0.40,
        },
        NonOptimumItem {
            name: "web_fail_safe_damage_tolerance",
            mass_kg: fail_safe,
            source: "Torenbeek 2013, Eq. 11.56 (book p. 340): 15 % of the basic shear material",
            applicability: "web stiffeners, rib posts, crack stoppers and the auxiliary-spar provision for fail safety; applied to the sized web mass, which already carries the torsional shear and the clear-panel buckling gauge, so the book's torsion share of this term is partly counted twice (direction: heavier)",
            relative_uncertainty: 0.40,
        },
        NonOptimumItem {
            name: "access_cutouts_lower_cover",
            mass_kg: cutouts,
            source: "Torenbeek 2013, Eq. 11.57 (book p. 340-341): R_ic = 1 + 2 w_ic b_st / S, w_ic = 0.25 m",
            applicability: "reinforcement of the lower cover around inspection and fuel-tank manholes; the cut-out to box-width ratio is taken from the sized box width instead of the book's 50 %-chord assumption; the book also quotes 1.5 times the cut-out material, which would be larger",
            relative_uncertainty: 0.40,
        },
        NonOptimumItem {
            name: "engine_pylon_attachment",
            mass_kg: pylons,
            source: "Torenbeek 2013, Eq. 11.58 (book p. 341): 0.015 (1 + 0.2 N_eng) W_pp, 'based on scanty statistical data for jet aircraft'",
            applicability: "pylon attachments, additional ribs and reinforcement of spars and covers at the engine stations; W_pp is the wing-mounted installed powerplant mass of the configuration (a turboprop's propeller and nacelle are not in it)",
            relative_uncertainty: 0.60,
        },
        NonOptimumItem {
            name: "main_gear_support",
            mass_kg: gear,
            source: "Torenbeek 2013, Eq. 11.59 (book p. 341): 0.006 W_ML for the load absorbed by wing-mounted gear; omitted if the legs attach to the fuselage",
            applicability: "gear bulkheads, auxiliary spar, aft-spar and rib reinforcement, wheel-well doors; charged on the wing-leg share of the landing mass (equal leg loads), zero for fuselage-mounted gear; the n_land refinement (11.60) needs a shock-absorber load factor the configuration does not carry",
            relative_uncertainty: 0.60,
        },
        NonOptimumItem {
            name: "wing_fuselage_attachment",
            mass_kg: fuselage,
            source: "Torenbeek 2013, Eq. 11.61 (book p. 341): 0.0003 n_ult W_MTO, continuous carry-through",
            applicability: "attachment structure, rib reinforcement and hardware at the side of body; assumes a continuous carry-through box",
            relative_uncertainty: 0.60,
        },
        NonOptimumItem {
            name: "torsional_stiffness",
            mass_kg: stiffness,
            source: "Torenbeek 2013, Eq. 11.62 (book p. 344): W_ref (q_D/q_ref)(b cos L0/b_ref)^3 (t/c)_ref^-2 (1 - sin L0.5){1 - (M_D cos L0.5)^2}^-0.5, W_ref = 200 N, q_ref = 30 kN/m2, b_ref = 50 m",
            applicability: "aeroelastic (flutter, aileron reversal) beef-up of the outboard box of an Al-alloy wing, about 2 % of the wing for regional jets and 5 % for wide-bodies; W_ref is the book's default for a missing calibration; zero for a composite box; the normal Mach number is capped at 0.95",
            relative_uncertainty: 0.60,
        },
        NonOptimumItem {
            name: "miscellaneous_sealant_fasteners_finish",
            mass_kg: miscellaneous,
            source: "Torenbeek 2013, section 11.6.7 (book p. 349): paint, fuel-tank sealant, rivets, nuts and bolts, jacking fittings; 10 % of the secondary wing weight when data are missing",
            applicability: "scaled by the movable high-lift, aileron and spoiler items of the non-box inventory only; the fixed non-box item already carries the FLOPS miscellaneous term for its own planform, so it is not scaled again",
            relative_uncertainty: 0.50,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> NonOptimumInputs {
        NonOptimumInputs {
            box_quantities: BoxQuantities {
                skin_kg: 2000.0,
                caps_kg: 1500.0,
                webs_kg: 800.0,
                lower_extra_kg: 100.0,
                cover_plan_area_m2: 61.0,
                web_area_m2: 25.0,
                structural_span_m: 37.5,
            },
            cover_density_kg_m3: 2800.0,
            construction: Construction::BuiltUp,
            composite: false,
            wing_area_m2: 122.0,
            span_m: 34.1,
            ultimate_load_factor: 3.75,
            design_gross_mass_kg: 78_000.0,
            landing_mass_kg: 66_000.0,
            wing_mounted_powerplant_kg: 5_200.0,
            wing_mounted_engines: 2,
            wing_gear_load_fraction: 1.0,
            dive_dynamic_pressure_pa: 0.5 * 1.225 * 200.0 * 200.0,
            dive_mach: 0.89,
            le_sweep_rad: 28.0_f64.to_radians(),
            mid_chord_sweep_rad: 25.0_f64.to_radians(),
            aileron_root_thickness_ratio: 0.11,
            secondary_mass_kg: 1200.0,
        }
    }

    fn mass(items: &[NonOptimumItem], name: &str) -> f64 {
        items.iter().find(|i| i.name == name).unwrap().mass_kg
    }

    #[test]
    fn each_item_follows_its_own_equation() {
        let items = evaluate(&inputs());
        // 11.55: 1 mm x 2800 kg/m3 x (2 x 61 + 25) m2.
        assert!((mass(&items, "joints_splices_fasteners") - 1.0e-3 * 2800.0 * 147.0).abs() < 1e-9);
        // 11.56: 15 % of the webs.
        assert!((mass(&items, "web_fail_safe_damage_tolerance") - 120.0).abs() < 1e-9);
        // 11.57: 0.25 x 37.5 / 61 on half of skin + caps + the extra area.
        let ratio = 0.25 * 37.5 / 61.0;
        assert!(
            (mass(&items, "access_cutouts_lower_cover") - ratio * (1750.0 + 100.0)).abs() < 1e-9
        );
        // 11.58, 11.59, 11.61, 11.6.7.
        assert!((mass(&items, "engine_pylon_attachment") - 0.015 * 1.4 * 5200.0).abs() < 1e-9);
        assert!((mass(&items, "main_gear_support") - 0.006 * 66_000.0).abs() < 1e-9);
        assert!((mass(&items, "wing_fuselage_attachment") - 0.0003 * 3.75 * 78_000.0).abs() < 1e-9);
        assert!((mass(&items, "miscellaneous_sealant_fasteners_finish") - 120.0).abs() < 1e-9);
    }

    #[test]
    fn the_gear_item_follows_the_wing_leg_share() {
        let mut i = inputs();
        i.wing_gear_load_fraction = 0.5;
        let half = mass(&evaluate(&i), "main_gear_support");
        assert!((half - 0.5 * 0.006 * 66_000.0).abs() < 1e-9);
    }

    #[test]
    fn the_cutout_penalty_is_torenbeeks_two_w_b_over_s_for_a_half_chord_box() {
        // Box = half the mean chord: plan area of one cover = S/2 (here per
        // wing, both wings), b_st = b.
        let mut i = inputs();
        i.box_quantities.cover_plan_area_m2 = 0.5 * i.wing_area_m2;
        i.box_quantities.structural_span_m = i.span_m;
        let items = evaluate(&i);
        let r_ic = 2.0 * 0.25 * i.span_m / i.wing_area_m2;
        let lower = i.box_quantities.lower_cover_kg();
        assert!((mass(&items, "access_cutouts_lower_cover") - r_ic * lower).abs() < 1e-9);
        // 5 % (large) to 15 % (small airliner) of Torenbeek's own range.
        assert!((0.05..=0.16).contains(&r_ic), "{r_ic}");
    }

    #[test]
    fn construction_scales_only_the_fastener_increment() {
        let built = evaluate(&inputs());
        let mut i = inputs();
        i.construction = Construction::Bonded;
        let bonded = evaluate(&i);
        i.construction = Construction::IntegrallyMachined;
        let machined = evaluate(&i);
        let j = |items: &[NonOptimumItem]| mass(items, "joints_splices_fasteners");
        assert!((j(&bonded) - 0.8 * j(&built)).abs() < 1e-9);
        assert!((j(&machined) - 0.5 * j(&built)).abs() < 1e-9);
        assert_eq!(
            mass(&bonded, "access_cutouts_lower_cover"),
            mass(&built, "access_cutouts_lower_cover")
        );
    }

    #[test]
    fn gear_on_the_fuselage_and_a_composite_box_drop_their_own_items_only() {
        let mut i = inputs();
        i.wing_gear_load_fraction = 0.0;
        i.composite = true;
        let items = evaluate(&i);
        assert_eq!(mass(&items, "main_gear_support"), 0.0);
        assert_eq!(mass(&items, "torsional_stiffness"), 0.0);
        assert!(mass(&items, "engine_pylon_attachment") > 0.0);
    }

    #[test]
    fn the_stiffness_penalty_is_a_few_percent_of_a_narrowbody_wing() {
        // Book p. 344: 2 % (regional jet) to 5 % (wide-body) of the wing with
        // the default W_ref. A320-like drivers land inside that range.
        let items = evaluate(&inputs());
        let stiffness = mass(&items, "torsional_stiffness");
        let wing = 4500.0 + 1200.0 + 1400.0;
        assert!(
            (0.015..0.06).contains(&(stiffness / wing)),
            "{}",
            stiffness / wing
        );
    }

    #[test]
    fn non_finite_drivers_zero_their_item_and_never_produce_nan() {
        let mut i = inputs();
        i.wing_mounted_powerplant_kg = f64::NAN;
        i.landing_mass_kg = f64::INFINITY;
        i.dive_dynamic_pressure_pa = f64::NAN;
        let items = evaluate(&i);
        assert!(items
            .iter()
            .all(|item| item.mass_kg.is_finite() && item.mass_kg >= 0.0));
        assert_eq!(mass(&items, "engine_pylon_attachment"), 0.0);
        assert_eq!(mass(&items, "torsional_stiffness"), 0.0);
    }
}
