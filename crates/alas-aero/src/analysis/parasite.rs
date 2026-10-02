// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Component parasite-drag buildup, reported term by term.
//!
//! Every component contributes `CD = Cf * FF * Q * S_wet / S_ref` (D. P.
//! Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed., 2018, eq. 12.24),
//! with `Cf` the compressible turbulent flat-plate skin friction on the
//! component's own length (eq. 12.27), `FF` its form factor and `Q` its
//! interference factor. The lumped `viscous_margin` then multiplies the sum.
//!
//! The product buildup reads every geometric input off the built aircraft:
//! wetted areas from the lofted bodies and the exposed planforms, fineness
//! ratios from the body stations, and the sweep of each surface's
//! maximum-thickness line. The reference-compatibility buildup replays the
//! frozen convention (one main-wing thickness and design sweep for every
//! surface, bodies as configured cylinders without a form factor, a fuselage
//! `Q` of 1.25) and must stay bit-identical.

use std::f64::consts::PI;

use alas_atmo::Atmosphere;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::wing::Wing;

use super::{wetted, AeroAnalysis};

/// Interference factor of the fuselage: Raymer 6th ed. section 12.5.5, "for
/// most fuselages the interference is negligible and Q = 1.0".
const FUSELAGE_INTERFERENCE: f64 = 1.0;

/// Interference factor of a conventional empennage surface: Raymer 6th ed.
/// section 12.5.5 gives 4-5 % for a conventional tail (3 % for a clean
/// V-tail, 8 % for an H-tail); this is the midpoint of the conventional
/// range.
const EMPENNAGE_INTERFERENCE: f64 = 1.045;

/// Thickness ratio below which Raymer's thin-surface wetted-area relation
/// applies (Raymer 6th ed. eq. 7.9 versus 7.10).
const THIN_SURFACE_THICKNESS: f64 = 0.05;

/// One component's share of the parasite drag:
/// `CD = Cf * FF * Q * S_wet / S_ref`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParasiteComponent<'p> {
    /// The surface or body name, as the geometry builder gave it.
    pub name: &'p str,
    /// Wetted area the skin friction acts on, m^2.
    pub wetted_area_m2: f64,
    /// Length the Reynolds number is built on, m: the mean aerodynamic chord
    /// of a lifting surface, the end-to-end length of a body. Only the skin
    /// friction depends on altitude, through `Re = rho V L / mu`, so a caller
    /// can move a component to another altitude with this length alone.
    pub reynolds_length_m: f64,
    /// Reynolds number on that length.
    pub reynolds: f64,
    /// Flat-plate skin-friction coefficient.
    pub skin_friction: f64,
    /// Form factor (pressure drag of the thickness distribution).
    pub form_factor: f64,
    /// Interference factor.
    pub interference_factor: f64,
    /// The component's drag coefficient on the aircraft reference area.
    pub cd: f64,
}

/// The parasite drag split by component, plus the lumped margin.
#[derive(Debug, Clone, PartialEq)]
pub struct ParasiteBreakdown<'p> {
    /// Components in accumulation order: wings (main wing first), primary
    /// body, nacelles.
    pub components: Vec<ParasiteComponent<'p>>,
    /// Sum of the component terms.
    pub cd_components: f64,
    /// What the configured `viscous_margin` adds to that sum: excrescence,
    /// gaps, roughness and the components the buildup does not draw (pylons,
    /// flap-track and wing-body fairings).
    pub cd_miscellaneous: f64,
    /// Total parasite drag coefficient, `cd_components + cd_miscellaneous`.
    pub cd0: f64,
}

impl ParasiteBreakdown<'_> {
    /// Total wetted area over the aircraft reference area.
    pub fn wetted_area_ratio(&self, s_ref: f64) -> f64 {
        self.components
            .iter()
            .map(|component| component.wetted_area_m2)
            .sum::<f64>()
            / s_ref
    }
}

/// Raymer 6th ed. eq. 12.30, the lifting-surface form factor:
/// `[1 + 0.6/(x/c)_m (t/c) + 100 (t/c)^4] [1.34 M^0.18 cos(sweep_m)^0.28]`,
/// with `sweep_m` the sweep of the maximum-thickness line.
pub fn lifting_surface_form_factor(
    thickness: f64,
    max_thickness_x_over_c: f64,
    mach: f64,
    sweep_m_deg: f64,
) -> f64 {
    (1.0 + 0.6 / max_thickness_x_over_c * thickness + 100.0 * thickness.powf(4.0))
        * (1.34 * mach.powf(0.18) * sweep_m_deg.to_radians().cos().powf(0.28))
}

