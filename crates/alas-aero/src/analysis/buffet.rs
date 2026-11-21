// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cruise buffet onset from the pre-shock local Mach number, by table.
//!
//! Buffet onset is not drag divergence: Obert (*Aerodynamic Design of
//! Transport Aircraft*, IOS Press, 2009, ch. 17 pp. 109-110, Figs. 17.5 and
//! 17.6) puts it where the maximum local Mach number in front of the shock
//! reaches 1.38 (shock at 55 to 60 % chord). The Korn relation locates the
//! drag-creep limit instead, so inverting it returns the lift a transport
//! cruises at.
//!
//! The estimate:
//! 1. Two wing-only VLM solves at the cruise Mach number give every strip's
//!    basic and additional loading, `cl_local = cl_basic + cl_add CL`, for
//!    the strips of the outboard wing (planform kink to eta 0.8).
//! 2. Simple sweep theory on each strip: `M_n = M cos L_s`, `L_s` being the
//!    sweep of the line at 0.55 c on the local planform panel (the mean of
//!    the 0.5 c and 0.6 c lines); normal-section thickness and camber are
//!    the streamwise ones over `cos L_s`.
//! 3. The onset lift `cl_n` of each normal section comes from the offline
//!    MSES table of the strip's section family ([`super::buffet_table`]),
//!    trilinear in (`M_n`, thickness, camber). Outside the table the answer
//!    is typed ([`BuffetOnset::Extrapolated`], [`BuffetOnset::Unavailable`]);
//!    there is no fallback model.
//! 4. Separation starts at the lowest strip onset,
//!    `CL = (cl_n cos^2 L_s - cl_basic) / cl_add`.
//! 5. The certified boundary, which published boundaries and the 1.3 g
//!    operating margin refer to, is [`CERTIFIED_TO_FIRST_SEPARATION`] times
//!    that lift.
//!
//! Validation: against the certified boundaries of Obert 2009, Figs. 28.3
//! (DC-10), 28.10 (747-400) and 28.13 (A320), none of which enters the
//! method; the values are in the data-file header. Tables are generated at
//! one normal Reynolds number (see the data file); a digitised section is
//! only as good as its trace.

use super::buffet_table::{
    family_section, shipped_tables, thickness_and_camber, BuffetTables, TableLookup,
};
use super::AeroAnalysis;
use crate::wing_analysis::{
    analyse, AlphaSweep, AttitudeInput, FlightCondition, SpeedInput, SurfaceSet,
    WingAnalysisInputs, WingModel,
};
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::wing::WingXSec;

pub use super::buffet_table::BUFFET_PRE_SHOCK_MACH;

/// Spanwise window of the critical-strip search, fractions of the semispan.
const STRIP_WINDOW: (f64, f64) = (0.3, 0.8);

/// Lift coefficient of the certified buffet-onset boundary over the lift
/// coefficient at which buffet is first perceptible, 1.13.
///
/// The local-Mach criterion of the tables is a separation-onset criterion
/// (Obert, *Aerodynamic Design of Transport Aircraft*, IOS Press, 2009,
/// Fig. 17.6 p. 110: "separation onset defined by trailing-edge pressure
/// divergence"), the wind-tunnel indicator of Fig. 28.9 p. 374, and the first
/// strip to reach it is the earliest event on the wing. Published
/// boundaries, and the boundary the 1.3 g operating margin is taken from,
/// are certified ones: accelerometer recordings at the pilot's seat with a
/// peak-to-peak amplitude of 0.25 g (Obert 2009, ch. 28 p. 372). Fig. 28.6
/// p. 372 (Lockheed L-1011-1, from AGARD CP-242 paper 21) gives three
/// boundaries of one aircraft; digitised lift coefficients, certified
/// (FAA flight test, 0.25 g) / predicted / first perceptible in flight:
///
/// | Mach | certified | predicted | first perceptible | certified / first |
/// |---|---|---|---|---|
/// | 0.74 | 0.759 | 0.712 | 0.690 | 1.10 |
/// | 0.78 | 0.741 | 0.688 | 0.664 | 1.12 |
/// | 0.82 | 0.708 | 0.647 | 0.623 | 1.14 |
/// | 0.83 | 0.694 | 0.624 | 0.611 | 1.14 |
/// | 0.85 | 0.644 | 0.57 | 0.57 | 1.13 |
///
/// The value is the mean over the transport cruise range, Mach 0.78 to 0.85.
/// Against the predicted boundary the same mean is 1.10. The first strip to
/// separate cannot come later than the first perceptible buffet, so the
/// ratio to the first-perceptible boundary is a lower bound of the transfer
/// from first separation.
///
/// One transport's flight-test ratio applied to every wing: a flagged
/// fleet-wide definition transfer, not a property of each aircraft. Checked
/// against the certified boundaries of three other aircraft (Obert 2009,
/// Figs. 28.3, 28.10 and 28.13); see the data-file header for the values.
pub const CERTIFIED_TO_FIRST_SEPARATION: f64 = 1.13;

