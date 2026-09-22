// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a wingbox sizing solve reports.
//!
//! The result types and the margin predicates that read them, split out of
//! [`crate::sizing`] so the entry points, the sizing law and the result
//! vocabulary each stay inside one reviewable module.

/// Material-modelling qualification carried when a wingbox sizing result
/// uses composite materials under an effective isotropic proxy.
///
/// An effective isotropic proxy is appropriate for preliminary sizing passes,
/// but must never be confused with or presented as a certified laminate
/// stress analysis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompositeProxyDeclaration {
    /// Material-family evidence tier and citation.
    pub source: &'static str,
    /// Model applicability and non-certification disclosure.
    pub applicability: &'static str,
    /// Calibrated relative uncertainty if known/calibrated, or `None` if uncalibrated.
    ///
    /// Per strict audit override, uncalibrated uncertainty must be represented
    /// explicitly as unknown (`None`), not as zero or a fabricated numeric figure.
    pub relative_uncertainty: Option<f64>,
}

/// Per-spar sizing result, sampled at [`WingboxSizing::y_stations`].
#[derive(Debug, Clone, PartialEq)]
pub struct SparSizing {
    /// The spar's chordwise position, as a fraction of local chord.
    pub chord_fraction: f64,
    /// Free web height at each station, m.
    pub h: Vec<f64>,
    /// Cap flange width (tapered) at each station, m.
    pub w_cap: Vec<f64>,
    /// Cap flange thickness (tapered) at each station, m.
    pub t_cap: Vec<f64>,
    /// One-flange cap area at each station, m^2.
    pub a_cap: Vec<f64>,
    /// Uniform web thickness, m.
    pub t_web: f64,
    /// Bending-moment fraction this spar carries at each station.
    pub frac_moment: Vec<f64>,
    /// Margin of safety at each station: `+inf` where the local demand is
    /// below 1 N.m (near the tip). Expected `>= 0` near the root.
    pub margin_of_safety: Vec<f64>,
}

/// The station returned by [`WingboxSizing::controlling_margin`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControllingMargin {
    /// Raw margin of safety at the controlling station, full precision.
    pub margin: f64,
    /// Index into [`WingboxSizing::spars`] and [`WingboxSizing::spar_fracs`].
    pub spar_index: usize,
    /// The spar's chordwise position, as a fraction of local chord.
    pub chord_fraction: f64,
    /// Index into [`WingboxSizing::y_stations`] and
    /// [`WingboxSizing::eta_stations`].
    pub station_index: usize,
    /// Spanwise station, m.
    pub y_m: f64,
    /// Normalized spanwise station, `y / semi_span`.
    pub eta: f64,
}

/// The sized wingbox: per-station geometry, rib layout and mass breakdown.
#[derive(Debug, Clone, PartialEq)]
pub struct WingboxSizing {
    /// Spanwise stations, m.
    pub y_stations: Vec<f64>,
    /// Normalized spanwise stations, `y / semi_span`.
    pub eta_stations: Vec<f64>,
    /// Local chord at each station, m.
    pub chord: Vec<f64>,
    /// The spar chordwise fractions, in the geometry's sorted order.
    pub spar_fracs: Vec<f64>,
    /// Per-spar sizing.
    pub spars: Vec<SparSizing>,
    /// Skin thickness, m.
    pub t_skin: f64,
    /// Number of ribs.
    pub num_ribs: i64,
    /// Panel-buckling allowable rib spacing, m.
    pub rib_spacing_m: f64,
    /// Semi-wing mass by component, kg.
    pub mass_breakdown_kg: MassBreakdown,
    /// Total semi-wing structural mass, kg.
    pub total_mass_kg: f64,
    /// The name of the load case that sized the box.
    pub sizing_load_case: &'static str,
    /// Material qualification declaration, present whenever any wingbox
    /// material is composite. `None` for all-metallic wings.
    pub composite_declaration: Option<CompositeProxyDeclaration>,
}

