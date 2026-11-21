// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Transonic wave drag: the drag-divergence relation of the wing's section
//! class on its outboard wing, the Lock rise above it, and the frozen Python
//! comparison law.
//!
//! Sources, by printed page:
//! - E. Torenbeek, *Advanced Aircraft Design* (2013), sec. 10.6.1, p. 298:
//!   the sweep is that of the mid-chord line of the outboard wing; the
//!   average thickness ratio is the root/tip mean for a straight tapered
//!   wing and the thickness ratio at the leading-edge kink for a complex
//!   one. P. 300: `M_dd` is the Mach number of `dCD/dM = 0.10`. P. 301:
//!   Korn's relation `M_dd + t/c + 0.10 cl = kappa` (Eq. 10.46) and its form
//!   for second-generation supercritical sections
//!   `M_dd + t/c + 0.10 cl^1.5 = M*`, `M* = 0.935` (Eq. 10.47). P. 302: that
//!   form carried to the wing by simple sweep theory, with a factor 1.1 for
//!   the higher-than-average loading of the outboard wing (Eq. 10.48).
//! - W. H. Mason, *Configuration Aerodynamics*, ch. 7: Korn's relation on the
//!   swept wing, `M_dd = kappa/cos L - t/c/cos^2 L - CL/(10 cos^3 L)`, with
//!   `kappa` 0.87 for NACA 6-series and 0.95 for supercritical sections, and
//!   the Lock rise `CD_w = 20 (M - M_crit)^4`,
//!   `M_crit = M_dd - (0.1/80)^(1/3)` (Eqs. 7-7, 7-8). The rise and the
//!   critical-Mach offset are Mason's, not Torenbeek's.
//!
//! The kink the outboard wing starts at is the wing's declared planform
//! (trailing-edge) kink. Reading Torenbeek's leading-edge kink as that
//! station is an interpretation of his Fig. 10.2(b), not a statement of the
//! text.
//!
//! The product default is the Shevell-Kroo method: the crest-critical Mach
//! number `M_cc` of the wing from its mean thickness ratio, lift coefficient
//! and quarter-chord sweep (chart for the peaky section family, with the
//! printed increment of the wing's section generation,
//! `alas_config::AirfoilClass::crest_critical_mach_increment`), the divergence
//! Mach number `M_div = M_cc [1.02 + 0.08 (1 - cos L)]` (`dCD/dM = 0.05`) and
//! Shevell's compressibility-drag curve `CD_c = 0.0019 cos^3 L (M/M_cc)^14.641`
//! (I. Kroo, *Aircraft Design: Synthesis and Analysis*, Stanford AA241 notes,
//! sec. 4.3; R. S. Shevell, *Fundamentals of Flight*; coefficients as carried
//! by SUAVE, `compressibility_drag_wing.py`). Unlike the Lock rise it carries
//! the drag creep below divergence that the type drag curves show (E. Obert,
//! *Aerodynamic Design of Transport Aircraft*, 2009: A320-200 Fig. 24.123,
//! DC-10-30 Fig. 24.26, A340-200 Fig. 24.131, 8 to 23 counts between Mach 0.5
//! and Mach 0.78 at CL 0.5, where the Lock rise gives 0 to 3).
//!
//! Korn's relation and Torenbeek's Eq. 10.48 with the Lock rise stay
//! selectable, `geometry.wing.drag_divergence_law`
//! (`alas_config::DragDivergenceLaw`). Among those two, the section-class rule:
//! a supercritical wing takes Eq. 10.48, the form stated for its section
//! class; a conventional (NACA 6-series) wing, for which Torenbeek gives no
//! `M*`, takes Korn's relation with its class `kappa`; a declared Korn factor
//! override takes Korn's relation with that factor. An explicit law selects
//! Korn's relation or Eq. 10.48 whatever the class. Against the DC-10-10 and
//! 747-100 flight drag rise (Obert, *Aerodynamic Design of Transport
//! Aircraft*, Fig. 24.24; Mason, Fig. 7-12) and the A320-200 and A340-200
//! drag curves (Obert, Figs. 24.123 and 24.131), this rule reads `M_dd` with
//! an rms error of 0.023 (worst +0.046 on the A340-200 estimate, DC-10 0.026
//! low), against 0.030 for Korn's relation on both classes over the 50-90 %
//! semispan window this module used before. That rms is in-sample: the rule
//! was chosen among candidates on these same points, so it is not an
//! independent validation.

use super::AeroAnalysis;
use alas_config::DivergenceRelation;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::WingXSec;

/// `M*` of Torenbeek's Eqs. 10.47 and 10.48 for second-generation
/// supercritical sections (Advanced Aircraft Design, p. 301).
pub const SUPERCRITICAL_M_STAR: f64 = 0.935;

/// rms error in Mach of [`DivergenceLaw::drag_divergence_mach`] against the registered
/// drag rises named in the module documentation (DC-10-10, 747-100, A320-200 and
/// A340-200; Obert, Figs. 24.24, 24.123 and 24.131; Mason, Fig. 7-12). In-sample: the
/// rule was chosen on the same points. Neither Torenbeek (2013, pp. 301-302) nor the
/// other handbooks consulted state an accuracy for Korn's relation or Eq. 10.48.
pub const DRAG_DIVERGENCE_MACH_RMS_ERROR: f64 = 0.023;

/// Scatter in Mach of the Shevell-Kroo divergence Mach (the crest-critical
/// chart with the section-class increments of
/// `alas_config::AirfoilClass::crest_critical_mach_increment`) against the 29
/// aircraft of S. Ciornei, *Mach number, relative thickness, sweep and lift
/// coefficient of the wing* (HAW Hamburg, 2005), Tables C.1-C.4 (8
/// conventional, 6 peaky, 8 older and 7 modern supercritical sections, each
/// with its quarter-chord sweep, t/c, CL and M_DD): mean error +0.005, standard
/// deviation 0.047 (rms 0.047, largest 0.128). Out of sample: none of those
/// points chose the chart or the increments.
pub const SHEVELL_KROO_DIVERGENCE_SCATTER: f64 = 0.047;

/// Eq. 10.48's factor on the wing lift coefficient for the higher-than-average
/// loading of the outboard wing (Advanced Aircraft Design, p. 302).
pub(super) const OUTBOARD_LOADING_FACTOR: f64 = 1.1;

/// Spanwise window of the outboard wing, as fractions of the lifting semispan,
/// over which [`AeroAnalysis::quarter_chord_sweep_deg`] reads the
/// quarter-chord line for the low-speed consumers (trim, buffet, maximum
/// lift, AVL) that are handed a bare airplane. No source gives the two
/// fractions; the transonic relation does not depend on them when the wing
/// declares its kink ([`OutboardWing`]).
const OUTBOARD_WINDOW: (f64, f64) = (0.5, 0.9);

/// The drag-divergence relation of a wing section class, on the flow normal
/// to the outboard mid-chord line, and the Lock offset to the critical Mach.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct DivergenceLaw {
    relation: Relation,
    cos_sweep: f64,
    thickness: f64,
    /// `M_dd - M_crit`: `(0.1/(4C))^(1/3)`, or zero for the frozen law.
    offset: f64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
enum Relation {
    /// Korn on the swept wing with technology factor `kappa`.
    Korn { kappa: f64 },
    /// Torenbeek's Eq. 10.48 with `M*`.
    Torenbeek { m_star: f64 },
    /// The Shevell-Kroo crest-critical Mach method with the section
    /// generation's increment; `cos_sweep` is then the quarter-chord line's
    /// and `thickness` the area-weighted one.
    Shevell { increment: f64 },
}

/// The crest-critical Mach chart of the Shevell-Kroo method on the flow normal
/// to the quarter-chord line, `M_cc cos L` against `(t/c)/cos L` and
/// `CL/cos^2 L`: the regression of the chart of I. Kroo, *Aircraft Design:
/// Synthesis and Analysis* (Stanford AA241 notes, sec. 4.3), as carried by
/// SUAVE, `compressibility_drag_wing.py` (the solver this mission model was
/// ported from), in the order constant, `tc`, `cl`, `tc^2`, `tc cl`, `cl^2`.
const SHEVELL_CREST_CHART: [f64; 6] = [
    0.922_321_524_499_352,
    -1.153_885_166_170_62,
    -0.304_541_067_183_461,
    0.332_881_324_404_729,
    0.467_317_361_111_105,
    0.087_490_431_201_549,
];

/// Shevell's compressibility-drag curve, `CD_c / cos^3 L = A (M / M_cc)^N`:
/// the fit of the curve of Kroo's notes carried by SUAVE (same file). It
/// carries the drag creep below divergence: 4 % of its `M = M_cc` value at
/// `0.8 M_cc` and 21 % at `0.9 M_cc`.
const SHEVELL_DRAG_CURVE: (f64, f64) = (0.0019, 14.641);

