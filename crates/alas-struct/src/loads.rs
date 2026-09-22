// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/structural_loads.py
// Reference: alas @ rust-port-baseline.

//! Shared spanwise load model for the wingbox.
//!
//! One elliptic-lift (+ inertial-relief) distributed load, integrated to shear
//! and bending moment via a cantilever (tip -> root) numerical integral. The
//! same primitives serve the strength-sizing model, the analytical deflection
//! estimate and the NASTRAN BDF FORCE cards, which is what guarantees the
//! three never disagree about the load model.
//!
//! # Inertia relief is part of the wing-bending design case, not a refinement
//!
//! Wing-root bending is produced by the mass the wing does **not** carry. Every
//! kilogram carried inside the wing itself - the structural box, the fuel in
//! the integral tanks between the spars, a wing-mounted powerplant or gear -
//! is balanced locally by the lift over its own station and relieves the root.
//! Writing the semi-wing load as `n g W/2` with no relief is not a
//! conservative version of that case, it is a different aircraft: one whose
//! entire take-off mass hangs from the centreline. On a long-range transport
//! that overstates the ultimate root bending moment by a factor of roughly two
//! (see [`WingInertiaRelief`]), and the cap area, which is linear in the
//! moment, with it.
//!
//! [`WingInertiaRelief`] carries the relieved running mass and any wing-mounted
//! point masses; [`net_distributed_load`] and [`apply_point_mass_relief`] apply
//! them. The historic no-relief form is reachable by passing an empty relief,
//! which is what the frozen reference sizing law
//! (`crate::sizing::size_wingbox_reference_compatibility`) and its parity
//! fixtures do.

use alas_config::{DesignRequirements, EngineConfig, MassModelConfig};

/// One structural design load case for the semi-wing.
///
/// Frozen upstream (`@dataclass(frozen=True)`); every field is a scalar or a
/// static name, so `Copy` is the faithful analogue of that immutability.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadCase {
    /// `"pull-up"`, `"push-down"` or `"level"`.
    pub name: &'static str,
    /// The signed ultimate load factor n, already including any additional
    /// safety factor.
    pub load_factor: f64,
    /// The signed total aerodynamic force on this semi-wing,
    /// `n * mtow_kg * g / 2`.
    pub total_force_n: f64,
}

/// Pull-up ultimate / push-down ultimate / 1g level.
///
/// Reuses [`DesignRequirements`]' own `ultimate_load_factor` /
/// `limit_load_factor_neg` fields with *exactly* the same derivation
/// `alas/physics/performance.py`'s V-n diagram uses (`n_ult_pos =
/// ultimate_load_factor`, `n_ult_neg = limit_load_factor_neg * 1.5`), so the
/// structural loads always match the V-n diagram shown elsewhere in the app,
/// not a second, independently-tuned load case. (`performance.py` is not yet
/// ported; the derivation is reproduced here from the requirement fields
/// directly, exactly as upstream does.)
pub fn load_cases(req: &DesignRequirements, additional_safety_factor: f64) -> [LoadCase; 3] {
    let g = req.gravity_m_s2;
    let w_n = req.mtow_kg * g;
    let n_ult_pos = req.ultimate_load_factor * additional_safety_factor;
    let n_ult_neg = req.limit_load_factor_neg * 1.5 * additional_safety_factor;
    [
        LoadCase {
            name: "pull-up",
            load_factor: n_ult_pos,
            total_force_n: n_ult_pos * w_n / 2.0,
        },
        LoadCase {
            name: "push-down",
            load_factor: n_ult_neg,
            total_force_n: n_ult_neg * w_n / 2.0,
        },
        LoadCase {
            name: "level",
            load_factor: 1.0,
            total_force_n: 1.0 * w_n / 2.0,
        },
    ]
}

/// The wing fuel that is *guaranteed* present at the wing-bending design case,
/// kg, for the whole aircraft.
///
/// The scalar form of [`crate::scope::WingFuelDesignCase`], which carries the
/// derivation of the bound and the typed distinction between a case an operating
/// limit bounds and one a full-tank assumption rests on. Both read the same
/// resolution, so the number and the declaration published beside it cannot
/// disagree.
///
/// Every argument and the result are kilogrammes for the **whole aircraft**, not
/// a semi-wing; a caller distributing the result over one semi-wing halves it
/// afterwards, as the published capacities are both-wings figures.
///
/// A caller that reports the sized box should take
/// [`crate::scope::WingFuelDesignCase::declared`] instead: this form cannot say
/// whether the returned capacity was bounded or assumed.
pub fn design_case_wing_fuel_kg(
    wing_tank_capacity_kg: f64,
    design_gross_mass_kg: f64,
    max_zero_fuel_mass_kg: Option<f64>,
) -> f64 {
    crate::scope::WingFuelDesignCase::declared(
        wing_tank_capacity_kg,
        design_gross_mass_kg,
        max_zero_fuel_mass_kg,
    )
    .design_case_kg()
}

