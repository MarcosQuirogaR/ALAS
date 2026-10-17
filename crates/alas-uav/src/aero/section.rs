// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Two-dimensional section models: NeuralFoil-native or an imported table.
//!
//! Angle of attack is relative to the section chord line, radians internally
//! (degrees only at the NeuralFoil boundary); `cm` is about the quarter chord,
//! nose-up positive.  Every evaluation is classified by
//! [`super::validity::Limits`], so a query outside the network's domain is
//! flagged by us rather than silently extrapolated.

use std::sync::Arc;

use alas_aero::neuralfoil::{Conditions, ModelSize, PreparedAirfoil};
use alas_geom::aircraft::airfoil::Airfoil;

use super::polar_csv::PolarTable;
use super::validity::{Limits, Validity};
use super::AeroError;

/// Where the section coefficients come from.
#[derive(Clone)]
pub enum SectionSource {
    /// NeuralFoil on the fitted Kulfan section.
    NeuralFoil {
        /// Fitted section (shared between strips).
        prepared: Arc<PreparedAirfoil>,
        /// Critical amplification factor (calibration parameter; tunnel-clean
        /// 9..12, RC surface with prop turbulence 5..7: plan 3.1, INFERENCE).
        n_crit: f64,
        /// Forced transition location, upper surface, fraction of chord
        /// (1 = natural).
        xtr_upper: f64,
        /// Forced transition location, lower surface, fraction of chord.
        xtr_lower: f64,
        /// Network size.
        size: ModelSize,
    },
    /// Imported polar table.
    Table(Arc<PolarTable>),
}

impl std::fmt::Debug for SectionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeuralFoil { n_crit, size, .. } => {
                write!(f, "NeuralFoil(n_crit={n_crit}, size={})", size.name())
            }
            Self::Table(t) => write!(f, "Table({})", t.name),
        }
    }
}

/// One evaluated section point.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionPoint {
    /// Lift coefficient on the local chord.
    pub cl: f64,
    /// Profile drag coefficient on the local chord.
    pub cd: f64,
    /// Quarter-chord moment coefficient.
    pub cm: f64,
    /// Domain findings.
    pub validity: Validity,
}

/// A section model with its validity limits.
#[derive(Debug, Clone)]
pub struct SectionModel {
    /// Display name.
    pub name: String,
    /// Coefficient source.
    pub source: SectionSource,
    /// Domain limits.
    pub limits: Limits,
}

impl SectionModel {
    /// NeuralFoil section at `n_crit` with natural transition and the
    /// default ("large") network, the size every other ALAS default uses.
    ///
    /// # Errors
    ///
    /// [`AeroError::Section`] when the coordinates cannot be fitted.
    pub fn neuralfoil(airfoil: &Airfoil, n_crit: f64) -> Result<Self, AeroError> {
        if !(n_crit.is_finite() && n_crit > 0.0) {
            return Err(AeroError::InvalidInput(format!(
                "n_crit {n_crit} must be positive"
            )));
        }
        let prepared = PreparedAirfoil::prepare(airfoil)
            .map_err(|e| AeroError::Section(format!("{}: {e}", airfoil.name)))?;
        Ok(Self {
            name: airfoil.name.clone(),
            source: SectionSource::NeuralFoil {
                prepared: Arc::new(prepared),
                n_crit,
                xtr_upper: 1.0,
                xtr_lower: 1.0,
                size: ModelSize::Large,
            },
            limits: Limits::default(),
        })
    }

    /// Section from an imported table.
    pub fn from_table(table: PolarTable) -> Self {
        Self {
            name: table.name.clone(),
            source: SectionSource::Table(Arc::new(table)),
            limits: Limits::default(),
        }
    }

    /// Evaluate at `alpha_rad` (chord-relative), chord Reynolds number `re`
    /// and Mach `mach`.
    ///
    /// # Errors
    ///
    /// [`AeroError::Section`] if the network rejects the section.
    pub fn eval(&self, alpha_rad: f64, re: f64, mach: f64) -> Result<SectionPoint, AeroError> {
        match &self.source {
            SectionSource::NeuralFoil {
                prepared,
                n_crit,
                xtr_upper,
                xtr_lower,
                size,
            } => {
                let alpha_deg = alpha_rad.to_degrees();
                let conditions = Conditions {
                    alpha_deg,
                    reynolds: re,
                    n_crit: *n_crit,
                    xtr_upper: *xtr_upper,
                    xtr_lower: *xtr_lower,
                };
                let aero = prepared
                    .aero(&conditions, mach.max(0.0), *size)
                    .map_err(|e| AeroError::Section(format!("{}: {e}", self.name)))?;
                let validity =
                    self.limits
                        .classify(re, alpha_deg, mach, Some(aero.analysis_confidence));
                Ok(SectionPoint {
                    cl: aero.cl,
                    cd: aero.cd,
                    cm: aero.cm,
                    validity,
                })
            }
            SectionSource::Table(table) => {
                let point = table.eval(alpha_rad, re);
                let mut validity = self.limits.classify(re, alpha_rad.to_degrees(), mach, None);
                // The table's own range replaces the NeuralFoil alpha window
                // and Re thresholds: an imported polar is the user's data.
                validity.flags.retain(|(_, f)| {
                    !matches!(
                        f,
                        super::validity::Flag::Alpha(_)
                            | super::validity::Flag::LowReynolds(_)
                            | super::validity::Flag::HighReynolds(_)
                    )
                });
                validity.extend(&point.validity);
                Ok(SectionPoint {
                    cl: point.cl,
                    cd: point.cd,
                    cm: point.cm,
                    validity,
                })
            }
        }
    }

    /// Maximum lift coefficient and the angle where it occurs (rad) at `re`:
    /// the first local maximum of `cl(alpha)` above the zero-lift angle,
    /// scanning 0.25 deg steps up to 22 deg (the network's post-stall blend
    /// centres at 20 deg).  Falls back to the largest `cl` in the window.
    ///
    /// # Errors
    ///
    /// As [`Self::eval`].
    pub fn cl_max(&self, re: f64, mach: f64) -> Result<(f64, f64), AeroError> {
        let (start, stop, step) = (-4.0_f64, 22.0_f64, 0.25_f64);
        let n = ((stop - start) / step).round() as usize + 1;
        let mut cls = Vec::with_capacity(n);
        for i in 0..n {
            let alpha = (start + step * i as f64).to_radians();
            cls.push(self.eval(alpha, re, mach)?.cl);
        }
        let mut best = 0usize;
        for (i, cl) in cls.iter().enumerate() {
            if *cl > cls[best] {
                best = i;
            }
        }
        // First local maximum after the cl = 0 crossing that drops for the
        // next 2 deg (8 steps): the stall peak rather than the post-stall rise.
        let zero = cls.iter().position(|c| *c > 0.0).unwrap_or(0);
        for i in zero + 1..n.saturating_sub(8) {
            let is_peak = cls[i] >= cls[i - 1] && cls[i + 1..=i + 8].iter().all(|c| *c < cls[i]);
            if is_peak {
                best = i;
                break;
            }
        }
        Ok((cls[best], (start + step * best as f64).to_radians()))
    }
}