/// Raymer 6th ed. eq. 12.31, the fuselage and smooth-canopy form factor
/// `1 + 60/f^3 + f/400`, with `f` the length over the equivalent diameter.
pub fn fuselage_form_factor(fineness: f64) -> f64 {
    1.0 + 60.0 / fineness.powi(3) + fineness / 400.0
}

/// Raymer 6th ed. eq. 12.32, the nacelle and smooth external store form
/// factor `1 + 0.35/f`.
pub fn nacelle_form_factor(fineness: f64) -> f64 {
    1.0 + 0.35 / fineness
}

/// Raymer 6th ed. eqs. 7.9-7.10: wetted area of a lifting surface from its
/// exposed planform area, `2.003 S_exp` for `t/c < 0.05`, else
/// `(1.977 + 0.52 t/c) S_exp`.
pub fn lifting_surface_wetted_area(exposed_area_m2: f64, thickness: f64) -> f64 {
    if thickness < THIN_SURFACE_THICKNESS {
        2.003 * exposed_area_m2
    } else {
        (1.977 + 0.52 * thickness) * exposed_area_m2
    }
}

/// Planform-area-weighted sweep, degrees, of the line at `chord_fraction` of
/// each panel's chord, measured in the panel's own plane
/// (`atan(dx / sqrt(dy^2 + dz^2))`), so that it applies to a vertical fin as
/// much as to a wing. `None` when no panel has a positive finite area.
fn chord_line_sweep_deg(wing: &Wing, chord_fraction: f64) -> Option<f64> {
    let mut weighted = 0.0;
    let mut area = 0.0;
    for pair in wing.xsecs.windows(2) {
        let dy = pair[1].xyz_le[1] - pair[0].xyz_le[1];
        let dz = pair[1].xyz_le[2] - pair[0].xyz_le[2];
        let span = dy.hypot(dz);
        let x_in = pair[0].xyz_le[0] + chord_fraction * pair[0].chord;
        let x_out = pair[1].xyz_le[0] + chord_fraction * pair[1].chord;
        let panel_area = span * 0.5 * (pair[0].chord + pair[1].chord);
        let sweep = (x_out - x_in).atan2(span).to_degrees();
        if panel_area.is_finite() && panel_area > 0.0 && sweep.is_finite() {
            weighted += panel_area * sweep;
            area += panel_area;
        }
    }
    (area > 0.0).then(|| weighted / area)
}

/// Length and largest equivalent diameter `sqrt(width * height)` of a body,
/// m. The equivalent diameter is exact for the elliptic sections the
/// builder lofts (`sqrt(4 A / pi)` with `A = pi w h / 4`).
fn body_length_and_diameter(body: &Fuselage) -> (f64, f64) {
    let diameter = body
        .xsecs
        .iter()
        .map(|xsec| (xsec.width * xsec.height).sqrt())
        .fold(0.0, f64::max);
    (AeroAnalysis::body_length(body), diameter)
}

impl<'a> AeroAnalysis<'a> {
    /// The component buildup behind [`Self::parasite_drag`].
    ///
    /// `atmosphere` and `section_thickness` are optional precomputed inputs,
    /// see [`Self::parasite_drag`]; `section_thickness` is only read by the
    /// reference-compatibility buildup.
    pub fn parasite_breakdown(
        &self,
        mach: f64,
        altitude_m: f64,
        atmosphere: Option<&Atmosphere>,
        section_thickness: Option<f64>,
    ) -> ParasiteBreakdown<'a> {
        let atmosphere = atmosphere
            .copied()
            .unwrap_or_else(|| Atmosphere::new(altitude_m));
        let velocity = mach * atmosphere.speed_of_sound();
        let density = atmosphere.density();
        let viscosity = atmosphere.dynamic_viscosity();
        let plane: &'a Airplane = self.plane;
        let s_ref = plane.s_ref;

