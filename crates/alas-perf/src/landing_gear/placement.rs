// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Longitudinal placement of a uniform main-gear group from static moments.
//!
//! X is aft-positive in the aircraft body frame; Z is up. All lengths are
//! metres and masses kilograms. Gear mass, loaded CG height and all other
//! mass moments are held fixed while the main group translates along X.
//! Installation bounds are supplied by the caller, never inferred from CG.
//! A forward CG boundary that moves with the gear (rotation authority) is
//! honoured when the caller supplies its affine coefficients; tire strength
//! and flight stability require separate checks after the complete
//! candidate ledger has been rebuilt.

use super::geometry::{tail_scrape_angle_deg, tip_back_angle_deg, FuselageLowerPoint};

/// One loading state's mass moments while the main gear translates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GearPlacementState {
    /// Total loaded mass, including the main gear, kg.
    pub mass_kg: f64,
    /// Aggregate mass translated to the proposed main-gear station, kg.
    pub main_gear_mass_kg: f64,
    /// Longitudinal moment excluding `main_gear_mass_kg * x_main`, kg m.
    /// The fixed nose-gear mass and its moment remain included.
    pub moment_without_main_gear_kg_m: f64,
    /// This state's loaded CG height above the supplied ground plane, m.
    pub cg_height_m: f64,
    /// Minimum static nose reaction divided by this state's weight.
    pub minimum_nose_fraction: f64,
    /// Maximum static nose reaction divided by this state's weight.
    pub maximum_nose_fraction: f64,
    /// A forward CG boundary of this state that moves with the main gear,
    /// such as the nose-wheel liftoff boundary at rotation; `None` when the
    /// state's phase has none.
    pub forward_cg_boundary: Option<ForwardCgBoundary>,
}

/// A forward centre-of-gravity boundary affine in the main-gear station:
/// the state is admissible when `x_cg >= intercept_m + slope * x_main`.
///
/// A moment balance about the main-gear contact (nose-wheel liftoff) is
/// affine in the contact station for fixed aerodynamic, thrust and inertia
/// terms; the caller measures both coefficients from its own boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForwardCgBoundary {
    /// Boundary station at `x_main = 0` in the body frame, m.
    pub intercept_m: f64,
    /// Boundary movement per metre of main-gear translation.
    pub slope: f64,
}

/// Geometry and loading cases of a proposed uniform main-gear placement.
#[derive(Debug, Clone, Copy)]
pub struct MainGearPlacementInput<'a> {
    /// Fixed nose-gear contact station in the body frame, m.
    pub nose_gear_x_m: f64,
    /// Preferred main-gear station; the nearest feasible station is returned.
    pub requested_main_gear_x_m: f64,
    /// Inclusive installation interval, m, supplied from attachment geometry.
    /// A fuselage-length interval alone does not establish an attachment bay.
    pub installation_bounds_m: (f64, f64),
    /// Minimum tip-back angle, degrees, from the vertical at ground contact.
    pub minimum_tip_back_deg: f64,
    /// Static ground-plane Z in the same body frame as the contour, m.
    pub ground_z_m: f64,
    /// Lower fuselage contour, including its aft end. Every point must lie
    /// strictly above the ground plane. Points ahead of a proposed station
    /// do not set its scrape angle, matching `geometry::tail_scrape_angle_deg`.
    pub lower_contour: &'a [FuselageLowerPoint],
    /// Every loading state whose ground reactions and tip-back must pass.
    pub states: &'a [GearPlacementState],
}

/// An invalid placement input, distinct from a valid but infeasible layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementInputError {
    /// Geometry or station inputs are outside the finite physical domain.
    Geometry,
    /// A loading state has invalid mass, moment, height or reaction limits.
    State {
        /// Index in `MainGearPlacementInput::states`.
        index: usize,
    },
    /// Finite inputs produce an unrepresentable intermediate value.
    Arithmetic,
}

impl std::fmt::Display for PlacementInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Geometry => formatter.write_str("invalid main-gear placement geometry"),
            Self::State { index } => write!(formatter, "invalid gear loading state {index}"),
            Self::Arithmetic => formatter.write_str("non-finite gear placement arithmetic"),
        }
    }
}

impl std::error::Error for PlacementInputError {}

type Interval = (f64, f64);

