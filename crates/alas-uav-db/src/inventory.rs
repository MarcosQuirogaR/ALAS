// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The field inventory: the single table of names, kinds, SI units, valid
//! ranges and the physics models each member is mandatory for.
//!
//! Validators, the Markdown schema document (`docs/uav-db-schema.md`) and the
//! importers' column mapping all read this table, so a unit or range exists in
//! exactly one place. Ranges are generous physical sanity bands for the
//! 0.5-25 kg fixed-wing scope (a value outside them is almost certainly a unit
//! mistake), not product limits.

use crate::component::Category;

/// How a member is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// `Field<f64>` in SI.
    Num,
    /// `Field<u32>`.
    Count,
    /// `Field<String>`.
    Text,
    /// `Field<Vec<String>>`.
    TextList,
    /// `Field<bool>`.
    Bool,
    /// `Field<[f64; 3]>`, length x width x height in metres.
    Dims3,
    /// One [`crate::Curve`].
    Curve,
    /// A list of curves (one per parameter value).
    CurveList,
    /// A [`crate::PropGeometry`] table.
    Geometry,
}

/// One inventory row.
#[derive(Debug, Clone, Copy)]
pub struct FieldInfo {
    /// Member name in the serialized row (carries the SI unit suffix).
    pub name: &'static str,
    /// Column stem for CSV import (`mass` for `mass_kg`).
    pub stem: &'static str,
    /// Storage kind.
    pub kind: FieldKind,
    /// SI unit of the value (ordinate for curves).
    pub unit_si: &'static str,
    /// SI unit of the curve abscissa; empty otherwise.
    pub x_unit_si: &'static str,
    /// Smallest plausible value (inclusive).
    pub min: f64,
    /// Largest plausible value (inclusive).
    pub max: f64,
    /// Physics models that cannot run without this member.
    pub required_for: &'static [&'static str],
    /// One-line description.
    pub doc: &'static str,
}

const fn num(
    name: &'static str,
    stem: &'static str,
    unit_si: &'static str,
    min: f64,
    max: f64,
    required_for: &'static [&'static str],
    doc: &'static str,
) -> FieldInfo {
    FieldInfo {
        name,
        stem,
        kind: FieldKind::Num,
        unit_si,
        x_unit_si: "",
        min,
        max,
        required_for,
        doc,
    }
}

const fn count(
    name: &'static str,
    min: f64,
    max: f64,
    required_for: &'static [&'static str],
    doc: &'static str,
) -> FieldInfo {
    FieldInfo {
        name,
        stem: name,
        kind: FieldKind::Count,
        unit_si: "1",
        x_unit_si: "",
        min,
        max,
        required_for,
        doc,
    }
}

const fn other(
    name: &'static str,
    kind: FieldKind,
    unit_si: &'static str,
    x_unit_si: &'static str,
    required_for: &'static [&'static str],
    doc: &'static str,
) -> FieldInfo {
    let (min, max) = match kind {
        FieldKind::Dims3 => (1e-4, 5.0),
        _ => (f64::NEG_INFINITY, f64::INFINITY),
    };
    FieldInfo {
        name,
        stem: name,
        kind,
        unit_si,
        x_unit_si,
        min,
        max,
        required_for,
        doc,
    }
}

const MOTOR_CIRCUIT: &[&str] = &["motor_circuit"];
const MASS: &[&str] = &["mass_budget"];
const PACK: &[&str] = &["packaging"];
const NONE: &[&str] = &[];