        let mut components = Vec::with_capacity(plane.wings.len() + plane.fuselages.len());
        let mut push = |name: &'a str, wetted: f64, length: f64, form_factor: f64, q: f64| {
            let reynolds = density * velocity * length / viscosity;
            let cf = Self::turbulent_cf(reynolds, mach);
            components.push(ParasiteComponent {
                name,
                wetted_area_m2: wetted,
                reynolds_length_m: length,
                reynolds,
                skin_friction: cf,
                form_factor,
                interference_factor: q,
                cd: cf * form_factor * q * (wetted / s_ref),
            });
        };

        if self.reference_compatibility {
            let thickness = section_thickness.unwrap_or_else(|| self.section_thickness());
            let form_factor = lifting_surface_form_factor(
                thickness,
                self.drag.max_thickness_chordwise_loc,
                mach,
                self.sweep_deg,
            );
            for wing in &plane.wings {
                push(
                    &wing.name,
                    wing.unfolded_area() * self.geometry.wing_wetted_area_factor,
                    wing.mean_aerodynamic_chord(),
                    form_factor,
                    self.drag.interference_factor_wing,
                );
            }
            if let Some(fuselage) = plane.fuselages.first() {
                let length = Self::body_length(fuselage);
                let wetted = PI
                    * self.geometry.fuselage.diameter_m
                    * length
                    * self.geometry.fuselage_wetted_factor;
                push(
                    &fuselage.name,
                    wetted,
                    length,
                    1.0,
                    self.drag.interference_factor_fuselage,
                );
            }
            for nacelle in plane.fuselages.iter().skip(1) {
                let length = Self::body_length(nacelle);
                let diameter = 2.0 * self.geometry.engine.radius_scale_m;
                let wetted = PI * diameter * length;
                push(
                    &nacelle.name,
                    wetted,
                    length,
                    1.0,
                    self.drag.interference_factor_nacelle,
                );
            }
        } else {
            let x_over_c = self.drag.max_thickness_chordwise_loc;
            let body = plane.fuselages.first();
            for (index, wing) in plane.wings.iter().enumerate() {
                let thickness = self.wing_thicknesses()[index];
                let sweep_m_deg = chord_line_sweep_deg(wing, x_over_c).unwrap_or(self.sweep_deg);
                // Exposed planform: the part of a symmetric surface inside
                // the body carries no skin friction (NDARC convention, see
                // `wetted`); the coefficient keeps the gross reference area.
                let buried = match body {
                    Some(body) if self.drag.exclude_buried_main_wing_area => {
                        wetted::buried_surface_area(wing, body)
                    }
                    _ => 0.0,
                };
                let exposed = (wing.unfolded_area() - buried).max(0.0);
                push(
                    &wing.name,
                    lifting_surface_wetted_area(exposed, thickness),
                    wing.mean_aerodynamic_chord(),
                    lifting_surface_form_factor(thickness, x_over_c, mach, sweep_m_deg),
                    if index == 0 {
                        self.drag.interference_factor_wing
                    } else {
                        EMPENNAGE_INTERFERENCE
                    },
                );
            }
            if let Some(fuselage) = body {
                let (length, diameter) = body_length_and_diameter(fuselage);
                push(
                    &fuselage.name,
                    fuselage.area_wetted(),
                    length,
                    Self::body_form_factor(length, diameter, fuselage_form_factor),
                    FUSELAGE_INTERFERENCE,
                );
            }
            for nacelle in plane.fuselages.iter().skip(1) {
                let (length, diameter) = body_length_and_diameter(nacelle);
                push(
                    &nacelle.name,
                    nacelle.area_wetted(),
                    length,
                    Self::body_form_factor(length, diameter, nacelle_form_factor),
                    self.drag.interference_factor_nacelle,
                );
            }
        }

        let mut cd_components = 0.0;
        for component in &components {
            cd_components += component.cd;
        }
        let cd0 = cd_components * self.drag.viscous_margin;
        ParasiteBreakdown {
            components,
            cd_components,
            cd_miscellaneous: cd0 - cd_components,
            cd0,
        }
    }

    /// A body's form factor from its fineness ratio, or 1 (friction only)
    /// for a degenerate body with no length or no diameter.
    fn body_form_factor(length: f64, diameter: f64, relation: fn(f64) -> f64) -> f64 {
        let fineness = length / diameter;
        if fineness.is_finite() && fineness > 0.0 {
            relation(fineness)
        } else {
            1.0
        }
    }
}

