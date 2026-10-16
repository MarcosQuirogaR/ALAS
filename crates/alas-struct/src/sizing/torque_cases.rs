// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The strength cases that carry the torque sources of
//! [`crate::torque_sources`], and the torque of every case.
//!
//! Each case is a [`Variant`]: a [`LoadCase`] of the manoeuvre or gust set
//! plus the extra running lift, pitching couple and thrust couples that make
//! it a torque case. All factors are ultimate: the limit loads of the
//! sources are multiplied by `1.5` and the configured additional safety
//! factor (CS 25.303), exactly like the load factor of the base cases.
//!
//! | case | speed | load factor (limit) | extras |
//! |---|---|---|---|
//! | pull-up + Cm0 | `V_D` (point D2, CS 25.333(b)) | `n_max` | `Cm0` couple |
//! | push-down + Cm0 | `V_C` (point F) | `-1` | `Cm0` couple |
//! | level + Cm0 | `V_D` (point D1) | 1 | `Cm0` couple |
//! | gust + Cm0 | gust speed | `1 + Delta n` | `Cm0` couple |
//! | flaps down | `V_F` (CS 25.345(a)) | 2.0 | flap lift, `Cm0` |
//! | aileron, up and down | `V_A`, `V_C`, `V_D` (CS 25.349(a)) | 0 and `2/3 n_max` | aileron lift, `Cm0` |
//! | take-off thrust | - | 1 | thrust couples |
//!
//! The engine inertia `n g m` at its centre of gravity acts in every case.
//! Cases whose torque is a lift offset only are the unchanged base cases.

use alas_config::{DesignRequirements, StructuresConfig};
use alas_geom::wing_structure::WingStructureGeometry;

use crate::loads::{LoadCase, WingInertiaRelief};
use crate::torque_sources::{self as ts, PreparedTorque, SectionTable};

use super::law::trapezoid;

/// One strength case with its torque extras.
pub(super) struct Variant {
    pub(super) case: LoadCase,
    /// Extra running lift `(N/m, x_abs m)`, ultimate, on the station grid.
    pub(super) extra_lift: Option<(Vec<f64>, Vec<f64>)>,
    /// Whether the extra lift is part of the case total (flaps: the base lift
    /// is reduced to keep `n W`) or comes on top (aileron: unbalanced).
    pub(super) extra_inside_total: bool,
    /// Pitching couple per unit span, N m/m, ultimate, nose-up positive.
    pub(super) couple: Option<Vec<f64>>,
    /// Point couples `(y m, nose-up N m)`.
    pub(super) point_couples: Vec<(f64, f64)>,
}

impl Variant {
    fn plain(case: LoadCase) -> Self {
        Self {
            case,
            extra_lift: None,
            extra_inside_total: false,
            couple: None,
            point_couples: Vec::new(),
        }
    }
}

const AILERON_NAMES: [&str; 12] = [
    "aileron-VA-down-n0",
    "aileron-VA-down-n2/3",
    "aileron-VA-up-n0",
    "aileron-VA-up-n2/3",
    "aileron-VC-down-n0",
    "aileron-VC-down-n2/3",
    "aileron-VC-up-n0",
    "aileron-VC-up-n2/3",
    "aileron-VD-down-n0",
    "aileron-VD-down-n2/3",
    "aileron-VD-up-n0",
    "aileron-VD-up-n2/3",
];

fn couple_of(table: &SectionTable, chord: &[f64], q_pa: f64, factor: f64) -> Vec<f64> {
    table
        .cm0
        .iter()
        .zip(chord)
        .map(|(&cm, &c)| factor * q_pa * c * c * cm)
        .collect()
}

/// The CS 25.349(a) aileron schedule of an aeroplane: the deflection at each
/// design speed and the steady roll rate it produces in strip theory.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AileronSchedule {
    /// `(V_A, V_C, V_D)`, m/s EAS.
    pub eas_m_s: [f64; 3],
    /// Aileron deflection at each speed, rad: the stop at `V_A`, the
    /// deflection that gives the roll rate of the `V_A` stop at `V_C`, and
    /// one third of that rate at `V_D`.
    pub delta_rad: [f64; 3],
    /// The section lift increment of the aileron at each speed.
    pub delta_cl: [f64; 3],
    /// Steady roll rate `p` of each condition in strip theory with uniform
    /// section slope `2 pi`, rad/s (equivalent airspeed as the true speed).
    pub roll_rate_rad_s: [f64; 3],
}