#[rustfmt::skip]
const MOTOR: &[FieldInfo] = &[
    num("kv_rad_s_per_v", "kv", "rad/s/V", 0.5, 3000.0, MOTOR_CIRCUIT, "Speed constant (vendor KV x 2 pi / 60)"),
    num("rm_ohm", "rm", "Ohm", 1e-5, 10.0, MOTOR_CIRCUIT, "Phase-to-phase winding resistance"),
    num("i0_a", "i0", "A", 0.01, 100.0, MOTOR_CIRCUIT, "No-load current"),
    num("i0_test_v", "i0_test", "V", 1.0, 200.0, MOTOR_CIRCUIT, "Voltage at which i0 was stated"),
    num("i_max_cont_a", "i_max_cont", "A", 0.1, 500.0, MOTOR_CIRCUIT, "Maximum continuous current"),
    num("i_max_burst_a", "i_max_burst", "A", 0.1, 1000.0, NONE, "Maximum burst current"),
    num("burst_s", "burst", "s", 0.1, 600.0, NONE, "Burst duration"),
    num("p_max_w", "p_max", "W", 1.0, 50000.0, NONE, "Maximum power"),
    count("n_poles", 2.0, 100.0, NONE, "Magnetic pole count"),
    num("mass_kg", "mass", "kg", 0.001, 50.0, MASS, "Mass"),
    num("diameter_m", "diameter", "m", 0.005, 0.5, PACK, "Can diameter"),
    num("length_m", "length", "m", 0.005, 0.5, PACK, "Length without shaft"),
    num("shaft_d_m", "shaft_d", "m", 0.001, 0.05, NONE, "Shaft diameter"),
    other("mount_pattern", FieldKind::Text, "1", "", NONE, "Mounting pattern text"),
    num("omega_max_rad_s", "omega_max", "rad/s", 10.0, 20000.0, NONE, "Maximum shaft speed"),
    num("thermal_r_k_per_w", "thermal_r", "K/W", 0.01, 100.0, NONE, "Winding-to-ambient thermal resistance"),
    num("winding_temp_max_k", "winding_temp_max", "K", 330.0, 520.0, NONE, "Maximum winding temperature"),
    count("cells_min", 1.0, 60.0, NONE, "Minimum LiPo series count"),
    count("cells_max", 1.0, 60.0, NONE, "Maximum LiPo series count"),
    num("thrust_static_max_n", "thrust_static_max", "N", 0.1, 2000.0, NONE, "Vendor-claimed maximum static thrust"),
    num("rec_prop_diameter_m", "rec_prop_diameter", "m", 0.05, 1.5, NONE, "Vendor-recommended propeller diameter"),
    num("rec_prop_pitch_m", "rec_prop_pitch", "m", 0.02, 1.5, NONE, "Vendor-recommended propeller pitch"),
    other("i0_vs_omega", FieldKind::Curve, "A", "rad/s", NONE, "No-load current versus shaft speed"),
];

#[rustfmt::skip]
const ESC: &[FieldInfo] = &[
    num("i_cont_a", "i_cont", "A", 0.5, 500.0, &["esc_limits"], "Continuous output current"),
    num("i_burst_a", "i_burst", "A", 0.5, 1000.0, NONE, "Burst output current"),
    num("v_min_v", "v_min", "V", 1.0, 200.0, NONE, "Minimum input voltage"),
    num("v_max_v", "v_max", "V", 1.0, 200.0, &["esc_limits"], "Maximum input voltage"),
    count("cells_min", 1.0, 60.0, NONE, "Minimum LiPo series count"),
    count("cells_max", 1.0, 60.0, NONE, "Maximum LiPo series count"),
    num("bec_v_min_v", "bec_v_min", "V", 1.0, 30.0, NONE, "Minimum adjustable BEC voltage"),
    num("bec_v_max_v", "bec_v_max", "V", 1.0, 30.0, NONE, "Maximum adjustable BEC voltage"),
    num("bec_a_cont_a", "bec_a_cont", "A", 0.01, 50.0, NONE, "Continuous BEC current"),
    num("bec_a_peak_a", "bec_a_peak", "A", 0.01, 50.0, NONE, "Peak BEC current"),
    other("protocols", FieldKind::TextList, "1", "", NONE, "Supported signal protocols"),
    num("mass_kg", "mass", "kg", 0.0005, 5.0, MASS, "Mass"),
    other("dims_m", FieldKind::Dims3, "m", "", PACK, "Bounding box length x width x height"),
    other("efficiency_vs_current", FieldKind::Curve, "1", "A", NONE, "Efficiency versus output current"),
];