impl WingboxSizing {
    /// Installed spanwise pitch between adjacent ribs, including the root and
    /// tip ribs. This is distinct from [`Self::rib_spacing_m`], which is the
    /// maximum pitch permitted by the panel-buckling calculation and may be
    /// larger or smaller than the pitch selected by an explicit rib-count
    /// override.
    pub fn installed_rib_spacing_m(&self) -> f64 {
        if self.num_ribs > 1 {
            let first = self.y_stations.first().copied().unwrap_or(f64::NAN);
            let last = self.y_stations.last().copied().unwrap_or(f64::NAN);
            (last - first) / (self.num_ribs - 1) as f64
        } else {
            f64::NAN
        }
    }

    /// Whether the selected rib count satisfies the panel-buckling limit.
    ///
    /// Automatically sized layouts obey this by construction. An explicit
    /// rib-count override is still checked here so a diagnostic sizing result
    /// cannot be promoted to a structural success when its installed bays
    /// are wider than the applicable allowable spacing.
    pub fn rib_spacing_pass(&self) -> bool {
        let installed = self.installed_rib_spacing_m();
        installed.is_finite() && self.rib_spacing_m.is_finite() && installed <= self.rib_spacing_m
    }

    /// The smallest strength margin found in the sized spars.
    ///
    /// A NaN margin is returned as `NaN` so callers cannot mistake an
    /// incomplete sizing calculation for a successful one. Positive infinity
    /// is a valid margin for a station whose demand is below the numerical
    /// reporting threshold.
    pub fn minimum_margin_of_safety(&self) -> f64 {
        self.controlling_margin().map_or(f64::NAN, |c| c.margin)
    }

    /// The station that controls [`Self::minimum_margin_of_safety`], with its
    /// location, for diagnostics.
    ///
    /// Rounding the controlling margin to a fixed number of decimals (as a
    /// failure message meant for humans naturally does) collapses every
    /// value between roughly `-5e-7` and `0` to the same displayed
    /// `-0.000000`, hiding whether the shortfall is floating-point noise at
    /// the active root boundary or a real, if small, structural deficit.
    /// Callers that need to tell those apart must use the raw
    /// [`ControllingMargin::margin`] here, not a display-rounded value.
    ///
    /// `None` only when there are no spar stations at all. As with
    /// [`Self::minimum_margin_of_safety`], a NaN margin takes priority over
    /// any finite one so an incomplete calculation is never reported as a
    /// located structural result.
    pub fn controlling_margin(&self) -> Option<ControllingMargin> {
        let mut best: Option<ControllingMargin> = None;
        for (spar_index, spar) in self.spars.iter().enumerate() {
            for (station_index, &margin) in spar.margin_of_safety.iter().enumerate() {
                let candidate = ControllingMargin {
                    margin,
                    spar_index,
                    chord_fraction: self.spar_fracs.get(spar_index).copied().unwrap_or(f64::NAN),
                    station_index,
                    y_m: self
                        .y_stations
                        .get(station_index)
                        .copied()
                        .unwrap_or(f64::NAN),
                    eta: self
                        .eta_stations
                        .get(station_index)
                        .copied()
                        .unwrap_or(f64::NAN),
                };
                let replace = match &best {
                    None => true,
                    Some(current) if current.margin.is_nan() => false,
                    Some(current) => margin.is_nan() || margin < current.margin,
                };
                if replace {
                    best = Some(candidate);
                }
            }
        }
        best
    }

    /// Whether every sized spar station carries its own load.
    ///
    /// A station is accepted when its margin is not NaN and is not below the
    /// numerical noise floor [`MARGIN_NUMERICAL_ZERO`] of a margin that was
    /// sized to exactly zero. It is **not** a widened acceptance: the band is
    /// four units in the last place of unity, about `8.9e-16`, and a real
    /// structural deficit of that size would be a cap flange short by one part
    /// in `10^16` - far below the width of an atom on any flange this model
    /// sizes. Any margin a reader would call small, `-1e-9` included, still
    /// fails here.
    pub fn strength_margins_pass(&self) -> bool {
        !self.spars.is_empty()
            && self.spars.iter().all(|spar| {
                !spar.margin_of_safety.is_empty()
                    && spar
                        .margin_of_safety
                        .iter()
                        .all(|&margin| margin_is_structurally_non_negative(margin))
            })
    }

