// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Low-Reynolds parasite drag build-up for the non-wing components
//! (UAV-PLAN 3.1 item 5).  The transport path is NOT used: it assumes fully
//! turbulent skin friction and carries a Mach-dependent form factor that falls
//! below 1 at UAV Mach (aero-drag finding DRG-07).
//!
//! Per component, `CD0_i = Cf FF Q S_wet / S_ref` (surfaces and bodies) or
//! `CD_i A_i Q / S_ref` (itemised parts), referenced to `s_ref` m2.  Units SI;
//! lengths m, areas m2.
//!
//! # Skin friction
//!
//! * Laminar: `Cf = 1.328 / sqrt(Re)` (Blasius), Raymer 6th ed. Eq. 12.26.
//! * Turbulent: `Cf = 0.455 / (log10 Re)^2.58 / (1 + 0.144 M^2)^0.65`,
//!   Prandtl-Schlichting with Raymer's compressibility term, Eq. 12.27
//!   (Torenbeek, Advanced Aircraft Design, Eq. 4.6, has the incompressible
//!   form).  Both read on the PDF pages 2026-10-12.
//! * Transitional: for transition at plate Reynolds number `Re_tr`,
//!   `Cf = Cf_turb(Re) - A / Re` with `A = Re_tr (Cf_turb(Re_tr) -
//!   Cf_lam(Re_tr))` (matching momentum thickness at transition; Schlichting).
//!   With the 0.455/log law `A = 1611` at `Re_tr = 5e5`; the 1700 quoted for
//!   that Re belongs to the `0.074 Re^-0.2` turbulent law.  `Cf = Cf_lam`
//!   for `Re <= Re_tr`, so the law is continuous.
//!
//! # Form factors (no Mach term)
//!
//! * Surfaces: Hoerner `1 + 2 t/c + 60 (t/c)^4`.  Cross-check on the page:
//!   Raymer Eq. 12.30 without its Mach factor, `1 + 0.6/(x/c)_max (t/c) +
//!   100 (t/c)^4`, differs by < 1 % at t/c = 0.12, (x/c)_max = 0.3 (test).
//! * Bodies: Hoerner `1 + 1.5 (d/l)^1.5 + 7 (d/l)^3`.  Hoerner's book was not
//!   available, so the formula is Hoerner's as widely quoted (NOT read in the
//!   book) and is cross-checked on two pages instead.  Torenbeek, Advanced
//!   Aircraft Design (2013), Fig. 4.2 p. 89 (PDF p. 110), gives the fuselage
//!   form factor against `S_front/S_wet` (about `1 + 3.5 S_front/S_wet`; for a
//!   body with `S_wet = 0.85 pi d l` that is 1.17 at `l/d = 6` and 1.34 at
//!   `l/d = 3`) against 1.13 and 1.55 here (-3 % and +15 %).  Raymer 6th ed.
//!   Eq. 12.31 (p. 422), `0.9 + 5/f^1.5 + f/400` with `f = l/d`, gives 1.26 and
//!   1.87: the book itself calls it conservative for short, fat fuselages and
//!   it grows without bound for slender bodies, so it is not used.
//!
//! # Interference and itemised parts
//!
//! Interference factors `Q` follow Raymer p. 425 (PDF 459).  Itemised drag
//! coefficients on frontal area are Raymer Table 12.6 (PDF 463); the landing
//! gear sum is multiplied by 1.2 for mutual interference (Raymer p. 430).
//! Excrescence/leakage is an explicit line (Raymer Table 12.8, p. 431, read on
//! the page: 5-10 % for propeller aircraft; "only race planes are subjected to
//! extreme cleanup"), default 10 %, the upper end, because a hand-built model
//! airframe has exposed hinge gaps, linkages, seams and wiring.

use super::air::AirState;
use super::AeroError;

/// Laminar flat-plate friction coefficient (Blasius), Re on plate length.
pub fn cf_laminar(re: f64) -> f64 {
    1.328 / re.sqrt()
}

/// Turbulent flat-plate friction coefficient (Prandtl-Schlichting).
pub fn cf_turbulent(re: f64, mach: f64) -> f64 {
    0.455 / (re.log10().powf(2.58) * (1.0 + 0.144 * mach * mach).powf(0.65))
}

