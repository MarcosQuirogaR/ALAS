// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The spanwise lift of the main wing as basic and additional loading, from two
//! vortex-lattice solves of the wing alone.
//!
//! Lift is linear in the angle of attack, so two solves at fixed angles give the strip sums `S_1(eta)`, `S_2(eta)` and the wing
//! lifts `C_1`, `C_2` (sums of the wing's panel force along +Z, geometry axes).
//! The additional loading per unit total lift is `a = (S_2 - S_1) / (C_2 - C_1)`
//! (unit integral). The basic loading is the strip lift at zero total lift,
//! `S_1 - C_1 a`, expressed per unit level-flight lift `q S CL_level` with
//! `CL_level = W / (q S)` at the cruise Mach and altitude (zero integral). A case
//! with semi-wing lift `F` then carries `F a + F_level b`. USAF DATCOM 4.1.4
//! and Abbott & von Doenhoff, *Theory of Wing Sections*, ch. 8, give the
//! decomposition: the additional loading scales with the case's lift coefficient,
//! the twist- and camber-induced basic loading does not. Only the main wing's
//! panels enter, so tail and body carry-over are excluded and the semi-span
//! integral is the wing lift by construction. Panels beyond the structural
//! semi-span are dropped. The shape is the incompressible lattice result at the
//! design point: no aeroelastic redistribution, no compressibility, no stall.

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm;
use alas_atmo::Atmosphere;
use alas_config::DesignRequirements;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::wing_structure::SpanLoadShape;

/// Label carried by a shape built here.
pub const VLM_LABEL: &str = "VLM basic + additional strip lift (two wing-only solves)";

/// Spanwise bins of the strip sum.
const BINS: usize = 24;

/// Angles of attack of the two solves, degrees, and their airspeed, m/s (the
/// strip lift is linear in alpha and the result is normalised, so neither
/// matters beyond conditioning).
const ALPHA_1_DEG: f64 = 2.0;
const ALPHA_2_DEG: f64 = 6.0;
const V_SOLVE: f64 = 60.0;

/// Strip sums and wing lift of the two solves: `(S_1, C_1, S_2, C_2)`.
type UnitStrips = ([f64; BINS], f64, [f64; BINS], f64);

fn strips(
    system: &vlm::VlmSystem<'_>,
    semi_span_m: f64,
    alpha_deg: f64,
) -> Option<([f64; BINS], f64)> {
    let op = OperatingPoint::new(Atmosphere::new(0.0), V_SOLVE, alpha_deg, 0.0, 0.0, 0.0, 0.0);
    let result = system.solve(&op).ok()?;
    let mut sums = [0.0_f64; BINS];
    for (panel, force) in result.panels.iter().zip(&result.panel_forces_geometry) {
        if panel.wing_index != 0 {
            continue;
        }
        let eta = panel.vortex_center[1].abs() / semi_span_m;
        if eta < 1.0 {
            sums[((eta * BINS as f64) as usize).min(BINS - 1)] += force[2];
        }
    }
    let total = sums.iter().sum();
    Some((sums, total))
}

/// Both solves on one lattice: the mesh and the factorization depend on the
/// geometry only, so they are built once and each angle is one back-solve.
fn solve_unit_strips(plane: &Airplane, semi_span_m: f64) -> Option<UnitStrips> {
    let system = vlm::VlmSystem::assemble(plane, 4, 4).ok()?;
    let (s1, c1) = strips(&system, semi_span_m, ALPHA_1_DEG)?;
    let (s2, c2) = strips(&system, semi_span_m, ALPHA_2_DEG)?;
    Some((s1, c1, s2, c2))
}

/// Lattices whose strip sums each thread keeps; see [`unit_strips`].
const HELD_STRIPS: usize = 4;

/// Whether the strip sums of `a` and `b` are the same computation: the same
/// lifting surfaces (each wing whole, its dynamic-pressure ratio included)
/// and reference quantities. The moment reference `xyz_ref` is left out:
/// at zero body rates it multiplies a zero angular velocity in the
/// rotation term and enters nothing else a panel force depends on. The
/// fuselages are left out because the lattice is meshed from the wings
/// alone.
fn same_strip_solve(a: &Airplane, b: &Airplane) -> bool {
    a.wings == b.wings && a.s_ref == b.s_ref && a.c_ref == b.c_ref && a.b_ref == b.b_ref
}