/// Largest sweep-normal lift coefficient the crest chart is read at: its
/// regression turns back up beyond the charted lift, so the reading follows
/// the chart's tangent from there.
const SHEVELL_CHART_MAX_NORMAL_CL: f64 = 1.0;

impl DivergenceLaw {
    /// Drag-divergence Mach number (`dCD/dM = 0.1`) at wing lift
    /// coefficient `cl`. Eq. 10.48's `CL^1.5` term takes a non-negative `cl`.
    pub fn drag_divergence_mach(&self, cl: f64) -> f64 {
        let c = self.cos_sweep;
        match self.relation {
            Relation::Korn { kappa } => {
                kappa / c - self.thickness / (c * c) - cl / (10.0 * c * c * c)
            }
            Relation::Torenbeek { m_star } => {
                let loading = OUTBOARD_LOADING_FACTOR * cl.max(0.0) / (c * c);
                (m_star - self.thickness / c - 0.10 * loading.powf(1.5)) / c
            }
            // Shevell's divergence Mach, `dCD/dM = 0.05`:
            // `M_div = M_cc [1.02 + 0.08 (1 - cos L)]`.
            Relation::Shevell { .. } => self.crest_critical(cl).0 * (1.02 + 0.08 * (1.0 - c)),
        }
    }

    /// Documented error in Mach of [`Self::drag_divergence_mach`], one
    /// estimate per law: [`SHEVELL_KROO_DIVERGENCE_SCATTER`] (the standard
    /// deviation over 29 aircraft, out of sample) for the Shevell-Kroo
    /// method; [`DRAG_DIVERGENCE_MACH_RMS_ERROR`] (the in-sample rms against
    /// the registered drag rises) for Korn's relation and Eq. 10.48, for
    /// which no independent data set was evaluated.
    #[must_use]
    pub fn divergence_mach_scatter(&self) -> f64 {
        match self.relation {
            Relation::Shevell { .. } => SHEVELL_KROO_DIVERGENCE_SCATTER,
            Relation::Korn { .. } | Relation::Torenbeek { .. } => DRAG_DIVERGENCE_MACH_RMS_ERROR,
        }
    }

    /// Crest-critical Mach number of the Shevell-Kroo method and its lift
    /// slope at `cl`.
    fn crest_critical(&self, cl: f64) -> (f64, f64) {
        let Relation::Shevell { increment } = self.relation else {
            return (f64::NAN, f64::NAN);
        };
        let c = self.cos_sweep;
        let [k0, kt, kl, ktt, ktl, kll] = SHEVELL_CREST_CHART;
        let tc = self.thickness / c;
        let normal = cl / (c * c);
        // Outside the charted lift the reading continues along the chart's
        // tangent at the edge, so the Mach number and its slope stay
        // continuous.
        let n = normal.clamp(0.0, SHEVELL_CHART_MAX_NORMAL_CL);
        let tangent = kl + ktl * tc + 2.0 * kll * n;
        let chart = k0
            + kt * tc
            + kl * n
            + ktt * tc * tc
            + ktl * tc * n
            + kll * n * n
            + tangent * (normal - n);
        (chart / c + increment, tangent / (c * c * c))
    }

    /// Critical Mach number: where the Lock rise starts, or the crest-critical
    /// Mach number of the Shevell-Kroo method.
    pub fn critical_mach(&self, cl: f64) -> f64 {
        match self.relation {
            Relation::Shevell { .. } => self.crest_critical(cl).0,
            _ => self.drag_divergence_mach(cl) - self.offset,
        }
    }

    /// `dM_crit/dCL` at `cl`.
    pub fn critical_mach_slope(&self, cl: f64) -> f64 {
        let c = self.cos_sweep;
        match self.relation {
            Relation::Korn { .. } => -1.0 / (10.0 * c * c * c),
            Relation::Torenbeek { .. } => {
                let per_cl = OUTBOARD_LOADING_FACTOR / (c * c);
                -0.10 * 1.5 * per_cl.powf(1.5) * cl.max(0.0).sqrt() / c
            }
            Relation::Shevell { .. } => self.crest_critical(cl).1,
        }
    }

    /// Positive lift coefficient at which [`Self::critical_mach`] loses its
    /// second derivative, `None` where it has none at `CL > 0`. The
    /// Shevell-Kroo chart is read as its quadratic up to the sweep-normal lift
    /// `SHEVELL_CHART_MAX_NORMAL_CL` and along its tangent beyond, so
    /// `d2 M_cc / dCL2` drops from `2 kll / cos^5 L` to zero at
    /// `CL = SHEVELL_CHART_MAX_NORMAL_CL cos^2 L`, and the compressibility
    /// drag `(M / M_cc)^14.641` takes that jump. A piecewise-cubic
    /// interpolant in CL keeps its `h^4` error only when this lift is a node;
    /// a cell straddling it has an `h^2` error. Korn's relation is linear in
    /// CL and Eq. 10.48 is smooth for `CL > 0`.
    pub fn lift_slope_break(&self) -> Option<f64> {
        match self.relation {
            Relation::Shevell { .. } => {
                Some(SHEVELL_CHART_MAX_NORMAL_CL * self.cos_sweep * self.cos_sweep)
            }
            Relation::Korn { .. } | Relation::Torenbeek { .. } => None,
        }
    }

    /// Mach number below which the law gives no compressibility drag at `cl`:
    /// the critical Mach of the Lock rise; none for Shevell's curve, which is
    /// smooth down to rest.
    pub fn onset_mach(&self, cl: f64) -> f64 {
        match self.relation {
            Relation::Shevell { .. } => 0.0,
            _ => self.critical_mach(cl),
        }
    }

    /// Whether the configured wave-drag onset Mach gates this law (the Lock
    /// rise); Shevell's curve carries its own creep below it.
    pub fn takes_onset_gate(&self) -> bool {
        !matches!(self.relation, Relation::Shevell { .. })
    }

    /// Compressibility drag at `mach` and `cl`: Lock's rise
    /// `coefficient (M - M_crit)^4` above the critical Mach, or Shevell's
    /// curve `0.0019 cos^3 L (M / M_cc)^14.641`.
    pub fn wave_cd(&self, mach: f64, cl: f64, coefficient: f64) -> f64 {
        match self.relation {
            Relation::Shevell { .. } => {
                let (a, n) = SHEVELL_DRAG_CURVE;
                let ratio = (mach / self.crest_critical(cl).0).max(0.0);
                a * self.cos_sweep.powi(3) * ratio.powf(n)
            }
            _ => {
                let excess = mach - self.critical_mach(cl);
                if excess > 0.0 {
                    coefficient * excess.powi(4)
                } else {
                    0.0
                }
            }
        }
    }

    /// `d CD_c / d CL` at `mach` and `cl`, given the drag `value` there.
    pub fn wave_cd_lift_slope(&self, mach: f64, cl: f64, value: f64) -> f64 {
        if value <= 0.0 {
            return 0.0;
        }
        match self.relation {
            Relation::Shevell { .. } => {
                let (crest, slope) = self.crest_critical(cl);
                -SHEVELL_DRAG_CURVE.1 * value / crest * slope
            }
            _ => {
                let excess = mach - self.critical_mach(cl);
                if excess > 0.0 {
                    -4.0 * value / excess * self.critical_mach_slope(cl)
                } else {
                    0.0
                }
            }
        }
    }
}

/// Slope `dx/dy` of the line at `chord_fraction` of the local chord across
/// [`OUTBOARD_WINDOW`] of `plane`'s main wing, in the planform (XY) projection.
/// A wing whose first lifting section is outboard of the window's inner end
/// reads its whole lifting span instead. `None` without a main wing of two
/// sections or without a retained panel of positive finite span.
fn outboard_chord_line_slope(plane: &Airplane, chord_fraction: f64) -> Option<f64> {
    let lifting = LiftingPanels::of(plane)?;
    let (root_y, tip_y) = lifting.span();
    let (inner, outer) = if root_y < OUTBOARD_WINDOW.0 * tip_y {
        (OUTBOARD_WINDOW.0 * tip_y, OUTBOARD_WINDOW.1 * tip_y)
    } else {
        (root_y, tip_y)
    };
    lifting.chord_line_slope(chord_fraction, inner, outer)
}

fn span_of(xsec: &WingXSec) -> f64 {
    xsec.xyz_le[1].abs()
}

/// The lifting panels of the main wing, inboard to outboard. Panels steeper
/// than 45 degrees (`|dz| > |dy|`), such as winglets, are not part of the
/// lifting planform.
struct LiftingPanels<'p> {
    panels: Vec<(&'p WingXSec, &'p WingXSec)>,
    /// Span of the main wing's outermost section, the base of the declared
    /// span fractions (the builder places the kink and rake stations on it).
    last_y: f64,
}