/// What one semi-wing carries itself at a structural design case.
///
/// The wing's own structure, the fuel in its integral tanks and anything hung
/// on it are reacted by the lift over their own stations. Only the remainder -
/// the fuselage, the payload, the tail and the systems inside them - has to be
/// carried across the root, so only that remainder produces root bending.
///
/// Both members describe **one** semi-wing, in the same frame the sizing grid
/// uses: `y = 0` at the aircraft centreline, `y = semi_span` at the tip.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WingInertiaRelief {
    /// Running mass carried by the semi-wing, kg/m, sampled at the same
    /// stations as the load grid. Empty means no distributed relief.
    pub running_mass_kg_m: Vec<f64>,
    /// Wing-mounted point masses as `(spanwise station m, mass kg)`, one entry
    /// per item on the modelled semi-wing.
    pub point_masses_kg: Vec<(f64, f64)>,
}

impl WingInertiaRelief {
    /// Whether this relief changes any load.
    pub fn is_empty(&self) -> bool {
        self.running_mass_kg_m.iter().all(|&m| m == 0.0) && self.point_masses_kg.is_empty()
    }

    /// Total relieved mass on the semi-wing, kg, for the stations `y`.
    ///
    /// `y` and [`Self::running_mass_kg_m`] must be the same length; a mismatch
    /// contributes only the point masses, which is what an empty distributed
    /// relief means.
    pub fn total_mass_kg(&self, y: &[f64]) -> f64 {
        let distributed = if self.running_mass_kg_m.len() == y.len() {
            let mut acc = 0.0;
            for i in 0..y.len().saturating_sub(1) {
                acc += (y[i + 1] - y[i])
                    * (self.running_mass_kg_m[i + 1] + self.running_mass_kg_m[i])
                    / 2.0;
            }
            acc
        } else {
            0.0
        };
        distributed + self.point_masses_kg.iter().map(|&(_, m)| m).sum::<f64>()
    }
}

/// Net running load [N/m]: the aerodynamic distribution less the inertia of the
/// mass the wing carries at the same station, `q_aero - n g m`.
///
/// `load_factor` must carry the same sign convention as `q_aero`: pass both
/// signed, or both as magnitudes. Mixing them makes the relief add to the load
/// on the push-down case instead of subtracting from it.
/// `relief_running_mass_kg_m` of a different length than `q_aero` is treated as
/// no distributed relief rather than silently truncating one of the two.
pub fn net_distributed_load(
    q_aero: &[f64],
    load_factor: f64,
    gravity_m_s2: f64,
    relief_running_mass_kg_m: &[f64],
) -> Vec<f64> {
    if relief_running_mass_kg_m.len() != q_aero.len() {
        return q_aero.to_vec();
    }
    let factor = load_factor * gravity_m_s2;
    q_aero
        .iter()
        .zip(relief_running_mass_kg_m)
        .map(|(&q, &m)| q - factor * m)
        .collect()
}

/// Subtract the moment a set of wing-mounted point masses relieves, in place.
///
/// A mass at `y_item` unloads every station inboard of it by
/// `n g m (y_item - y)`; stations outboard of it are untouched. `moment_nm` and
/// `load_factor` carry the same sign convention as each other, as in
/// [`net_distributed_load`].
pub fn apply_point_mass_relief(
    y: &[f64],
    moment_nm: &mut [f64],
    load_factor: f64,
    gravity_m_s2: f64,
    point_masses_kg: &[(f64, f64)],
) {
    let factor = load_factor * gravity_m_s2;
    for &(y_item, mass_kg) in point_masses_kg {
        let force_n = factor * mass_kg;
        for (station, moment) in y.iter().zip(moment_nm.iter_mut()) {
            if *station <= y_item {
                *moment -= force_n * (y_item - station);
            }
        }
    }
}

