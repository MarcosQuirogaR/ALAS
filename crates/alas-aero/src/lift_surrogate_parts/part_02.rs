// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


impl LiftSurrogate {
    /// Run the vortex lattice over the grid and fit the surfaces.
    ///
    /// This is `sample_training` followed by `build_surrogate`, and it is one
    /// call because upstream's `initialize` makes it one: nothing ever
    /// samples without fitting.
    ///
    /// # Errors
    ///
    /// See [`SurrogateError`].
    pub fn train(
        geometry: &VlmGeometry,
        settings: &VlmSettings,
        grid: &TrainingGrid,
    ) -> Result<Self, SurrogateError> {
        if let Some(&mach) = grid.mach.iter().find(|&&mach| mach >= 1.0) {
            return Err(SurrogateError::Supersonic { mach });
        }

        let n_alpha = grid.angle_of_attack_rad.len();
        let n_mach = grid.mach.len();

        // The grid is flattened Mach-major: `sample_training` tiles the angle
        // vector once per Mach number and the Mach vector once per angle, so
        // the solve sees every angle at Mach 0 before it sees any at Mach
        // 0.1. The reshape below undoes it, and undoing it the other way
        // round produces a surface that is smooth, plausible and transposed.
        //
        // The velocity is left at zero, which is what upstream's conditions
        // carry: `VLM` substitutes 1e-6 for it under `use_surrogate`, and the
        // rotation rates are all zero, so nothing depends on the number.
        let conditions: Vec<vorlax::VlmCondition> = grid
            .mach
            .iter()
            .flat_map(|&mach| {
                grid.angle_of_attack_rad
                    .iter()
                    .map(move |&angle_of_attack_rad| vorlax::VlmCondition {
                        angle_of_attack_rad,
                        mach,
                        side_slip_angle_rad: 0.0,
                        pitch_rate_rad_s: 0.0,
                        roll_rate_rad_s: 0.0,
                        yaw_rate_rad_s: 0.0,
                        velocity_m_s: 0.0,
                    })
            })
            .collect();

        let results = vorlax::run(geometry, settings, &conditions)?;
        let areas = &results.distribution.wing_areas_m2;

        let reshape = |flat: &dyn Fn(usize) -> f64| -> Vec<Vec<f64>> {
            (0..n_alpha)
                .map(|i| (0..n_mach).map(|j| flat(j * n_alpha + i)).collect())
                .collect()
        };

        let lift_table = reshape(&|k| results.cases[k].cl);
        let drag_table = reshape(&|k| results.cases[k].cdi);

        // `calculate_VLM`'s regrouping: a symmetric wing occupies two columns
        // of the per-surface arrays. They are dimensionalized on the surfaces'
        // own areas, summed, and divided by the wing's reference area, which
        // is what makes a wing's coefficient comparable across the two.
        let mut wing_lift_tables = BTreeMap::new();
        let mut wing_drag_tables = BTreeMap::new();
        let mut wing_tags = Vec::with_capacity(geometry.wings.len());
        let mut surface = 0usize;
        for wing in &geometry.wings {
            let count = if wing.symmetric { 2 } else { 1 };
            let sides = surface..surface + count;
            let lift = reshape(&|k| {
                sides
                    .clone()
                    .map(|s| results.cases[k].cl_wing[s] * f64::from(areas[s]))
                    .sum::<f64>()
                    / wing.area_reference_m2
            });
            let drag = reshape(&|k| {
                sides
                    .clone()
                    .map(|s| results.cases[k].cdi_wing[s] * f64::from(areas[s]))
                    .sum::<f64>()
                    / wing.area_reference_m2
            });
            wing_lift_tables.insert(wing.tag.clone(), lift);
            wing_drag_tables.insert(wing.tag.clone(), drag);
            wing_tags.push(wing.tag.clone());
            surface += count;
        }

        Self::from_training(
            grid,
            &wing_tags,
            &TrainingTables {
                lift_coefficient: lift_table,
                drag_coefficient: drag_table,
                wing_lift_coefficient: wing_lift_tables,
                wing_drag_coefficient: wing_drag_tables,
            },
        )
    }