/// Wing lift coefficient at which the most loaded strip is named when no
/// onset exists (a family without a table): the generator's station listing
/// only, never an onset.
const UNTABULATED_WING_CL: f64 = 0.7;

/// Incidences of the two lattice solves that separate the basic from the
/// additional loading, degrees (any two distinct values: the lattice is
/// linear).
const LOADING_ALPHAS_DEG: (f64, f64) = (0.0, 4.0);

/// The critical strip and its normal-flow section.
#[derive(Debug, Clone)]
pub struct BuffetStrip {
    /// Semispan fraction of the strip.
    pub eta: f64,
    /// Basic section lift of the strip: its lift at zero wing lift (twist
    /// and camber loading of the wing-only lattice).
    pub basic_cl: f64,
    /// Additional section lift of the strip per unit wing lift.
    pub additional_cl: f64,
    /// Sweep of the 0.55 c line, degrees.
    pub sweep_deg: f64,
    /// Streamwise chord of the strip, m.
    pub chord_m: f64,
    /// The family with the largest weight in [`Self::families`].
    pub family: String,
    /// Section families the loft blends at the strip, with their weights
    /// (summing to one): the root section inboard of the kink, the linear
    /// root-to-tip blend outboard of it, as the planform builder lofts them.
    pub families: Vec<(String, f64)>,
    /// Streamwise section at the strip, as lofted.
    pub section: Airfoil,
    /// Normal Mach number `M cos L_s`.
    pub mach_normal: f64,
    /// Normal-section maximum thickness over chord.
    pub thickness_normal: f64,
    /// Normal-section maximum camber over chord.
    pub camber_normal: f64,
}

impl BuffetStrip {
    /// Section lift of the strip at the wing lift coefficient `wing_cl`:
    /// basic plus additional loading (Abbott and von Doenhoff, *Theory of
    /// Wing Sections*, Dover, 1959, ch. 1 sec. 1.4: the span loading of a
    /// twisted wing is the sum of a basic distribution, independent of the
    /// wing lift, and an additional one proportional to it).
    pub fn local_cl(&self, wing_cl: f64) -> f64 {
        self.basic_cl + self.additional_cl * wing_cl
    }

    /// Normal-section lift of the strip at the wing lift coefficient
    /// `wing_cl`: `cl_local / cos^2 L_s`.
    pub fn cl_n_at(&self, wing_cl: f64) -> f64 {
        self.local_cl(wing_cl) / self.sweep_deg.to_radians().cos().powi(2)
    }

    /// Wing lift coefficient at which the strip's normal-section lift is
    /// `cl_n`.
    pub fn wing_cl_at(&self, cl_n: f64) -> f64 {
        (cl_n * self.sweep_deg.to_radians().cos().powi(2) - self.basic_cl) / self.additional_cl
    }
}

/// Why no buffet onset could be given.
#[derive(Debug, Clone, PartialEq)]
pub enum BuffetUnavailable {
    /// The airplane has no lifting wing with a readable planform.
    NoWing,
    /// The lattice solve failed.
    LatticeFailed,
    /// No strip with eta in the search window.
    NoStrip,
    /// The strip's section family has no table.
    FamilyNotTabulated(String),
    /// The interpolation box has a cell without a value.
    MissingCell,
    /// The strip geometry is outside the physics of the method (sweep at or
    /// beyond 80 degrees, non-positive lift ratio, or non-finite values).
    OutOfDomain,
}