/// Where the boundary layer transitions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Transition {
    /// Fully turbulent from the leading edge.
    Turbulent,
    /// Laminar up to plate Reynolds number `Re_tr`, turbulent after
    /// (5e5 natural, clean smooth plate; lower for rough noses and seams).
    AtReynolds(f64),
}

/// Transitional flat-plate friction coefficient at plate Reynolds `re`.
pub fn cf_transitional(re: f64, mach: f64, transition: Transition) -> f64 {
    match transition {
        Transition::Turbulent => cf_turbulent(re, mach),
        Transition::AtReynolds(re_tr) => {
            if re <= re_tr {
                cf_laminar(re)
            } else {
                let a = re_tr * (cf_turbulent(re_tr, mach) - cf_laminar(re_tr));
                cf_turbulent(re, mach) - a / re
            }
        }
    }
}

/// Hoerner surface form factor, `t_over_c` the thickness ratio.
pub fn form_factor_surface(t_over_c: f64) -> f64 {
    1.0 + 2.0 * t_over_c + 60.0 * t_over_c.powi(4)
}

/// Hoerner body form factor, `d_over_l` the diameter-to-length ratio.
pub fn form_factor_body(d_over_l: f64) -> f64 {
    1.0 + 1.5 * d_over_l.powf(1.5) + 7.0 * d_over_l.powi(3)
}

/// Interference factors `Q` (Raymer p. 425; ranges collapsed to a typical value).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Junction {
    /// Fuselage, boundary-layer diverter, isolated body: 1.0.
    Isolated,
    /// High or mid wing, or well-filleted low wing: 1.0.
    WingFilleted,
    /// Unfilleted low wing: 1.1..1.4, 1.25 used.
    WingLowUnfilleted,
    /// Clean V tail: 1.03.
    VTail,
    /// Conventional tail: 1.04..1.05, 1.045 used.
    ConventionalTail,
    /// H tail: 1.08.
    HTail,
    /// Nacelle or store mounted directly on the body: 1.5.
    NacelleOnBody,
    /// Nacelle less than about one diameter away: 1.3.
    NacelleNear,
    /// Value chosen by the user.
    Custom(f64),
}

impl Junction {
    /// The factor.
    pub fn q(self) -> f64 {
        match self {
            Self::Isolated | Self::WingFilleted => 1.0,
            Self::WingLowUnfilleted => 1.25,
            Self::VTail => 1.03,
            Self::ConventionalTail => 1.045,
            Self::HTail => 1.08,
            Self::NacelleOnBody => 1.5,
            Self::NacelleNear => 1.3,
            Self::Custom(q) => q,
        }
    }
}

/// Raymer Table 12.6 drag coefficients on frontal area (PDF p. 463).
pub mod table_12_6 {
    /// Regular wheel and tire.
    pub const WHEEL_REGULAR: f64 = 0.25;
    /// Streamlined wheel and tire.
    pub const WHEEL_STREAMLINED: f64 = 0.18;
    /// Wheel and tire with fairing.
    pub const WHEEL_FAIRED: f64 = 0.13;
    /// Streamlined strut, 1/6 < t/c < 1/3.
    pub const STRUT_STREAMLINED: f64 = 0.05;
    /// Round strut or wire, Re > 3e5.
    pub const ROUND_SUPERCRITICAL: f64 = 0.30;
    /// Round strut or wire, Re < 3e5.
    pub const ROUND_SUBCRITICAL: f64 = 1.17;
    /// Flat spring gear leg.
    pub const FLAT_SPRING_LEG: f64 = 1.40;
    /// Landing-gear mutual interference multiplier (Raymer p. 430).
    pub const GEAR_INTERFERENCE: f64 = 1.2;
}