/// The strip sums of [`solve_unit_strips`], kept per thread for the latest
/// [`HELD_STRIPS`] lattices.
///
/// They depend on the airplane and the semi-span only; the mass of a sizing
/// pass enters later through the level-flight lift coefficient. A closure that
/// re-sizes the box of one geometry at several masses therefore asks for the
/// same strips repeatedly, and gets the held values when the lattice compares
/// equal ([`same_strip_solve`]) and the semi-span matches bit for bit. One
/// evaluation alternates between the airplane as built and the airplane with
/// its horizontal-tail dynamic-pressure ratio set, so a single held entry
/// would be evicted on every other call.
fn unit_strips(plane: &Airplane, semi_span_m: f64) -> Option<UnitStrips> {
    type Held = Vec<(Airplane, u64, Option<UnitStrips>)>;
    thread_local! {
        static HELD: std::cell::RefCell<Held> = const { std::cell::RefCell::new(Vec::new()) };
    }
    let span = semi_span_m.to_bits();
    if let Some(hit) = HELD.with(|held| {
        let mut held = held.borrow_mut();
        let index = held.iter().position(|(other, other_span, _)| {
            *other_span == span && same_strip_solve(other, plane)
        })?;
        let entry = held.remove(index);
        let value = entry.2;
        held.push(entry);
        Some(value)
    }) {
        return hit;
    }
    let value = solve_unit_strips(plane, semi_span_m);
    HELD.with(|held| {
        let mut held = held.borrow_mut();
        if held.len() == HELD_STRIPS {
            held.remove(0);
        }
        held.push((plane.clone(), span, value));
    });
    value
}

/// The design level-flight lift coefficient of the wing on the reference area:
/// `W / (q S)` at the requirement's cruise Mach and altitude, with `W` the
/// requirement's (design) mass. Search and delivery both build the shape from
/// this one number, so they size the same box.
pub fn design_level_cl(plane: &Airplane, req: &DesignRequirements) -> Option<f64> {
    let atmosphere = Atmosphere::new(req.cruise_altitude_m);
    let speed = req.cruise_mach * atmosphere.speed_of_sound();
    let q = 0.5 * atmosphere.density() * speed * speed;
    let cl = req.mtow_kg * req.gravity_m_s2 / (q * plane.s_ref);
    (cl.is_finite() && cl > 0.0).then_some(cl)
}

/// The main wing's (first wing's) basic and additional loading for a level-flight
/// lift coefficient `cl_level`, or `None` when a lattice solve fails or the lift
/// curve is degenerate (callers then keep the Schrenk default and say so).
pub fn vlm_span_load_shape(
    plane: &Airplane,
    semi_span_m: f64,
    cl_level: f64,
) -> Option<SpanLoadShape> {
    if !(semi_span_m.is_finite() && semi_span_m > 0.0 && cl_level.is_finite() && cl_level > 0.0) {
        return None;
    }
    let (s1, c1, s2, c2) = unit_strips(plane, semi_span_m)?;
    if !(c1 > 0.0 && c2 > c1) {
        return None;
    }
    let width = 1.0 / BINS as f64;
    let q_solve = 0.5 * Atmosphere::new(0.0).density() * V_SOLVE * V_SOLVE;
    let c_level = cl_level * q_solve * plane.s_ref;
    let eta: Vec<f64> = (0..BINS).map(|i| (i as f64 + 0.5) * width).collect();
    let unit: Vec<f64> = (0..BINS).map(|i| (s2[i] - s1[i]) / (c2 - c1)).collect();
    let additional: Vec<f64> = unit.iter().map(|u| u / width).collect();
    // Lift at zero total lift, per unit level-flight lift.
    let basic: Vec<f64> = (0..BINS)
        .map(|i| (s1[i] - c1 * unit[i]) / c_level / width)
        .collect();
    SpanLoadShape::new(eta, additional, basic, VLM_LABEL)
}

