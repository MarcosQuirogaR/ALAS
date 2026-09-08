// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::f64::consts::PI;

use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;
use alas_math::linalg;
use thiserror::Error;

/// Resolution used for the thin-airfoil mean-camber integral.
///
/// A composite midpoint rule in the thin-airfoil angular coordinate avoids
/// the leading-edge clustering error of a uniform chordwise grid. The unit
/// tests bound the result against the analytic NACA 2412 camber-line integral.
const THIN_AIRFOIL_INTERVALS: usize = 512;

/// One section defining a symmetric half-wing lifting line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiftingLineSection {
    /// Distance from the centerline divided by the projected semispan.
    pub span_fraction: f64,
    /// Local chord in meters.
    pub chord_m: f64,
    /// Geometric incidence relative to the aircraft reference axis, radians.
    pub twist_rad: f64,
    /// Thin-airfoil zero-lift angle of the local section, radians.
    pub zero_lift_angle_rad: f64,
}

/// Geometry and discretization for one isolated symmetric lifting surface.
#[derive(Debug, Clone, PartialEq)]
pub struct FourierLiftingLineSurface {
    /// Human-readable surface name retained in result provenance.
    pub name: String,
    /// Full projected span in meters.
    pub span_m: f64,
    /// Full projected planform area in square meters.
    pub area_m2: f64,
    /// Two-dimensional section lift-curve slope, per radian.
    pub section_lift_slope_per_rad: f64,
    /// Number of retained odd harmonics and half-span collocation stations.
    pub harmonic_count: usize,
    /// Half-span definitions ordered from root (`0`) to tip (`1`).
    pub sections: Vec<LiftingLineSection>,
}

/// Loading at one positive-half-span collocation station.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiftingLineStationResult {
    /// Distance from the centerline divided by projected semispan.
    pub span_fraction: f64,
    /// Local section lift coefficient based on local chord.
    pub section_lift_coefficient: f64,
    /// Local induced angle from the Fourier wake solution, radians.
    pub induced_angle_rad: f64,
}

/// One isolated-surface Fourier lifting-line result.
#[derive(Debug, Clone, PartialEq)]
pub struct FourierLiftingLineResult {
    /// Surface name copied from the input model.
    pub surface_name: String,
    /// Geometric aircraft angle of attack, radians.
    pub alpha_rad: f64,
    /// Lift coefficient referenced to this surface's projected area.
    pub lift_coefficient: f64,
    /// Trefftz-plane induced-drag coefficient on the same area.
    pub induced_drag_coefficient: f64,
    /// Span efficiency, absent when lift or induced drag is zero.
    pub span_efficiency: Option<f64>,
    /// Odd-harmonic coefficients `A_1, A_3, ...` in circulation order.
    pub fourier_coefficients: Vec<f64>,
    /// Positive-half-span loading, from near tip to root.
    pub stations: Vec<LiftingLineStationResult>,
}

/// Independent lifting-line model of all symmetric aircraft surfaces.
#[derive(Debug, Clone, PartialEq)]
pub struct AircraftFourierLiftingLine {
    /// Aircraft coefficient reference area in square meters.
    pub reference_area_m2: f64,
    /// Isolated symmetric lifting surfaces included in the sum.
    pub surfaces: Vec<FourierLiftingLineSurface>,
}

/// One aircraft-level result, normalized to the aircraft reference area.
#[derive(Debug, Clone, PartialEq)]
pub struct AircraftLiftingLineResult {
    /// Geometric aircraft angle of attack, radians.
    pub alpha_rad: f64,
    /// Sum of isolated-surface lift on the aircraft reference area.
    pub lift_coefficient: f64,
    /// Sum of isolated-surface induced drag on the aircraft reference area.
    pub induced_drag_coefficient: f64,
    /// Per-surface results before reference-area conversion.
    pub surfaces: Vec<FourierLiftingLineResult>,
}