/// What a component is.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Lifting-surface-like part (tail, fin, strut fairing, wing estimate).
    Surface {
        /// Mean chord, m (plate length for Re).
        chord_m: f64,
        /// Wetted area, m2 (both sides).
        wetted_m2: f64,
        /// Thickness ratio.
        t_over_c: f64,
    },
    /// Body of revolution-like part (fuselage, boom, pod, motor cowl).
    Body {
        /// Length, m.
        length_m: f64,
        /// Equivalent diameter, m.
        diameter_m: f64,
        /// Wetted area, m2.
        wetted_m2: f64,
    },
    /// Itemised part with a drag coefficient on its frontal area.
    Item {
        /// Drag coefficient on `frontal_m2`.
        cd: f64,
        /// Frontal area, m2.
        frontal_m2: f64,
    },
    /// Round wire or strut in cross flow (Re-dependent Raymer Table 12.6 value).
    Cylinder {
        /// Diameter, m.
        diameter_m: f64,
        /// Length exposed to the flow, m.
        length_m: f64,
    },
}

/// One drag-contributing part.
#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    /// Display name.
    pub name: String,
    /// Geometry.
    pub shape: Shape,
    /// Transition model (surfaces and bodies).
    pub transition: Transition,
    /// Interference factor source.
    pub junction: Junction,
    /// Number of identical parts.
    pub count: u32,
    /// Multiplier applied after `Q` (landing-gear interference etc.).
    pub extra_multiplier: f64,
    /// The part sits in the propeller wash (its drag grows with the
    /// slipstream dynamic pressure, see `coupled::slipstream_drag`).
    pub in_slipstream: bool,
}

impl Component {
    /// A component with natural transition at Re 5e5, isolated, count 1.
    pub fn new(name: &str, shape: Shape) -> Self {
        Self {
            name: name.to_owned(),
            shape,
            transition: Transition::AtReynolds(5.0e5),
            junction: Junction::Isolated,
            count: 1,
            extra_multiplier: 1.0,
            in_slipstream: false,
        }
    }

    /// Mark the part as sitting in the propeller wash.
    pub fn in_slipstream(mut self) -> Self {
        self.in_slipstream = true;
        self
    }

    /// Same with an interference junction.
    pub fn with_junction(mut self, junction: Junction) -> Self {
        self.junction = junction;
        self
    }

    /// Same with a part count.
    pub fn with_count(mut self, count: u32) -> Self {
        self.count = count;
        self
    }

    /// Same with a transition model.
    pub fn with_transition(mut self, transition: Transition) -> Self {
        self.transition = transition;
        self
    }

    /// Landing-gear part: applies the 1.2 mutual-interference multiplier.
    pub fn as_gear(mut self) -> Self {
        self.extra_multiplier = table_12_6::GEAR_INTERFERENCE;
        self
    }
}

/// One line of the build-up table.
#[derive(Debug, Clone, PartialEq)]
pub struct DragLine {
    /// Component name.
    pub name: String,
    /// Plate Reynolds number (NaN for itemised parts).
    pub re: f64,
    /// Skin-friction coefficient (NaN for itemised parts).
    pub cf: f64,
    /// Form factor (1 for itemised parts).
    pub form_factor: f64,
    /// Interference factor times extra multiplier.
    pub q: f64,
    /// Drag area `D/q`, m2 (all `count` parts).
    pub cda_m2: f64,
    /// Contribution to the zero-lift drag coefficient `cda / s_ref`.
    pub cd0: f64,
}

/// The complete build-up.
#[derive(Debug, Clone, PartialEq)]
pub struct DragBuildUp {
    /// Component lines (excluding the allowance).
    pub lines: Vec<DragLine>,
    /// Explicit excrescence/leakage allowance line.
    pub excrescence: DragLine,
    /// Sum of component lines.
    pub cd0_components: f64,
    /// Components plus allowance.
    pub cd0_total: f64,
}

/// Default excrescence/leakage fraction (Raymer Table 12.8, p. 431: propeller
/// aircraft 5-10 %; the upper end, a judgement for hand-built model airframes
/// with open hinge gaps and linkages, NOT calibrated).
pub const DEFAULT_EXCRESCENCE_FRACTION: f64 = 0.10;

fn finite_pos(x: f64, what: &str) -> Result<(), AeroError> {
    if x.is_finite() && x > 0.0 {
        Ok(())
    } else {
        Err(AeroError::InvalidInput(format!(
            "{what} must be positive and finite, got {x}"
        )))
    }
}