/// Steady roll rate over speed in strip theory: with the lift change
/// `q c (dc_l_aileron(y) - c_l_alpha (p / V) y)` and zero net rolling moment,
/// `p c_l_alpha / V = sum(c y dc_l) / sum(c y^2)` over the semi-span.
fn steady_roll_over_speed(
    ail: &ts::AileronSystem,
    delta_cl: f64,
    y: &[f64],
    chord: &[f64],
    eta: &[f64],
) -> f64 {
    let n = y.len();
    let num: Vec<f64> = (0..n)
        .map(|j| {
            if eta[j] >= ail.eta.0 && eta[j] <= ail.eta.1 {
                chord[j] * y[j] * delta_cl
            } else {
                0.0
            }
        })
        .collect();
    let den: Vec<f64> = (0..n).map(|j| chord[j] * y[j] * y[j]).collect();
    let (num, den) = (trapezoid(&num, y), trapezoid(&den, y));
    if den > 0.0 {
        num / den / (2.0 * std::f64::consts::PI)
    } else {
        0.0
    }
}

/// The aileron deflection schedule of CS 25.349(a) (Amendment 27, PDF p. 241):
/// the stop at `V_A`; at `V_C` the deflection that produces a roll rate not
/// less than that of the `V_A` stop; at `V_D` one third of it. The strip-theory
/// roll rate is proportional to `V dc_l`, so the matched increments are
/// `dc_l(V_C) = dc_l(V_A) V_A / V_C` and `dc_l(V_D) = dc_l(V_A) V_A / (3 V_D)`;
/// the deflections follow by inverting the plain-flap increment. The wing is
/// rigid: no aeroelastic relief or aileron reversal.
pub fn aileron_schedule(
    ail: &ts::AileronSystem,
    speeds: (f64, f64, f64),
    y: &[f64],
    chord: &[f64],
    eta: &[f64],
) -> AileronSchedule {
    let (v_a, v_c, v_d) = speeds;
    let max = ts::aileron_max_deflection_rad();
    let dcl_a = ts::plain_flap_delta_cl(ail.chord_fraction, max);
    let target = [dcl_a, dcl_a * v_a / v_c, dcl_a * v_a / (3.0 * v_d)];
    let delta_rad =
        target.map(|dcl| ts::plain_flap_deflection_for_delta_cl(ail.chord_fraction, dcl));
    let eas_m_s = [v_a, v_c, v_d];
    let roll_rate_rad_s =
        std::array::from_fn(|i| steady_roll_over_speed(ail, target[i], y, chord, eta) * eas_m_s[i]);
    AileronSchedule {
        eas_m_s,
        delta_rad,
        delta_cl: target,
        roll_rate_rad_s,
    }
}

