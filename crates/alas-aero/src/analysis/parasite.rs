// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Component parasite-drag buildup, reported term by term.
//!
//! Every component contributes `CD = Cf * FF * Q * S_wet / S_ref` (D. P.
//! Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed., 2018, eq. 12.24),
//! with `Cf` the compressible turbulent flat-plate skin friction on the
//! component's own length (eq. 12.27), `FF` its form factor and `Q` its
//! interference factor. The product buildup adds pylons, the wing-body
//! fairing, upsweep and base drag (see `parasite_items`); the lumped
//! `viscous_margin` then multiplies the sum.
//!
//! The product buildup reads every geometric input off the built aircraft:
//! wetted areas from the lofted bodies and the exposed planforms, fineness
//! ratios from the body stations, and the sweep of each surface's
//! maximum-thickness line. The reference-compatibility buildup replays the
//! frozen convention (one main-wing thickness and design sweep for every
//! surface, bodies as configured cylinders without a form factor, a fuselage
//! `Q` of 1.25) and must stay bit-identical.
//!
//! The product buildup takes the wing, horizontal tail and fin from E.
//! Torenbeek, *Advanced Aircraft Design*, 2013: `Cf = 0.044 / Re^(1/6)`
//! (Eq. 4.7, p. 88, adjusted to M 0.5) and the form factor
//! `1 + r_phi (t/c) cos^2(sweep)` (Eqs. 10.36-10.39, p. 299), with no Mach
//! term. The compressibility drag `CD_c` of Eq. 10.42 (p. 300) is the wave
//! model's (`wave.rs`, Eq. 10.48 and the Lock rise), not added here.
//!
//! A turbofan aircraft takes instead the jet-transport build-up of Poll and
//! Schumann (2021, App. B; [`super::parasite_poll`]), whose interference and
//! secondary-drag factors hold the pylons, fairings, afterbody and
//! excrescences: it has no separate items. A turboprop keeps the components
//! above with the upsweep and base drag of its afterbody, by geometry, and
//! books Obert's excrescence rows (2009, Fig. 40.29;
//! [`super::parasite_obert`]) on the component sum. Both replace Raymer's leakage and
//! protuberance allowance, so the margin defaults to 1
//! (`DragModelConfig::default_viscous_margin`).

use std::cell::Cell;
use std::f64::consts::PI;

use alas_atmo::Atmosphere;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::wing::Wing;

use alas_config::engines::PropulsionTechnology;

use super::parasite_items::{
    gear_sponson_area, upsweep_drag_area, wing_body_fairing_area, DragAreaItem,
    PYLON_WETTED_FRACTION, UPSWEEP_ITEM,
};
use super::parasite_obert::TURBOPROP_EXCRESCENCES;
use super::parasite_poll as poll;
use super::{wetted, AeroAnalysis};
use alas_geom::aircraft::wing::WINGTIP_DEVICE_NAME;

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

/// The skin-friction law of a component, kept with it so a drag table moves
/// it to another Reynolds number with the same law.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SkinFriction {
    /// Torenbeek's `0.044 / Re^(1/6)` of a lifting surface up to t/c 0.15.
    Torenbeek,
    /// Raymer's compressible turbulent flat plate (Eq. 12.27).
    Raymer,
    /// Poll and Schumann's `0.0269 / Re^0.14` (2021, Eq. 7).
    PollSchumann,
}