fn line_for(component: &Component, air: &AirState, s_ref: f64) -> Result<DragLine, AeroError> {
    let n = f64::from(component.count);
    let q = component.junction.q() * component.extra_multiplier;
    let mach = air.mach();
    let (re, cf, ff, cda) = match component.shape {
        Shape::Surface {
            chord_m,
            wetted_m2,
            t_over_c,
        } => {
            finite_pos(chord_m, "surface chord")?;
            finite_pos(wetted_m2, "surface wetted area")?;
            if !(0.0..=0.4).contains(&t_over_c) {
                return Err(AeroError::InvalidInput(format!(
                    "t/c {t_over_c} outside 0..0.4"
                )));
            }
            let re = air.reynolds(chord_m);
            let cf = cf_transitional(re, mach, component.transition);
            let ff = form_factor_surface(t_over_c);
            (re, cf, ff, cf * ff * q * wetted_m2 * n)
        }
        Shape::Body {
            length_m,
            diameter_m,
            wetted_m2,
        } => {
            finite_pos(length_m, "body length")?;
            finite_pos(diameter_m, "body diameter")?;
            finite_pos(wetted_m2, "body wetted area")?;
            let re = air.reynolds(length_m);
            let cf = cf_transitional(re, mach, component.transition);
            let ff = form_factor_body(diameter_m / length_m);
            (re, cf, ff, cf * ff * q * wetted_m2 * n)
        }
        Shape::Item { cd, frontal_m2 } => {
            finite_pos(cd, "item drag coefficient")?;
            finite_pos(frontal_m2, "item frontal area")?;
            (f64::NAN, f64::NAN, 1.0, cd * frontal_m2 * q * n)
        }
        Shape::Cylinder {
            diameter_m,
            length_m,
        } => {
            finite_pos(diameter_m, "cylinder diameter")?;
            finite_pos(length_m, "cylinder length")?;
            let re = air.reynolds(diameter_m);
            let cd = if re > 3.0e5 {
                table_12_6::ROUND_SUPERCRITICAL
            } else {
                table_12_6::ROUND_SUBCRITICAL
            };
            (re, f64::NAN, 1.0, cd * diameter_m * length_m * q * n)
        }
    };
    Ok(DragLine {
        name: component.name.clone(),
        re,
        cf,
        form_factor: ff,
        q,
        cda_m2: cda,
        cd0: cda / s_ref,
    })
}

/// Build the drag table for `components` at `air`, referenced to `s_ref` m2.
/// `excrescence_fraction` multiplies the sum of the component lines
/// (0 switches the allowance off; it stays visible as a zero line).
///
/// # Errors
///
/// [`AeroError::InvalidInput`] for non-positive geometry, a bad reference
/// area or a negative allowance.
pub fn build_up(
    components: &[Component],
    air: &AirState,
    s_ref: f64,
    excrescence_fraction: f64,
) -> Result<DragBuildUp, AeroError> {
    finite_pos(s_ref, "reference area")?;
    if !(excrescence_fraction.is_finite() && excrescence_fraction >= 0.0) {
        return Err(AeroError::InvalidInput(
            "excrescence fraction must be >= 0".into(),
        ));
    }
    let lines = components
        .iter()
        .map(|c| line_for(c, air, s_ref))
        .collect::<Result<Vec<_>, _>>()?;
    let cd0_components: f64 = lines.iter().map(|l| l.cd0).sum();
    let cda_exc = excrescence_fraction * lines.iter().map(|l| l.cda_m2).sum::<f64>();
    let excrescence = DragLine {
        name: "excrescence and leakage allowance".to_owned(),
        re: f64::NAN,
        cf: f64::NAN,
        form_factor: 1.0,
        q: 1.0 + excrescence_fraction,
        cda_m2: cda_exc,
        cd0: cda_exc / s_ref,
    };
    let cd0_total = cd0_components + excrescence.cd0;
    Ok(DragBuildUp {
        lines,
        excrescence,
        cd0_components,
        cd0_total,
    })
}