#[rustfmt::skip]
const BATTERY: &[FieldInfo] = &[
    other("chemistry", FieldKind::Text, "1", "", &["battery_sag"], "Chemistry text"),
    count("cells_s", 1.0, 60.0, &["battery_sag"], "Series cell count"),
    count("parallel_p", 1.0, 20.0, NONE, "Parallel string count"),
    num("capacity_c", "capacity", "C", 36.0, 1e6, &["battery_sag"], "Rated capacity (1 Ah = 3600 C)"),
    num("v_nom_v", "v_nom", "V", 1.0, 200.0, &["battery_sag"], "Nominal pack voltage"),
    num("v_max_v", "v_max", "V", 1.0, 200.0, NONE, "Fully charged voltage"),
    num("v_min_v", "v_min", "V", 1.0, 200.0, NONE, "Cut-off voltage"),
    num("energy_j", "energy", "J", 100.0, 1e8, NONE, "Stored energy (usually derived)"),
    num("r_dc_ohm", "r_dc", "Ohm", 1e-4, 5.0, &["battery_sag"], "Pack DC internal resistance"),
    num("i_cont_a", "i_cont", "A", 0.1, 2000.0, NONE, "Continuous discharge current"),
    num("i_burst_a", "i_burst", "A", 0.1, 4000.0, NONE, "Burst discharge current"),
    num("claimed_c_rating", "claimed_c_rating", "1/h", 0.5, 500.0, NONE, "Vendor C rating as printed (a claim)"),
    num("mass_kg", "mass", "kg", 0.001, 50.0, &["battery_sag", "mass_budget"], "Pack mass"),
    other("dims_m", FieldKind::Dims3, "m", "", PACK, "Bounding box length x width x height"),
    other("connector", FieldKind::Text, "1", "", NONE, "Main connector text"),
    other("voc_vs_soc", FieldKind::Curve, "V", "1", NONE, "Open-circuit voltage versus state of charge"),
    other("r_dc_vs_soc_t", FieldKind::CurveList, "Ohm", "1", NONE, "Resistance versus state of charge, one curve per temperature"),
];

#[rustfmt::skip]
const SERVO: &[FieldInfo] = &[
    other("torque_vs_voltage", FieldKind::Curve, "N*m", "V", &["servo_sizing"], "Stall torque versus supply voltage"),
    other("speed_vs_voltage", FieldKind::Curve, "rad/s", "V", NONE, "No-load speed versus supply voltage"),
    other("i_stall_vs_voltage", FieldKind::Curve, "A", "V", NONE, "Stall current versus supply voltage"),
    num("v_min_v", "v_min", "V", 1.0, 50.0, &["servo_sizing"], "Minimum supply voltage"),
    num("v_max_v", "v_max", "V", 1.0, 50.0, &["servo_sizing"], "Maximum supply voltage"),
    num("mass_kg", "mass", "kg", 0.0005, 2.0, MASS, "Mass"),
    other("dims_m", FieldKind::Dims3, "m", "", PACK, "Bounding box length x width x height"),
    other("gear", FieldKind::Text, "1", "", NONE, "Gear type"),
];

#[rustfmt::skip]
const AVIONICS: &[FieldInfo] = &[
    other("role", FieldKind::Text, "1", "", NONE, "Functional role text"),
    num("mass_kg", "mass", "kg", 0.0001, 5.0, MASS, "Mass"),
    num("v_in_min_v", "v_in_min", "V", 1.0, 200.0, &["avionics_budget"], "Minimum supply voltage"),
    num("v_in_max_v", "v_in_max", "V", 1.0, 200.0, &["avionics_budget"], "Maximum supply voltage"),
    num("i_draw_a", "i_draw", "A", 1e-4, 20.0, &["avionics_budget"], "Supply current draw"),
    num("i_max_a", "i_max", "A", 1e-3, 200.0, NONE, "Rated maximum current of an output or limiter (not a draw)"),
    count("channels", 1.0, 64.0, NONE, "Output channel count"),
    other("protocols", FieldKind::TextList, "1", "", NONE, "Supported protocols"),
    other("telemetry", FieldKind::Bool, "1", "", NONE, "Provides telemetry"),
    other("dims_m", FieldKind::Dims3, "m", "", PACK, "Bounding box length x width x height"),
];