/// Resolve the nearest main-gear station satisfying every supplied ground case.
///
/// Static nose reactions use `x_cg = (C + G*x_main)/M`, including the gear's
/// own first moment. Tip-back must clear both the configured minimum and the
/// scrape angle recomputed at the proposed station. A supplied
/// [`ForwardCgBoundary`] is met with the same moving CG. The contour comparison
/// can yield disjoint feasible intervals; none is discarded before choosing
/// the nearest station. Equal-distance choices take the forward station.
/// Returns `Ok(None)` when no representable feasible station exists.
///
/// # Errors
///
/// [`PlacementInputError`] for empty, non-finite or nonphysical input data.
pub fn solve_main_gear_station(
    input: &MainGearPlacementInput<'_>,
) -> Result<Option<f64>, PlacementInputError> {
    validate(input)?;
    let mut intervals = vec![(
        input.installation_bounds_m.0.max(input.nose_gear_x_m),
        input.installation_bounds_m.1,
    )];
    if intervals[0].0 > intervals[0].1 {
        return Ok(None);
    }
    for state in input.states {
        let fixed_moment_m = state.moment_without_main_gear_kg_m / state.mass_kg;
        let fixed_fraction = (state.mass_kg - state.main_gear_mass_kg) / state.mass_kg;
        let tip_offset_m = state.cg_height_m * input.minimum_tip_back_deg.to_radians().tan();
        clip_linear(
            &mut intervals,
            fixed_fraction,
            fixed_moment_m + tip_offset_m,
        )?;
        clip_linear(
            &mut intervals,
            fixed_fraction - state.minimum_nose_fraction,
            fixed_moment_m - state.minimum_nose_fraction * input.nose_gear_x_m,
        )?;
        clip_linear(
            &mut intervals,
            state.maximum_nose_fraction - fixed_fraction,
            state.maximum_nose_fraction * input.nose_gear_x_m - fixed_moment_m,
        )?;
        if let Some(boundary) = state.forward_cg_boundary {
            // (C + G x)/M >= a + b x.
            clip_linear(
                &mut intervals,
                (1.0 - fixed_fraction) - boundary.slope,
                boundary.intercept_m - fixed_moment_m,
            )?;
        }
        let scrape = scrape_intervals(input, state, fixed_moment_m, fixed_fraction)?;
        intervals = intersections(&intervals, &scrape);
        if intervals.is_empty() {
            return Ok(None);
        }
    }
    let mut nearest: Option<f64> = None;
    for interval in intervals {
        if let Some(station) = checked_nearest(input, interval) {
            if nearest.is_none_or(|best| {
                (station - input.requested_main_gear_x_m).abs()
                    < (best - input.requested_main_gear_x_m).abs()
            }) {
                nearest = Some(station);
            }
        }
    }
    Ok(nearest)
}

fn validate(input: &MainGearPlacementInput<'_>) -> Result<(), PlacementInputError> {
    let (lower, upper) = input.installation_bounds_m;
    if ![
        input.nose_gear_x_m,
        input.requested_main_gear_x_m,
        lower,
        upper,
        input.minimum_tip_back_deg,
        input.ground_z_m,
    ]
    .iter()
    .all(|value| value.is_finite())
        || lower > upper
        || !(0.0..90.0).contains(&input.minimum_tip_back_deg)
        || input.lower_contour.is_empty()
        || input.states.is_empty()
        || input.lower_contour.iter().any(|point| {
            !point.x_m.is_finite()
                || !point.z_bottom_m.is_finite()
                || point.z_bottom_m <= input.ground_z_m
        })
    {
        return Err(PlacementInputError::Geometry);
    }
    for (index, state) in input.states.iter().enumerate() {
        if ![
            state.mass_kg,
            state.main_gear_mass_kg,
            state.moment_without_main_gear_kg_m,
            state.cg_height_m,
            state.minimum_nose_fraction,
            state.maximum_nose_fraction,
        ]
        .iter()
        .all(|value| value.is_finite())
            || state.mass_kg <= 0.0
            || state.main_gear_mass_kg < 0.0
            || state.main_gear_mass_kg >= state.mass_kg
            || state.cg_height_m <= 0.0
            || state.minimum_nose_fraction < 0.0
            || state.maximum_nose_fraction > 1.0
            || state.minimum_nose_fraction > state.maximum_nose_fraction
            || state.forward_cg_boundary.is_some_and(|boundary| {
                !boundary.intercept_m.is_finite() || !boundary.slope.is_finite()
            })
        {
            return Err(PlacementInputError::State { index });
        }
    }
    Ok(())
}