/// Give `geometry` the delivery path's spanwise lift shape: basic and additional
/// loading from two wing-only lattice solves at the design level-flight lift
/// coefficient; without a solve, Schrenk's approximation (NACA TM 948).
pub fn attach_design_span_load(
    geometry: &mut alas_geom::wing_structure::WingStructureGeometry,
    plane: &Airplane,
    requirements: &DesignRequirements,
) {
    let shape = design_level_cl(plane, requirements)
        .and_then(|cl| vlm_span_load_shape(plane, geometry.semi_span, cl));
    geometry.span_load = shape.or_else(|| SpanLoadShape::schrenk(|eta| geometry.local_chord(eta)));
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{presets, AlasConfig};
    use alas_geom::builder::AircraftBuilder;

    fn plane(name: &str) -> Airplane {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let design = presets::registry()
            .iter()
            .find(|preset| preset.name == name)
            .map(|preset| preset.design_vector)
            .unwrap();
        AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .unwrap()
    }

    /// One independent mesh, factorization and solve per angle.
    fn separate_solve(plane: &Airplane, semi_span_m: f64, alpha_deg: f64) -> ([f64; BINS], f64) {
        let op = OperatingPoint::new(Atmosphere::new(0.0), V_SOLVE, alpha_deg, 0.0, 0.0, 0.0, 0.0);
        let result = vlm::run(plane, &op, 4, 4).unwrap();
        let mut sums = [0.0_f64; BINS];
        for (panel, force) in result.panels.iter().zip(&result.panel_forces_geometry) {
            if panel.wing_index != 0 {
                continue;
            }
            let eta = panel.vortex_center[1].abs() / semi_span_m;
            if eta < 1.0 {
                sums[((eta * BINS as f64) as usize).min(BINS - 1)] += force[2];
            }
        }
        let total = sums.iter().sum();
        (sums, total)
    }

    fn bits(values: &[f64]) -> Vec<u64> {
        values.iter().map(|value| value.to_bits()).collect()
    }

    #[test]
    fn shared_lattice_and_held_strips_match_separate_solves_bit_for_bit() {
        let a320 = plane("A320-200");
        let semi_span = 0.5 * a320.b_ref;
        let (s1, c1) = separate_solve(&a320, semi_span, ALPHA_1_DEG);
        let (s2, c2) = separate_solve(&a320, semi_span, ALPHA_2_DEG);

        let shared = solve_unit_strips(&a320, semi_span).unwrap();
        assert_eq!(bits(&shared.0), bits(&s1));
        assert_eq!(shared.1.to_bits(), c1.to_bits());
        assert_eq!(bits(&shared.2), bits(&s2));
        assert_eq!(shared.3.to_bits(), c2.to_bits());

        // First call fills the per-thread entry, the second reads it; a
        // different mass only rescales the basic loading.
        let first = vlm_span_load_shape(&a320, semi_span, 0.5).unwrap();
        let held = vlm_span_load_shape(&a320, semi_span, 0.5).unwrap();
        assert_eq!(bits(&first.additional), bits(&held.additional));
        assert_eq!(bits(&first.basic), bits(&held.basic));
        let heavier = vlm_span_load_shape(&a320, semi_span, 0.6).unwrap();
        assert_eq!(bits(&first.additional), bits(&heavier.additional));
        assert_ne!(bits(&first.basic), bits(&heavier.basic));

        // Another geometry replaces the entry instead of reading it.
        let other = plane("ATR72-600");
        let other_span = 0.5 * other.b_ref;
        let other_shape = vlm_span_load_shape(&other, other_span, 0.5).unwrap();
        assert_ne!(bits(&first.additional), bits(&other_shape.additional));
        let fresh = solve_unit_strips(&other, other_span).unwrap();
        assert_eq!(unit_strips(&other, other_span).unwrap(), fresh);
        let again = vlm_span_load_shape(&a320, semi_span, 0.5).unwrap();
        assert_eq!(bits(&first.additional), bits(&again.additional));
        assert_eq!(bits(&first.basic), bits(&again.basic));
    }
}
