// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sizing load envelope beyond the symmetric pull-up.
//!
//! The product law sizes every station to the largest demand over a set of
//! cases rather than to one governing case:
//!
//! - the manoeuvre set of [`crate::loads::load_cases`] (pull-up, push-down, 1 g
//!   level), each relieved by the inertia the wing carries itself;
//! - the quasi-static CS 25.341(a) discrete vertical gust at `V_C` and `V_D`
//!   ([`crate::gust`]), with the ultimate load factor `1.5 (1 + Delta n)`;
//! - the torque of the aerodynamic load about the elastic axis
//!   ([`crate::torsion`]), carried by the spar webs as a Bredt-Batho shear
//!   flow added to the web shear of the same case;
//! - the fatigue limit of the lower cover, a 1 g tensile stress limit set by
//!   the design service life (Torenbeek, *Advanced Aircraft Design*, 2013,
//!   section 11.7.1, book p. 350), applied to metallic covers when the
//!   configuration declares it. It is not a load case: it adds area to the
//!   lower flange only ([`super::fatigue`]), from the 1 g level moment.
//!
//! The per-station demand is `max` over the strength cases, moment and shear
//! separately, the same decoupled envelope the pull-up sizing already used.

use alas_config::materials::MaterialSpec;
use alas_config::{DesignRequirements, StructuresConfig};
use alas_geom::wing_structure::WingStructureGeometry;

use crate::gust::{self, GustSpeeds, GustWing};
use crate::loads::{self, LoadCase, WingInertiaRelief};
use crate::torsion;

use super::law::trapezoid;
use super::torque_cases;
pub use super::torque_cases::{aileron_schedule, AileronSchedule};

/// Ratio of the design cruising to the design dive speed, `V_C <= 0.8 V_D`
/// (CS 25.335(b), PDF p. 212 of CS-25 Amdt 27).
pub(crate) const CRUISE_TO_DIVE_SPEED_RATIO: f64 = 0.8;

/// Factor of safety on the prescribed limit load (CS 25.303, PDF p. 201).
const FACTOR_OF_SAFETY: f64 = 1.5;

/// Which of the cases beyond the manoeuvre set the sizing evaluates.
///
/// The product always evaluates all of them; the switches exist so that the
/// mass each one adds can be measured, and so that tests can state what each
/// one does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaseSet {
    /// The CS 25.341(a) discrete gust at `V_C` and `V_D`.
    pub gust: bool,
    /// The aerodynamic torque shear flow in the webs.
    pub torque: bool,
    /// The lower-cover fatigue limit, where the configuration declares one.
    pub fatigue: bool,
    /// The CS 25.349(a) aileron rolling cases ([`crate::torque_sources`]).
    pub aileron: bool,
}

impl CaseSet {
    /// Every case the product evaluates: manoeuvre, gust, torque (lift
    /// offset, section moment, flaps down, engine inertia and thrust) and the
    /// fatigue limit. The CS 25.349(a) aileron rolling cases are **not** in
    /// it: they need the aeroelastic aileron effectiveness of the wing and the
    /// roll-rate and control-law limits of the aircraft, and a rigid strip at
    /// full deflection over-predicts (about 1.3 times the FLOPS wing for the
    /// A340 and A380, roll rates far above real transports, the AVE box above
    /// its own complete-wing mass). See [`CaseSet::WITH_AILERON`] and
    /// [`crate::scope::AILERON_CASES_NOT_IN_PRODUCT_SET`].
    pub const ALL: Self = Self {
        gust: true,
        torque: true,
        fatigue: true,
        aileron: false,
    };
    /// [`CaseSet::ALL`] plus the rigid-wing aileron rolling cases, available
    /// and tested but not used by the product.
    pub const WITH_AILERON: Self = Self {
        aileron: true,
        ..Self::ALL
    };
    /// The manoeuvre set only: what the box was sized to before the gust,
    /// torque and fatigue cases existed.
    pub const MANOEUVRE_ONLY: Self = Self {
        gust: false,
        torque: false,
        fatigue: false,
        aileron: false,
    };
}

impl Default for CaseSet {
    fn default() -> Self {
        Self::ALL
    }
}

/// The critical discrete gusts of the aircraft the requirements describe, or
/// `None` when the inputs do not define them.
pub fn discrete_gusts(
    wsg: &WingStructureGeometry,
    req: &DesignRequirements,
) -> Option<[gust::DiscreteGust; 2]> {
    let speeds = GustSpeeds {
        cruise_eas_m_s: CRUISE_TO_DIVE_SPEED_RATIO * req.dive_speed_m_s,
        dive_eas_m_s: req.dive_speed_m_s,
        cruise_mach: req.cruise_mach,
    };
    gust::critical_discrete_gusts(
        &GustWing::from_geometry(wsg),
        req.mtow_kg,
        req.gravity_m_s2,
        &speeds,
        req.cruise_altitude_m,
    )
}