    /// Fit the surfaces through tables that have already been sampled.
    ///
    /// This is upstream's `build_surrogate` on its own: the half of
    /// [`Self::train`] that turns the sampled grid into splines, without the
    /// vortex-lattice sweep that produced it. `initialize` never calls one
    /// without the other, so this is not a second way to reach a surrogate;
    /// it is the seam that lets a caller *supply* the samples instead of
    /// solving for them.
    ///
    /// `alas-mission`'s segment solver is why it exists. A mission run has to
    /// be handed the tables the reference trained on rather than resampling
    /// them, for the reason `alas-aero::drag_buildup`'s row records: a parity
    /// test that re-derives an input through a second model reports that
    /// model's disagreement as its own. The vortex lattice's agreement is
    /// [`Self::train`]'s claim and `alas-aero::vorlax`'s, checked there.
    ///
    /// # Errors
    ///
    /// [`SurrogateError::Supersonic`] if the grid reaches Mach 1, and
    /// [`SurrogateError::Fit`] naming the table whose fit failed. A table
    /// whose shape does not match the grid fails as a [`SurrogateError::Fit`]
    /// on that table, since that is what the fit reports.
    pub fn from_training(
        grid: &TrainingGrid,
        wing_tags: &[String],
        training: &TrainingTables,
    ) -> Result<Self, SurrogateError> {
        if let Some(&mach) = grid.mach.iter().find(|&&mach| mach >= 1.0) {
            return Err(SurrogateError::Supersonic { mach });
        }

        let fit = |quantity: &str, table: &[Vec<f64>]| {
            BicubicSpline::interpolate(&grid.angle_of_attack_rad, &grid.mach, table).map_err(
                |source| SurrogateError::Fit {
                    quantity: quantity.to_owned(),
                    source,
                },
            )
        };

        let missing = |tag: &str, quantity: &str| SurrogateError::MissingWingTable {
            tag: tag.to_owned(),
            quantity: quantity.to_owned(),
        };

        let lift = fit("lift_coefficient", &training.lift_coefficient)?;
        let drag = fit("drag_coefficient", &training.drag_coefficient)?;
        let mut wing_lift = Vec::with_capacity(wing_tags.len());
        let mut wing_drag = Vec::with_capacity(wing_tags.len());
        for tag in wing_tags {
            let lift_table = training
                .wing_lift_coefficient
                .get(tag)
                .ok_or_else(|| missing(tag, "lift_coefficient"))?;
            let drag_table = training
                .wing_drag_coefficient
                .get(tag)
                .ok_or_else(|| missing(tag, "drag_coefficient"))?;
            wing_lift.push(fit(&format!("{tag} lift_coefficient"), lift_table)?);
            wing_drag.push(fit(&format!("{tag} drag_coefficient"), drag_table)?);
        }

        Ok(Self {
            grid: grid.clone(),
            wing_tags: wing_tags.to_vec(),
            training: training.clone(),
            lift,
            drag,
            wing_lift,
            wing_drag,
        })
    }

    /// Evaluate the surrogate at one flight condition.
    ///
    /// Outside the training rectangle the answer is the value on the nearest
    /// edge, not an extrapolation of it. The module doc says why.
    pub fn evaluate(&self, angle_of_attack_rad: f64, mach: f64) -> LiftSolution {
        let domain = self.domain_status(angle_of_attack_rad, mach);
        LiftSolution {
            inviscid_lift_coefficient: self.lift.evaluate(angle_of_attack_rad, mach),
            inviscid_induced_drag_coefficient: self.drag.evaluate(angle_of_attack_rad, mach),
            wing_lift_coefficient: self
                .wing_lift
                .iter()
                .map(|spline| spline.evaluate(angle_of_attack_rad, mach))
                .collect(),
            wing_induced_drag_coefficient: self
                .wing_drag
                .iter()
                .map(|spline| spline.evaluate(angle_of_attack_rad, mach))
                .collect(),
            domain,
        }
    }