/// The extra cases for the sources in `relief.torque`, after the base cases
/// `base` (manoeuvre and gust).
pub(super) fn variants(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    y: &[f64],
    relief: &WingInertiaRelief,
    base: Vec<LoadCase>,
    set: super::envelope::CaseSet,
) -> Vec<Variant> {
    let mut out: Vec<Variant> = base.iter().copied().map(Variant::plain).collect();
    let prepared: &PreparedTorque = &relief.torque;
    if !set.torque || y.len() < 2 {
        return out;
    }
    let n = y.len();
    let ultimate = 1.5 * cfg.additional_safety_factor;
    let weight_half = req.mtow_kg * req.gravity_m_s2 / 2.0;
    let eta: Vec<f64> = y.iter().map(|&v| v / wsg.semi_span).collect();
    let chord: Vec<f64> = eta.iter().map(|&e| wsg.local_chord(e)).collect();
    let x_le: Vec<f64> = eta.iter().map(|&e| wsg.x_le(e)).collect();
    let find = |name: &str| base.iter().find(|c| c.name == name).copied();
    let (cruise, dive) = (prepared.cruise.as_ref(), prepared.dive.as_ref());

    // Manoeuvre and gust cases with the section moment at their own speed.
    // Pull-up at point D2 and the 1 g level case at D1 both sit at V_D; the
    // push-down case at F sits at V_C.
    if let (Some(dive), Some(pull)) = (dive, find("pull-up")) {
        let mut v = Variant::plain(LoadCase {
            name: "pull-up+Cm0-VD",
            ..pull
        });
        v.couple = Some(couple_of(dive, &chord, dive.q_pa, ultimate));
        out.push(v);
    }
    if let (Some(cruise), Some(push)) = (cruise, find("push-down")) {
        let mut v = Variant::plain(LoadCase {
            name: "push-down+Cm0-VC",
            ..push
        });
        v.couple = Some(couple_of(cruise, &chord, cruise.q_pa, ultimate));
        out.push(v);
    }
    if let Some(dive) = dive {
        let mut v = Variant::plain(LoadCase {
            name: "level-VD+Cm0",
            load_factor: ultimate,
            total_force_n: ultimate * weight_half,
        });
        v.couple = Some(couple_of(dive, &chord, dive.q_pa, ultimate));
        out.push(v);
    }
    for (gust_name, extra_name, table) in [
        ("gust-VC", "gust-VC+Cm0", cruise),
        ("gust-VD", "gust-VD+Cm0", dive),
    ] {
        if let (Some(case), Some(table)) = (find(gust_name), table) {
            let mut v = Variant::plain(LoadCase {
                name: extra_name,
                ..case
            });
            v.couple = Some(couple_of(table, &chord, table.q_pa, ultimate));
            out.push(v);
        }
    }

    // Flaps down at V_F: n = 2.0 (CS 25.345(a)(1)).
    if let (Some(flap), Some(table)) = (prepared.sources.flap, prepared.flap.as_ref()) {
        let ult_n = 2.0 * ultimate;
        let lift: Vec<f64> = (0..n)
            .map(|j| {
                if eta[j] >= flap.eta.0 && eta[j] <= flap.eta.1 {
                    let dcl = ts::slotted_flap_delta_cl(
                        table.cl_alpha[j],
                        flap.chord_fraction,
                        flap.deflection_rad,
                    );
                    ultimate * table.q_pa * chord[j] * dcl
                } else {
                    0.0
                }
            })
            .collect();
        let x_cp = ts::flap_xcp_fraction(true, flap.chord_fraction);
        let x: Vec<f64> = (0..n).map(|j| x_le[j] + x_cp * chord[j]).collect();
        let mut v = Variant::plain(LoadCase {
            name: "flaps-VF",
            load_factor: ult_n,
            total_force_n: ult_n * weight_half,
        });
        v.couple = Some(couple_of(table, &chord, table.q_pa, ultimate));
        v.extra_lift = Some((lift, x));
        v.extra_inside_total = true;
        out.push(v);
    }

    // Aileron to the stop (CS 25.349(a)).
    if let (true, Some(ail), Some((v_a, v_c, v_d)), Some(pull)) = (
        set.aileron,
        prepared.sources.aileron,
        prepared.speeds_eas,
        find("pull-up"),
    ) {
        let x_cp = ts::flap_xcp_fraction(false, ail.chord_fraction);
        let x: Vec<f64> = (0..n).map(|j| x_le[j] + x_cp * chord[j]).collect();
        // V_A: sudden deflection to the stop (zero roll rate, the
        // angular-acceleration condition CS 25.349(a)(1) allows without a time
        // history). V_C and V_D: steady roll at the roll rate of the V_A stop
        // and at a third of it, where roll damping balances the aileron moment
        // and the net rolling moment is zero; the deflections come from the
        // strip-theory steady roll rate ([`aileron_schedule`]).
        let schedule = aileron_schedule(&ail, (v_a, v_c, v_d), y, &chord, &eta);
        let conditions = [
            (v_a, schedule.delta_cl[0], cruise, false),
            (v_c, schedule.delta_cl[1], cruise, true),
            (v_d, schedule.delta_cl[2], dive, true),
        ];
        let mut index = 0;
        for (speed, dcl, table, steady_roll) in conditions {
            let q = PreparedTorque::q_of_eas(speed);
            let in_span = |j: usize| eta[j] >= ail.eta.0 && eta[j] <= ail.eta.1;
            // Strip-theory damping, uniform section slope: the lift change per
            // unit span is q c (dcl_a(y) - K y) with K the roll rate over the
            // speed in units of the slope, from sum(y dL) = 0.
            let damping = if steady_roll {
                let num: Vec<f64> = (0..n)
                    .map(|j| {
                        if in_span(j) {
                            chord[j] * y[j] * dcl
                        } else {
                            0.0
                        }
                    })
                    .collect();
                let den: Vec<f64> = (0..n).map(|j| chord[j] * y[j] * y[j]).collect();
                let (num, den) = (trapezoid(&num, y), trapezoid(&den, y));
                if den > 0.0 {
                    num / den
                } else {
                    0.0
                }
            } else {
                0.0
            };
            for sign in [1.0, -1.0] {
                for load_share in [0.0, 2.0 / 3.0] {
                    let lift: Vec<f64> = (0..n)
                        .map(|j| {
                            let aileron = if in_span(j) { dcl } else { 0.0 };
                            sign * ultimate * q * chord[j] * (aileron - damping * y[j])
                        })
                        .collect();
                    let mut v = Variant::plain(LoadCase {
                        name: AILERON_NAMES[index],
                        load_factor: load_share * pull.load_factor,
                        total_force_n: load_share * pull.total_force_n,
                    });
                    if let Some(table) = table {
                        v.couple = Some(couple_of(table, &chord, q, ultimate));
                    }
                    v.extra_lift = Some((lift, x.clone()));
                    out.push(v);
                    index += 1;
                }
            }
        }
    }

    // Take-off thrust at the engine centreline below the reference plane:
    // a nose-up couple inboard of each pylon, with the 1 g lift (ultimate).
    let couples: Vec<(f64, f64)> = prepared
        .sources
        .engines
        .iter()
        .filter(|e| e.thrust_n > 0.0)
        .map(|e| {
            let arm = wsg.z_le(e.y_m / wsg.semi_span) - e.z_m;
            (e.y_m, ultimate * e.thrust_n * arm)
        })
        .collect();
    if !couples.is_empty() {
        let mut v = Variant::plain(LoadCase {
            name: "thrust-TO",
            load_factor: ultimate,
            total_force_n: ultimate * weight_half,
        });
        v.point_couples = couples;
        out.push(v);
    }
    out
}