impl<'p> LiftingPanels<'p> {
    fn of(plane: &'p Airplane) -> Option<Self> {
        let wing = plane.wings.first().filter(|wing| wing.xsecs.len() >= 2)?;
        let panels: Vec<(&WingXSec, &WingXSec)> = wing
            .xsecs
            .windows(2)
            .filter(|pair| {
                let dy = (pair[1].xyz_le[1] - pair[0].xyz_le[1]).abs();
                let dz = (pair[1].xyz_le[2] - pair[0].xyz_le[2]).abs();
                dy > 0.0 && dy.is_finite() && dz <= dy
            })
            .map(|pair| (&pair[0], &pair[1]))
            .collect();
        panels.first()?;
        let last_y = span_of(wing.xsecs.last()?);
        Some(Self { panels, last_y })
    }

    /// Span of the first and the last lifting section.
    fn span(&self) -> (f64, f64) {
        (
            span_of(self.panels[0].0),
            span_of(self.panels[self.panels.len() - 1].1),
        )
    }

    /// The panel holding span station `y` and the fraction of it at `y`.
    fn panel_at(&self, y: f64) -> (&'p WingXSec, &'p WingXSec, f64) {
        let (first, last) = (self.panels[0].0, self.panels[self.panels.len() - 1].1);
        let (inboard, outboard) = self
            .panels
            .iter()
            .copied()
            .find(|(_, outboard)| y <= span_of(outboard))
            .unwrap_or((first, last));
        let t = (y - span_of(inboard)) / (span_of(outboard) - span_of(inboard));
        (inboard, outboard, t)
    }

    /// Slope `dx/dy` of the chord line at `chord_fraction` between `inner`
    /// and `outer`, on the straight line through its points there.
    fn chord_line_slope(&self, chord_fraction: f64, inner: f64, outer: f64) -> Option<f64> {
        let line_x_at = |y: f64| {
            let (inboard, outboard, t) = self.panel_at(y);
            let line = |xsec: &WingXSec| xsec.xyz_le[0] + chord_fraction * xsec.chord;
            line(inboard) + t * (line(outboard) - line(inboard))
        };
        let slope = (line_x_at(outer) - line_x_at(inner)) / (outer - inner);
        slope.is_finite().then_some(slope)
    }

    /// Maximum thickness-to-chord at span `y`, linear between sections.
    fn thickness_at(&self, y: f64, samples: &[f64]) -> f64 {
        let (inboard, outboard, t) = self.panel_at(y);
        let (t0, t1) = (
            inboard.airfoil.max_thickness(samples),
            outboard.airfoil.max_thickness(samples),
        );
        t0 + t.clamp(0.0, 1.0) * (t1 - t0)
    }

    /// `dc/dy` of the panel just inboard and the panel just outboard of `y`;
    /// the same panel twice when `y` lies inside it.
    fn chord_slopes_across(&self, y: f64) -> (f64, f64) {
        let slope =
            |(a, b): (&WingXSec, &WingXSec)| (b.chord - a.chord) / (span_of(b) - span_of(a));
        let tolerance = 1e-9 * self.last_y.max(1.0);
        let inboard = self
            .panels
            .iter()
            .copied()
            .rev()
            .find(|(a, _)| span_of(a) < y - tolerance)
            .unwrap_or(self.panels[0]);
        let outboard = self
            .panels
            .iter()
            .copied()
            .find(|(_, b)| span_of(b) > y + tolerance)
            .unwrap_or(self.panels[self.panels.len() - 1]);
        (slope(inboard), slope(outboard))
    }
}

/// The plane-only inputs of [`AeroAnalysis::korn_sweep_deg`], computed once
/// per analysis: the outboard wing's `(mid, quarter)` chord-line slopes and
/// the outboard-window quarter- and mid-chord slopes, `dx/dy`.
#[derive(Debug, Clone, Copy)]
pub(super) struct KornSweepGeometry {
    outboard: Option<(f64, f64)>,
    window_c4: Option<f64>,
    window_c2: Option<f64>,
}

/// The outboard wing of Torenbeek's sec. 10.6.1 on the built main wing: from
/// the declared kink (`kink_span_fraction`, else `break_span_fraction`) to the
/// declared start of a raked tip (`tip_rake_start_span_fraction`), else to the
/// outermost lifting section. The fractions are of the span of the main
/// wing's outermost section, where the builder places those stations.
struct OutboardWing {
    /// Mid-chord and quarter-chord slopes `dx/dy` from kink to rake or tip.
    mid_chord_slope: f64,
    quarter_chord_slope: f64,
    /// Torenbeek's average thickness ratio: the t/c at the kink of a wing
    /// whose chord distribution kinks there, else the mean of the root and
    /// tip t/c.
    thickness: f64,
}

impl OutboardWing {
    fn of(plane: &Airplane, wing: &alas_config::WingConfig, samples: &[f64]) -> Option<Self> {
        let lifting = LiftingPanels::of(plane)?;
        let (root_y, tip_y) = lifting.span();
        let kink_y = wing.kink_span_fraction.unwrap_or(wing.break_span_fraction) * lifting.last_y;
        let outer = wing
            .tip_rake_start_span_fraction
            .map_or(tip_y, |fraction| (fraction * lifting.last_y).min(tip_y));
        if !(root_y < kink_y && kink_y < outer) {
            return None;
        }
        let mid_chord_slope = lifting.chord_line_slope(0.5, kink_y, outer)?;
        let quarter_chord_slope = lifting.chord_line_slope(0.25, kink_y, outer)?;
        let (inboard, outboard) = lifting.chord_slopes_across(kink_y);
        let cranked =
            (inboard - outboard).abs() > 1e-9 * inboard.abs().max(outboard.abs()).max(1.0);
        let thickness = if cranked {
            lifting.thickness_at(kink_y, samples)
        } else {
            0.5 * (lifting.thickness_at(root_y, samples) + lifting.thickness_at(tip_y, samples))
        };
        thickness.is_finite().then_some(Self {
            mid_chord_slope,
            quarter_chord_slope,
            thickness,
        })
    }
}