/// Invalid geometry or numerical failure in the lifting-line solve.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum FourierLiftingLineError {
    /// A scalar or coordinate was NaN or infinite.
    #[error("all lifting-line inputs must be finite")]
    NonFiniteInput,
    /// An area, span, or section lift slope was zero or negative.
    #[error("lifting-line dimensions and section lift slope must be positive")]
    NonPositiveScale,
    /// The collocation basis contains no odd Fourier harmonics.
    #[error("harmonic count must be at least one")]
    EmptyHarmonicSet,
    /// A half-wing definition did not run strictly from root to tip.
    #[error("sections must be ordered strictly from span fraction zero to one")]
    InvalidSectionOrder,
    /// A section chord was negative or a collocation chord was zero.
    #[error("section chords must be non-negative and positive at collocation stations")]
    InvalidChord,
    /// An airfoil could not supply a finite mean-camber line.
    #[error("airfoil coordinates cannot define a thin-airfoil camber line")]
    InvalidAirfoil,
    /// No symmetric horizontal lifting surface was present.
    #[error("aircraft contains no eligible symmetric lifting surface")]
    NoEligibleSurface,
    /// The Fourier collocation matrix was singular at this pivot.
    #[error("lifting-line collocation matrix is singular at pivot {0}")]
    SingularSystem(usize),
}

/// Compute a section zero-lift angle from its mean-camber line.
///
/// Thin-airfoil theory gives
/// `alpha_l0 = (1/pi) integral(dz/dx * (1 - cos(theta)) dtheta)`, with
/// `x/c = (1 - cos(theta))/2`. Coordinates and the result are dimensionless
/// and radians respectively.
pub fn thin_airfoil_zero_lift_angle_rad(airfoil: &Airfoil) -> Result<f64, FourierLiftingLineError> {
    if airfoil.upper_coordinates().len() < 2
        || airfoil.lower_coordinates().len() < 2
        || airfoil
            .coordinates
            .iter()
            .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(FourierLiftingLineError::InvalidAirfoil);
    }

    let paired_camber = paired_camber_coordinates(airfoil);

    let interval_angle = PI / THIN_AIRFOIL_INTERVALS as f64;
    let mut integral = 0.0;
    for index in 0..THIN_AIRFOIL_INTERVALS {
        let theta_lower = index as f64 * interval_angle;
        let theta_upper = (index + 1) as f64 * interval_angle;
        let theta_mid = (theta_lower + theta_upper) / 2.0;
        let x_lower = 0.5 * (1.0 - theta_lower.cos());
        let x_upper = 0.5 * (1.0 - theta_upper.cos());
        let camber = paired_camber.as_ref().map_or_else(
            || airfoil.local_camber(&[x_lower, x_upper]),
            |coordinates| {
                vec![
                    interpolate_ordinate(x_lower, coordinates),
                    interpolate_ordinate(x_upper, coordinates),
                ]
            },
        );
        let delta_x = x_upper - x_lower;
        if camber.len() != 2 || delta_x <= 0.0 || camber.iter().any(|value| !value.is_finite()) {
            return Err(FourierLiftingLineError::InvalidAirfoil);
        }
        let camber_slope = (camber[1] - camber[0]) / delta_x;
        integral += camber_slope * (1.0 - theta_mid.cos()) * interval_angle;
    }
    let zero_lift_angle = integral / PI;
    if zero_lift_angle.is_finite() {
        Ok(zero_lift_angle)
    } else {
        Err(FourierLiftingLineError::InvalidAirfoil)
    }
}