/// The ultimate gust cases: `n_ult = 1.5 (1 + Delta n)` times the configured
/// additional safety factor, applied to the whole design mass like the
/// manoeuvre cases (a positive gust accelerates every mass by the same
/// increment).
fn gust_load_cases(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
) -> Vec<LoadCase> {
    let Some(gusts) = discrete_gusts(wsg, req) else {
        return Vec::new();
    };
    let weight_n = req.mtow_kg * req.gravity_m_s2;
    gusts
        .iter()
        .map(|g| {
            let n_ult = FACTOR_OF_SAFETY * g.limit_load_factor() * cfg.additional_safety_factor;
            LoadCase {
                name: g.name,
                load_factor: n_ult,
                total_force_n: n_ult * weight_n / 2.0,
            }
        })
        .collect()
}

/// The ultimate cases the product law sizes to.
pub(super) fn strength_cases(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    set: CaseSet,
) -> Vec<LoadCase> {
    let mut cases = loads::load_cases(req, cfg.additional_safety_factor).to_vec();
    if set.gust {
        cases.extend(gust_load_cases(wsg, cfg, req));
    }
    cases
}

/// The lower-cover 1 g tensile stress limit, Pa, when it applies.
///
/// `None` for a composite cap, an undeclared limit, or a switched-off case.
pub(super) fn fatigue_limit_pa(
    cfg: &StructuresConfig,
    cap: &MaterialSpec,
    set: CaseSet,
) -> Option<f64> {
    let limit = cfg.lower_cover_fatigue_stress_1g_pa;
    (set.fatigue && limit.is_finite() && limit > 0.0 && cap.category != "composite")
        .then_some(limit)
}

/// One case's loads on the sizing stations.
pub(super) struct CaseLoads {
    pub(super) case: LoadCase,
    /// Net running load, N/m.
    pub(super) q_net: Vec<f64>,
    /// Relieved bending moment, N m.
    pub(super) moment: Vec<f64>,
    /// Relieved vertical shear plus the web-equivalent of the torque, N.
    pub(super) shear_equivalent: Vec<f64>,
}

/// Net load and relieved moment of an aerodynamic running load, signed throughout: a push-down
/// case has a negative load and load factor, so the relief term `-n g m` adds
/// back and reduces the magnitude.
pub(super) fn relieved_from_aero(
    y: &[f64],
    q_aero: Vec<f64>,
    case: &LoadCase,
    gravity_m_s2: f64,
    relief: &WingInertiaRelief,
) -> (Vec<f64>, Vec<f64>) {
    if relief.is_empty() {
        let (_, moment) = loads::cantilever_shear_moment(y, &q_aero);
        return (q_aero, moment);
    }
    let q_net = loads::net_distributed_load(
        &q_aero,
        case.load_factor,
        gravity_m_s2,
        &relief.running_mass_kg_m,
    );
    let (_, mut moment) = loads::cantilever_shear_moment(y, &q_net);
    loads::apply_point_mass_relief(
        y,
        &mut moment,
        case.load_factor,
        gravity_m_s2,
        &relief.point_masses_kg,
    );
    (q_net, moment)
}

/// Depth of every spar at `eta`, m, zero outboard of the break for a
/// partial-span spar (it carries no moment, shear or mass there).
pub(super) fn spar_depths_m(wsg: &WingStructureGeometry, eta: f64) -> Vec<f64> {
    wsg.spar_fracs
        .iter()
        .zip(&wsg.spar_full_span)
        .map(|(&fraction, &full_span)| {
            if !full_span && eta > wsg.break_eta + 1e-9 {
                0.0
            } else {
                wsg.spar_height(eta, fraction)
            }
        })
        .collect()
}

