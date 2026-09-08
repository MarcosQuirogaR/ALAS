// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


impl AircraftFourierLiftingLine {
    /// Build an aircraft model from all symmetric horizontal surfaces.
    pub fn from_airplane(
        airplane: &Airplane,
        harmonic_count: usize,
    ) -> Result<Self, FourierLiftingLineError> {
        if !airplane.s_ref.is_finite() {
            return Err(FourierLiftingLineError::NonFiniteInput);
        }
        if airplane.s_ref <= 0.0 {
            return Err(FourierLiftingLineError::NonPositiveScale);
        }
        let surfaces = airplane
            .wings
            .iter()
            .filter(|wing| wing.symmetric)
            .map(|wing| FourierLiftingLineSurface::from_wing(wing, harmonic_count))
            .collect::<Result<Vec<_>, _>>()?;
        if surfaces.is_empty() {
            return Err(FourierLiftingLineError::NoEligibleSurface);
        }
        Ok(Self {
            reference_area_m2: airplane.s_ref,
            surfaces,
        })
    }

    /// Solve all isolated surfaces and normalize their forces consistently.
    pub fn solve(
        &self,
        alpha_rad: f64,
    ) -> Result<AircraftLiftingLineResult, FourierLiftingLineError> {
        if !self.reference_area_m2.is_finite() {
            return Err(FourierLiftingLineError::NonFiniteInput);
        }
        if self.reference_area_m2 <= 0.0 {
            return Err(FourierLiftingLineError::NonPositiveScale);
        }
        let surfaces = self
            .surfaces
            .iter()
            .map(|surface| surface.solve(alpha_rad))
            .collect::<Result<Vec<_>, _>>()?;
        let lift_coefficient = surfaces
            .iter()
            .zip(&self.surfaces)
            .map(|(result, surface)| {
                result.lift_coefficient * surface.area_m2 / self.reference_area_m2
            })
            .sum();
        let induced_drag_coefficient = surfaces
            .iter()
            .zip(&self.surfaces)
            .map(|(result, surface)| {
                result.induced_drag_coefficient * surface.area_m2 / self.reference_area_m2
            })
            .sum();
        Ok(AircraftLiftingLineResult {
            alpha_rad,
            lift_coefficient,
            induced_drag_coefficient,
            surfaces,
        })
    }

    /// Evaluate an angle schedule without changing its order.
    pub fn sweep(
        &self,
        alpha_rad: &[f64],
    ) -> Result<Vec<AircraftLiftingLineResult>, FourierLiftingLineError> {
        alpha_rad.iter().map(|&alpha| self.solve(alpha)).collect()
    }
}