#[rustfmt::skip]
const PROPELLER: &[FieldInfo] = &[
    num("diameter_m", "diameter", "m", 0.02, 3.0, &["prop_map", "bemt"], "Diameter"),
    num("pitch_m", "pitch", "m", 0.005, 3.0, &["prop_map"], "Geometric pitch"),
    count("blades", 1.0, 12.0, &["prop_map", "bemt"], "Blade count"),
    other("family", FieldKind::Text, "1", "", NONE, "Product family text"),
    num("mass_kg", "mass", "kg", 0.0001, 5.0, MASS, "Mass"),
    num("hub_bore_m", "hub_bore", "m", 0.001, 0.05, NONE, "Hub bore"),
    other("folding", FieldKind::Bool, "1", "", NONE, "Blades fold"),
    num("inertia_kg_m2", "inertia", "kg*m2", 1e-9, 1.0, NONE, "Polar moment of inertia"),
    other("electric_compatible", FieldKind::Bool, "1", "", NONE, "Vendor permits electric propulsion"),
    other("geometry", FieldKind::Geometry, "m", "m", &["bemt"], "Blade geometry table (r, chord, twist, airfoil)"),
];

#[rustfmt::skip]
const WHEEL: &[FieldInfo] = &[
    num("diameter_m", "diameter", "m", 0.01, 0.6, PACK, "Outer diameter"),
    num("width_m", "width", "m", 0.002, 0.2, NONE, "Tyre width"),
    num("mass_kg", "mass", "kg", 0.0005, 5.0, MASS, "Mass"),
    num("crr_grass", "crr_grass", "1", 0.0, 0.5, NONE, "Rolling-resistance coefficient on short grass"),
];

#[rustfmt::skip]
const GEAR: &[FieldInfo] = &[
    other("form", FieldKind::Text, "1", "", NONE, "Form text"),
    num("mass_kg", "mass", "kg", 0.0005, 20.0, MASS, "Assembly mass"),
    num("max_aircraft_mass_kg", "max_aircraft_mass", "kg", 0.05, 200.0, NONE, "Maximum supported aircraft mass"),
    num("nominal_voltage_v", "nominal_voltage", "V", 1.0, 60.0, NONE, "Nominal mechanism voltage"),
    other("dims_m", FieldKind::Dims3, "m", "", PACK, "Bounding box length x width x height"),
];

#[rustfmt::skip]
const CONNECTOR: &[FieldInfo] = &[
    num("i_cont_a", "i_cont", "A", 1.0, 1000.0, NONE, "Continuous current rating"),
    num("r_contact_ohm", "r_contact", "Ohm", 1e-6, 1e-2, NONE, "Contact-pair resistance"),
    num("mass_kg", "mass", "kg", 1e-5, 1.0, MASS, "Mass of a mated pair"),
];

#[rustfmt::skip]
const MATERIAL: &[FieldInfo] = &[
    other("form", FieldKind::Text, "1", "", NONE, "Form text"),
    num("density_kg_m3", "density", "kg/m3", 10.0, 25000.0, &["stock_mass"], "Density"),
    num("e_pa", "e", "Pa", 1e6, 1e12, NONE, "Young's modulus"),
    num("g_pa", "g", "Pa", 1e5, 1e12, NONE, "Shear modulus"),
    num("strength_pa", "strength", "Pa", 1e5, 1e11, NONE, "Failure or allowable strength"),
    num("stock_mass_kg", "stock_mass", "kg", 1e-4, 1000.0, NONE, "Purchased stock mass"),
    other("stock_dims_m", FieldKind::Dims3, "m", "", NONE, "Purchased stock bounding box"),
];