/// Loads of every strength case on the stations `y`, with the torque of each
/// folded into the shear that sizes the webs.
pub(super) fn case_loads(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    set: CaseSet,
    y: &[f64],
    relief: &WingInertiaRelief,
) -> Vec<CaseLoads> {
    let n = y.len();
    let eta: Vec<f64> = y.iter().map(|&yi| yi / wsg.semi_span).collect();
    let depths: Vec<Vec<f64>> = eta.iter().map(|&e| spar_depths_m(wsg, e)).collect();
    let depth_sum: Vec<f64> = depths.iter().map(|d| d.iter().sum::<f64>()).collect();
    let (front, rear) = crate::sizing::box_chord_band(wsg);
    let axis_fraction = 0.5 * (front + rear);
    let area: Vec<f64> = (0..n)
        .map(|j| torsion::enclosed_area_m2(wsg.local_chord(eta[j]), &wsg.spar_fracs, &depths[j]))
        .collect();
    let chord: Vec<f64> = eta.iter().map(|&e| wsg.local_chord(e)).collect();
    let x_quarter: Vec<f64> = eta
        .iter()
        .zip(&chord)
        .map(|(&e, &c)| wsg.x_le(e) + 0.25 * c)
        .collect();
    let base = strength_cases(wsg, cfg, req, set);
    torque_cases::variants(wsg, cfg, req, y, relief, base, set)
        .into_iter()
        .map(|variant| {
            let case = variant.case;
            let extra_total = variant
                .extra_lift
                .as_ref()
                .map_or(0.0, |(lift, _)| trapezoid(lift, y));
            let base_total = if variant.extra_inside_total {
                (case.total_force_n - extra_total).max(0.0)
            } else {
                case.total_force_n
            };
            let q_base = loads::elliptic_distributed_load(y, wsg.semi_span, base_total);
            let mut q_aero = q_base.clone();
            if let Some((lift, _)) = &variant.extra_lift {
                for (q, extra) in q_aero.iter_mut().zip(lift) {
                    *q += extra;
                }
            }
            let (q_net, moment) = relieved_from_aero(y, q_aero, &case, req.gravity_m_s2, relief);
            let (mut shear, _) = loads::cantilever_shear_moment(y, &q_net);
            loads::apply_point_mass_shear_relief(
                y,
                &mut shear,
                case.load_factor,
                req.gravity_m_s2,
                &relief.point_masses_kg,
            );
            let torque = if set.torque {
                let mut loads = torsion::TorqueLoads {
                    lifts: vec![(&q_base, &x_quarter)],
                    couple_n_per_m: variant.couple.as_deref(),
                    couples: variant.point_couples.clone(),
                    ..Default::default()
                };
                if let Some((lift, x)) = &variant.extra_lift {
                    loads.lifts.push((lift, x));
                }
                // Engine inertia n g m down at its centre of gravity.
                for engine in &relief.torque.sources.engines {
                    let x_cg = wsg.x_le(engine.y_m / wsg.semi_span) - engine.cg_ahead_of_le_m;
                    loads.forces.push((
                        engine.y_m,
                        -case.load_factor * req.gravity_m_s2 * engine.mass_kg,
                        x_cg,
                    ));
                }
                torsion::torque_of_loads_nm(wsg, y, axis_fraction, &loads)
            } else {
                vec![0.0; n]
            };
            // The worst web carries the uniform shear of the vertical load
            // plus the torsional shear flow, q_V + q_T with q_V = V / sum(h),
            // so the equivalent uniform-web shear is V + q_T sum(h).
            let shear_equivalent = (0..n)
                .map(|j| {
                    shear[j].abs()
                        + torsion::bredt_shear_flow_n_per_m(torque[j], area[j]) * depth_sum[j]
                })
                .collect();
            CaseLoads {
                case,
                q_net,
                moment,
                shear_equivalent,
            }
        })
        .collect()
}

/// Relative guard on the design moment and shear of the sizing.
///
/// The sizing is exact to a few units in the last place at the strength
/// boundary, and the relieved-load fixed point iterates it against the previous
/// pass's running mass, so a settled iteration still differs from its
/// predecessor by 1e-14 of the box mass. That is enough to leave the final
/// section a few 1e-14 short of its own final loads, which the strict final
/// check refuses. Sizing to 1e-11 above the demand puts the settled section
/// strictly inside the check by three orders of magnitude of that residual; it
/// is a numerical guard on the fixed point, not a margin (it changes a box mass
/// by 1e-11 of itself).
const RELIEF_RESIDUAL_GUARD: f64 = 1.0e-11;

/// The per-station demand the product law sizes to.
pub(super) struct StationEnvelope {
    /// Design bending moment, N m.
    pub(super) moment_nm: Vec<f64>,
    /// Equivalent design web shear, N.
    pub(super) shear_n: Vec<f64>,
    /// Relieved 1 g level-flight moment, N m, for the fatigue limit.
    pub(super) level_moment_nm: Vec<f64>,
}

/// Reduce the cases to the station envelope.
pub(super) fn station_envelope(cases: &[CaseLoads]) -> StationEnvelope {
    let n = cases.first().map_or(0, |c| c.moment.len());
    let mut moment_nm = vec![0.0_f64; n];
    let mut shear_n = vec![0.0_f64; n];
    let mut level_moment_nm = vec![0.0_f64; n];
    for loaded in cases {
        for j in 0..n {
            moment_nm[j] = moment_nm[j].max(loaded.moment[j].abs() * (1.0 + RELIEF_RESIDUAL_GUARD));
            shear_n[j] = shear_n[j].max(loaded.shear_equivalent[j] * (1.0 + RELIEF_RESIDUAL_GUARD));
            if loaded.case.name == "level" {
                level_moment_nm[j] = loaded.moment[j].abs();
            }
        }
    }
    StationEnvelope {
        moment_nm,
        shear_n,
        level_moment_nm,
    }
}