impl SkinFriction {
    /// The coefficient at `reynolds` on the component's length and `mach`.
    #[must_use]
    pub fn cf(self, reynolds: f64, mach: f64) -> f64 {
        match self {
            Self::Torenbeek => torenbeek_surface_cf(reynolds),
            Self::Raymer => AeroAnalysis::turbulent_cf(reynolds, mach),
            Self::PollSchumann => poll::skin_friction(reynolds),
        }
    }
}

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
    /// The law `skin_friction` was taken from.
    pub friction: SkinFriction,
    /// Whether the component is a lifting surface, which takes Torenbeek's
    /// friction and form factor ([`AeroAnalysis::component_skin_friction`]).
    pub lifting_surface: bool,
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
    /// body, nacelles; the product buildup then adds the wing-body fairing
    /// and the turbofan pylons (see `parasite_items`).
    pub components: Vec<ParasiteComponent<'p>>,
    /// Sum of the component terms.
    pub cd_components: f64,
    /// Excrescence rows booked as shares of `cd_components` (product
    /// turboprop: Obert 2009, Fig. 40.29), each at this condition.
    pub excrescences: Vec<DragAreaItem>,
    /// `1 +` the sum of those shares: the factor on the component sum, so
    /// the rows move with the components' Reynolds number. 1 without rows.
    pub component_scale: f64,
    /// Drag areas outside the component sum (Torenbeek and Raymer product
    /// buildup only):
    /// aft-fuselage upsweep at body angle of attack 0 (Torenbeek 1982, Fig.
    /// 3-27b) and base drag (Raymer Eqs. 12.37-12.38).
    pub drag_areas: Vec<DragAreaItem>,
    /// Centreline upsweep of the primary body the upsweep item was read at,
    /// rad; zero without one. A trimmed polar re-evaluates that item at its
    /// own body attitude with `upsweep_drag_area`.
    pub upsweep_rad: f64,
    /// Sum of the drag-area terms.
    pub cd_drag_areas: f64,
    /// What the configured `viscous_margin` adds to the components and drag
    /// areas: leakage and protuberance drag (Raymer Table 12.8), a
    /// percentage of the total parasite drag. Flap-track fairings are not
    /// drawn and not in it.
    pub cd_miscellaneous: f64,
    /// Total parasite drag coefficient, `cd_components` plus the
    /// excrescence rows, `cd_drag_areas` and `cd_miscellaneous`.
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

/// Largest mean thickness ratio at which Torenbeek's lifting-surface form
/// factor applies: the shape factor `r_phi` is given "for thickness ratios up
/// to 0.15" (E. Torenbeek, *Advanced Aircraft Design*, 2013, p. 299, below
/// Eq. 10.36). A thicker surface keeps Raymer's Eqs. 12.30 and 12.27. The
/// friction fit of Eq. 4.7 (p. 88) is stated for Reynolds numbers of 1e6 to
/// 1e8; the switch is on the thickness alone, so one surface keeps one law
/// along a mission.
pub const TORENBEEK_MAX_THICKNESS: f64 = 0.15;

/// Shape factor `r_phi` of Torenbeek's lifting-surface form factor: "between
/// 2.5 and 3.5 for thickness ratios up to 0.15", depending on the transition
/// point and the location of maximum suction (E. Torenbeek, *Advanced
/// Aircraft Design*, 2013, p. 299, below Eq. 10.36). The midpoint of the
/// printed range, one value for every surface and preset.
pub const TORENBEEK_SHAPE_FACTOR: f64 = 3.0;

/// Form factor of a lifting surface, `1 + r_phi (t/c) cos^2(sweep)`: the
/// factor of Eq. 10.36 (`c_dp = 2 (1 + r_phi t/c) Cf`) carried to the swept
/// wing by Eq. 10.37, with `(t/c)` the mean thickness ratio (E. Torenbeek,
/// *Advanced Aircraft Design*, 2013, p. 299), read as the form factor
/// `Phi_f` of Eq. 4.8 (p. 88). It has no Mach term: compressibility drag is
/// the separate `CD_c` of Eq. 10.42 (p. 300), carried by the wave model.
/// `sweep_m_deg` is the sweep of the maximum-thickness line.
pub fn torenbeek_surface_form_factor(thickness: f64, sweep_m_deg: f64) -> f64 {
    1.0 + TORENBEEK_SHAPE_FACTOR * thickness * sweep_m_deg.to_radians().cos().powi(2)
}

/// Turbulent flat-plate skin friction `Cf = 0.044 / Re^(1/6)` "adjusted to
/// Mach 0.5" for Reynolds numbers between 1e6 and 1e8 (E. Torenbeek,
/// *Advanced Aircraft Design*, 2013, p. 88, Eq. 4.7). It carries no Mach
/// argument: the correction is fixed at M 0.5, as Torenbeek states. The
/// Reynolds number is that of the surface's mean aerodynamic chord.
pub fn torenbeek_surface_cf(reynolds: f64) -> f64 {
    0.044 / reynolds.powf(1.0 / 6.0)
}