/// Half-elliptic spanwise load distribution [N/m], integrating to
/// `total_force_n` over `[0, semi_span]`.
///
/// A classic, well-precedented preliminary-design simplification for wing
/// structural loads (the same one the reference scripts used, validated there
/// to <20% vs. real NASTRAN deformations). The `clamp(0.0, 1.0)` reproduces
/// upstream's `np.clip`, guarding the `sqrt` argument against going negative
/// at a station just past the tip.
pub fn elliptic_distributed_load(y: &[f64], semi_span: f64, total_force_n: f64) -> Vec<f64> {
    let q0 = 4.0 * total_force_n / (std::f64::consts::PI * semi_span);
    y.iter()
        .map(|&yi| {
            let arg = (1.0 - (yi / semi_span).powi(2)).clamp(0.0, 1.0);
            q0 * arg.sqrt()
        })
        .collect()
}

/// Shear `V(y)` and bending moment `M(y)` for a cantilever beam (free at the
/// tip, fixed at the root) under a net distributed load `q_net` [N/m] sampled
/// at `y`, via cumulative trapezoidal integration from tip to root.
///
/// The root reaction is never referenced directly: V/M at `y = 0` fall out
/// of the integral, matching the reference's own approach. The accumulation
/// runs from the last segment down to the first, reproducing the summation
/// order of NumPy's `np.cumsum(seg[::-1])[::-1]` exactly, which is what keeps
/// this at the `closed` tier over a dense (FEM-resolution) station vector.
pub fn cantilever_shear_moment(y: &[f64], q_net: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = y.len();
    let mut v = vec![0.0; n];
    let mut m = vec![0.0; n];
    // With fewer than two stations there are no segments to integrate; upstream
    // leaves `v`/`m` at their `np.zeros(n)` fill, which `vec![0.0; n]` already
    // matches (an empty vector for `n == 0`, a single zero for `n == 1`).
    if n < 2 {
        return (v, m);
    }

    // Trapezoidal segment loads, then a suffix sum so that `v[i]` carries every
    // segment outboard of station `i`. The last segment is added first, matching
    // the reversed-cumsum order upstream uses.
    let mut segment = vec![0.0; n - 1];
    for i in 0..n - 1 {
        let dy = y[i + 1] - y[i];
        segment[i] = 0.5 * (q_net[i] + q_net[i + 1]) * dy;
    }
    let mut running = 0.0;
    for i in (0..n - 1).rev() {
        running += segment[i];
        v[i] = running;
    }

    // Integrate the shear the same way to get the bending moment.
    for i in 0..n - 1 {
        let dy = y[i + 1] - y[i];
        segment[i] = 0.5 * (v[i] + v[i + 1]) * dy;
    }
    let mut running = 0.0;
    for i in (0..n - 1).rev() {
        running += segment[i];
        m[i] = running;
    }

    (v, m)
}