fn paired_camber_coordinates(airfoil: &Airfoil) -> Option<Vec<(f64, f64)>> {
    if airfoil.coordinates.len() % 2 == 0 {
        return None;
    }
    let join = airfoil.coordinates.len() / 2;
    let minimum_x = airfoil
        .coordinates
        .iter()
        .map(|point| point.0)
        .fold(f64::INFINITY, f64::min);
    let maximum_x = airfoil
        .coordinates
        .iter()
        .map(|point| point.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let chord = maximum_x - minimum_x;
    if chord <= 0.0 || airfoil.coordinates[join].0 - minimum_x > 1.0e-3 * chord {
        return None;
    }

    let mut upper = airfoil.coordinates[..=join].to_vec();
    upper.reverse();
    let lower = &airfoil.coordinates[join..];
    let camber = upper
        .iter()
        .zip(lower)
        .map(|(&(upper_x, upper_y), &(lower_x, lower_y))| {
            ((upper_x + lower_x) / 2.0, (upper_y + lower_y) / 2.0)
        })
        .collect::<Vec<_>>();
    if camber.windows(2).any(|pair| pair[1].0 <= pair[0].0) {
        return None;
    }
    Some(camber)
}

fn interpolate_ordinate(x: f64, coordinates: &[(f64, f64)]) -> f64 {
    if x <= coordinates[0].0 {
        return coordinates[0].1;
    }
    if x >= coordinates[coordinates.len() - 1].0 {
        return coordinates[coordinates.len() - 1].1;
    }
    let upper_index = coordinates
        .partition_point(|&(coordinate_x, _)| coordinate_x < x)
        .min(coordinates.len() - 1);
    let lower = coordinates[upper_index - 1];
    let upper = coordinates[upper_index];
    let fraction = (x - lower.0) / (upper.0 - lower.0);
    lower.1 + fraction * (upper.1 - lower.1)
}

impl FourierLiftingLineSurface {
    /// Build a lifting-line surface from an existing geometric wing.
    ///
    /// The aircraft geometry axes are `x` aft, `y` starboard, `z` up. Chord
    /// and projected lateral span are read in meters; sweep and dihedral are
    /// deliberately not collapsed into empirical correction factors.
    pub fn from_wing(wing: &Wing, harmonic_count: usize) -> Result<Self, FourierLiftingLineError> {
        if !wing.symmetric || wing.xsecs.len() < 2 {
            return Err(FourierLiftingLineError::NoEligibleSurface);
        }
        let span_m = wing.projected_span();
        let area_m2 = wing.projected_area();
        if !span_m.is_finite() || !area_m2.is_finite() || span_m <= 0.0 || area_m2 <= 0.0 {
            return Err(FourierLiftingLineError::NonPositiveScale);
        }

        let mut distances = Vec::with_capacity(wing.xsecs.len());
        distances.push(0.0);
        for pair in wing.xsecs.windows(2) {
            let increment = (pair[1].xyz_le[1] - pair[0].xyz_le[1]).abs();
            let next = distances.last().copied().unwrap_or(0.0) + increment;
            distances.push(next);
        }
        let semispan_m = span_m / 2.0;
        let sections = wing
            .xsecs
            .iter()
            .zip(distances)
            .map(|(section, distance_m)| {
                Ok(LiftingLineSection {
                    span_fraction: distance_m / semispan_m,
                    chord_m: section.chord,
                    twist_rad: section.twist.to_radians(),
                    zero_lift_angle_rad: thin_airfoil_zero_lift_angle_rad(&section.airfoil)?,
                })
            })
            .collect::<Result<Vec<_>, FourierLiftingLineError>>()?;

        let surface = Self {
            name: wing.name.clone(),
            span_m,
            area_m2,
            section_lift_slope_per_rad: 2.0 * PI,
            harmonic_count,
            sections,
        };
        surface.validate()?;
        Ok(surface)
    }

    /// Geometric aspect ratio based on projected span and area.
    pub fn aspect_ratio(&self) -> Result<f64, FourierLiftingLineError> {
        self.validate()?;
        Ok(self.span_m.powi(2) / self.area_m2)
    }

    /// Solve the odd-harmonic collocation system at one aircraft angle.
    pub fn solve(
        &self,
        alpha_rad: f64,
    ) -> Result<FourierLiftingLineResult, FourierLiftingLineError> {
        self.validate()?;
        if !alpha_rad.is_finite() {
            return Err(FourierLiftingLineError::NonFiniteInput);
        }

        let count = self.harmonic_count;
        let mut matrix = vec![vec![0.0; count]; count];
        let mut rhs = vec![vec![0.0]; count];
        let mut station_inputs = Vec::with_capacity(count);

        for (row, (matrix_row, rhs_row)) in matrix.iter_mut().zip(&mut rhs).enumerate() {
            let theta = (row + 1) as f64 * PI / (2.0 * count as f64);
            let span_fraction = theta.cos();
            let section = self.interpolate_section(span_fraction);
            if section.chord_m <= 0.0 {
                return Err(FourierLiftingLineError::InvalidChord);
            }
            rhs_row[0] = alpha_rad + section.twist_rad - section.zero_lift_angle_rad;
            for (column, coefficient) in matrix_row.iter_mut().enumerate() {
                let harmonic = (2 * column + 1) as f64;
                *coefficient = (harmonic * theta).sin()
                    * (4.0 * self.span_m / (self.section_lift_slope_per_rad * section.chord_m)
                        + harmonic / theta.sin());
            }
            station_inputs.push((theta, span_fraction, section.chord_m));
        }

        let solved =
            linalg::solve(&matrix, &rhs).map_err(FourierLiftingLineError::SingularSystem)?;
        let coefficients = solved.into_iter().map(|row| row[0]).collect::<Vec<_>>();
        let aspect_ratio = self.span_m.powi(2) / self.area_m2;
        let lift_coefficient = PI * aspect_ratio * coefficients[0];
        let induced_drag_coefficient = PI
            * aspect_ratio
            * coefficients
                .iter()
                .enumerate()
                .map(|(index, coefficient)| (2 * index + 1) as f64 * coefficient.powi(2))
                .sum::<f64>();
        let span_efficiency =
            if lift_coefficient.abs() > f64::EPSILON && induced_drag_coefficient > f64::EPSILON {
                Some(lift_coefficient.powi(2) / (PI * aspect_ratio * induced_drag_coefficient))
            } else {
                None
            };
        let stations = station_inputs
            .into_iter()
            .map(|(theta, span_fraction, chord_m)| {
                let circulation_sum = coefficients
                    .iter()
                    .enumerate()
                    .map(|(index, coefficient)| {
                        coefficient * (((2 * index + 1) as f64) * theta).sin()
                    })
                    .sum::<f64>();
                let induced_sum = coefficients
                    .iter()
                    .enumerate()
                    .map(|(index, coefficient)| {
                        let harmonic = (2 * index + 1) as f64;
                        harmonic * coefficient * (harmonic * theta).sin() / theta.sin()
                    })
                    .sum::<f64>();
                LiftingLineStationResult {
                    span_fraction,
                    section_lift_coefficient: 4.0 * self.span_m * circulation_sum / chord_m,
                    induced_angle_rad: induced_sum,
                }
            })
            .collect();

        Ok(FourierLiftingLineResult {
            surface_name: self.name.clone(),
            alpha_rad,
            lift_coefficient,
            induced_drag_coefficient,
            span_efficiency,
            fourier_coefficients: coefficients,
            stations,
        })
    }

    fn validate(&self) -> Result<(), FourierLiftingLineError> {
        if self.harmonic_count == 0 {
            return Err(FourierLiftingLineError::EmptyHarmonicSet);
        }
        let scales = [self.span_m, self.area_m2, self.section_lift_slope_per_rad];
        if scales.iter().any(|value| !value.is_finite())
            || self.sections.iter().any(|section| {
                !section.span_fraction.is_finite()
                    || !section.chord_m.is_finite()
                    || !section.twist_rad.is_finite()
                    || !section.zero_lift_angle_rad.is_finite()
            })
        {
            return Err(FourierLiftingLineError::NonFiniteInput);
        }
        if scales.iter().any(|value| *value <= 0.0) {
            return Err(FourierLiftingLineError::NonPositiveScale);
        }
        if self.sections.len() < 2
            || self.sections.first().map(|section| section.span_fraction) != Some(0.0)
            || self.sections.last().map(|section| section.span_fraction) != Some(1.0)
            || self
                .sections
                .windows(2)
                .any(|pair| pair[1].span_fraction <= pair[0].span_fraction)
        {
            return Err(FourierLiftingLineError::InvalidSectionOrder);
        }
        if self.sections.iter().any(|section| section.chord_m < 0.0) {
            return Err(FourierLiftingLineError::InvalidChord);
        }
        Ok(())
    }

    fn interpolate_section(&self, span_fraction: f64) -> LiftingLineSection {
        let upper_index = self
            .sections
            .partition_point(|section| section.span_fraction < span_fraction)
            .min(self.sections.len() - 1);
        if upper_index == 0 {
            return self.sections[0];
        }
        let lower = self.sections[upper_index - 1];
        let upper = self.sections[upper_index];
        let fraction =
            (span_fraction - lower.span_fraction) / (upper.span_fraction - lower.span_fraction);
        LiftingLineSection {
            span_fraction,
            chord_m: lower.chord_m + fraction * (upper.chord_m - lower.chord_m),
            twist_rad: lower.twist_rad + fraction * (upper.twist_rad - lower.twist_rad),
            zero_lift_angle_rad: lower.zero_lift_angle_rad
                + fraction * (upper.zero_lift_angle_rad - lower.zero_lift_angle_rad),
        }
    }
}