/// A buffet-onset answer with its provenance.
#[derive(Debug, Clone)]
pub enum BuffetOnset {
    /// Inside the table axes.
    Interpolated(BuffetEstimate),
    /// At least one axis was outside the table; the value is the clamped
    /// edge value and `axes` names what was clamped.
    Extrapolated(BuffetEstimate, Vec<&'static str>),
    /// No answer.
    Unavailable(BuffetUnavailable),
}

impl BuffetOnset {
    /// The wing lift coefficient at buffet onset for an interpolated or an
    /// extrapolated answer; `None` when unavailable.
    pub fn cl_b(&self) -> Option<f64> {
        match self {
            Self::Interpolated(e) | Self::Extrapolated(e, _) => Some(e.cl_b),
            Self::Unavailable(_) => None,
        }
    }
}

/// The onset lift and its working.
#[derive(Debug, Clone)]
pub struct BuffetEstimate {
    /// Wing lift coefficient at buffet onset as certified boundaries define
    /// it: [`CERTIFIED_TO_FIRST_SEPARATION`] times [`Self::separation_cl`].
    pub cl_b: f64,
    /// Wing lift coefficient at which the first strip reaches the
    /// separation-onset criterion.
    pub separation_cl: f64,
    /// Onset lift coefficient of the normal section.
    pub cl_n_onset: f64,
    /// The critical strip.
    pub strip: BuffetStrip,
}

/// The panel of `xsecs` holding `y` and the fraction along it.
fn panel_at(xsecs: &[WingXSec], y: f64) -> Option<(&WingXSec, &WingXSec, f64)> {
    let lifting: Vec<_> = xsecs
        .windows(2)
        .filter(|p| {
            let dy = (p[1].xyz_le[1] - p[0].xyz_le[1]).abs();
            let dz = (p[1].xyz_le[2] - p[0].xyz_le[2]).abs();
            dy > 0.0 && dz <= dy
        })
        .collect();
    let p = lifting
        .iter()
        .find(|p| y <= p[1].xyz_le[1].abs())
        .or(lifting.last())?;
    let (ya, yb) = (p[0].xyz_le[1].abs(), p[1].xyz_le[1].abs());
    Some((&p[0], &p[1], ((y - ya) / (yb - ya)).clamp(0.0, 1.0)))
}

/// Sweep, degrees, of the line at `fraction` of the local chord on a panel.
fn line_sweep_deg(a: &WingXSec, b: &WingXSec, fraction: f64) -> f64 {
    let dy = (b.xyz_le[1] - a.xyz_le[1]).abs();
    let dx = (b.xyz_le[0] + fraction * b.chord) - (a.xyz_le[0] + fraction * a.chord);
    (dx / dy).atan().to_degrees()
}

/// Onset lift of the normal section of `strip` from `tables`: each family's
/// table at the strip's normal conditions, blended with the loft's weights,
/// and the names of the table axes that were clamped.
///
/// # Errors
///
/// A family without a table, or an interpolation box without a value.
pub fn strip_onset_cl_n(
    tables: &BuffetTables,
    strip: &BuffetStrip,
) -> Result<(f64, Vec<&'static str>), BuffetUnavailable> {
    let mut cl_n = 0.0;
    let mut clamped: Vec<&'static str> = Vec::new();
    for (family, weight) in &strip.families {
        let Some(table) = tables.family(family) else {
            return Err(BuffetUnavailable::FamilyNotTabulated(family.clone()));
        };
        match table.lookup(
            strip.mach_normal,
            strip.thickness_normal,
            strip.camber_normal,
        ) {
            TableLookup::Interpolated(value) => cl_n += weight * value,
            TableLookup::Extrapolated { cl_n: value, axes } => {
                cl_n += weight * value;
                for axis in axes {
                    if !clamped.contains(&axis) {
                        clamped.push(axis);
                    }
                }
            }
            TableLookup::MissingCell => return Err(BuffetUnavailable::MissingCell),
        }
    }
    Ok((cl_n, clamped))
}

/// Which planform line sets the sweep of a strip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SweepLine {
    /// The shock line: mean of the 0.5 c and 0.6 c lines (the shipped choice).
    Shock,
    /// The line at this fraction of the local chord.
    Chord(f64),
}

