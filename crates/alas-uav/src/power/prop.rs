// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Propeller model: measured or computed maps first, BEMT as the fallback.
//!
//! A [`PropModel`] hides where the loads come from.  Resolution order
//! (see [`super::from_spec::resolve_propeller`]): a measured `PropPerfSet`, an
//! installed PER3 table read through the fetch-on-demand reader
//! ([`crate::propulsion_electric::apc_performance_map`]), a computed
//! `PropPerfSet`, then BEMT from the database geometry or, last, a generic
//! blade from diameter and pitch.  The source is retained for the report.

use crate::propulsion_electric::PropellerPerformanceMap;

use super::bemt::BemtGeometry;
use super::prop_map::PropMapTable;
use super::PowerError;

/// Speed of sound used for the BEMT tip-Mach speed ceiling, m/s (ISA sea level).
const A_REF_M_S: f64 = 340.3;
/// Tip Mach number above which the incompressible BEMT is not used.
const TIP_MACH_MAX: f64 = 0.7;

/// Where the loads of a propeller came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropSource {
    /// Database `PropPerfSet` of kind measured.
    MeasuredMap,
    /// PER3 table read from the local data directory (computed by the vendor).
    InstalledPer3,
    /// Database `PropPerfSet` of kind computed.
    ComputedMap,
    /// BEMT on the database blade geometry.
    Bemt,
    /// BEMT on a generic blade built from diameter and pitch only.
    BemtGeneric,
}

/// Propeller loads at one operating condition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropLoads {
    /// Thrust, N (positive forward).
    pub thrust_n: f64,
    /// Shaft torque absorbed, N*m.
    pub torque_n_m: f64,
    /// Shaft power absorbed, W.
    pub p_shaft_w: f64,
    /// Advance ratio `V / (n D)`.
    pub j: f64,
    /// Thrust coefficient.
    pub ct: f64,
    /// Power coefficient.
    pub cp: f64,
    /// True when BEMT annuli fell back to zero induction (result degraded).
    pub degraded: bool,
    /// True when the BEMT section Reynolds number at 0.75 R is below
    /// [`super::polar::LOW_RE_WARN`]: the NeuralFoil section polars are in
    /// their warning band (maps are never flagged).
    pub low_re: bool,
}

#[derive(Debug, Clone)]
enum Kind {
    Map(PropMapTable),
    Per3(PropellerPerformanceMap),
    Bemt(BemtGeometry),
}

/// A propeller as the solver sees it.
#[derive(Debug, Clone)]
pub struct PropModel {
    kind: Kind,
    source: PropSource,
    diameter_m: f64,
}

impl PropModel {
    /// From a database map (`measured` or `computed`).
    pub fn from_map(table: PropMapTable, measured: bool) -> Self {
        let diameter_m = table.diameter_m();
        let source = if measured {
            PropSource::MeasuredMap
        } else {
            PropSource::ComputedMap
        };
        Self {
            kind: Kind::Map(table),
            source,
            diameter_m,
        }
    }

    /// From an installed PER3 table.
    pub fn from_per3(table: PropellerPerformanceMap) -> Self {
        let diameter_m = table.diameter_m();
        Self {
            kind: Kind::Per3(table),
            source: PropSource::InstalledPer3,
            diameter_m,
        }
    }

    /// From a blade geometry.
    pub fn from_bemt(geometry: BemtGeometry) -> Self {
        let diameter_m = 2.0 * geometry.radius_m();
        let source = if geometry.generic {
            PropSource::BemtGeneric
        } else {
            PropSource::Bemt
        };
        Self {
            kind: Kind::Bemt(geometry),
            source,
            diameter_m,
        }
    }

    /// Provenance of the loads.
    pub fn source(&self) -> PropSource {
        self.source
    }

    /// Diameter, m.
    pub fn diameter_m(&self) -> f64 {
        self.diameter_m
    }

    /// Tip-speed ceiling of the BEMT model, rad/s (`None` for tabulated data).
    pub fn bemt_omega_ceiling(&self) -> Option<f64> {
        match &self.kind {
            Kind::Bemt(g) => Some(TIP_MACH_MAX * A_REF_M_S / g.radius_m()),
            _ => None,
        }
    }

    /// Usable shaft-speed interval at airspeed `v`, rad/s.
    pub fn omega_range(&self, v: f64) -> Option<(f64, f64)> {
        match &self.kind {
            Kind::Map(t) => Some(t.omega_range()),
            Kind::Per3(t) => t.rpm_bounds_at_speed(v).map(|(a, b)| {
                (
                    a * std::f64::consts::TAU / 60.0,
                    b * std::f64::consts::TAU / 60.0,
                )
            }),
            Kind::Bemt(g) => Some((20.0, TIP_MACH_MAX * A_REF_M_S / g.radius_m())),
        }
    }

    /// Loads at shaft speed `omega` (rad/s), airspeed `v` (m/s), air density
    /// `rho` (kg/m3) and dynamic viscosity `mu` (Pa*s).
    ///
    /// # Errors
    ///
    /// [`PowerError::OutOfRange`] when a map has no data there or BEMT fails.
    pub fn loads(&self, omega: f64, v: f64, rho: f64, mu: f64) -> Result<PropLoads, PowerError> {
        let d = self.diameter_m;
        let n = omega / std::f64::consts::TAU;
        let j = v / (n * d);
        let out_of_range =
            || PowerError::OutOfRange(format!("omega = {omega:.1} rad/s, V = {v:.2} m/s"));
        if !(omega.is_finite() && omega > 0.0 && v >= 0.0) {
            return Err(out_of_range());
        }
        let from_coefficients = |ct: f64, cp: f64| {
            let p = cp * rho * n.powi(3) * d.powi(5);
            PropLoads {
                thrust_n: ct * rho * n * n * d.powi(4),
                torque_n_m: p / omega,
                p_shaft_w: p,
                j,
                ct,
                cp,
                degraded: false,
                low_re: false,
            }
        };
        match &self.kind {
            Kind::Map(t) => t
                .coefficients(omega, j)
                .map(|(ct, cp)| from_coefficients(ct, cp))
                .ok_or_else(out_of_range),
            Kind::Per3(t) => t
                .coefficients_at(omega * 60.0 / std::f64::consts::TAU, v)
                .map(|s| from_coefficients(s.thrust_coefficient, s.power_coefficient))
                .ok_or_else(out_of_range),
            Kind::Bemt(g) => {
                let s = g.solve(omega, v, rho, mu).ok_or_else(out_of_range)?;
                let p = s.torque_n_m * omega;
                Ok(PropLoads {
                    thrust_n: s.thrust_n,
                    torque_n_m: s.torque_n_m,
                    p_shaft_w: p,
                    j,
                    ct: s.thrust_n / (rho * n * n * d.powi(4)),
                    cp: p / (rho * n.powi(3) * d.powi(5)),
                    degraded: s.fallback_fraction > 0.1,
                    low_re: s.re_75 < super::polar::LOW_RE_WARN,
                })
            }
        }
    }
}