/// Intersect with `coefficient * x >= limit`, including reversed/constant cases.
fn clip_linear(
    intervals: &mut Vec<Interval>,
    coefficient: f64,
    limit: f64,
) -> Result<(), PlacementInputError> {
    if !coefficient.is_finite() || !limit.is_finite() {
        return Err(PlacementInputError::Arithmetic);
    }
    if coefficient == 0.0 {
        if limit > 0.0 {
            intervals.clear();
        }
        return Ok(());
    }
    let boundary = limit / coefficient;
    if !boundary.is_finite() {
        return Err(PlacementInputError::Arithmetic);
    }
    intervals.retain_mut(|(lower, upper)| {
        if coefficient > 0.0 {
            *lower = lower.max(boundary);
        } else {
            *upper = upper.min(boundary);
        }
        *lower <= *upper
    });
    Ok(())
}

fn scrape_intervals(
    input: &MainGearPlacementInput<'_>,
    state: &GearPlacementState,
    fixed_moment_m: f64,
    fixed_fraction: f64,
) -> Result<Vec<Interval>, PlacementInputError> {
    let fixed_cg_m = fixed_moment_m / fixed_fraction;
    let mut intervals = Vec::new();
    for point in input.lower_contour {
        let distance_m = point.x_m - fixed_cg_m;
        let product_m2 = state.cg_height_m * (point.z_bottom_m - input.ground_z_m) / fixed_fraction;
        let discriminant = distance_m * distance_m - 4.0 * product_m2;
        if ![fixed_cg_m, distance_m, product_m2, discriminant]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(PlacementInputError::Arithmetic);
        }
        if distance_m <= 0.0 || discriminant < 0.0 {
            continue;
        }
        // (x - C/(M-G))*(p-x) >= M*h*(z_p-z_ground)/(M-G).
        // The smaller root's offset uses the conjugate to avoid subtraction
        // of nearly equal roots. Both roots lie strictly forward of p.
        let offset_m = 2.0 * product_m2 / (distance_m + discriminant.sqrt());
        intervals.push((fixed_cg_m + offset_m, point.x_m - offset_m));
    }
    Ok(merged(intervals))
}

fn merged(mut intervals: Vec<Interval>) -> Vec<Interval> {
    intervals.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut result: Vec<Interval> = Vec::new();
    for interval in intervals {
        if let Some(previous) = result.last_mut().filter(|last| interval.0 <= last.1) {
            previous.1 = previous.1.max(interval.1);
        } else {
            result.push(interval);
        }
    }
    result
}

fn intersections(left: &[Interval], right: &[Interval]) -> Vec<Interval> {
    let mut result = Vec::new();
    for first in left {
        for second in right {
            let interval = (first.0.max(second.0), first.1.min(second.1));
            if interval.0 <= interval.1 {
                result.push(interval);
            }
        }
    }
    merged(result)
}

fn feasible(input: &MainGearPlacementInput<'_>, station_m: f64) -> bool {
    let wheelbase_m = station_m - input.nose_gear_x_m;
    let Some(scrape_deg) = tail_scrape_angle_deg(input.lower_contour, station_m, input.ground_z_m)
    else {
        return false;
    };
    wheelbase_m > 0.0
        && input.states.iter().all(|state| {
            let cg_m = (state.moment_without_main_gear_kg_m + state.main_gear_mass_kg * station_m)
                / state.mass_kg;
            let nose_fraction = (station_m - cg_m) / wheelbase_m;
            let tip_deg = tip_back_angle_deg(station_m, cg_m, state.cg_height_m);
            nose_fraction >= state.minimum_nose_fraction
                && nose_fraction <= state.maximum_nose_fraction
                && tip_deg >= input.minimum_tip_back_deg.max(scrape_deg)
                && state.forward_cg_boundary.is_none_or(|boundary| {
                    cg_m >= boundary.intercept_m + boundary.slope * station_m
                })
        })
}