impl AeroAnalysis<'_> {
    /// The critical buffet strip of the main wing at cruise Mach `mach` and
    /// altitude `altitude_m`.
    pub fn buffet_strip(
        &self,
        mach: f64,
        altitude_m: f64,
    ) -> Result<BuffetStrip, BuffetUnavailable> {
        match self.buffet_onset(mach, altitude_m) {
            BuffetOnset::Interpolated(e) | BuffetOnset::Extrapolated(e, _) => Ok(e.strip),
            BuffetOnset::Unavailable(_) => self
                .buffet_strip_candidates(mach, altitude_m, SweepLine::Shock, (1, 8))?
                .into_iter()
                .max_by(|a, b| {
                    a.cl_n_at(UNTABULATED_WING_CL)
                        .total_cmp(&b.cl_n_at(UNTABULATED_WING_CL))
                })
                .ok_or(BuffetUnavailable::NoStrip),
        }
    }

    /// Every strip of the search window with its normal-flow section, for
    /// the sweep line `line` and the lattice resolution `(spanwise,
    /// chordwise)`, each with its basic and additional loading.
    pub fn buffet_strip_candidates(
        &self,
        mach: f64,
        altitude_m: f64,
        line: SweepLine,
        resolution: (usize, usize),
    ) -> Result<Vec<BuffetStrip>, BuffetUnavailable> {
        let wing = self
            .plane
            .wings
            .first()
            .filter(|w| w.xsecs.len() >= 2)
            .ok_or(BuffetUnavailable::NoWing)?;
        let model =
            WingModel::from_surfaces("buffet strip", vec![wing.clone()], self.plane.xyz_ref)
                .map_err(|_| BuffetUnavailable::NoWing)?;
        // Two incidences give the basic and the additional loading of every
        // strip: the lattice is linear in incidence, a twisted wing's strip
        // lift is not proportional to the wing lift.
        let solve = |alpha_deg: f64| {
            let inputs = WingAnalysisInputs {
                surfaces: SurfaceSet::WingOnly,
                condition: FlightCondition {
                    altitude_m,
                    speed: SpeedInput::Mach(mach),
                    attitude: AttitudeInput::AngleOfAttack(alpha_deg),
                },
                moment_reference_m: model.reference().moment_reference_m,
                spanwise_resolution: resolution.0,
                chordwise_resolution: resolution.1,
                sweep: AlphaSweep {
                    min_deg: 0.0,
                    max_deg: 2.0,
                    points: 2,
                },
            };
            analyse(&model, &inputs, &|| false).map_err(|_| BuffetUnavailable::LatticeFailed)
        };
        let (low, high) = (solve(LOADING_ALPHAS_DEG.0)?, solve(LOADING_ALPHAS_DEG.1)?);
        let wing_cl_step = high.cl - low.cl;
        if !(wing_cl_step.is_finite() && wing_cl_step > 1e-6)
            || low.span_load.len() != high.span_load.len()
        {
            return Err(BuffetUnavailable::LatticeFailed);
        }
        let xsecs = &model.airplane().wings[0].xsecs;
        let mut strips = Vec::new();
        for (s, s_high) in low.span_load.iter().zip(&high.span_load) {
            let eta = s.y_over_semispan.abs();
            if !(STRIP_WINDOW.0..=STRIP_WINDOW.1).contains(&eta)
                || !s.section_cl.is_finite()
                || !s_high.section_cl.is_finite()
            {
                continue;
            }
            let additional_cl = (s_high.section_cl - s.section_cl) / wing_cl_step;
            let basic_cl = s.section_cl - additional_cl * low.cl;
            let y = s.y_m.abs();
            let Some((a, b, t)) = panel_at(xsecs, y) else {
                continue;
            };
            let sweep_deg = match line {
                SweepLine::Shock => 0.5 * (line_sweep_deg(a, b, 0.5) + line_sweep_deg(a, b, 0.6)),
                SweepLine::Chord(f) => line_sweep_deg(a, b, f),
            };
            let cos = sweep_deg.to_radians().cos();
            let Ok(section) = WingXSec::between(a, b, t) else {
                continue;
            };
            let normal = Airfoil::from_coordinates(
                section.airfoil.name.clone(),
                section
                    .airfoil
                    .coordinates
                    .iter()
                    .map(|&(x, y)| (x, y / cos))
                    .collect(),
            );
            let (thickness_normal, camber_normal) = thickness_and_camber(&normal);
            if !(cos > 0.17
                && basic_cl.is_finite()
                && additional_cl.is_finite()
                && additional_cl > 0.0
                && thickness_normal.is_finite())
            {
                continue;
            }
            let families = self.section_families(eta);
            let family = families
                .iter()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map_or_else(String::new, |f| f.0.clone());
            strips.push(BuffetStrip {
                eta,
                basic_cl,
                additional_cl,
                sweep_deg,
                chord_m: section.chord,
                family,
                families,
                section: section.airfoil,
                mach_normal: mach * cos,
                thickness_normal,
                camber_normal: camber_normal.abs(),
            });
        }
        if strips.is_empty() {
            return Err(BuffetUnavailable::NoStrip);
        }
        Ok(strips)
    }

    /// Wing lift coefficient at buffet onset at cruise Mach `mach` and
    /// altitude `altitude_m`, with its provenance; see the module
    /// documentation.
    pub fn buffet_onset(&self, mach: f64, altitude_m: f64) -> BuffetOnset {
        self.buffet_onset_with(shipped_tables(), mach, altitude_m)
    }

    /// [`Self::buffet_onset`] against explicit tables.
    pub fn buffet_onset_with(
        &self,
        tables: &BuffetTables,
        mach: f64,
        altitude_m: f64,
    ) -> BuffetOnset {
        let strips = match self.buffet_strip_candidates(mach, altitude_m, SweepLine::Shock, (1, 8))
        {
            Ok(strips) => strips,
            Err(reason) => return BuffetOnset::Unavailable(reason),
        };
        // Separation starts where the first strip of the outboard wing
        // reaches onset: every strip's onset wing lift from its own section,
        // normal conditions and loading, and the lowest of them. Strips
        // inboard of the planform kink are read only when no outboard strip
        // has a table value.
        let kink = self.outboard_wing_start();
        let count = strips.iter().filter(|s| s.eta >= kink).count();
        let mut valued = 0_usize;
        let mut first: Option<(BuffetEstimate, Vec<&'static str>)> = None;
        let mut inboard: Option<(BuffetEstimate, Vec<&'static str>)> = None;
        for strip in strips {
            let (cl_n, flags) = match strip_onset_cl_n(tables, &strip) {
                Ok(onset) => onset,
                Err(BuffetUnavailable::MissingCell) => continue,
                Err(reason) => return BuffetOnset::Unavailable(reason),
            };
            let outboard = strip.eta >= kink;
            valued += usize::from(outboard);
            let separation_cl = strip.wing_cl_at(cl_n);
            let slot = if outboard { &mut first } else { &mut inboard };
            if separation_cl.is_finite()
                && slot
                    .as_ref()
                    .is_none_or(|(e, _)| separation_cl < e.separation_cl)
            {
                *slot = Some((
                    BuffetEstimate {
                        cl_b: CERTIFIED_TO_FIRST_SEPARATION * separation_cl,
                        separation_cl,
                        cl_n_onset: cl_n,
                        strip,
                    },
                    flags,
                ));
            }
        }
        let (estimate, mut clamped) = match (first, inboard) {
            (Some(outboard), _) => outboard,
            (None, Some((estimate, mut flags))) => {
                flags.push("inboard-strips");
                (estimate, flags)
            }
            (None, None) => return BuffetOnset::Unavailable(BuffetUnavailable::MissingCell),
        };
        // Strips without a table value cannot be ruled out as the first to
        // reach onset: the answer says so.
        if valued < count {
            clamped.push("partial-strips");
        }
        if clamped.is_empty() {
            BuffetOnset::Interpolated(estimate)
        } else {
            BuffetOnset::Extrapolated(estimate, clamped)
        }
    }

    /// Semispan fraction where the outboard wing starts: the planform kink.
    ///
    /// Simple sweep theory holds on the outboard panel, where the isobars
    /// follow the constant-chord-fraction lines; inboard of the kink the
    /// trailing-edge extension unsweeps the local chord lines while the
    /// shock does not follow them, and the wind-tunnel separation indicator
    /// is read on the outboard wing (Obert 2009, ch. 28 p. 374: "trailing-edge
    /// pressure divergence on the outboard wing").
    fn outboard_wing_start(&self) -> f64 {
        let wing = &self.geometry.wing;
        let kink = wing.kink_span_fraction.unwrap_or(wing.break_span_fraction);
        if kink.is_finite() && kink < STRIP_WINDOW.1 {
            kink.max(STRIP_WINDOW.0)
        } else {
            STRIP_WINDOW.0
        }
    }

    /// Section families of the main wing at semispan fraction `eta`, with
    /// their loft weights. Without named section stations the planform
    /// builder draws the root section from the centreline to the kink and
    /// blends it linearly to the tip airfoil from the kink to the tip; with
    /// them (`section_stations`) the section law runs through the root
    /// section at the centreline, each named section at its station and the
    /// tip airfoil at the tip, linear in span between consecutive ones. A
    /// declared custom section is its own airfoil at its station, blended
    /// linearly with its neighbours.
    pub fn section_families(&self, eta: f64) -> Vec<(String, f64)> {
        let wing = &self.geometry.wing;
        let kink = wing
            .kink_span_fraction
            .unwrap_or(wing.break_span_fraction)
            .clamp(0.0, 1.0);
        let root = wing.root_airfoil.as_str();
        let tip = wing.tip_airfoil.as_str();
        let law: Vec<(f64, &str)> = if wing.section_stations.is_empty() {
            vec![(0.0, root), (kink, root), (1.0, tip)]
        } else {
            std::iter::once((0.0, root))
                .chain(
                    wing.section_stations
                        .iter()
                        .map(|station| (station.span_fraction, station.airfoil.as_str())),
                )
                .chain(std::iter::once((1.0, tip)))
                .collect()
        };
        let blended = |at: f64| -> Vec<(&str, f64)> {
            let outboard = law
                .iter()
                .position(|(station, _)| *station > at)
                .unwrap_or(law.len() - 1)
                .max(1);
            let ((eta_a, a), (eta_b, b)) = (law[outboard - 1], law[outboard]);
            let w = if eta_b > eta_a {
                ((at - eta_a) / (eta_b - eta_a)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            vec![(a, 1.0 - w), (b, w)]
        };
        // Loft stations: the built wing's cross-sections, each a custom
        // section's own airfoil or else the root-to-tip blend at its
        // station; the loft interpolates linearly between neighbours (which
        // reproduces the root-to-tip blend exactly when no custom section is
        // declared). Centreline, kink and tip anchor a wing without stations.
        let custom = |at: f64| {
            wing.custom_sections
                .iter()
                .find(|section| (section.span_fraction - at).abs() < 1e-6)
                .map(|section| vec![(section.airfoil.as_str(), 1.0)])
        };
        let built: Vec<f64> = self
            .plane
            .wings
            .first()
            .and_then(|w| {
                let tip = w
                    .xsecs
                    .iter()
                    .map(|x| x.xyz_le[1].abs())
                    .fold(0.0, f64::max);
                (tip > 0.0).then(|| w.xsecs.iter().map(|x| x.xyz_le[1].abs() / tip).collect())
            })
            .unwrap_or_default();
        let mut stations: Vec<(f64, Vec<(&str, f64)>)> = built
            .into_iter()
            .chain([0.0, 1.0])
            .chain((kink > 0.0 && kink < 1.0).then_some(kink))
            .map(|at| (at, custom(at).unwrap_or_else(|| blended(at))))
            .collect();
        for section in &wing.custom_sections {
            if !stations
                .iter()
                .any(|s| (s.0 - section.span_fraction).abs() < 1e-6)
            {
                stations.push((section.span_fraction, vec![(section.airfoil.as_str(), 1.0)]));
            }
        }
        stations.sort_by(|a, b| a.0.total_cmp(&b.0));
        stations.dedup_by(|outer, inner| (outer.0 - inner.0).abs() < 1e-9);
        let eta = eta.clamp(0.0, 1.0);
        let weights: Vec<(&str, f64)> = match stations.windows(2).find(|p| eta <= p[1].0) {
            Some([inner, outer]) if outer.0 > inner.0 => {
                let t = (eta - inner.0) / (outer.0 - inner.0);
                inner
                    .1
                    .iter()
                    .map(|&(f, w)| (f, w * (1.0 - t)))
                    .chain(outer.1.iter().map(|&(f, w)| (f, w * t)))
                    .collect()
            }
            _ => blended(eta),
        };
        let mut out: Vec<(String, f64)> = Vec::new();
        for (family, weight) in weights {
            if weight <= 0.0 {
                continue;
            }
            match out.iter_mut().find(|(f, _)| f == family) {
                Some(entry) => entry.1 += weight,
                None => out.push((family.to_owned(), weight)),
            }
        }
        out
    }

    /// Wing lift coefficient at buffet onset, `None` when unavailable; the
    /// typed answer is [`Self::buffet_onset`].
    pub fn buffet_onset_cl(&self, mach: f64, altitude_m: f64) -> Option<f64> {
        self.buffet_onset(mach, altitude_m).cl_b()
    }
}

/// The family section a table cell is generated for, re-exported for the
/// generator.
pub fn table_cell_section(base: &Airfoil, tc: f64, camber: f64) -> Option<Airfoil> {
    family_section(base, tc, camber)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alas_config::AlasConfig;

    fn plane_of(preset: &str) -> (alas_geom::aircraft::airplane::Airplane, AlasConfig, f64) {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset })).unwrap();
        let design = alas_config::presets::get(preset).unwrap().design_vector;
        let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .unwrap();
        (plane, config, design.sweep_deg)
    }

    fn analysis<'a>(
        plane: &'a alas_geom::aircraft::airplane::Airplane,
        config: &AlasConfig,
        sweep: f64,
    ) -> AeroAnalysis<'a> {
        AeroAnalysis::new(
            plane,
            AeroAnalysis::quarter_chord_sweep_deg(plane, sweep),
            Some(config.geometry.clone()),
            Some(config.resolved_drag_model()),
            Some(config.analysis.clone()),
        )
    }

    #[test]
    fn the_critical_strip_reads_the_lattice_and_the_loft() {
        let (plane, config, sweep) = plane_of("A320-200");
        let aero = analysis(&plane, &config, sweep);
        let req = &config.requirements;
        let strip = aero
            .buffet_strip(req.cruise_mach, req.cruise_altitude_m)
            .unwrap();
        // The families are the loft's at the strip: root inboard of the kink,
        // the linear root-to-tip blend outboard; weights sum to one and the
        // named family is the heaviest.
        let wing = &config.geometry.wing;
        let kink = wing.kink_span_fraction.unwrap_or(wing.break_span_fraction);
        let tip_weight = ((strip.eta - kink) / (1.0 - kink)).clamp(0.0, 1.0);
        let weight_of = |name: &str| -> f64 {
            strip
                .families
                .iter()
                .filter(|f| f.0 == name)
                .map(|f| f.1)
                .sum()
        };
        assert!((strip.families.iter().map(|f| f.1).sum::<f64>() - 1.0).abs() < 1e-12);
        if wing.root_airfoil != wing.tip_airfoil {
            assert!((weight_of(&wing.tip_airfoil) - tip_weight).abs() < 1e-12);
        }
        let heaviest = strip.families.iter().map(|f| f.1).fold(0.0, f64::max);
        assert!((weight_of(&strip.family) - heaviest).abs() < 1e-12);
        assert!((0.3..=0.8).contains(&strip.eta));
        assert!(
            (0.7..1.3).contains(&strip.additional_cl),
            "{}",
            strip.additional_cl
        );
        assert!(strip.basic_cl.abs() < 0.3, "{}", strip.basic_cl);
        assert!(
            (strip.wing_cl_at(strip.cl_n_at(0.6)) - 0.6).abs() < 1e-12,
            "the loading inverts"
        );
        assert!(
            (15.0..35.0).contains(&strip.sweep_deg),
            "{}",
            strip.sweep_deg
        );
        assert!(
            (strip.mach_normal - req.cruise_mach * strip.sweep_deg.to_radians().cos()).abs()
                < 1e-12
        );
        assert!(
            (0.08..0.16).contains(&strip.thickness_normal),
            "{}",
            strip.thickness_normal
        );
    }

    #[test]
    fn a_family_without_a_table_is_unavailable_not_estimated() {
        let (plane, config, sweep) = plane_of("A320-200");
        let aero = analysis(&plane, &config, sweep);
        let req = &config.requirements;
        let none = BuffetTables::default();
        match aero.buffet_onset_with(&none, req.cruise_mach, req.cruise_altitude_m) {
            BuffetOnset::Unavailable(BuffetUnavailable::FamilyNotTabulated(name)) => {
                assert_eq!(name, config.geometry.wing.root_airfoil);
            }
            other => panic!("{other:?}"),
        }
    }

    /// A constant-onset table of `family` (cl_n = `value` everywhere).
    fn constant_table(family: &str, value: f64) -> String {
        format!(
            "family {family}\naxis mach 0.1 0.9\naxis tc 0.01 0.30\naxis camber 0.0001 0.10\n{}end\n",
            (0..8)
                .map(|i| format!("cell {} {} {} {value}\n", i / 4, (i / 2) % 2, i % 2))
                .collect::<String>()
        )
    }

    /// Every section family the loft blends at a strip of the search window.
    fn window_families(aero: &AeroAnalysis, mach: f64, altitude_m: f64) -> Vec<String> {
        let mut families: Vec<String> = Vec::new();
        for strip in aero
            .buffet_strip_candidates(mach, altitude_m, SweepLine::Shock, (1, 8))
            .unwrap_or_else(|error| panic!("{error:?}"))
        {
            for (family, _) in strip.families {
                if !families.contains(&family) {
                    families.push(family);
                }
            }
        }
        families
    }

    #[test]
    fn blended_families_blend_the_onset_with_the_loft_weights() {
        let (plane, config, sweep) = plane_of("A320-200");
        let aero = analysis(&plane, &config, sweep);
        let req = &config.requirements;
        let families = window_families(&aero, req.cruise_mach, req.cruise_altitude_m);
        // A constant onset per family: the blend is the weighted mean at the
        // strip that reaches onset first.
        let value = |family: &str| {
            1.0 + 0.1
                * families
                    .iter()
                    .position(|f| f == family)
                    .map_or(0.0, |i| f64::from(u32::try_from(i).unwrap_or(0)))
        };
        let text: String = families
            .iter()
            .map(|family| constant_table(family, value(family)))
            .collect();
        let tables = BuffetTables::parse(&text).unwrap_or_else(|error| panic!("{error}"));
        match aero.buffet_onset_with(&tables, req.cruise_mach, req.cruise_altitude_m) {
            BuffetOnset::Interpolated(estimate) => {
                let expected: f64 = estimate
                    .strip
                    .families
                    .iter()
                    .map(|(family, weight)| weight * value(family))
                    .sum();
                assert!((estimate.cl_n_onset - expected).abs() < 1e-12);
            }
            other => panic!("{other:?}"),
        }
        // Any blended family without a table leaves the onset unavailable.
        if families.len() > 1 {
            let last = &families[families.len() - 1];
            let partial: String = families
                .iter()
                .filter(|f| *f != last)
                .map(|family| constant_table(family, 1.0))
                .collect();
            let tables = BuffetTables::parse(&partial).unwrap_or_else(|error| panic!("{error}"));
            match aero.buffet_onset_with(&tables, req.cruise_mach, req.cruise_altitude_m) {
                BuffetOnset::Unavailable(BuffetUnavailable::FamilyNotTabulated(name)) => {
                    assert_eq!(&name, last);
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn onset_lift_follows_the_table_cell_and_flags_clamping() {
        let (plane, config, sweep) = plane_of("A320-200");
        let aero = analysis(&plane, &config, sweep);
        let req = &config.requirements;
        // One-cell constant table around every strip: onset cl_n 1.0 everywhere.
        let text: String = window_families(&aero, req.cruise_mach, req.cruise_altitude_m)
            .iter()
            .map(|family| constant_table(family, 1.0))
            .collect();
        let tables = BuffetTables::parse(&text).unwrap();
        let onset = aero.buffet_onset_with(&tables, req.cruise_mach, req.cruise_altitude_m);
        let BuffetOnset::Interpolated(estimate) = onset else {
            panic!("{onset:?}")
        };
        // Separation starts at the lowest strip onset of the outboard wing;
        // the certified boundary is the definition transfer above it.
        let kink = aero.outboard_wing_start();
        let lowest = aero
            .buffet_strip_candidates(
                req.cruise_mach,
                req.cruise_altitude_m,
                SweepLine::Shock,
                (1, 8),
            )
            .unwrap()
            .iter()
            .filter(|s| s.eta >= kink)
            .map(|s| s.wing_cl_at(1.0))
            .fold(f64::INFINITY, f64::min);
        assert!((estimate.separation_cl - lowest).abs() < 1e-9);
        assert!((estimate.separation_cl - estimate.strip.wing_cl_at(1.0)).abs() < 1e-12);
        assert!(estimate.strip.eta >= kink);
        assert!(
            (estimate.cl_b - CERTIFIED_TO_FIRST_SEPARATION * estimate.separation_cl).abs() < 1e-12
        );
        // Narrow the Mach axis below the strip: clamped and flagged.
        let low = text.replace("axis mach 0.1 0.9", "axis mach 0.1 0.2");
        let tables = BuffetTables::parse(&low).unwrap();
        match aero.buffet_onset_with(&tables, req.cruise_mach, req.cruise_altitude_m) {
            BuffetOnset::Extrapolated(_, axes) => assert_eq!(axes, vec!["mach"]),
            other => panic!("{other:?}"),
        }
    }

    /// Every registered preset that cruises at or above its wave-drag onset
    /// Mach (where the buffet margin applies) has an onset table for every
    /// wing section family the loft can blend at a buffet strip (root, tip,
    /// named section stations whose loft interval meets the strip window, and
    /// declared custom sections), so a preset
    /// section change cannot
    /// leave the 1.3 g floor without a value.
    #[test]
    fn every_transonic_preset_wing_family_has_an_onset_table() {
        let mut missing = Vec::new();
        for preset in alas_config::presets::available() {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
                .unwrap_or_else(|error| panic!("{preset}: {error}"));
            if config.requirements.cruise_mach < config.drag_model.wave_drag_onset_mach {
                continue;
            }
            let wing = &config.geometry.wing;
            // Loft stations in span order (root at 0, tip at 1); a station's
            // family carries weight only between its two neighbours, so it
            // can reach a strip only if that interval meets the strip window.
            let mut stations = vec![(0.0, &wing.root_airfoil)];
            stations.extend(
                wing.section_stations
                    .iter()
                    .map(|s| (s.span_fraction, &s.airfoil)),
            );
            stations.push((1.0, &wing.tip_airfoil));
            let reachable = (0..stations.len()).filter_map(|i| {
                let lo = stations[i.saturating_sub(1)].0;
                let hi = stations[(i + 1).min(stations.len() - 1)].0;
                (hi > STRIP_WINDOW.0 && lo < STRIP_WINDOW.1).then_some(stations[i].1)
            });
            let families =
                reachable.chain(wing.custom_sections.iter().map(|section| &section.airfoil));
            for family in families {
                if shipped_tables().family(family).is_none() {
                    missing.push(format!("{preset}: {family}"));
                }
            }
        }
        assert!(missing.is_empty(), "no buffet-onset table: {missing:?}");
    }
}