/// [`lifting_surface_form_factor`] for the product buildup, with the Mach
/// term `1.34 M^0.18` floored at 1 (DRG-07).
///
/// Raymer's Mach factor is a high-subsonic correlation: unclamped it falls
/// below 1 under M = (1/1.34)^(1/0.18) = 0.195 (0.78 at M 0.05, 0.885 at
/// M 0.10), which would make a low-speed wing's pressure drag smaller than
/// that of the incompressible thickness factor alone. The floor leaves every
/// M >= 0.2 value bit-identical. The frozen reference path keeps the
/// unclamped function.
pub fn lifting_surface_form_factor_floored(
    thickness: f64,
    max_thickness_x_over_c: f64,
    mach: f64,
    sweep_m_deg: f64,
) -> f64 {
    (1.0 + 0.6 / max_thickness_x_over_c * thickness + 100.0 * thickness.powf(4.0))
        * ((1.34 * mach.powf(0.18)).max(1.0) * sweep_m_deg.to_radians().cos().powf(0.28))
}

/// Raymer 6th ed. eq. 12.31 (p. 422), the fuselage and smooth-canopy form
/// factor `0.9 + 5/f^1.5 + f/400`, with `f` the length over the equivalent
/// diameter. The footnote on that page gives `1 + 60/f^3 + f/400` as the form
/// of the prior editions, replaced in the 6th.
pub fn fuselage_form_factor(fineness: f64) -> f64 {
    0.9 + 5.0 / fineness.powf(1.5) + fineness / 400.0
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
pub(super) fn body_length_and_diameter(body: &Fuselage) -> (f64, f64) {
    let diameter = body
        .xsecs
        .iter()
        .map(|xsec| (xsec.width * xsec.height).sqrt())
        .fold(0.0, f64::max);
    (AeroAnalysis::body_length(body), diameter)
}

impl<'a> AeroAnalysis<'a> {
    /// Whether this airframe takes Poll and Schumann's jet-transport
    /// build-up ([`super::parasite_poll`]): a product turbofan. Every other
    /// product airframe takes the Torenbeek and Raymer build-up with its
    /// upsweep, base and excrescence items.
    pub(super) fn poll_build_up(&self) -> bool {
        !self.reference_compatibility
            && matches!(
                self.geometry.engine.propulsion_technology,
                PropulsionTechnology::Turbofan
            )
    }

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

        let mut drag_areas: Vec<DragAreaItem> = Vec::new();
        // The Torenbeek and Raymer build-up books the afterbody upsweep and
        // base drag by geometry; Poll and Schumann's fuselage FIFS holds them.
        let mut upsweep_rad = 0.0;
        let mut components = Vec::with_capacity(plane.wings.len() + 2 * plane.fuselages.len());
        // Lifting surfaces up to t/c 0.15 take Torenbeek's friction and form factor, every
        // other component Raymer's (see the module note).
        let lifting = Cell::new(false);
        // A turbofan airframe takes Poll and Schumann's build-up, every other
        // product airframe Torenbeek's and Raymer's (see the module note).
        let jet = self.poll_build_up();
        let mut push = |name: &'a str, wetted: f64, length: f64, form_factor: f64, q: f64| {
            let reynolds = density * velocity * length / viscosity;
            let law = if jet {
                SkinFriction::PollSchumann
            } else if lifting.get() {
                SkinFriction::Torenbeek
            } else {
                SkinFriction::Raymer
            };
            let cf = law.cf(reynolds, mach);
            components.push(ParasiteComponent {
                name,
                wetted_area_m2: wetted,
                reynolds_length_m: length,
                reynolds,
                skin_friction: cf,
                friction: law,
                lifting_surface: lifting.get(),
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
                // The plan inside the body is under the fuselage and the fairing:
                // it adds no wetted area and enters neither the Reynolds length
                // nor the sweep of the exposed wing's form factor. A dorsal's skin is
                // what stands ahead of the fin edge; its thickness stays that of the
                // built sections.
                let carried = wing.skin_planform();
                let thickness = self.wing_thicknesses()[index];
                let sweep_m_deg =
                    chord_line_sweep_deg(&carried, x_over_c).unwrap_or(self.sweep_deg);
                // Exposed planform: the part of a symmetric surface inside
                // the body carries no skin friction (NDARC convention, see
                // `wetted`); the coefficient keeps the gross reference area. A plan
                // the geometry already measured outside the body loses nothing more.
                let buried = match body {
                    Some(body)
                        if self.drag.exclude_buried_main_wing_area
                            && !wing.skin_planform_is_exposed() =>
                    {
                        wetted::buried_surface_area(&carried, body)
                    }
                    _ => 0.0,
                };
                let exposed = (carried.unfolded_area() - buried).max(0.0);
                if jet {
                    // Eqs. B-6 to B-11: the main wing and its tip device
                    // take the wing factors, every other surface the tail's.
                    let main = index == 0 || wing.name == WINGTIP_DEVICE_NAME;
                    lifting.set(true);
                    push(
                        &wing.name,
                        lifting_surface_wetted_area(exposed, thickness),
                        poll::mean_chord(carried.unfolded_area(), carried.unfolded_span()),
                        if main {
                            poll::wing_form_factor(thickness, sweep_m_deg)
                        } else if wing.symmetric {
                            poll::HORIZONTAL_TAIL_FF
                        } else {
                            poll::VERTICAL_TAIL_FF
                        },
                        if main {
                            poll::WING_FIFS
                        } else {
                            poll::TAIL_FIFS
                        },
                    );
                    lifting.set(false);
                    continue;
                }
                let torenbeek = thickness <= TORENBEEK_MAX_THICKNESS;
                lifting.set(torenbeek);
                push(
                    &wing.name,
                    lifting_surface_wetted_area(exposed, thickness),
                    carried.mean_aerodynamic_chord(),
                    if torenbeek {
                        torenbeek_surface_form_factor(thickness, sweep_m_deg)
                    } else {
                        lifting_surface_form_factor_floored(thickness, x_over_c, mach, sweep_m_deg)
                    },
                    if index == 0 {
                        self.drag.interference_factor_wing
                    } else {
                        EMPENNAGE_INTERFERENCE
                    },
                );
                lifting.set(false);
            }
            let mut fairing = None;
            if let Some(fuselage) = body.filter(|_| jet) {
                // Eqs. B-1 to B-5: FIFS holds the fairing, sponsons and afterbody.
                let (length, diameter) = body_length_and_diameter(fuselage);
                push(
                    &fuselage.name,
                    fuselage.area_wetted(),
                    length,
                    poll::fuselage_form_factor(length / diameter),
                    poll::FUSELAGE_FIFS,
                );
            } else if let Some(fuselage) = body {
                let (length, diameter) = body_length_and_diameter(fuselage);
                let form_factor = Self::body_form_factor(length, diameter, fuselage_form_factor);
                push(
                    &fuselage.name,
                    fuselage.area_wetted(),
                    length,
                    form_factor,
                    FUSELAGE_INTERFERENCE,
                );
                // The fairing is body surface (Torenbeek 2013, Eq. A.11).
                let area = plane
                    .wings
                    .first()
                    .map_or(0.0, |wing| wing_body_fairing_area(wing, fuselage));
                fairing = Some((area, length, form_factor));
                let drawn = &self.geometry.fuselage;
                if let (Some(sponson_length), Some(half_width), Some(depth)) = (
                    drawn.gear_sponson_length_m,
                    drawn.gear_sponson_half_width_m,
                    drawn.gear_sponson_depth_m,
                ) {
                    push(
                        "Gear sponsons",
                        gear_sponson_area(fuselage, sponson_length, half_width, depth),
                        length,
                        form_factor,
                        FUSELAGE_INTERFERENCE,
                    );
                }
                if let Some(aft) = self.aft_body(fuselage) {
                    upsweep_rad = aft.upsweep_rad;
                    for (name, drag_area_m2) in [
                        (
                            UPSWEEP_ITEM,
                            upsweep_drag_area(aft.upsweep_rad, aft.max_area_m2, 0.0),
                        ),
                        ("Fuselage base", aft.base_drag_area(mach)),
                    ] {
                        drag_areas.push(DragAreaItem {
                            name,
                            drag_area_m2,
                            cd: drag_area_m2 / s_ref,
                        });
                    }
                }
            }
            let podded_turbofan = matches!(
                self.geometry.engine.propulsion_technology,
                PropulsionTechnology::Turbofan
            );
            let mut pylons = Vec::new();
            for nacelle in plane.fuselages.iter().skip(1) {
                let (length, diameter) = body_length_and_diameter(nacelle);
                if jet {
                    // Eqs. B-12 and B-13: pod and pylon in one factor.
                    push(
                        &nacelle.name,
                        nacelle.area_wetted(),
                        length,
                        poll::NACELLE_FF_FI_FS,
                        1.0,
                    );
                    continue;
                }
                let form_factor = Self::body_form_factor(length, diameter, nacelle_form_factor);
                push(
                    &nacelle.name,
                    nacelle.area_wetted(),
                    length,
                    form_factor,
                    self.drag.interference_factor_nacelle,
                );
                // Wing pods hang on a pylon (Torenbeek 2013, App. A.4); the
                // builder hangs every off-centreline pod under the wing and
                // puts a centreline engine in the tail, without one.
                let wing_pod = nacelle
                    .xsecs
                    .first()
                    .is_some_and(|xsec| xsec.xyz_c[1] != 0.0);
                if podded_turbofan && wing_pod {
                    pylons.push((
                        PYLON_WETTED_FRACTION * nacelle.area_wetted(),
                        length,
                        form_factor,
                    ));
                }
            }
            if let Some((area, length, form_factor)) = fairing.filter(|(area, ..)| *area > 0.0) {
                push(
                    "Wing-body fairing",
                    area,
                    length,
                    form_factor,
                    FUSELAGE_INTERFERENCE,
                );
            }
            for (area, length, form_factor) in pylons {
                push(
                    "Pylon",
                    area,
                    length,
                    form_factor,
                    self.drag.interference_factor_nacelle,
                );
            }
        }

        let mut cd_components = 0.0;
        for component in &components {
            cd_components += component.cd;
        }
        // A product turboprop books Obert's excrescence rows as shares of its
        // component sum: they scale with it, Reynolds number included.
        let mut excrescences = Vec::new();
        let mut component_scale = 1.0;
        if !self.reference_compatibility && !jet {
            for (name, share) in TURBOPROP_EXCRESCENCES {
                let cd = share * cd_components;
                excrescences.push(DragAreaItem {
                    name,
                    drag_area_m2: cd * s_ref,
                    cd,
                });
                component_scale += share;
            }
        }
        let cd_excrescences = cd_components * (component_scale - 1.0);
        let cd_drag_areas: f64 = drag_areas.iter().map(|item| item.cd).sum();
        let cd0 = (cd_components * component_scale + cd_drag_areas) * self.drag.margin();
        ParasiteBreakdown {
            components,
            cd_components,
            excrescences,
            component_scale,
            drag_areas,
            cd_drag_areas,
            upsweep_rad,
            cd_miscellaneous: cd0 - cd_components - cd_excrescences - cd_drag_areas,
            cd0,
        }
    }

    /// Skin friction of a component at Reynolds number `reynolds` on its own
    /// length: Torenbeek's Mach-free `Cf` for a lifting surface
    /// ([`torenbeek_surface_cf`]), Raymer's compressible flat plate
    /// ([`Self::turbulent_cf`]) for every other component.
    pub fn component_skin_friction(lifting_surface: bool, reynolds: f64, mach: f64) -> f64 {
        if lifting_surface {
            torenbeek_surface_cf(reynolds)
        } else {
            Self::turbulent_cf(reynolds, mach)
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
        let items: f64 = breakdown.drag_areas.iter().map(|d| d.cd).sum();
        assert!((items - breakdown.cd_drag_areas).abs() < 1e-15);
        let rows: f64 = breakdown.excrescences.iter().map(|d| d.cd).sum();
        assert!((rows - breakdown.cd_components * (breakdown.component_scale - 1.0)).abs() < 1e-15);
        assert!(
            (breakdown.cd0
                - breakdown.cd_components * breakdown.component_scale
                - breakdown.cd_drag_areas
                - breakdown.cd_miscellaneous)
                .abs()
                < 1e-15
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
        // Fineness 20 / 4 = 5, Poll and Schumann Eq. B-3: 1 + 2.2/5^1.5 + 3.8/5^3.
        let poll = 1.0 + 2.2 / 5.0_f64.powf(1.5) + 3.8 / 125.0;
        assert!((body.form_factor - poll).abs() < 1e-12);
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
        // Eq. 12.31 at f = 10: 0.9 + 0.158114 + 0.025.
        assert!((fuselage_form_factor(10.0) - 1.083_113_883).abs() < 1e-9);
        // Eq. 12.32 at f = 2: 1 + 0.175.
        assert!((nacelle_form_factor(2.0) - 1.175).abs() < 1e-15);
        // Eq. 7.10 at t/c 0.12: 2.0394 S_exp; eq. 7.9 below t/c 0.05.
        assert!((lifting_surface_wetted_area(100.0, 0.12) - 203.94).abs() < 1e-9);
        assert!((lifting_surface_wetted_area(100.0, 0.04) - 200.3).abs() < 1e-9);
    }

    #[test]
    fn the_product_wing_takes_torenbeeks_mach_free_form_factor_and_friction() {
        // Eq. 10.37 at t/c 0.10, sweep 30 deg: 1 + 3.0 * 0.10 * 0.75.
        assert!((torenbeek_surface_form_factor(0.10, 30.0) - 1.225).abs() < 1e-12);
        // Eq. 4.7 at Re 1e7: 0.044 / 10^(7/6).
        let cf = torenbeek_surface_cf(1.0e7);
        assert!((cf - 0.044 / 10.0_f64.powf(7.0 / 6.0)).abs() < 1e-15);
        assert!((cf - 0.002997).abs() < 1e-5);
    }

    #[test]
    fn the_product_wing_form_factor_is_floored_below_mach_0_195_and_equal_above() {
        // DRG-07: the Mach term is 1 below M = 0.195, so the ratio to the
        // unclamped Raymer value is exactly 1 / (1.34 M^0.18).
        for mach in [0.05_f64, 0.10, 0.15] {
            let raymer = lifting_surface_form_factor(0.12, 0.35, mach, 25.0);
            let floored = lifting_surface_form_factor_floored(0.12, 0.35, mach, 25.0);
            let ratio = floored / raymer;
            assert!(
                (ratio - 1.0 / (1.34 * mach.powf(0.18))).abs() < 1e-12,
                "{mach}"
            );
            assert!(ratio > 1.0);
        }
        assert!((1.34_f64 * 0.05_f64.powf(0.18) - 0.78).abs() < 5e-3);
        for mach in [0.20, 0.45, 0.78, 0.85] {
            assert_eq!(
                lifting_surface_form_factor_floored(0.12, 0.35, mach, 25.0),
                lifting_surface_form_factor(0.12, 0.35, mach, 25.0),
                "{mach}"
            );
        }
    }

    #[test]
    fn the_fuselage_form_factor_falls_with_fineness_up_to_its_optimum() {
        // d/df (5 f^-1.5 + f/400) = 0 at f = 3000^(2/5) = 24.6; below it
        // the factor falls monotonically with fineness, as a slender body's
        // pressure drag does.
        let optimum = 3_000.0_f64.powf(0.4);
        assert!(fuselage_form_factor(6.0) > fuselage_form_factor(9.0));
        assert!(fuselage_form_factor(9.0) > fuselage_form_factor(optimum));
        assert!(fuselage_form_factor(optimum) < fuselage_form_factor(optimum + 1.0));
    }
}