impl AeroAnalysis<'_> {
    /// Quarter-chord sweep of `plane`'s main wing, degrees: the sweep of the
    /// quarter-chord line across [`OUTBOARD_WINDOW`] of the lifting semispan,
    /// for the consumers handed a bare airplane (trim, buffet, maximum lift,
    /// AVL, the form factor).
    ///
    /// The design vector's `sweep_deg` is the leading-edge sweep the geometry
    /// builder lays out, not the quarter-chord sweep the swept compressibility
    /// correction, the form factor and the buffet estimate are written for. An
    /// inboard trailing-edge extension (Yehudi) adds root chord without
    /// sweeping the outer wing, so an area-weighted mean over every panel would
    /// let the low-sweep Yehudi panel pull the value 0.6 to 2.3 degrees below
    /// the published ones. The `alas-config` test `preset_sweep_conventions`
    /// pins the kink-to-rake quarter-chord line of the A380-800, DC-10, B787-9
    /// and A340-300 to their sources; the window here is not that line. It
    /// reads 38.5 degrees on the B747-400, whose drawn outer wing has a
    /// 39.7 degree panel between 42 and 69 % semispan and 37.4 degrees
    /// elsewhere, against the published 37.5.
    ///
    /// The line is read between fixed span fractions, not from whichever
    /// stations happen to bound the outermost straight run: a curved or raked
    /// tip is many short runs of rising sweep, and two stations added at 97 %
    /// and 99 % of the span moved a run-based reading of a 13 degree wing to
    /// 46 degrees.
    ///
    /// `fallback` is returned only when there is no main wing with a root and
    /// a tip section, or no retained panel has a positive finite span.
    pub fn quarter_chord_sweep_deg(plane: &Airplane, fallback: f64) -> f64 {
        outboard_chord_line_slope(plane, 0.25).map_or(fallback, |slope| slope.atan().to_degrees())
    }

    /// The outboard wing of the analysed main wing, `None` when the declared
    /// kink and rake stations do not bound a lifting panel of it.
    fn outboard_wing(&self) -> Option<OutboardWing> {
        let samples =
            alas_geom::aircraft::spacing::linspace(0.0, 1.0, super::MAX_THICKNESS_SAMPLES);
        OutboardWing::of(self.plane, &self.geometry.wing, &samples)
    }

    /// Sweep the transonic relation takes, degrees: the mid-chord line of the
    /// outboard wing (Torenbeek, Advanced Aircraft Design, sec. 10.6.1). The
    /// relation is a 2-D section law carried to the wing by simple sweep
    /// theory, so its sweep is that of the isobars over the shock-bearing
    /// part of the section, not of the aerodynamic centre line.
    ///
    /// A caller's quarter-chord [`Self::sweep_deg`] keeps its meaning: the
    /// result is the outboard mid-chord line moved by the caller's departure
    /// from [`Self::quarter_chord_sweep_deg`], so the production caller,
    /// which passes that value, gets the outboard mid-chord line itself.
    /// Without a readable outboard wing it is the caller's sweep carried to
    /// mid chord by the taper over [`OUTBOARD_WINDOW`]. Frozen fixtures keep
    /// the quarter-chord value they were made with.
    pub fn korn_sweep_deg(&self) -> f64 {
        if self.reference_compatibility || self.frozen_wave_drag {
            return self.sweep_deg;
        }
        let tan_c4 = self.sweep_deg.to_radians().tan();
        let geometry = self.korn_sweep_cache.get_or_init(|| KornSweepGeometry {
            outboard: self
                .outboard_wing()
                .map(|outboard| (outboard.mid_chord_slope, outboard.quarter_chord_slope)),
            window_c4: outboard_chord_line_slope(self.plane, 0.25),
            window_c2: outboard_chord_line_slope(self.plane, 0.5),
        });
        let window_c4 = geometry.window_c4;
        let tan = match (geometry.outboard, window_c4) {
            (Some((mid_chord_slope, _)), Some(window_c4)) => mid_chord_slope + (tan_c4 - window_c4),
            (Some((mid_chord_slope, quarter_chord_slope)), None) => {
                tan_c4 + mid_chord_slope - quarter_chord_slope
            }
            (None, Some(quarter)) => match geometry.window_c2 {
                Some(mid) => tan_c4 + mid - quarter,
                None => tan_c4,
            },
            (None, None) => tan_c4,
        };
        tan.atan().to_degrees()
    }

    /// Thickness-to-chord the transonic relation takes: Torenbeek's average
    /// thickness ratio (Advanced Aircraft Design, sec. 10.6.1), the t/c at the
    /// kink of a wing whose chord distribution kinks there, else the mean of
    /// the root and tip t/c. Without a readable outboard wing, the
    /// exposed-area-weighted t/c of the first wing. Frozen and
    /// reference-compatibility analyses keep the values their fixtures were
    /// made with: the root section, and the area-weighted t/c.
    pub fn korn_thickness(&self) -> f64 {
        if self.plane.wings.is_empty() || self.reference_compatibility {
            return self.section_thickness();
        }
        if self.frozen_wave_drag {
            return self.wing_thicknesses()[0];
        }
        *self.korn_thickness_cache.get_or_init(|| {
            self.outboard_wing()
                .map_or_else(|| self.wing_thicknesses()[0], |outboard| outboard.thickness)
        })
    }

    /// The Korn technology factor `kappa_A` of the analysed wing: the one its
    /// declared section class fixes, `geometry.wing.airfoil_class` (0.87
    /// conventional, 0.95 supercritical; Mason, *Configuration
    /// Aerodynamics*, ch. 7, the Korn equation; Malone and Mason 1995), or
    /// the in-range `geometry.wing.korn_technology_factor_override` when set.
    /// [`Self::divergence_law`] takes it whenever the wing's
    /// `drag_divergence_law` resolves to Korn's relation (the section-class
    /// rule on a conventional wing or with an override, or Korn selected).
    pub fn korn_technology_factor(&self) -> f64 {
        self.geometry.wing.korn_technology_factor()
    }

    /// The drag-divergence relation of this wing at section thickness
    /// `section_thickness` (else [`Self::korn_thickness`]) and sweep
    /// [`Self::korn_sweep_deg`]: the one `geometry.wing.drag_divergence_law`
    /// resolves to (`alas_config::WingConfig::divergence_relation`),
    /// Torenbeek's Eq. 10.48 (`M* = 0.935`) or Korn's relation with
    /// [`Self::korn_technology_factor`]. Under the default section-class rule
    /// that is Eq. 10.48 for a supercritical wing without a Korn factor
    /// override and Korn's relation otherwise. The frozen and
    /// reference-compatibility analyses and a wingless airplane take Korn's
    /// relation whatever the selection.
    pub fn divergence_law(&self, section_thickness: Option<f64>) -> DivergenceLaw {
        let product =
            !self.reference_compatibility && !self.frozen_wave_drag && !self.plane.wings.is_empty();
        let selected = self.geometry.wing.divergence_relation();
        if product && selected == DivergenceRelation::Shevell {
            // Shevell's chart is read with the quarter-chord sweep and the
            // wing's mean thickness ratio (the exposed-area-weighted one).
            return DivergenceLaw {
                relation: Relation::Shevell {
                    increment: self
                        .geometry
                        .wing
                        .airfoil_class
                        .crest_critical_mach_increment(),
                },
                cos_sweep: self.sweep_deg.to_radians().cos(),
                thickness: section_thickness.unwrap_or_else(|| self.wing_thicknesses()[0]),
                offset: 0.0,
            };
        }
        let thickness = section_thickness.unwrap_or_else(|| self.korn_thickness());
        let torenbeek = product && selected == DivergenceRelation::Torenbeek1048;
        let relation = if torenbeek {
            Relation::Torenbeek {
                m_star: SUPERCRITICAL_M_STAR,
            }
        } else {
            Relation::Korn {
                kappa: self.korn_technology_factor(),
            }
        };
        // Lock's fourth-power rise `CD_wave = C (M - M_crit)^4` reaches the
        // drag-divergence slope `dCD/dM = 0.1` at
        // `M_dd - M_crit = (0.1 / (4 C))^(1/3)`. Two coefficients are in use,
        // each in its own scope, and their critical Mach numbers are not
        // interchangeable:
        // - here, the WING: `C = drag.wave_drag_coefficient` (20 by default),
        //   offset 0.1077; this is the critical Mach the wave drag of the
        //   polar, the drag table and the mission starts from;
        // - the SECTION post-processing of `neuralfoil::corrections`:
        //   `C = 80` written as `(0.1 / 320)^(1/3)`, offset 0.0679, a fixed
        //   constant of the translated upstream section model, compared
        //   value-for-value with its reference fixtures. It sets the critical
        //   Mach of a two-dimensional section result only and does not feed
        //   this wing-level law.
        let offset = if self.frozen_wave_drag {
            0.0
        } else {
            (0.1 / (4.0 * self.drag.wave_drag_coefficient)).cbrt()
        };
        DivergenceLaw {
            relation,
            cos_sweep: self.korn_sweep_deg().to_radians().cos(),
            thickness,
            offset,
        }
    }

    /// The transonic wave-drag estimate.
    ///
    /// Zero below the configured onset Mach, and zero again above it while
    /// the critical Mach the section, sweep and lift coefficient set has not
    /// been passed. `M_dd` is the drag-divergence Mach, defined by
    /// `dCD/dM = 0.1`, not the onset of wave drag. The Lock law is
    /// `CD_w = C (M - M_crit)^4` with `M_crit = M_dd - (0.1/(4 C))^(1/3)`. At
    /// the default C=20, the offset is about 0.1077 (Mason, *Configuration
    /// Aerodynamics*, Eqs. 7-7 and 7-8; Lock 1985). The frozen Python
    /// fixture instead used `M_dd` as the start; `parity_analysis.rs`
    /// compares against the corrected expectation without widening
    /// tolerance.
    pub fn wave_drag(&self, mach: f64, cl: f64, section_thickness: Option<f64>) -> f64 {
        let law = self.divergence_law(section_thickness);
        if !law.takes_onset_gate() {
            return law.wave_cd(mach, cl, self.drag.wave_drag_coefficient);
        }
        if mach < self.drag.wave_drag_onset_mach {
            return 0.0;
        }
        let (_, mach_crit) = self.korn_mach_numbers(cl, section_thickness);
        if mach > mach_crit {
            self.drag.wave_drag_coefficient * (mach - mach_crit).powf(4.0)
        } else {
            0.0
        }
    }

    /// The drag-divergence and critical Mach numbers `(M_dd, M_crit)` at lift
    /// coefficient `cl` of [`Self::divergence_law`], the relations
    /// [`Self::wave_drag`] evaluates. Korn's relation on the swept wing,
    /// `M_dd = kappa/cos(L) - t/c/cos^2(L) - CL/(10 cos^3(L))`, is the 2-D law
    /// `M_dd + cl/10 + t/c = kappa` on the flow normal to the sweep line
    /// (`M_n = M cos L`, `(t/c)_n = (t/c)/cos L`, `cl_n = CL/cos^2 L`);
    /// Eq. 10.48 is `M_dd cos L + t/c / cos L + 0.10 (1.1 CL/cos^2 L)^1.5 = M*`.
    /// The frozen-law analysis, used only by objective parity fixtures
    /// generated with the old law, reports `M_crit = M_dd`. There is no
    /// onset-Mach gate here: this is the analytic value, not a drag.
    pub fn korn_mach_numbers(&self, cl: f64, section_thickness: Option<f64>) -> (f64, f64) {
        let law = self.divergence_law(section_thickness);
        (law.drag_divergence_mach(cl), law.critical_mach(cl))
    }
}