    /// Whether the controlling margin is a real deficit rather than the noise
    /// floor of a zero-margin design, for a caller that wants to report the
    /// distinction rather than only act on it.
    pub fn controlling_margin_is_numerical_zero(&self) -> bool {
        self.controlling_margin().is_some_and(|controlling| {
            controlling.margin.is_finite()
                && controlling.margin < 0.0
                && controlling.margin >= -MARGIN_NUMERICAL_ZERO
        })
    }
}

/// The largest negative margin of safety that is arithmetic noise rather than a
/// strength deficit, `4 f64::EPSILON` ~ `8.882e-16`.
///
/// # Why a fully stressed box needs this, and why the number is what it is
///
/// [`size_wingbox`] sizes every station to a margin of **exactly zero**: it
/// solves `A_req = M / (F_allow h_eff)` and then builds a flange of that area.
/// The margin it afterwards reports recomputes the same equality from the
/// built flange,
///
/// ```text
/// MS = (w t) F_allow h_eff / M - 1
/// ```
///
/// and in binary floating point that round trip is not exact. Counting the
/// operations whose rounding does **not** cancel between the two sides: the
/// required area is `(frac M)/(F_allow h_eff)`, three of them; the flange is
/// `t = A_req/w` then `A = w t`, two more; the admissible moment is
/// `A F_allow h_eff`, two more; and the ratio is one division. Eight roundings,
/// each bounded by the unit roundoff `u = 2^-53 = f64::EPSILON/2`, put the
/// ratio in `1 +/- 8u`, so `MS` lands anywhere in `+/-4 f64::EPSILON`. (`|a b|` is
/// exactly `|a| |b|`, so the demand and the sizing numerator share one rounding
/// rather than two, and `x - 1` for `x` near `1` is exact by Sterbenz's lemma;
/// neither contributes.) The two values this product actually produces are
/// `-2^-52` and `-2^-53`, one and a half units in the last place - a factor of
/// four inside the bound.
///
/// Before the box was sized station by station only the root sat on this
/// boundary. A fully stressed box puts **every** station on it, so a predicate
/// written as `margin >= 0.0` rejects a correctly sized wing on the sign of a
/// rounding error - which is what was making `crates/alas-pipeline`'s
/// `sizing_failure_detail` report an infeasible wingbox, and with it skip
/// NASTRAN and Patran, on aircraft whose structure is exactly as designed.
///
/// The band is a statement about the arithmetic, not about the structure. It is
/// `4 f64::EPSILON` because that is `8u` for the eight roundings in the round
/// trip above, and it is not tuned: no preset's acceptance was consulted
/// in choosing it, and widening it further would change nothing, because the
/// next distinguishable margin below it is still nine orders of magnitude
/// short of any engineering significance.
pub const MARGIN_NUMERICAL_ZERO: f64 = 4.0 * f64::EPSILON;

/// Whether one margin of safety is non-negative once the noise floor of a
/// zero-margin design is accounted for. NaN is never accepted.
pub fn margin_is_structurally_non_negative(margin: f64) -> bool {
    !margin.is_nan() && margin >= -MARGIN_NUMERICAL_ZERO
}

/// The four semi-wing mass components upstream keys by name in its
/// `mass_breakdown_kg` dict.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassBreakdown {
    /// Spar caps, kg.
    pub spar_caps: f64,
    /// Spar webs, kg.
    pub spar_webs: f64,
    /// Skin, kg.
    pub skin: f64,
    /// Ribs, kg.
    pub ribs: f64,
}