fn checked_nearest(input: &MainGearPlacementInput<'_>, (lower, upper): Interval) -> Option<f64> {
    let mut edge = input.requested_main_gear_x_m.clamp(lower, upper);
    if feasible(input, edge) {
        return Some(edge);
    }
    let mut inside = 0.5 * lower + 0.5 * upper;
    if !feasible(input, inside) {
        return None;
    }
    // Move a rounded boundary inward until adjacent representable stations
    // bracket the unchanged physical predicates; no tolerance is widened.
    loop {
        let middle = 0.5 * edge + 0.5 * inside;
        if middle == edge || middle == inside {
            return Some(inside);
        }
        if feasible(input, middle) {
            inside = middle;
        } else {
            edge = middle;
        }
    }
}

#[cfg(test)]
// Tests construct and independently check every solved fixture, so a
// failed unwrap is the assertion failing.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn state() -> GearPlacementState {
        GearPlacementState {
            mass_kg: 1_000.0,
            main_gear_mass_kg: 100.0,
            moment_without_main_gear_kg_m: 7_200.0,
            cg_height_m: 2.0,
            minimum_nose_fraction: 0.1,
            maximum_nose_fraction: 0.2,
            forward_cg_boundary: None,
        }
    }

    #[test]
    fn a_forward_boundary_moving_with_the_gear_caps_the_station_or_empties_it() {
        // x_cg = 7.2 + 0.1 x. A rotation-like boundary x_cg >= 6.3 + 0.2 x
        // holds for x <= 9 m, below the 10 m maximum-nose station.
        let states = [GearPlacementState {
            forward_cg_boundary: Some(ForwardCgBoundary {
                intercept_m: 6.3,
                slope: 0.2,
            }),
            ..state()
        }];
        let mut problem = input(&states, &CONTOUR);
        problem.requested_main_gear_x_m = 11.0;
        let station = solve_main_gear_station(&problem).unwrap().unwrap();
        assert!((station - 9.0).abs() < 16.0 * f64::EPSILON * station);
        assert!(feasible(&problem, station));
        assert!(!feasible(&problem, 9.0 + 1.0e-9));
        // x_cg >= 6.4 + 0.2 x needs x <= 8 m, but the 10 % nose minimum
        // needs x >= 8.875 m: no station satisfies both.
        let states = [GearPlacementState {
            forward_cg_boundary: Some(ForwardCgBoundary {
                intercept_m: 6.4,
                slope: 0.2,
            }),
            ..state()
        }];
        assert_eq!(solve_main_gear_station(&input(&states, &CONTOUR)), Ok(None));
    }

    fn input<'a>(
        states: &'a [GearPlacementState],
        contour: &'a [FuselageLowerPoint],
    ) -> MainGearPlacementInput<'a> {
        MainGearPlacementInput {
            nose_gear_x_m: 1.0,
            requested_main_gear_x_m: 8.0,
            installation_bounds_m: (6.0, 12.0),
            minimum_tip_back_deg: 15.0,
            ground_z_m: -1.0,
            lower_contour: contour,
            states,
        }
    }

    const CONTOUR: [FuselageLowerPoint; 1] = [FuselageLowerPoint {
        x_m: 20.0,
        z_bottom_m: 0.0,
    }];

    #[test]
    fn translated_gear_mass_is_included_in_both_static_reactions() {
        let states = [state()];
        let mut problem = input(&states, &CONTOUR);
        let station = solve_main_gear_station(&problem).unwrap().unwrap();
        // 900 kg at x=8 plus 100 kg at x: a 10% nose reaction gives
        // 1000*x - (7200 + 100*x) = 100*(x-1), hence x=8.875 m.
        assert!((station - 8.875).abs() < 16.0 * f64::EPSILON * station);
        let moment = 900.0 * 8.0 + 100.0 * station;
        let reaction = (1_000.0 * station - moment) / (station - 1.0);
        assert!(reaction >= 100.0);
        problem.requested_main_gear_x_m = 11.0;
        let upper = solve_main_gear_station(&problem).unwrap().unwrap();
        // The same moment equation at 20% nose reaction gives x=10 m.
        assert!((upper - 10.0).abs() < 16.0 * f64::EPSILON * upper);
        assert!(feasible(&problem, upper));
    }

    #[test]
    fn scrape_geometry_is_rechecked_and_can_admit_disjoint_station_intervals() {
        let states = [GearPlacementState {
            main_gear_mass_kg: 0.0,
            moment_without_main_gear_kg_m: 2_000.0,
            cg_height_m: 1.0,
            minimum_nose_fraction: 0.0,
            maximum_nose_fraction: 1.0,
            ..state()
        }];
        let contour = [
            FuselageLowerPoint {
                x_m: 10.0,
                z_bottom_m: 11.0,
            },
            FuselageLowerPoint {
                x_m: 20.0,
                z_bottom_m: 79.0,
            },
        ];
        let mut problem = input(&states, &contour);
        problem.installation_bounds_m = (3.0, 15.0);
        problem.minimum_tip_back_deg = 0.0;
        problem.requested_main_gear_x_m = 9.0;
        // (x-2)*(10-x)>=12 gives [4,8]; (x-2)*(20-x)>=80 gives [10,12].
        let station = solve_main_gear_station(&problem).unwrap().unwrap();
        assert_eq!(station, 8.0);
        assert!(!feasible(&problem, 9.0));
        problem.requested_main_gear_x_m = 11.0;
        assert_eq!(solve_main_gear_station(&problem), Ok(Some(11.0)));
        problem.installation_bounds_m = (8.5, 9.5);
        assert_eq!(solve_main_gear_station(&problem), Ok(None));
    }

    #[test]
    fn every_loading_state_and_supplied_installation_bounds_are_respected() {
        let states = [
            state(),
            GearPlacementState {
                moment_without_main_gear_kg_m: 7_400.0,
                ..state()
            },
        ];
        let mut problem = input(&states, &CONTOUR);
        let station = solve_main_gear_station(&problem).unwrap().unwrap();
        assert!((station - 9.125).abs() < 16.0 * f64::EPSILON * station);
        assert!(feasible(&problem, station));
        problem.installation_bounds_m = (6.0, 9.0);
        assert_eq!(solve_main_gear_station(&problem), Ok(None));
        problem.installation_bounds_m = (9.2, 9.4);
        assert_eq!(solve_main_gear_station(&problem), Ok(Some(9.2)));
    }

    #[test]
    fn placement_is_invariant_under_mass_scaling_and_body_frame_translation() {
        let states = [state()];
        let problem = input(&states, &CONTOUR);
        let original = solve_main_gear_station(&problem).unwrap().unwrap();
        let scaled = [GearPlacementState {
            mass_kg: 8_000.0,
            main_gear_mass_kg: 800.0,
            moment_without_main_gear_kg_m: 57_600.0,
            ..state()
        }];
        assert_eq!(
            solve_main_gear_station(&input(&scaled, &CONTOUR)),
            Ok(Some(original))
        );
        let translated = [GearPlacementState {
            moment_without_main_gear_kg_m: 7_200.0 + 900.0 * 100.0,
            ..state()
        }];
        let contour = [FuselageLowerPoint {
            x_m: 120.0,
            z_bottom_m: 25.0,
        }];
        let moved = MainGearPlacementInput {
            nose_gear_x_m: 101.0,
            requested_main_gear_x_m: 108.0,
            installation_bounds_m: (106.0, 112.0),
            ground_z_m: 24.0,
            states: &translated,
            lower_contour: &contour,
            ..problem
        };
        let result = solve_main_gear_station(&moved).unwrap().unwrap();
        assert!((result - original - 100.0).abs() < 32.0 * f64::EPSILON * result);
        assert!(feasible(&moved, result));
    }

    #[test]
    fn invalid_states_fail_and_degenerate_nose_coefficients_do_not_divide_by_zero() {
        let mut states = [state()];
        states[0].cg_height_m = f64::NAN;
        assert_eq!(
            solve_main_gear_station(&input(&states, &CONTOUR)),
            Err(PlacementInputError::State { index: 0 })
        );
        states[0] = GearPlacementState {
            maximum_nose_fraction: 0.9,
            ..state()
        };
        assert!(solve_main_gear_station(&input(&states, &CONTOUR))
            .unwrap()
            .is_some());
        states[0].main_gear_mass_kg = states[0].mass_kg;
        assert_eq!(
            solve_main_gear_station(&input(&states, &CONTOUR)),
            Err(PlacementInputError::State { index: 0 })
        );
        let mut problem = input(&states, &CONTOUR);
        problem.minimum_tip_back_deg = 90.0;
        assert_eq!(
            solve_main_gear_station(&problem),
            Err(PlacementInputError::Geometry)
        );
    }
}