    /// Evaluate only when the query is inside the trained rectangle.
    ///
    /// [`Self::evaluate`] intentionally preserves the upstream FITPACK edge
    /// clamp for parity. This checked entry point makes the model boundary an
    /// explicit policy choice for mission/product callers instead of silently
    /// freezing lift and induced drag at the last training knot.
    pub fn evaluate_checked(
        &self,
        angle_of_attack_rad: f64,
        mach: f64,
    ) -> Result<LiftSolution, SurrogateDomainError> {
        if !angle_of_attack_rad.is_finite() || !mach.is_finite() {
            return Err(SurrogateDomainError::NonFinite {
                alpha_rad: angle_of_attack_rad,
                mach,
            });
        }
        let domain = self.domain_status(angle_of_attack_rad, mach);
        if !domain.in_domain() {
            return Err(SurrogateDomainError::OutOfDomain {
                alpha_rad: angle_of_attack_rad,
                mach,
                alpha_min: self
                    .grid
                    .angle_of_attack_rad
                    .first()
                    .copied()
                    .unwrap_or(f64::NAN),
                alpha_max: *self.grid.angle_of_attack_rad.last().unwrap_or(&f64::NAN),
                mach_min: self.grid.mach.first().copied().unwrap_or(f64::NAN),
                mach_max: *self.grid.mach.last().unwrap_or(&f64::NAN),
            });
        }
        Ok(self.evaluate(angle_of_attack_rad, mach))
    }

    fn domain_status(&self, angle_of_attack_rad: f64, mach: f64) -> SurrogateDomainStatus {
        let alpha_min = self
            .grid
            .angle_of_attack_rad
            .first()
            .copied()
            .unwrap_or(f64::NAN);
        let alpha_max = self
            .grid
            .angle_of_attack_rad
            .last()
            .copied()
            .unwrap_or(f64::NAN);
        let mach_min = self.grid.mach.first().copied().unwrap_or(f64::NAN);
        let mach_max = self.grid.mach.last().copied().unwrap_or(f64::NAN);
        let outside_distance = |query: f64, minimum: f64, maximum: f64| {
            if query.is_nan() || minimum.is_nan() || maximum.is_nan() {
                f64::NAN
            } else if query < minimum {
                minimum - query
            } else if query > maximum {
                query - maximum
            } else {
                0.0
            }
        };
        SurrogateDomainStatus {
            alpha_clamped: !angle_of_attack_rad.is_finite()
                || angle_of_attack_rad < alpha_min
                || angle_of_attack_rad > alpha_max,
            mach_clamped: !mach.is_finite() || mach < mach_min || mach > mach_max,
            alpha_distance_rad: outside_distance(angle_of_attack_rad, alpha_min, alpha_max),
            mach_distance: outside_distance(mach, mach_min, mach_max),
        }
    }

    /// The grid this surrogate was fitted through.
    pub fn grid(&self) -> &TrainingGrid {
        &self.grid
    }

    /// The wing tags, in the order [`LiftSolution`]'s per-wing vectors use.
    pub fn wing_tags(&self) -> &[String] {
        &self.wing_tags
    }

    /// The sampled tables the surfaces were fitted through.
    pub fn training(&self) -> &TrainingTables {
        &self.training
    }

    /// The aircraft lift surface, for a caller that wants its knots.
    pub fn lift_surface(&self) -> &BicubicSpline {
        &self.lift
    }

    /// The aircraft induced-drag surface.
    pub fn drag_surface(&self) -> &BicubicSpline {
        &self.drag
    }

    /// One wing's lift surface, by position in [`Self::wing_tags`].
    pub fn wing_lift_surface(&self, wing: usize) -> &BicubicSpline {
        &self.wing_lift[wing]
    }

    /// One wing's induced-drag surface.
    pub fn wing_drag_surface(&self, wing: usize) -> &BicubicSpline {
        &self.wing_drag[wing]
    }
}

/// The rest of `Fidelity_Zero`'s lift chain, which is one multiplication.
///
/// `compute.lift.vortex` is `mission analysis model.Methods.skip`, `compute.lift.fuselage` is
/// `fuselage_correction` (the whole of which is
/// `CL * settings.fuselage_lift_correction`, overwriting the same field)
/// and `compute.lift.total` is `aircraft_total`, which returns what it was
/// handed. They are here rather than in a module of their own because
/// together they are three lines, and because the correction factor is the
/// one number in them a reader would want to find.
pub fn aircraft_lift_coefficient(inviscid_lift_coefficient: f64, fuselage_correction: f64) -> f64 {
    inviscid_lift_coefficient * fuselage_correction
}