/// Per-engine `(y_position_m, installed_mass_kg)` for every WING-mounted engine
/// on the modeled (positive-Y, right) semi-wing.
///
/// **The second element is a mass in kilogrammes, not the force the `_n` suffix
/// suggests.** It is what [`WingInertiaRelief::point_masses_kg`] takes, and
/// [`apply_point_mass_relief`] is what turns it into the newtons `n g m` the
/// load case needs. The spanwise station is metres from the aircraft centreline,
/// the same datum the sizing grid runs on.
///
/// The resolved half of [`crate::scope::wing_mounted_relief`]. This form drops
/// the wing-carried items that could not be resolved - a turboprop's propeller
/// and nacelle, a wing-mounted gear leg - which leave the sized box heavier than
/// the aircraft needs. A caller that reports the box takes the scoped form, so
/// that omission is published rather than inferred from an empty list.
pub fn engine_point_loads_n(
    engine_cfg: &EngineConfig,
    mass_cfg: &MassModelConfig,
    req: &DesignRequirements,
) -> Vec<(f64, f64)> {
    crate::scope::wing_mounted_relief(engine_cfg, mass_cfg, req).point_masses_kg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requirements() -> DesignRequirements {
        DesignRequirements::default()
    }

    #[test]
    fn load_cases_reproduce_the_v_n_diagram_derivation() {
        let req = DesignRequirements {
            mtow_kg: 100_000.0,
            gravity_m_s2: 9.81,
            ultimate_load_factor: 3.75,
            limit_load_factor_neg: -1.0,
            ..Default::default()
        };

        let cases = load_cases(&req, 1.0);
        let w_n = 100_000.0 * 9.81;

        assert_eq!(cases[0].name, "pull-up");
        assert_eq!(cases[0].load_factor, 3.75);
        assert_eq!(cases[0].total_force_n, 3.75 * w_n / 2.0);

        assert_eq!(cases[1].name, "push-down");
        // limit_load_factor_neg * 1.5, with the ultimate factor already folded in.
        let n_ult_neg = req.limit_load_factor_neg * 1.5;
        assert_eq!(cases[1].load_factor, n_ult_neg);
        assert_eq!(cases[1].total_force_n, n_ult_neg * w_n / 2.0);

        assert_eq!(cases[2].name, "level");
        assert_eq!(cases[2].load_factor, 1.0);
        assert_eq!(cases[2].total_force_n, w_n / 2.0);
    }

    #[test]
    fn additional_safety_factor_scales_both_ultimate_cases_but_not_the_level_case() {
        let req = requirements();
        let base = load_cases(&req, 1.0);
        let scaled = load_cases(&req, 1.5);
        assert!((scaled[0].load_factor - base[0].load_factor * 1.5).abs() < 1e-12);
        assert!((scaled[1].load_factor - base[1].load_factor * 1.5).abs() < 1e-12);
        // The 1g level case is a literal 1.0, untouched by the safety factor.
        assert_eq!(scaled[2].load_factor, 1.0);
    }

    #[test]
    fn the_elliptic_load_integrates_to_the_requested_total() {
        let semi_span = 30.0;
        let total = 800_000.0;
        let y: Vec<f64> = (0..=2000).map(|i| i as f64 * semi_span / 2000.0).collect();
        let q = elliptic_distributed_load(&y, semi_span, total);
        // Trapezoidal integral of the sampled distribution over [0, semi_span].
        let mut integral = 0.0;
        for i in 0..y.len() - 1 {
            integral += 0.5 * (q[i] + q[i + 1]) * (y[i + 1] - y[i]);
        }
        assert!((integral - total).abs() / total < 1e-4);
    }

    #[test]
    fn a_station_past_the_tip_is_clipped_to_zero_load_not_a_nan() {
        let q = elliptic_distributed_load(&[10.0, 10.000_001], 10.0, 100_000.0);
        assert_eq!(q[0], 0.0);
        assert_eq!(q[1], 0.0);
        assert!(q.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn shear_and_moment_vanish_at_the_tip_and_peak_at_the_root() {
        // A uniform load over a cantilever: shear and moment are largest at the
        // fixed root (y = 0) and zero at the free tip.
        let y: Vec<f64> = (0..=8).map(|i| i as f64 * 2.0).collect();
        let q_net = vec![5_000.0; y.len()];
        let (v, m) = cantilever_shear_moment(&y, &q_net);
        let last = y.len() - 1;
        assert_eq!(v[last], 0.0);
        assert_eq!(m[last], 0.0);
        assert!(v[0] > v[1]);
        assert!(m[0] > m[1]);
        // Root shear equals the whole integrated load: q * span.
        let span = y[last];
        assert!((v[0] - 5_000.0 * span).abs() / (5_000.0 * span) < 1e-12);
    }

    #[test]
    fn a_degenerate_single_station_integrates_to_zero() {
        let (v, m) = cantilever_shear_moment(&[3.0], &[1_000.0]);
        assert_eq!(v, vec![0.0]);
        assert_eq!(m, vec![0.0]);
    }

    #[test]
    fn relief_subtracts_the_carried_inertia_at_each_station() {
        // 100 kg/m carried at 3 g takes 100 x 3 x 9.81 N/m off the load.
        let q = vec![10_000.0; 4];
        let net = net_distributed_load(&q, 3.0, 9.81, &[100.0, 100.0, 0.0, 0.0]);
        assert!((net[0] - (10_000.0 - 2943.0)).abs() < 1e-9);
        assert_eq!(net[2], 10_000.0);
    }

    #[test]
    fn a_relief_of_the_wrong_length_is_ignored_rather_than_truncated() {
        // Silently zipping a short relief onto a long grid would relieve the
        // inboard stations and leave the rest at full load, which is a load
        // case nobody asked for.
        let q = vec![10_000.0; 4];
        assert_eq!(net_distributed_load(&q, 3.0, 9.81, &[100.0, 100.0]), q);
    }

    #[test]
    fn the_push_down_case_is_relieved_in_its_own_direction() {
        // Signed load factor and signed load: a negative case must have its
        // magnitude reduced by the relief, not increased.
        let q = vec![-10_000.0; 2];
        let net = net_distributed_load(&q, -1.5, 9.81, &[100.0, 100.0]);
        assert!(net[0] > q[0], "{net:?}");
        assert!((net[0] - (-10_000.0 + 1471.5)).abs() < 1e-9);
    }

    #[test]
    fn a_point_mass_relieves_only_the_stations_inboard_of_it() {
        let y = vec![0.0, 5.0, 10.0, 15.0];
        let mut moment = vec![1.0e7, 1.0e7, 1.0e7, 1.0e7];
        apply_point_mass_relief(&y, &mut moment, 3.0, 10.0, &[(10.0, 5_000.0)]);
        // n g m = 150 000 N at a 10 m arm from the root.
        assert!((moment[0] - (1.0e7 - 150_000.0 * 10.0)).abs() < 1e-6);
        assert!((moment[1] - (1.0e7 - 150_000.0 * 5.0)).abs() < 1e-6);
        assert_eq!(moment[2], 1.0e7, "the mass's own station carries no arm");
        assert_eq!(moment[3], 1.0e7, "outboard of it nothing changes");
    }

    #[test]
    fn the_relieved_mass_totals_the_distributed_and_the_point_items() {
        let y = vec![0.0, 10.0];
        let relief = WingInertiaRelief {
            running_mass_kg_m: vec![200.0, 100.0],
            point_masses_kg: vec![(4.0, 2_500.0)],
        };
        // Trapezoid of 200 -> 100 over 10 m is 1 500 kg, plus the 2 500 kg item.
        assert!((relief.total_mass_kg(&y) - 4_000.0).abs() < 1e-9);
        assert!(!relief.is_empty());
        assert!(WingInertiaRelief::default().is_empty());
        // A mismatched grid contributes only the point items, which is what a
        // rejected distributed relief means.
        assert!((relief.total_mass_kg(&[0.0, 5.0, 10.0]) - 2_500.0).abs() < 1e-9);
    }

    #[test]
    fn the_design_case_wing_fuel_is_the_least_the_envelope_guarantees() {
        // A340-300: EASA.A.064 260 000 kg / 178 000 kg, 92 850 L of integral
        // wing cells at 0.8 kg/L. 82 000 kg must be on board at the design
        // gross mass and the wings hold only 74 280 kg, so they are full and
        // the declared capacity stands unchanged.
        let a340 = design_case_wing_fuel_kg(74_280.0, 260_000.0, Some(178_000.0));
        assert!((a340 - 74_280.0).abs() < 1e-9, "{a340}");

        // ATR 72-600: 23 000 kg / 21 000 kg against 5 065.2 kg of wing tanks
        // (6 300 L at the configured 804 kg/m^3). At the maximum structural
        // payload only 2 000 kg of fuel is on board, so three fifths of the
        // capacity is relief the box must not be credited with.
        let atr = design_case_wing_fuel_kg(5_065.2, 23_000.0, Some(21_000.0));
        assert!((atr - 2_000.0).abs() < 1e-9, "{atr}");

        // A380-800: 560 000 kg / 361 000 kg against a wing-tank capacity well
        // above the 199 000 kg difference. The 236 488 kg below is the figure
        // this fixture was written against; the preset's certified
        // integral-cell capacity is now 239 878.4 kg (EASA TCDS EASA.A.110
        // Issue 17 section 3.3), still above the difference, so the expected
        // result is unchanged and the fixture is left as written.
        let a380 = design_case_wing_fuel_kg(236_488.0, 560_000.0, Some(361_000.0));
        assert!((a380 - 199_000.0).abs() < 1e-9, "{a380}");
    }

    #[test]
    fn the_design_case_wing_fuel_reproduces_the_bending_mass_identity() {
        // `DG - design_case_wing_fuel` must equal `max(MZFW, DG - C)` at every
        // point of the envelope, which is the statement the derivation makes.
        for &(capacity, dg, mzfw) in &[
            (74_280.0, 260_000.0, 178_000.0),
            (5_065.2, 23_000.0, 21_000.0),
            (236_488.0, 560_000.0, 361_000.0),
            (12_767.2, 78_000.0, 62_500.0),
            // A zero-fuel limit at or above the design gross mass: nothing is
            // guaranteed on board, so no fuel relieves the case.
            (10_000.0, 50_000.0, 50_000.0),
            (10_000.0, 50_000.0, 60_000.0),
        ] {
            let fuel = design_case_wing_fuel_kg(capacity, dg, Some(mzfw));
            let bending = dg - fuel;
            assert!(
                (bending - mzfw.min(dg).max(dg - capacity)).abs() < 1e-9,
                "DG {dg}, C {capacity}, MZFW {mzfw}: {bending}"
            );
            assert!((0.0..=capacity).contains(&fuel), "{fuel}");
        }
    }

    #[test]
    fn an_undeclared_zero_fuel_limit_keeps_the_declared_capacity_unchanged() {
        // No envelope to bound the case with: the previous assumption stands,
        // and it is the caller's to report as an assumption.
        assert_eq!(
            design_case_wing_fuel_kg(74_280.0, 260_000.0, None),
            74_280.0
        );
        assert_eq!(
            design_case_wing_fuel_kg(74_280.0, 260_000.0, Some(f64::NAN)),
            74_280.0
        );
        assert_eq!(
            design_case_wing_fuel_kg(74_280.0, 260_000.0, Some(0.0)),
            74_280.0
        );
        // A capacity that is not a mass relieves nothing rather than poisoning
        // the load case with a NaN.
        assert_eq!(
            design_case_wing_fuel_kg(f64::NAN, 260_000.0, Some(178_000.0)),
            0.0
        );
        assert_eq!(
            design_case_wing_fuel_kg(-1.0, 260_000.0, Some(178_000.0)),
            0.0
        );
    }

    #[test]
    fn a_declared_shaft_power_installation_relieves_its_own_wing() {
        // A turboprop's jet rating is exactly zero by design, so the thrust
        // branch returns nothing for it. The declared certificated record is
        // what makes the installation a relieving mass.
        let config = alas_config::AlasConfig::from_value(&serde_json::json!({
            "preset": "ATR72-600"
        }))
        .unwrap_or_else(|error| panic!("{error}"));
        let loads = engine_point_loads_n(
            &config.geometry.engine,
            &config.mass_model,
            &config.requirements,
        );
        assert_eq!(
            loads.len(),
            1,
            "one wing-mounted installation per semi-wing"
        );
        let (station_m, mass_kg) = loads[0];
        assert!(station_m > 0.0);
        // EASA TCDS IM.E.041 PW127M dry 481.7 kg (gearbox inside, section III.2) plus
        // half of the declared 308 kg two-engine installation, plus the
        // declared zero propeller accessories.
        let expected = 481.7 + 308.0 / 2.0;
        assert!(
            (mass_kg - expected).abs() < 1.0e-9,
            "{mass_kg} kg against {expected} kg"
        );
    }

    #[test]
    fn an_undeclared_shaft_power_installation_contributes_nothing() {
        // Without a declared dry mass there is no defensible point load, and
        // one must not be invented from a thrust the engine does not produce.
        let mut config = alas_config::AlasConfig::from_value(&serde_json::json!({
            "preset": "ATR72-600"
        }))
        .unwrap_or_else(|error| panic!("{error}"));
        config.mass_model.flops_turboprop.engine_dry_mass_kg = None;
        assert!(engine_point_loads_n(
            &config.geometry.engine,
            &config.mass_model,
            &config.requirements
        )
        .is_empty());
    }

    #[test]
    fn zero_thrust_loads_nothing_regardless_of_positions() {
        let mut engine = EngineConfig {
            spanwise_positions_m: vec![9.8, -9.8],
            ..Default::default()
        };
        engine.turbofan.as_mut().unwrap().rated_thrust_kn = 0.0;
        let loads = engine_point_loads_n(&engine, &MassModelConfig::default(), &requirements());
        assert!(loads.is_empty());
    }

    #[test]
    fn only_starboard_wing_stations_survive_the_semi_wing_filter() {
        // A symmetric pair, a centerline engine, and a below-threshold station.
        let engine = EngineConfig {
            spanwise_positions_m: vec![9.8, -9.8, 0.0, 1e-7, 22.5],
            ..Default::default()
        };
        let loads = engine_point_loads_n(&engine, &MassModelConfig::default(), &requirements());
        let positions: Vec<f64> = loads.iter().map(|&(y, _)| y).collect();
        assert_eq!(positions, vec![9.8, 22.5]);
        // Every surviving engine carries the same one-engine dry mass.
        assert_eq!(loads[0].1, loads[1].1);
        assert!(loads[0].1 > 0.0);
    }
}