const CLT: &[&str] = &["clt"];
#[rustfmt::skip]
const LAMINATE: &[FieldInfo] = &[
    num("ply_thickness_m", "ply_thickness", "m", 1e-5, 5e-3, CLT, "Cured ply thickness"),
    num("areal_mass_kg_m2", "areal_mass", "kg/m2", 0.01, 5.0, CLT, "Ply areal mass"),
    num("ply_e1_pa", "ply_e1", "Pa", 1e9, 1e12, CLT, "Fibre-direction modulus E1"),
    num("ply_e2_pa", "ply_e2", "Pa", 1e8, 1e11, CLT, "Transverse modulus E2"),
    num("ply_g12_pa", "ply_g12", "Pa", 1e8, 1e11, CLT, "In-plane shear modulus G12"),
    num("ply_nu12", "ply_nu12", "1", 0.01, 0.6, CLT, "Major Poisson ratio"),
    num("ply_xt_pa", "ply_xt", "Pa", 1e6, 1e10, NONE, "Longitudinal tensile strength"),
    num("ply_xc_pa", "ply_xc", "Pa", 1e6, 1e10, NONE, "Longitudinal compressive strength"),
    num("ply_yt_pa", "ply_yt", "Pa", 1e6, 1e9, NONE, "Transverse tensile strength"),
    num("ply_yc_pa", "ply_yc", "Pa", 1e6, 1e9, NONE, "Transverse compressive strength"),
    num("ply_s12_pa", "ply_s12", "Pa", 1e6, 1e9, NONE, "In-plane shear strength"),
    num("knockdown", "knockdown", "1", 0.05, 1.0, NONE, "Design knock-down from the coupon programme"),
    other("coupon_source", FieldKind::Text, "1", "", NONE, "Coupon campaign identifier"),
];

#[rustfmt::skip]
const KIT: &[FieldInfo] = &[
    num("mass_kg", "mass", "kg", 0.05, 100.0, MASS, "Kit mass"),
    num("span_m", "span", "m", 0.2, 10.0, NONE, "Wing span"),
    num("area_m2", "area", "m2", 0.01, 10.0, NONE, "Wing area"),
    num("intended_auw_kg", "intended_auw", "kg", 0.05, 100.0, NONE, "Intended all-up weight"),
];

/// The inventory rows of one category (members that carry provenance).
pub fn fields(category: Category) -> &'static [FieldInfo] {
    match category {
        Category::Motor => MOTOR,
        Category::Propeller => PROPELLER,
        Category::Esc => ESC,
        Category::Battery => BATTERY,
        Category::Servo => SERVO,
        Category::Receiver | Category::FlightController | Category::Electronics => AVIONICS,
        Category::Wheel => WHEEL,
        Category::LandingGear => GEAR,
        Category::Material => MATERIAL,
        Category::Laminate => LAMINATE,
        Category::Kit => KIT,
        Category::Connector => CONNECTOR,
    }
}

/// Look up one member of a category.
pub fn field(category: Category, name: &str) -> Option<&'static FieldInfo> {
    fields(category).iter().find(|info| info.name == name)
}

/// Render the schema reference table of every category as Markdown.
///
/// The output is committed as `docs/uav-db-schema.md` (generated section); a
/// test keeps the committed text equal to this function's output.
pub fn render_markdown() -> String {
    let mut out = String::new();
    for category in Category::ALL {
        if matches!(category, Category::FlightController | Category::Electronics) {
            continue;
        }
        let title = match category {
            Category::Receiver => "receiver, flight_controller, electronics (shared avionics body)",
            c => c.key(),
        };
        out.push_str(&format!("#### `{title}`\n\n"));
        out.push_str(
            "| member | kind | SI unit | x unit | range | mandatory for | description |\n",
        );
        out.push_str("|---|---|---|---|---|---|---|\n");
        for info in fields(category) {
            let range = if info.min.is_finite() && info.max.is_finite() {
                format!("{} .. {}", info.min, info.max)
            } else {
                "-".to_owned()
            };
            let required = if info.required_for.is_empty() {
                "-".to_owned()
            } else {
                info.required_for.join(", ")
            };
            let x_unit = if info.x_unit_si.is_empty() {
                "-"
            } else {
                info.x_unit_si
            };
            out.push_str(&format!(
                "| `{}` | {:?} | {} | {} | {} | {} | {} |\n",
                info.name, info.kind, info.unit_si, x_unit, range, required, info.doc
            ));
        }
        out.push('\n');
    }
    out
}