// A test constructs the geometry it asserts on directly, so a failed expect is
// the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::spacing::linspace;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn plane_with(sections: &[([f64; 3], f64)]) -> Airplane {
        let airfoil = Airfoil::from_name("naca2412").expect("a 4-digit NACA name");
        let xsecs = sections
            .iter()
            .map(|&(xyz, chord)| WingXSec::new(xyz, chord, 0.0, airfoil.clone()))
            .collect();
        Airplane {
            name: "sweep probe".to_owned(),
            xyz_ref: [0.0; 3],
            wings: vec![Wing::new("Main Wing", xsecs, true)],
            fuselages: Vec::new(),
            s_ref: 1.0,
            c_ref: 1.0,
            b_ref: 1.0,
        }
    }

    /// Leading-edge x that puts the quarter chord at `qc_x`.
    fn le_for_quarter_chord(qc_x: f64, chord: f64) -> f64 {
        qc_x - 0.25 * chord
    }

    #[test]
    fn a_yehudi_planform_reads_the_reference_trapezoid_sweep() {
        // One 34 degree leading edge from root to tip, with the inboard
        // trailing edge extended so that the inboard panel's quarter-chord
        // line is less swept than the outboard one (the AVE/777 layout). The
        // reference trapezoid is the outboard panel carried to the
        // centreline, so its quarter-chord sweep is the outboard panel's.
        let tan_le = 34.0_f64.to_radians().tan();
        let (kink_y, tip_y) = (12.5, 35.0);
        let (root_c, kink_c, tip_c) = (16.0, 8.0, 2.1);
        let outboard_c4 = (tan_le - 0.25 * (kink_c - tip_c) / (tip_y - kink_y))
            .atan()
            .to_degrees();
        let inboard_c4 = (tan_le - 0.25 * (root_c - kink_c) / kink_y)
            .atan()
            .to_degrees();
        // The builder subdivides each planform panel; two subdivisions of the
        // outboard panel must read as one panel.
        let mid_y = 0.5 * (kink_y + tip_y);
        let mid_c = 0.5 * (kink_c + tip_c);
        let plane = plane_with(&[
            ([0.0, 0.0, 0.0], root_c),
            ([kink_y * tan_le, kink_y, 0.5], kink_c),
            ([mid_y * tan_le, mid_y, 1.2], mid_c),
            ([tip_y * tan_le, tip_y, 2.0], tip_c),
        ]);
        let sweep = AeroAnalysis::quarter_chord_sweep_deg(&plane, 0.0);
        assert!(
            (sweep - outboard_c4).abs() < 1e-9,
            "{sweep} vs {outboard_c4}"
        );
        assert!(inboard_c4 < outboard_c4 - 3.0);
    }

    /// A two-panel wing with a raked tip from 90 % of the span outward: crank at
    /// 40 %, straight panel from there to 90 %.
    fn cranked_raked_sections() -> Vec<([f64; 3], f64)> {
        vec![
            ([0.0, 0.0, 0.0], 9.0),
            ([4.8, 8.0, 0.4], 5.0),
            ([11.4, 18.0, 1.0], 2.2),
            ([14.2, 20.0, 1.1], 0.8),
        ]
    }

    /// `sections` with panel `index` split at `fraction` by a linearly lofted
    /// station, which leaves the wing's shape as it was.
    fn split_panel(
        sections: &[([f64; 3], f64)],
        index: usize,
        fraction: f64,
    ) -> Vec<([f64; 3], f64)> {
        let (a, b) = (sections[index], sections[index + 1]);
        let lerp = |p: f64, q: f64| p + fraction * (q - p);
        let added = (
            [
                lerp(a.0[0], b.0[0]),
                lerp(a.0[1], b.0[1]),
                lerp(a.0[2], b.0[2]),
            ],
            lerp(a.1, b.1),
        );
        let mut out = sections.to_vec();
        out.insert(index + 1, added);
        out
    }

    #[test]
    fn splitting_a_panel_without_changing_its_shape_changes_no_sweep_and_no_wave_drag() {
        let base = cranked_raked_sections();
        let plain = plane_with(&base);
        let sweep = AeroAnalysis::quarter_chord_sweep_deg(&plain, 0.0);
        let analysis = AeroAnalysis::new(&plain, sweep, None, None, None);
        let korn = analysis.korn_sweep_deg();
        let wave = analysis.wave_drag(0.84, 0.5, Some(0.11));
        assert!(wave > 0.0);
        for index in 0..base.len() - 1 {
            for fraction in [0.137, 0.5, 0.9732] {
                let split = plane_with(&split_panel(&base, index, fraction));
                let split_sweep = AeroAnalysis::quarter_chord_sweep_deg(&split, 0.0);
                let split_analysis = AeroAnalysis::new(&split, split_sweep, None, None, None);
                assert!((split_sweep - sweep).abs() < 1e-11, "{index} {fraction}");
                assert!((split_analysis.korn_sweep_deg() - korn).abs() < 1e-11);
                assert!(
                    (split_analysis.korn_thickness() - analysis.korn_thickness()).abs() < 1e-12
                );
                let split_wave = split_analysis.wave_drag(0.84, 0.5, Some(0.11));
                assert!((split_wave - wave).abs() < 1e-12, "{index} {fraction}");
            }
        }
    }

    /// A wing of `(y, chord, NACA name)` sections, unswept at the quarter
    /// chord.
    fn sectioned_plane(sections: &[(f64, f64, &str)]) -> Airplane {
        let mut plane = plane_with(&[([0.0, 0.0, 0.0], 1.0), ([0.0, 1.0, 0.0], 1.0)]);
        plane.wings[0].xsecs = sections
            .iter()
            .map(|&(y, chord, name)| {
                let airfoil = Airfoil::from_name(name).expect("a 4-digit NACA name");
                WingXSec::new(
                    [le_for_quarter_chord(0.0, chord), y, 0.0],
                    chord,
                    0.0,
                    airfoil,
                )
            })
            .collect();
        plane
    }

    fn t_over_c(name: &str) -> f64 {
        let samples = linspace(0.0, 1.0, super::super::MAX_THICKNESS_SAMPLES);
        Airfoil::from_name(name)
            .expect("a 4-digit NACA name")
            .max_thickness(&samples)
    }

    fn geometry_with_kink(kink: f64, rake: Option<f64>) -> alas_config::GeometryConfig {
        let mut geometry = alas_config::GeometryConfig::default();
        geometry.wing.kink_span_fraction = Some(kink);
        geometry.wing.tip_rake_start_span_fraction = rake;
        geometry
    }

    #[test]
    fn a_cranked_wing_takes_the_thickness_at_its_leading_edge_kink() {
        // Torenbeek, Advanced Aircraft Design, sec. 10.6.1: for a complex wing
        // the thickness ratio at the kink, not a mean over the span. The chord
        // falls 0.5/m inboard of the 40 % kink and 0.25/m outboard of it.
        let plane = sectioned_plane(&[
            (0.0, 9.0, "naca0015"),
            (8.0, 5.0, "naca0012"),
            (20.0, 2.0, "naca0009"),
        ]);
        let analysis =
            AeroAnalysis::new(&plane, 0.0, Some(geometry_with_kink(0.4, None)), None, None);
        assert!((analysis.korn_thickness() - t_over_c("naca0012")).abs() < 1e-12);
        // The same declared kink inside a panel, where the chord slope does
        // not change, is no kink: the wing is straight tapered there.
        let straight = sectioned_plane(&[
            (0.0, 9.0, "naca0015"),
            (8.0, 6.0, "naca0012"),
            (20.0, 1.5, "naca0009"),
        ]);
        let analysis = AeroAnalysis::new(
            &straight,
            0.0,
            Some(geometry_with_kink(0.4, None)),
            None,
            None,
        );
        let mean = 0.5 * (t_over_c("naca0015") + t_over_c("naca0009"));
        assert!((analysis.korn_thickness() - mean).abs() < 1e-12);
    }

    #[test]
    fn a_straight_tapered_wing_takes_the_mean_of_root_and_tip_thickness() {
        // Torenbeek, Advanced Aircraft Design, sec. 10.6.1: the mean of the
        // root and the tip thickness ratios, whatever lies between.
        let plane = sectioned_plane(&[(0.0, 4.0, "naca0015"), (10.0, 2.0, "naca0009")]);
        let analysis = AeroAnalysis::new(
            &plane,
            0.0,
            Some(geometry_with_kink(0.37, None)),
            None,
            None,
        );
        let mean = 0.5 * (t_over_c("naca0015") + t_over_c("naca0009"));
        assert!((analysis.korn_thickness() - mean).abs() < 1e-12);
    }

    #[test]
    fn the_outboard_wing_runs_from_the_declared_kink_to_the_rake_start() {
        // Cranked at 40 %, raked from 90 %: the mid-chord line between them is
        // the straight middle panel's, however the rake is drawn.
        let base = cranked_raked_sections();
        let mid = |s: &([f64; 3], f64)| s.0[0] + 0.5 * s.1;
        let expected = ((mid(&base[2]) - mid(&base[1])) / (base[2].0[1] - base[1].0[1]))
            .atan()
            .to_degrees();
        let mut raked = base.clone();
        for fraction in [0.3, 0.5, 0.8] {
            raked = split_panel(&raked, raked.len() - 2, fraction);
        }
        for wing in [&base, &raked] {
            let plane = plane_with(wing);
            let sweep = AeroAnalysis::quarter_chord_sweep_deg(&plane, 0.0);
            let analysis = AeroAnalysis::new(
                &plane,
                sweep,
                Some(geometry_with_kink(0.4, Some(0.9))),
                None,
                None,
            );
            assert!(
                (analysis.korn_sweep_deg() - expected).abs() < 1e-11,
                "{}",
                analysis.korn_sweep_deg()
            );
        }
    }

    #[test]
    fn a_raked_tip_of_any_station_count_does_not_set_the_sweep() {
        // The rake over the last 10 % of the semispan is outside the window,
        // however many stations draw it.
        let base = cranked_raked_sections();
        let expected = {
            let qc = |s: &([f64; 3], f64)| s.0[0] + 0.25 * s.1;
            ((qc(&base[2]) - qc(&base[1])) / (base[2].0[1] - base[1].0[1]))
                .atan()
                .to_degrees()
        };
        let mut sections = base.clone();
        for fraction in [0.3, 0.5, 0.8] {
            sections = split_panel(&sections, sections.len() - 2, fraction);
        }
        for wing in [&base, &sections] {
            let sweep = AeroAnalysis::quarter_chord_sweep_deg(&plane_with(wing), 0.0);
            assert!((sweep - expected).abs() < 1e-11, "{sweep} vs {expected}");
        }
    }

    #[test]
    fn the_korn_relation_reads_the_mid_chord_line_of_the_outboard_wing() {
        // Quarter chord swept 30 deg, chord falling 0.2 per metre of span: the
        // mid-chord line has tan L = tan 30 - 0.25 * 0.2.
        let tan_qc = 30.0_f64.to_radians().tan();
        let chord = |y: f64| 6.0 - 0.2 * y;
        let section = |y: f64| {
            (
                [le_for_quarter_chord(y * tan_qc, chord(y)), y, 0.0],
                chord(y),
            )
        };
        let plane = plane_with(&[section(0.0), section(20.0)]);
        let eq_10_48 = {
            let mut geometry = alas_config::GeometryConfig::default();
            geometry.wing.drag_divergence_law = alas_config::DragDivergenceLaw::Torenbeek1048;
            Some(geometry)
        };
        let analysis = AeroAnalysis::new(&plane, 30.0, eq_10_48, None, None);
        let expected = (tan_qc - 0.05).atan().to_degrees();
        assert!((analysis.korn_sweep_deg() - expected).abs() < 1e-11);
        // Eq. 10.48 is read on that line.
        let cos = expected.to_radians().cos();
        let mach_dd = (0.935 - 0.11 / cos - 0.10 * (1.1 * 0.5 / (cos * cos)).powf(1.5)) / cos;
        let (actual, _) = analysis.korn_mach_numbers(0.5, Some(0.11));
        assert!((actual - mach_dd).abs() < 1e-12);
    }

    #[test]
    fn a_straight_wing_has_zero_quarter_chord_sweep() {
        let plane = plane_with(&[([0.0, 0.0, 0.0], 3.0), ([0.0, 8.0, 0.0], 3.0)]);
        assert!(AeroAnalysis::quarter_chord_sweep_deg(&plane, 12.0).abs() < 1e-12);
    }

    #[test]
    fn a_tapered_wing_with_an_unswept_quarter_chord_reads_zero() {
        let plane = plane_with(&[
            ([0.0, 0.0, 0.0], 4.0),
            ([le_for_quarter_chord(1.0, 2.0), 10.0, 0.0], 2.0),
        ]);
        assert!(AeroAnalysis::quarter_chord_sweep_deg(&plane, 12.0).abs() < 1e-12);
    }

    #[test]
    fn a_near_vertical_winglet_panel_does_not_move_the_sweep() {
        // A 25 degree quarter-chord wing with and without a 2.5 m winglet that
        // rises 0.4 m outboard for every 2.5 m up and is swept back 1.5 m.
        let tip_qc = 16.0 * 25.0_f64.to_radians().tan();
        let wing = [
            ([0.0, 0.0, 0.0], 6.0),
            (
                [le_for_quarter_chord(0.25 * 6.0 + tip_qc, 2.0), 16.0, 0.0],
                2.0,
            ),
        ];
        let winglet_tip = (
            [
                le_for_quarter_chord(0.25 * 6.0 + tip_qc + 1.5, 1.0),
                16.4,
                2.5,
            ],
            1.0,
        );
        let clean = AeroAnalysis::quarter_chord_sweep_deg(&plane_with(&wing), 0.0);
        let with_winglet = AeroAnalysis::quarter_chord_sweep_deg(
            &plane_with(&[wing[0], wing[1], winglet_tip]),
            0.0,
        );
        assert!((clean - 25.0).abs() < 1e-9, "{clean}");
        assert!((with_winglet - clean).abs() < 1e-12, "{with_winglet}");
        // A wing made only of a vertical panel has no retained panel.
        let fin = plane_with(&[([0.0, 0.0, 0.0], 2.0), ([0.5, 0.1, 3.0], 1.0)]);
        assert_eq!(AeroAnalysis::quarter_chord_sweep_deg(&fin, 12.0), 12.0);
    }

    #[test]
    fn the_korn_mach_numbers_bracket_the_wave_drag_onset() {
        let plane = plane_with(&[
            ([0.0, 0.0, 0.0], 5.0),
            ([le_for_quarter_chord(6.0, 2.0), 15.0, 0.0], 2.0),
        ]);
        let lock_rise = {
            let mut geometry = alas_config::GeometryConfig::default();
            geometry.wing.drag_divergence_law = alas_config::DragDivergenceLaw::Torenbeek1048;
            Some(geometry)
        };
        let analysis = AeroAnalysis::new(&plane, 30.0, lock_rise, None, None);
        let (mach_dd, mach_crit) = analysis.korn_mach_numbers(0.5, None);
        assert!(mach_dd.is_finite() && mach_crit.is_finite());
        assert!(mach_crit < mach_dd);
        // The accessor and the drag law share one critical Mach: no wave drag
        // below it, a positive rise above it.
        assert_eq!(analysis.wave_drag(mach_crit - 1e-6, 0.5, None), 0.0);
        assert!(analysis.wave_drag(mach_crit + 0.02, 0.5, None) > 0.0);
    }

    #[test]
    fn a_wing_without_two_sections_returns_the_fallback() {
        let plane = plane_with(&[([0.0, 0.0, 0.0], 3.0)]);
        assert_eq!(AeroAnalysis::quarter_chord_sweep_deg(&plane, 12.0), 12.0);
    }

    /// The divergence law of a straight test wing analysed at `sweep_deg`
    /// with the declared section class `class` and Korn factor override
    /// `kappa`.
    fn law_of(
        class: alas_config::AirfoilClass,
        kappa: Option<f64>,
        sweep_deg: f64,
        thickness: f64,
    ) -> DivergenceLaw {
        let plane = plane_with(&[([0.0, 0.0, 0.0], 3.0), ([0.0, 8.0, 0.0], 3.0)]);
        let mut geometry = alas_config::GeometryConfig::default();
        geometry.wing.airfoil_class = class;
        geometry.wing.korn_technology_factor_override = kappa;
        // The section-class rule among the two Lock-rise relations.
        geometry.wing.drag_divergence_law = if class.is_supercritical() && kappa.is_none() {
            alas_config::DragDivergenceLaw::Torenbeek1048
        } else {
            alas_config::DragDivergenceLaw::Korn
        };
        AeroAnalysis::new(&plane, sweep_deg, Some(geometry), None, None)
            .divergence_law(Some(thickness))
    }

    /// The default (Shevell-Kroo) law of the same test wing.
    fn shevell_law(
        class: alas_config::AirfoilClass,
        sweep_deg: f64,
        thickness: f64,
    ) -> DivergenceLaw {
        let plane = plane_with(&[([0.0, 0.0, 0.0], 3.0), ([0.0, 8.0, 0.0], 3.0)]);
        let mut geometry = alas_config::GeometryConfig::default();
        geometry.wing.airfoil_class = class;
        AeroAnalysis::new(&plane, sweep_deg, Some(geometry), None, None)
            .divergence_law(Some(thickness))
    }

    #[test]
    fn the_default_law_is_the_shevell_kroo_chart_with_the_generation_increment() {
        use alas_config::AirfoilClass::{
            Conventional, Peaky, Supercritical, SupercriticalAdvanced,
        };
        // The chart regression as SUAVE carries it, written out for a 25 deg
        // quarter-chord sweep, t/c 0.11 and CL 0.5.
        let (sweep_deg, thickness, cl) = (25.0_f64, 0.11, 0.5);
        let cos = sweep_deg.to_radians().cos();
        let (tc, n) = (thickness / cos, cl / (cos * cos));
        let chart = 0.922_321_524_499_352 - 1.153_885_166_170_62 * tc - 0.304_541_067_183_461 * n
            + 0.332_881_324_404_729 * tc * tc
            + 0.467_317_361_111_105 * tc * n
            + 0.087_490_431_201_549 * n * n;
        let peaky = shevell_law(Peaky, sweep_deg, thickness);
        assert!((peaky.critical_mach(cl) - chart / cos).abs() < 1e-12);
        assert!(
            (peaky.drag_divergence_mach(cl) - chart / cos * (1.02 + 0.08 * (1.0 - cos))).abs()
                < 1e-12
        );
        for (class, increment) in [
            (Conventional, -0.04),
            (Supercritical, 0.035),
            (SupercriticalAdvanced, 0.06),
        ] {
            let law = shevell_law(class, sweep_deg, thickness);
            assert!((law.critical_mach(cl) - peaky.critical_mach(cl) - increment).abs() < 1e-12);
        }
        // Shevell's curve: 0.0019 cos^3 at the crest-critical Mach, and the
        // creep below it.
        let crest = peaky.critical_mach(cl);
        assert!((peaky.wave_cd(crest, cl, 20.0) - 0.0019 * cos.powi(3)).abs() < 1e-15);
        let creep = peaky.wave_cd(0.9 * crest, cl, 20.0) / peaky.wave_cd(crest, cl, 20.0);
        assert!((creep - 0.9_f64.powf(14.641)).abs() < 1e-12);
        assert!(!peaky.takes_onset_gate() && peaky.onset_mach(cl) == 0.0);
    }

    #[test]
    fn the_shevell_drag_and_its_lift_slope_are_smooth_past_the_charted_lift() {
        use alas_config::AirfoilClass::Supercritical;
        let law = shevell_law(Supercritical, 25.0, 0.11);
        let mach = 0.78;
        // Across the edge of the chart (sweep-normal CL 1) and below zero lift.
        for cl in [-0.1, 0.0, 0.3, 0.6, 0.80, 0.8214, 0.83, 1.2] {
            let h = 1e-6;
            let value = law.wave_cd(mach, cl, 20.0);
            let numeric =
                (law.wave_cd(mach, cl + h, 20.0) - law.wave_cd(mach, cl - h, 20.0)) / (2.0 * h);
            let slope = law.wave_cd_lift_slope(mach, cl, value);
            assert!(
                (slope - numeric).abs() < 1e-6 * (1.0 + numeric.abs()),
                "{cl}: {slope} {numeric}"
            );
            assert!(value > 0.0 && value.is_finite());
        }
        // More lift, more compressibility drag, all the way up.
        let mut last = 0.0;
        for step in 0..=30 {
            let value = law.wave_cd(mach, 0.05 * f64::from(step), 20.0);
            assert!(value > last);
            last = value;
        }
    }

    /// `M_dd` of [`law_of`] at `cl`.
    fn mach_dd(
        class: alas_config::AirfoilClass,
        kappa: Option<f64>,
        sweep_deg: f64,
        thickness: f64,
        cl: f64,
    ) -> f64 {
        law_of(class, kappa, sweep_deg, thickness).drag_divergence_mach(cl)
    }

    #[test]
    fn the_korn_relation_is_the_two_dimensional_law_on_the_sweep_normal_flow() {
        use alas_config::AirfoilClass::{Conventional, Supercritical};
        // Unswept, it is Korn's 2-D relation M_dd + cl/10 + t/c = kappa
        // (Mason, Configuration Aerodynamics, ch. 7): a 10 % section with a
        // declared factor 0.95 at cl 0.5 diverges at 0.95 - 0.10 - 0.05 =
        // 0.80, a 12 % NACA 6-series section at cl 0.4 at 0.87 - 0.12 - 0.04
        // = 0.71.
        assert!((mach_dd(Supercritical, Some(0.95), 0.0, 0.10, 0.5) - 0.80).abs() < 1e-12);
        assert!((mach_dd(Conventional, None, 0.0, 0.12, 0.4) - 0.71).abs() < 1e-12);
        // Swept, simple sweep theory applies the same law to the flow normal
        // to the sweep line: M_n = M cos L, (t/c)_n = (t/c)/cos L and
        // cl_n = CL/cos^2 L.
        for sweep_deg in [10.0_f64, 25.0, 35.0] {
            let cos = sweep_deg.to_radians().cos();
            let (thickness, cl) = (0.11, 0.55);
            let normal = 0.87 - thickness / cos - cl / (cos * cos) / 10.0;
            let swept = mach_dd(Conventional, None, sweep_deg, thickness, cl);
            assert!((swept * cos - normal).abs() < 1e-12, "{sweep_deg}");
        }
    }

    #[test]
    fn a_supercritical_wing_takes_torenbeeks_eq_10_48() {
        use alas_config::AirfoilClass::Supercritical;
        // Advanced Aircraft Design, Eqs. 10.47 and 10.48: unswept, a 10 %
        // section at wing CL 0.5 (section cl 1.1 x 0.5 outboard) diverges at
        // 0.935 - 0.10 - 0.10 x 0.55^1.5 = 0.79421.
        assert!((mach_dd(Supercritical, None, 0.0, 0.10, 0.5) - 0.794210).abs() < 1e-6);
        // Swept: M_dd cos L + t/c / cos L + 0.10 (1.1 CL / cos^2 L)^1.5 = M*.
        for sweep_deg in [10.0_f64, 25.0, 35.0] {
            let cos = sweep_deg.to_radians().cos();
            let (thickness, cl) = (0.11, 0.55);
            let lhs = mach_dd(Supercritical, None, sweep_deg, thickness, cl) * cos
                + thickness / cos
                + 0.10 * (1.1 * cl / (cos * cos)).powf(1.5);
            assert!((lhs - SUPERCRITICAL_M_STAR).abs() < 1e-12, "{sweep_deg}");
        }
        // A negative CL carries no lift term rather than a complex one.
        let at_zero = mach_dd(Supercritical, None, 25.0, 0.11, 0.0);
        assert_eq!(mach_dd(Supercritical, None, 25.0, 0.11, -0.2), at_zero);
    }

    #[test]
    fn the_critical_mach_slope_is_the_derivative_of_the_law() {
        use alas_config::AirfoilClass::{Conventional, Supercritical};
        for law in [
            law_of(Supercritical, None, 28.0, 0.11),
            law_of(Conventional, None, 28.0, 0.11),
        ] {
            for cl in [0.2, 0.5, 0.8] {
                let h = 1e-6;
                let numeric = (law.critical_mach(cl + h) - law.critical_mach(cl - h)) / (2.0 * h);
                assert!((law.critical_mach_slope(cl) - numeric).abs() < 1e-7, "{cl}");
            }
        }
    }

    #[test]
    fn divergence_rises_with_technology_and_falls_with_thickness_and_lift() {
        use alas_config::AirfoilClass::{Conventional, Supercritical};
        // Over the transport domain (c/4 sweep 0-40 deg, t/c 0.08-0.14,
        // CL 0.3-0.7) the thickness and lift terms of both laws move M_dd one
        // way, and the supercritical law diverges above the NACA 6-series one.
        // Korn's sweep term raises M_dd while kappa cos^2 L > 2 (t/c) cos L +
        // 0.3 CL, which holds throughout (0.511 > 0.424 for the conventional
        // 0.87 at the corner L = 40 deg, t/c 0.14, CL 0.7). Eq. 10.48's lift
        // term grows as 1/cos^4 L, so at that corner sweep lowers its M_dd; it
        // is not asserted to rise there.
        for sweep in [0.0, 20.0, 40.0] {
            for thickness in [0.08, 0.11, 0.14] {
                for cl in [0.3, 0.5, 0.7] {
                    let base = mach_dd(Supercritical, None, sweep, thickness, cl);
                    let korn = mach_dd(Conventional, None, sweep, thickness, cl);
                    assert!(korn < base);
                    assert!(mach_dd(Conventional, None, sweep + 0.5, thickness, cl) > korn);
                    for class in [Supercritical, Conventional] {
                        let at = mach_dd(class, None, sweep, thickness, cl);
                        assert!(mach_dd(class, None, sweep, thickness + 0.005, cl) < at);
                        assert!(mach_dd(class, None, sweep, thickness, cl + 0.05) < at);
                    }
                }
            }
        }
        // A declared Korn factor shifts Korn's M_dd by the factor difference
        // over cos L.
        let cos = 30.0_f64.to_radians().cos();
        let shift = mach_dd(Supercritical, Some(0.95), 30.0, 0.11, 0.5)
            - mach_dd(Conventional, None, 30.0, 0.11, 0.5);
        assert!((shift - (0.95 - 0.87) / cos).abs() < 1e-12);
    }

    /// Saving and reloading a configuration must not change its physics: a
    /// LJ60 (declared conventional, no Korn override) redeclared
    /// supercritical keeps kappa_A = 0.95, its drag-divergence Mach and its
    /// wave drag through a JSON round trip, though the preset reapplies its
    /// own class on load.
    #[test]
    fn a_saved_airfoil_class_that_differs_from_its_preset_survives_reload() {
        use alas_config::{AirfoilClass, AlasConfig};
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "LJ60" }))
            .expect("the LJ60 preset loads");
        assert_eq!(
            config.geometry.wing.airfoil_class,
            AirfoilClass::Conventional
        );
        assert_eq!(config.geometry.wing.korn_technology_factor_override, None);
        config.geometry.wing.airfoil_class = AirfoilClass::Supercritical;
        let saved = serde_json::to_value(&config).expect("the configuration serializes");
        let reloaded = AlasConfig::from_value(&saved).expect("the saved configuration loads");
        assert_eq!(
            reloaded.geometry.wing.airfoil_class,
            AirfoilClass::Supercritical
        );

        let tan_le = 35.0_f64.to_radians().tan();
        let plane = plane_with(&[([0.0, 0.0, 0.0], 10.0), ([20.0 * tan_le, 20.0, 0.0], 3.0)]);
        let physics = |config: &AlasConfig| {
            let analysis =
                AeroAnalysis::new(&plane, 35.0, Some(config.geometry.clone()), None, None);
            let (mach_dd, _) = analysis.korn_mach_numbers(0.5, Some(0.11));
            let wave = analysis.wave_drag(0.86, 0.5, Some(0.11));
            assert!(mach_dd.is_finite() && wave.is_finite() && wave >= 0.0);
            (analysis.korn_technology_factor(), mach_dd, wave)
        };
        let before = physics(&config);
        assert_eq!(before.0, 0.95);
        assert_eq!(physics(&reloaded), before);
    }

    #[test]
    fn the_a320_korn_point_reproduces_the_audit_closed_form() {
        // DRG-04 worked point: kappa 0.95, sweep 25 deg, t/c 0.11, CL 0.5.
        // M_dd = 0.95/cos25 - 0.11/cos^2 25 - 0.5/(10 cos^3 25) = 0.84712,
        // M_crit = M_dd - (0.1/80)^(1/3) = 0.73940, CDw = 20 (M - M_crit)^4:
        // 0.54 count at M 0.78, 8.44 counts at M 0.82. The chord is constant
        // so that every chord line has the same sweep. Korn's relation with
        // that factor is the declared-override path.
        let tan_qc = 25.0_f64.to_radians().tan();
        let plane = plane_with(&[([0.0, 0.0, 0.0], 4.0), ([16.0 * tan_qc, 16.0, 0.0], 4.0)]);
        let mut geometry = alas_config::GeometryConfig::default();
        geometry.wing.korn_technology_factor_override = Some(0.95);
        let analysis = AeroAnalysis::new(&plane, 25.0, Some(geometry), None, None);
        let (mach_dd, mach_crit) = analysis.korn_mach_numbers(0.5, Some(0.11));
        assert!((mach_dd - 0.847124).abs() < 1e-5, "{mach_dd}");
        assert!((mach_crit - 0.739402).abs() < 1e-5, "{mach_crit}");
        let counts = |mach: f64| 1.0e4 * analysis.wave_drag(mach, 0.5, Some(0.11));
        assert!((counts(0.78) - 0.543).abs() < 0.01, "{}", counts(0.78));
        assert!((counts(0.82) - 8.44).abs() < 0.02, "{}", counts(0.82));
    }

    /// An explicit law holds whatever the section class; the default is the
    /// Shevell-Kroo method, a Korn factor override selects Korn's relation
    /// under it and changes nothing under Eq. 10.48.
    #[test]
    fn an_explicit_divergence_law_holds_on_either_section_class() {
        use alas_config::{AirfoilClass, DragDivergenceLaw, GeometryConfig};
        let tan_le = 30.0_f64.to_radians().tan();
        let plane = plane_with(&[([0.0, 0.0, 0.0], 8.0), ([18.0 * tan_le, 18.0, 0.0], 2.5)]);
        let mach_dd = |law, class, korn_override| {
            let mut geometry = GeometryConfig::default();
            geometry.wing.drag_divergence_law = law;
            geometry.wing.airfoil_class = class;
            geometry.wing.korn_technology_factor_override = korn_override;
            let analysis = AeroAnalysis::new(&plane, 30.0, Some(geometry), None, None);
            analysis.korn_mach_numbers(0.5, Some(0.11)).0
        };
        use AirfoilClass::{Conventional, Supercritical};
        use DragDivergenceLaw::{Auto, Korn, Shevell, Torenbeek1048};
        let torenbeek = mach_dd(Torenbeek1048, Supercritical, None);
        let korn_supercritical = mach_dd(Korn, Supercritical, None);
        let korn_conventional = mach_dd(Korn, Conventional, None);
        // The default is the Shevell-Kroo method on either class.
        for class in [Supercritical, Conventional] {
            assert_eq!(mach_dd(Auto, class, None), mach_dd(Shevell, class, None));
            assert_eq!(
                mach_dd(Shevell, class, Some(0.90)),
                mach_dd(Shevell, class, None)
            );
        }
        assert!(mach_dd(Auto, Supercritical, None) > mach_dd(Auto, Conventional, None));
        assert_eq!(mach_dd(Auto, Supercritical, Some(0.95)), korn_supercritical);
        assert_eq!(mach_dd(Torenbeek1048, Conventional, None), torenbeek);
        assert_eq!(mach_dd(Torenbeek1048, Supercritical, Some(0.90)), torenbeek);
        assert!(
            (korn_supercritical - korn_conventional - 0.08 / 30.0_f64.to_radians().cos()).abs()
                < 0.02
        );
        assert!((torenbeek - korn_supercritical).abs() > 1.0e-4);
        assert!(mach_dd(Korn, Supercritical, Some(0.90)) < korn_supercritical);
    }

    #[test]
    fn the_korn_override_replaces_the_class_value_inside_its_range_only() {
        use alas_config::geometry::GeometryConfig;
        let plane = plane_with(&[([0.0, 0.0, 0.0], 6.0), ([8.0, 16.0, 0.0], 2.0)]);
        let with = |value: Option<f64>| {
            let mut geometry = GeometryConfig::default();
            geometry.wing.korn_technology_factor_override = value;
            AeroAnalysis::new(&plane, 25.0, Some(geometry), None, None).korn_technology_factor()
        };
        // Supercritical default 0.95; override 0.95 and 0.87 equal the enum.
        assert_eq!(with(None), 0.95);
        assert_eq!(with(Some(0.95)), 0.95);
        assert_eq!(with(Some(0.87)), 0.87);
        assert_eq!(with(Some(0.91)), 0.91);
        // Out-of-range and non-finite overrides are never used.
        assert_eq!(with(Some(1.5)), 0.95);
        assert_eq!(with(Some(f64::NAN)), 0.95);
        let mut wing = alas_config::geometry::GeometryConfig::default().wing;
        wing.korn_technology_factor_override = Some(0.5);
        assert!(wing.korn_override_error().is_some());
    }

    #[test]
    fn the_shevell_chart_break_is_where_the_critical_mach_curvature_jumps() {
        let law = DivergenceLaw {
            relation: Relation::Shevell { increment: 0.035 },
            cos_sweep: 19.5_f64.to_radians().cos(),
            thickness: 0.14,
            offset: 0.0,
        };
        let cl = law.lift_slope_break().unwrap();
        assert!((cl - law.cos_sweep * law.cos_sweep).abs() < 1e-15);
        // Second differences of M_cc on either side: the chart's curvature
        // 2 kll / cos^5 L below the break, zero along the tangent above.
        let h = 1e-3;
        let second = |at: f64| {
            (law.critical_mach(at + h) - 2.0 * law.critical_mach(at) + law.critical_mach(at - h))
                / (h * h)
        };
        let expected = 2.0 * SHEVELL_CREST_CHART[5] / law.cos_sweep.powi(5);
        assert!((second(cl - 2.0 * h) - expected).abs() < 1e-6 * expected);
        assert!(second(cl + 2.0 * h).abs() < 1e-6);
        let korn = DivergenceLaw {
            relation: Relation::Korn { kappa: 0.95 },
            ..law
        };
        assert_eq!(korn.lift_slope_break(), None);
    }
}