/// The name of the case that sets the design moment at each station.
pub fn governing_cases(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    set: CaseSet,
    y: &[f64],
    relief: &WingInertiaRelief,
) -> Vec<&'static str> {
    let cases = case_loads(wsg, cfg, req, set, y, relief);
    (0..y.len())
        .map(|j| {
            let mut best = ("", -1.0_f64);
            for loaded in &cases {
                let moment = loaded.moment[j].abs();
                if moment > best.1 {
                    best = (loaded.case.name, moment);
                }
            }
            best.0
        })
        .collect()
}

/// One strength case's demand on the stations, for reporting.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseDemand {
    /// Case name.
    pub name: &'static str,
    /// Ultimate load factor of the case.
    pub load_factor: f64,
    /// Relieved bending moment, N m.
    pub moment_nm: Vec<f64>,
    /// Equivalent web shear (vertical shear plus torque shear flow), N.
    pub shear_equivalent_n: Vec<f64>,
}

/// Every strength case's moment and equivalent shear on the stations `y`.
pub fn case_demands(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    set: CaseSet,
    y: &[f64],
    relief: &WingInertiaRelief,
) -> Vec<CaseDemand> {
    case_loads(wsg, cfg, req, set, y, relief)
        .into_iter()
        .map(|c| CaseDemand {
            name: c.case.name,
            load_factor: c.case.load_factor,
            moment_nm: c.moment,
            shear_equivalent_n: c.shear_equivalent,
        })
        .collect()
}