// A test constructs the geometry it asserts on directly, so a failed expect is
// the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::airplane::Airplane;
    use alas_geom::aircraft::fuselage::FuselageXSec;
    use alas_geom::aircraft::wing::WingXSec;

    /// A cylinder body of radius 2 m from x = 0 to 20 m, and a symmetric
    /// NACA 0012 tail (root chord 2 m, tip chord 1 m, semispan 12 m) whose
    /// root leading edge sits at `(tail_x, 0, tail_z)`.
    fn body_and_tail(tail_x: f64, tail_z: f64) -> Airplane {
        let foil = Airfoil::from_name("naca0012").unwrap();
        let tail = Wing::new(
            "Horizontal Stabilizer",
            vec![
                WingXSec::new([tail_x, 0.0, tail_z], 2.0, 0.0, foil.clone()),
                WingXSec::new([tail_x + 1.0, 12.0, tail_z], 1.0, 0.0, foil),
            ],
            true,
        );
        let station =
            |x: f64| FuselageXSec::new([x, 0.0, 0.0], Some(2.0), None, None, 2.0).unwrap();
        Airplane {
            name: "body and tail".to_owned(),
            xyz_ref: [10.0, 0.0, 0.0],
            wings: vec![tail],
            fuselages: vec![Fuselage::new("Fuselage", vec![station(0.0), station(20.0)])],
            s_ref: 30.0,
            c_ref: 1.5,
            b_ref: 24.0,
        }
    }

    #[test]
    fn components_sum_to_the_parasite_drag_the_polar_uses() {
        let plane = body_and_tail(15.0, 0.0);
        let analysis = AeroAnalysis::new(&plane, 20.0, None, None, None);
        let breakdown = analysis.parasite_breakdown(0.78, 11_000.0, None, None);
        let sum: f64 = breakdown.components.iter().map(|c| c.cd).sum();
        assert!((sum - breakdown.cd_components).abs() < 1e-15);
        assert!(
            (breakdown.cd0 - breakdown.cd_components - breakdown.cd_miscellaneous).abs() < 1e-15
        );
        assert_eq!(
            breakdown.cd0,
            analysis.parasite_drag(0.78, 11_000.0, 0.5, None, None)
        );
    }

    #[test]
    fn bodies_take_their_wetted_area_from_the_loft_and_tails_their_exposed_planform() {
        let inside = body_and_tail(15.0, 0.0);
        let analysis = AeroAnalysis::new(&inside, 20.0, None, None, None);
        let breakdown = analysis.parasite_breakdown(0.78, 11_000.0, None, None);
        let tail = &breakdown.components[0];
        let body = &breakdown.components[1];
        // The lofted cylinder: 2 pi r L to the perimeter fit's stated 0.2 %.
        let cylinder = 2.0 * PI * 2.0 * 20.0;
        assert!((body.wetted_area_m2 / cylinder - 1.0).abs() < 2e-3);
        assert_eq!(body.wetted_area_m2, inside.fuselages[0].area_wetted());
        // Fineness 20 / 4 = 5: 1 + 60/125 + 5/400.
        assert!((body.form_factor - 1.4925).abs() < 1e-12);
        // Chord 2 - y/12 across the 2 m body half-width, both sides:
        // 2 * (4 - 4/24) = 7.6667 m2 buried of 36 m2.
        let exposed = inside.wings[0].unfolded_area() - 2.0 * (4.0 - 4.0 / 24.0);
        let expected = (1.977 + 0.52 * 0.12) * exposed;
        assert!(
            (tail.wetted_area_m2 / expected - 1.0).abs() < 2e-3,
            "{} vs {expected}",
            tail.wetted_area_m2
        );
        // Lifted clear of the body the same tail is fully exposed.
        let clear = body_and_tail(15.0, 3.0);
        let clear_tail = AeroAnalysis::new(&clear, 20.0, None, None, None)
            .parasite_breakdown(0.78, 11_000.0, None, None)
            .components[0]
            .wetted_area_m2;
        assert!(
            (clear_tail / ((1.977 + 0.52 * 0.12) * clear.wings[0].unfolded_area()) - 1.0).abs()
                < 2e-3
        );
    }

    #[test]
    fn form_factors_equal_raymer_at_known_inputs() {
        // Eq. 12.30 at t/c 0.12, (x/c)m 0.35, M 0.80, sweep_m 30 deg, by
        // hand: (1 + 0.205714 + 0.020736) (1.34 * 0.960630 * 0.960525)
        // = 1.226450 * 1.236431 = 1.51642.
        let form_factor = lifting_surface_form_factor(0.12, 0.35, 0.8, 30.0);
        assert!((form_factor - 1.51642).abs() < 2e-5, "{form_factor}");
        // Eq. 12.31 at f = 10: 1 + 0.06 + 0.025.
        assert!((fuselage_form_factor(10.0) - 1.085).abs() < 1e-15);
        // Eq. 12.32 at f = 2: 1 + 0.175.
        assert!((nacelle_form_factor(2.0) - 1.175).abs() < 1e-15);
        // Eq. 7.10 at t/c 0.12: 2.0394 S_exp; eq. 7.9 below t/c 0.05.
        assert!((lifting_surface_wetted_area(100.0, 0.12) - 203.94).abs() < 1e-9);
        assert!((lifting_surface_wetted_area(100.0, 0.04) - 200.3).abs() < 1e-9);
    }

    #[test]
    fn the_fuselage_form_factor_falls_with_fineness_up_to_its_optimum() {
        // d/df (60 f^-3 + f/400) = 0 at f = (72000)^(1/4) = 16.4; below it
        // the factor falls monotonically with fineness, as a slender body's
        // pressure drag does.
        let optimum = 72_000.0_f64.powf(0.25);
        assert!(fuselage_form_factor(6.0) > fuselage_form_factor(9.0));
        assert!(fuselage_form_factor(9.0) > fuselage_form_factor(optimum));
        assert!(fuselage_form_factor(optimum) < fuselage_form_factor(optimum + 1.0));
    }
}