/// Every `(moment, shear)` demand pair of the envelope, case by case, each
/// strength case at its own relieved moment and equivalent shear.
pub(super) fn demand_pairs(cases: &[CaseLoads]) -> Vec<(Vec<f64>, Vec<f64>)> {
    cases
        .iter()
        .map(|c| (c.moment.clone(), c.shear_equivalent.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::wing_mounted_relief;
    use crate::sizing::{size_wingbox_with_case_set, sizing_stations, WingboxSizing};
    use alas_config::{materials, presets, AlasConfig};
    use alas_geom::builder::AircraftBuilder;

    struct Probe {
        config: AlasConfig,
        wsg: WingStructureGeometry,
    }

    fn probe(name: &str) -> Probe {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .unwrap_or_else(|error| panic!("{name} loads: {error}"));
        let design = presets::get(name)
            .unwrap_or_else(|error| panic!("{name}: {error}"))
            .design_vector;
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap_or_else(|error| panic!("{name} builds: {error}"));
        let wing = plane
            .wings
            .iter()
            .find(|wing| wing.name == "Main Wing")
            .unwrap_or_else(|| panic!("{name} has a main wing"));
        let (root, tip) = (wing.xsecs.first().unwrap(), wing.xsecs.last().unwrap());
        let (fractions, full_span) = config.structures.resolved_spars();
        let wsg = WingStructureGeometry::new(
            &design,
            &config.geometry.wing,
            &root.airfoil,
            &tip.airfoil,
            &fractions,
            Some(&full_span),
        )
        .unwrap_or_else(|error| panic!("{name} wing builds: {error}"));
        Probe { config, wsg }
    }

    fn material(name: &str) -> &'static MaterialSpec {
        materials::get(name).unwrap_or_else(|error| panic!("{name}: {error}"))
    }

    fn point_masses(probe: &Probe, req: &DesignRequirements) -> Vec<(f64, f64)> {
        wing_mounted_relief(&probe.config.geometry.engine, &probe.config.mass_model, req)
            .point_masses_kg
    }

    fn size(probe: &Probe, req: &DesignRequirements, set: CaseSet) -> WingboxSizing {
        let st = &probe.config.structures;
        size_wingbox_with_case_set(
            &probe.wsg,
            st,
            req,
            material(&st.skin_material),
            material(&st.spar_web_material),
            material(&st.spar_cap_material),
            material(&st.rib_material),
            None,
            &point_masses(probe, req),
            set,
        )
    }

    fn only(gust: bool, torque: bool, fatigue: bool) -> CaseSet {
        CaseSet {
            gust,
            torque,
            fatigue,
            aileron: false,
        }
    }

    #[test]
    fn a_jet_transport_at_gross_mass_is_not_sized_by_the_gust() {
        // Torenbeek (2013), p. 325: manoeuvre loads are critical for a high
        // wing loading. The gust cases are evaluated and change nothing.
        let probe = probe("A320-200");
        let req = &probe.config.requirements;
        let base = size(&probe, req, CaseSet::MANOEUVRE_ONLY);
        let gust = size(&probe, req, only(true, false, false));
        assert_eq!(gust.sizing_load_case, "pull-up");
        assert!((gust.total_mass_kg - base.total_mass_kg).abs() <= 1.0e-9 * base.total_mass_kg);
    }

    #[test]
    fn a_light_wing_loading_makes_the_gust_the_sizing_case() {
        // The same wing at 30 t has a wing loading 2.6 times lighter; the gust
        // load factor rises as 1 / (W/S) and passes the 2.5 manoeuvre limit.
        let probe = probe("A320-200");
        let mut req = probe.config.requirements.clone();
        req.mtow_kg = 30_000.0;
        let base = size(&probe, &req, CaseSet::MANOEUVRE_ONLY);
        let gust = size(&probe, &req, only(true, false, false));
        assert_eq!(gust.sizing_load_case, "gust-VC");
        assert!(gust.total_mass_kg > base.total_mass_kg);
        let gusts = discrete_gusts(&probe.wsg, &req).expect("a gust is defined");
        assert!(FACTOR_OF_SAFETY * gusts[0].limit_load_factor() > req.ultimate_load_factor);
    }

    #[test]
    fn the_torque_case_thickens_the_webs_and_adds_mass() {
        let probe = probe("A320-200");
        let req = &probe.config.requirements;
        let base = size(&probe, req, CaseSet::MANOEUVRE_ONLY);
        let torque = size(&probe, req, only(false, true, false));
        assert!(torque.spars[0].t_web > base.spars[0].t_web);
        assert!(torque.total_mass_kg > base.total_mass_kg);
        // The torque of the load offset from the axis is of the order of the
        // bending moment over the aspect ratio (Torenbeek, p. 334), so it
        // stays a moderate addition to the web shear flow.
        assert!(torque.total_mass_kg < 1.25 * base.total_mass_kg);
    }

    #[test]
    fn a_load_near_the_box_axis_carries_much_less_torque_than_one_at_the_quarter_chord() {
        // The residual of the on-axis case is the kink of the planform: the
        // local axis sweep differs from the sweep the outboard load sits on.
        let probe = probe("A320-200");
        let st = &probe.config.structures;
        let y = sizing_stations(&probe.wsg, st);
        let req = &probe.config.requirements;
        let case = loads::load_cases(req, 1.0)[0];
        let q = loads::elliptic_distributed_load(&y, probe.wsg.semi_span, case.total_force_n);
        let (front, rear) = crate::sizing::box_chord_band(&probe.wsg);
        let offset = torsion::aerodynamic_torque_nm(&probe.wsg, &y, &q, 0.5 * (front + rear));
        let on_axis = torsion::aerodynamic_torque_nm(&probe.wsg, &y, &q, 0.25);
        assert!(
            offset[0].abs() > 2.0 * on_axis[0].abs(),
            "{} {}",
            offset[0],
            on_axis[0]
        );
        // No outboard load, no torque at the tip.
        assert_eq!(offset[y.len() - 1], 0.0);
    }

    #[test]
    fn the_fatigue_limit_raises_a_short_haul_lower_cover_and_not_a_long_haul_one() {
        // A320: 75 MPa at 1 g is below 380 MPa / 3.75 = 101 MPa. A340: 110 MPa
        // is above it, so the ultimate case still governs.
        let a320 = probe("A320-200");
        assert_eq!(
            a320.config.structures.lower_cover_fatigue_stress_1g_pa,
            75.0e6
        );
        let req = &a320.config.requirements;
        let without = size(&a320, req, CaseSet::MANOEUVRE_ONLY);
        let with = size(&a320, req, only(false, false, true));
        let extra: f64 = with
            .spars
            .iter()
            .flat_map(|spar| spar.lower_extra_area.iter())
            .sum();
        assert!(extra > 0.0);
        assert!(with.mass_breakdown_kg.spar_caps > without.mass_breakdown_kg.spar_caps);
        // Only the lower flange grows: the symmetric flange areas of the
        // strength section stay within the (tiny) relief feedback of the added
        // mass.
        for (a, b) in with.spars.iter().zip(&without.spars) {
            for (x, y) in a.a_cap.iter().zip(&b.a_cap) {
                assert!(
                    *x <= *y * 1.02 + 1.0e-12,
                    "upper/symmetric flange grew: {x} {y}"
                );
            }
        }

        let a340 = probe("A340-300");
        assert_eq!(
            a340.config.structures.lower_cover_fatigue_stress_1g_pa,
            110.0e6
        );
        let req = &a340.config.requirements;
        let without = size(&a340, req, CaseSet::MANOEUVRE_ONLY);
        let with = size(&a340, req, only(false, false, true));
        assert!(
            (with.total_mass_kg - without.total_mass_kg).abs() <= 1.0e-9 * without.total_mass_kg
        );
    }

    #[test]
    fn the_fatigue_sized_lower_cover_carries_the_one_g_stress_limit_at_the_root() {
        let probe = probe("A320-200");
        let st = &probe.config.structures;
        let req = &probe.config.requirements;
        let sizing = size(&probe, req, only(false, false, true));
        let (skin, web, cap) = (
            material(&st.skin_material),
            material(&st.spar_web_material),
            material(&st.spar_cap_material),
        );
        let (front, rear) = crate::sizing::box_chord_band(&probe.wsg);
        let mut running = crate::tanks::integral_fuel_running_mass_kg_m(
            &probe.wsg,
            &sizing.y_stations,
            front,
            rear,
        );
        let own = crate::sizing::box_running_mass_kg_m(&sizing, skin, web, cap, front, rear);
        for (mass, own) in running.iter_mut().zip(own) {
            *mass += own;
        }
        let relief = WingInertiaRelief {
            running_mass_kg_m: running,
            point_masses_kg: point_masses(&probe, req),
            ..Default::default()
        };
        let cases = case_loads(
            &probe.wsg,
            st,
            req,
            only(false, false, true),
            &sizing.y_stations,
            &relief,
        );
        let level = cases
            .iter()
            .find(|c| c.case.name == "level")
            .expect("1 g case");
        // Stress of the lower fibre of the asymmetric section.
        let ea = crate::sizing::fatigue::station_ea_for_tests(&sizing, 0, skin, web, cap);
        let ei = crate::sizing::section::station_ei(&sizing, 0, skin, web, cap);
        let (mut first, mut second, mut added) = (0.0, 0.0, 0.0);
        for spar in &sizing.spars {
            let arm = 0.5 * (spar.h[0] - spar.t_cap[0]);
            first += cap.e_pa * spar.lower_extra_area[0] * arm;
            second += cap.e_pa * spar.lower_extra_area[0] * arm * arm;
            added += cap.e_pa * spar.lower_extra_area[0];
        }
        let shift = first / (ea + added);
        let inertia = ei + second - first * first / (ea + added);
        let half = sizing.spars[0].h[0] * 0.5;
        let stress = cap.e_pa * level.moment[0].abs() * (half - shift) / inertia;
        let limit = st.lower_cover_fatigue_stress_1g_pa;
        // The check relief is the geometric fuel estimate the sizing also
        // used, so the root sits at the limit within the fixed-point tolerance.
        assert!(
            stress <= limit * 1.002,
            "1 g stress {stress} Pa above {limit} Pa"
        );
        assert!(
            stress >= 0.97 * limit,
            "1 g stress {stress} Pa is not at {limit} Pa"
        );
    }

    #[test]
    fn a_composite_cover_ignores_the_fatigue_limit() {
        let probe = probe("B787-9");
        let mut st = probe.config.structures.clone();
        st.lower_cover_fatigue_stress_1g_pa = 75.0e6;
        let cap = material(&st.spar_cap_material);
        assert_eq!(cap.category, "composite");
        assert!(fatigue_limit_pa(&st, cap, CaseSet::ALL).is_none());
        let metal = material("Al 7075-T6");
        assert_eq!(fatigue_limit_pa(&st, metal, CaseSet::ALL), Some(75.0e6));
        assert!(fatigue_limit_pa(&st, metal, CaseSet::MANOEUVRE_ONLY).is_none());
    }

    #[test]
    fn the_governing_case_is_the_pull_up_on_a_jet_transport_at_gross_mass() {
        let a320 = probe("A320-200");
        let st = &a320.config.structures;
        let req = &a320.config.requirements;
        let y = sizing_stations(&a320.wsg, st);
        let relief = WingInertiaRelief::default();
        let names = governing_cases(&a320.wsg, st, req, CaseSet::ALL, &y, &relief);
        assert!(names.iter().all(|name| *name == "pull-up"));
    }

    #[test]
    fn each_station_has_its_own_web_gauge_larger_inboard_than_outboard() {
        let probe = probe("A320-200");
        let req = &probe.config.requirements;
        let sizing = size(&probe, req, CaseSet::ALL);
        let gauges = &sizing.spars[0].t_web;
        let minimum = probe.config.structures.t_web_min_m;
        assert!(gauges.iter().all(|g| *g >= minimum * (1.0 - 1.0e-12)));
        assert!(gauges[0] > 2.0 * minimum, "root gauge {}", gauges[0]);
        assert!((gauges[gauges.len() - 1] - minimum).abs() <= 1.0e-9 * minimum);
        // The gauge follows the local shear and panel depth: it is not uniform        // and the inboard half carries more of it than the outboard half        // (it is not monotone: spar depth and rib pitch change along the span).        let half = gauges.len() / 2;        let inboard: f64 = gauges[..half].iter().sum();        let outboard: f64 = gauges[half..].iter().sum();        assert!(inboard > 1.5 * outboard, "{inboard} {outboard}");
    }

    fn torque_relief(probe: &Probe, req: &DesignRequirements) -> (Vec<f64>, WingInertiaRelief) {
        let st = &probe.config.structures;
        let y = sizing_stations(&probe.wsg, st);
        let sources = crate::torque_sources::TorqueSources::from_config(&probe.config, req);
        let relief = WingInertiaRelief {
            torque: crate::torque_sources::PreparedTorque::new(&sources, &probe.wsg, req, &y),
            ..Default::default()
        };
        (y, relief)
    }

    fn demand<'a>(demands: &'a [CaseDemand], name: &str) -> &'a CaseDemand {
        demands
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("no case {name}"))
    }

    #[test]
    fn the_product_set_carries_the_torque_cases_and_leaves_the_aileron_out() {
        let probe = probe("A320-200");
        let req = &probe.config.requirements;
        let (y, relief) = torque_relief(&probe, req);
        let st = &probe.config.structures;
        let with = case_demands(&probe.wsg, st, req, CaseSet::WITH_AILERON, &y, &relief);
        let product = case_demands(&probe.wsg, st, req, CaseSet::ALL, &y, &relief);
        for name in [
            "pull-up+Cm0-VD",
            "push-down+Cm0-VC",
            "level-VD+Cm0",
            "flaps-VF",
            "thrust-TO",
        ] {
            demand(&product, name);
        }
        assert!(product.iter().all(|d| !d.name.starts_with("aileron")));
        assert_eq!(
            with.iter()
                .filter(|d| d.name.starts_with("aileron"))
                .count(),
            12
        );
        // Manoeuvre-only has no torque variants at all.
        let base = case_demands(&probe.wsg, st, req, CaseSet::MANOEUVRE_ONLY, &y, &relief);
        assert_eq!(base.len(), 3);
    }

    #[test]
    fn the_section_moment_is_nose_down_and_a_few_tenths_of_a_percent_of_chord_scale() {
        // Supercritical sections: Cm0 about the quarter chord is negative
        // (NeuralFoil, incompressible and attached) and of the order -0.1.
        let probe = probe("A320-200");
        let (_, relief) = torque_relief(&probe, &probe.config.requirements);
        let dive = relief.torque.dive.as_ref().expect("dive table");
        assert!(
            dive.cm0.iter().all(|&cm| (-0.25..-0.02).contains(&cm)),
            "{:?}",
            dive.cm0
        );
        let flap = relief.torque.flap.as_ref().expect("flap table");
        assert!(flap.cl_alpha.iter().all(|&a| (5.0..9.0).contains(&a)));
        assert!(dive.q_pa > flap.q_pa);
    }

    #[test]
    fn engine_inertia_ahead_of_the_axis_relieves_the_nose_up_torque_inboard_of_the_pylon() {
        // Weight acts down ahead of the elastic axis: nose-down, against the
        // nose-up torque of the lift, so the shear that carries the torque
        // drops inboard of the engine and is unchanged outboard of it.
        let probe = probe("A320-200");
        let req = &probe.config.requirements;
        let st = &probe.config.structures;
        let (y, with) = torque_relief(&probe, req);
        let mut without = with.clone();
        without.torque.sources.engines.clear();
        let a = case_demands(&probe.wsg, st, req, CaseSet::ALL, &y, &with);
        let b = case_demands(&probe.wsg, st, req, CaseSet::ALL, &y, &without);
        let engine_y = with.torque.sources.engines[0].y_m;
        let inboard = y.iter().position(|&v| v > 0.5 * engine_y).unwrap();
        let outboard = y.iter().position(|&v| v > engine_y + 0.5).unwrap();
        let (pa, pb) = (demand(&a, "pull-up"), demand(&b, "pull-up"));
        assert_ne!(
            pa.shear_equivalent_n[inboard],
            pb.shear_equivalent_n[inboard]
        );
        assert!((pa.shear_equivalent_n[outboard] - pb.shear_equivalent_n[outboard]).abs() < 1e-6);
        // The push-down case reverses the sign of the lift torque and of the
        // engine inertia together, so the relief is the same sign there too.
        assert_eq!(pa.moment_nm, pb.moment_nm);
    }

    #[test]
    fn the_flap_case_keeps_total_lift_and_the_steady_roll_has_no_net_rolling_moment() {
        let probe = probe("A320-200");
        let req = &probe.config.requirements;
        let st = &probe.config.structures;
        let (y, relief) = torque_relief(&probe, req);
        let base: Vec<LoadCase> = strength_cases(&probe.wsg, st, req, CaseSet::WITH_AILERON);
        let variants = torque_cases::variants(
            &probe.wsg,
            st,
            req,
            &y,
            &relief,
            base,
            CaseSet::WITH_AILERON,
        );
        let flap = variants.iter().find(|v| v.case.name == "flaps-VF").unwrap();
        let (lift, _) = flap.extra_lift.as_ref().unwrap();
        assert!(trapezoid(lift, &y) > 0.0);
        // n = 2.0 at the ultimate factor, on the whole aeroplane.
        let expected = 2.0 * 1.5 * req.mtow_kg * req.gravity_m_s2 / 2.0;
        assert!((flap.case.total_force_n - expected).abs() < 1e-6 * expected);
        // Steady roll at V_C: sum(y dL) = 0 on the semi-wing.
        let steady = variants
            .iter()
            .find(|v| v.case.name == "aileron-VC-down-n0")
            .unwrap();
        let (dl, _) = steady.extra_lift.as_ref().unwrap();
        let arm: Vec<f64> = dl.iter().zip(&y).map(|(l, &yy)| l * yy).collect();
        let sudden = variants
            .iter()
            .find(|v| v.case.name == "aileron-VA-down-n0")
            .unwrap();
        let (da, _) = sudden.extra_lift.as_ref().unwrap();
        let sudden_moment = trapezoid(
            &da.iter().zip(&y).map(|(l, &yy)| l * yy).collect::<Vec<_>>(),
            &y,
        );
        let steady_moment = trapezoid(&arm, &y);
        assert!(sudden_moment > 0.0 && steady_moment.abs() < 1e-9 * sudden_moment);
    }

    #[test]
    fn the_aileron_cases_raise_the_box_and_the_product_set_does_not_carry_them() {
        let probe = probe("A320-200");
        let req = &probe.config.requirements;
        let st = &probe.config.structures;
        let sources = crate::torque_sources::TorqueSources::from_config(&probe.config, req);
        let run = |set| {
            crate::sizing::size_wingbox_with_torque_sources(
                &probe.wsg,
                st,
                req,
                material(&st.skin_material),
                material(&st.spar_web_material),
                material(&st.spar_cap_material),
                material(&st.rib_material),
                None,
                &point_masses(&probe, req),
                &sources,
                set,
            )
        };
        let without = run(CaseSet::ALL);
        let rolling = run(CaseSet::WITH_AILERON);
        assert!(rolling.total_mass_kg > without.total_mass_kg);
        assert_eq!(without.sizing_load_case, "pull-up");
    }

    #[test]
    fn the_aileron_schedule_matches_the_roll_rates_of_cs_25_349_a() {
        let probe = probe("A320-200");
        let req = &probe.config.requirements;
        let (y, relief) = torque_relief(&probe, req);
        let ail = relief.torque.sources.aileron.expect("an aileron");
        let (v_a, v_c, v_d) = relief.torque.speeds_eas.expect("speeds");
        let eta: Vec<f64> = y.iter().map(|&v| v / probe.wsg.semi_span).collect();
        let chord: Vec<f64> = eta.iter().map(|&e| probe.wsg.local_chord(e)).collect();
        let s = torque_cases::aileron_schedule(&ail, (v_a, v_c, v_d), &y, &chord, &eta);
        // (a)(2) the stop at V_A; (a)(3) the V_A roll rate at V_C; (a)(4) a third at V_D.
        assert!((s.delta_rad[0] - 25.0_f64.to_radians()).abs() < 1e-12);
        assert!((s.roll_rate_rad_s[1] / s.roll_rate_rad_s[0] - 1.0).abs() < 1e-6);
        assert!((s.roll_rate_rad_s[2] / s.roll_rate_rad_s[0] - 1.0 / 3.0).abs() < 1e-6);
        assert!(s.delta_rad[1] < s.delta_rad[0] && s.delta_rad[2] < s.delta_rad[1]);
        assert!(v_a <= v_c && v_c < v_d);
        // A transport rolls at tens of degrees per second at V_A with the stop.
        let rate_deg_s = s.roll_rate_rad_s[0].to_degrees();
        assert!((5.0..80.0).contains(&rate_deg_s), "{rate_deg_s}");
    }
}
