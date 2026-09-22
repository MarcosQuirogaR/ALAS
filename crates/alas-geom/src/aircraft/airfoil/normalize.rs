// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from reference geometry/geometry/airfoil/airfoil.py
// (Airfoil.normalize, and the translate/scale/rotate it composes).
// Upstream: reference geometry 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! Putting a section into the frame the airfoil surrogate is trained in:
//! leading edge at the origin, trailing edge at `(1, 0)`, unit chord.
//!
//! This exists for `alas-aero::neuralfoil`, which is its only caller. The
//! network is trained on Kulfan weights of unit-chord sections at zero
//! incidence, so every path from a shape to a polar starts by measuring how
//! far the shape is from that frame, moving it there, and then correcting the
//! answer back. The four numbers [`Normalization`] carries are exactly what
//! that correction needs: a translation the moment coefficient has to be
//! moved back across, a scale the Reynolds number has to be divided by, and a
//! rotation the angle of attack has to be offset by, which is why upstream's
//! `return_dict=True` branch is the one translated and the bare
//! airfoil-returning branch is not.
//!
//! # How the frame is found
//!
//! The trailing edge is the *midpoint* of the first and last coordinates, not
//! a vertex: a section with an open trailing edge has two of them and neither
//! is the point the chord line ends at. The leading edge, by contrast, is
//! always one of the original vertices: the one furthest from that midpoint,
//! which is what makes the chord the longest line that fits inside the
//! section. Upstream's `np.argmax` takes the first vertex on a tie, and so
//! does this.
//!
//! # Scope
//!
//! `translate`, `scale` and `rotate` are public methods upstream and private
//! helpers here, because `normalize` is the only thing in this program that
//! reaches any of them. That scoping also sidesteps the one branch of `scale`
//! that would be delicate to reproduce: a negative scale factor reverses the
//! coordinate order so the section still runs upper-trailing-edge-first, in
//! two reversed slices whose bounds are easy to get subtly wrong and which no
//! fixture here could reach. `normalize` scales by `1 / chord`, which is
//! positive on every section that has a chord at all, so that branch is
//! unreachable from this crate and is left untranslated rather than written
//! blind. A future caller that needs a mirrored airfoil should add it
//! deliberately, with a fixture that exercises the reordering.

use super::Airfoil;

/// The output of [`Airfoil::normalize`]: the moved section, and the four
/// numbers describing the move.
///
/// Each field is the *required change* (what had to be done to the original
/// to put it in the standard frame) following upstream's convention, which
/// is what makes them directly usable as corrections on the way back out.
#[derive(Debug, Clone, PartialEq)]
pub struct Normalization {
    /// The section in the standard frame.
    pub airfoil: Airfoil,
    /// How far the leading edge had to move in `x` to reach the origin.
    pub x_translation: f64,
    /// How far the leading edge had to move in `y` to reach the origin.
    pub y_translation: f64,
    /// What the section had to be multiplied by to reach unit chord. Greater
    /// than one means the original was smaller than unit chord.
    pub scale_factor: f64,
    /// How far the section had to be rotated counter-clockwise, in degrees,
    /// to put the trailing edge on the positive `x` axis.
    pub rotation_angle_deg: f64,
}

impl Airfoil {
    /// A copy of this airfoil with its leading edge at `(0, 0)`, its trailing
    /// edge at `(1, 0)` and unit chord, together with the translation, scale
    /// and rotation that got it there: `normalize(return_dict=True)`.
    ///
    /// An airfoil with no coordinates comes back unchanged with an identity
    /// transform, since there is no trailing edge to measure from. Upstream
    /// raises on that input; nothing in this crate constructs one, and
    /// [`Airfoil::le_index`] already answers the same question the same way
    /// rather than panicking.
    pub fn normalize(&self) -> Normalization {
        let Some(&(first_x, first_y)) = self.coordinates.first() else {
            return Normalization {
                airfoil: self.clone(),
                x_translation: 0.0,
                y_translation: 0.0,
                scale_factor: 1.0,
                rotation_angle_deg: 0.0,
            };
        };
        // Unwrap-free because `first` succeeded, so `last` cannot fail.
        let (last_x, last_y) = self.coordinates[self.coordinates.len() - 1];

        // The trailing edge is between the two trailing-edge vertices, not on
        // either of them: an open section has both, and the chord ends at
        // neither.
        let x_te = (first_x + last_x) / 2.0;
        let y_te = (first_y + last_y) / 2.0;

        let mut le_index = 0;
        let mut chord = f64::NEG_INFINITY;
        for (index, &(x, y)) in self.coordinates.iter().enumerate() {
            // `**0.5` upstream, which NumPy's power loop dispatches to `sqrt`
            // for that exponent; the same value, without a `pow` in the way.
            let distance = ((x - x_te).powi(2) + (y - y_te).powi(2)).sqrt();
            if distance > chord {
                chord = distance;
                le_index = index;
            }
        }

        let (le_x, le_y) = self.coordinates[le_index];
        let x_translation = -le_x;
        let y_translation = -le_y;
        let scale_factor = 1.0 / chord;

        // Translation and scaling are two separate passes upstream, and the
        // rounding of `(x + t) * s` is not the rounding of `x * s + t * s`.
        let moved = self
            .translated(x_translation, y_translation)
            .scaled(scale_factor);

        let x_te =
            (moved.coordinates[0].0 + moved.coordinates[moved.coordinates.len() - 1].0) / 2.0;
        let y_te =
            (moved.coordinates[0].1 + moved.coordinates[moved.coordinates.len() - 1].1) / 2.0;
        let rotation_angle = -y_te.atan2(x_te);

        Normalization {
            airfoil: moved.rotated(rotation_angle),
            x_translation,
            y_translation,
            scale_factor,
            rotation_angle_deg: rotation_angle.to_degrees(),
        }
    }

    /// Every coordinate moved by `(translate_x, translate_y)`: `translate`.
    fn translated(&self, translate_x: f64, translate_y: f64) -> Self {
        Self {
            name: self.name.clone(),
            coordinates: self
                .coordinates
                .iter()
                .map(|&(x, y)| (x + translate_x, y + translate_y))
                .collect(),
        }
    }

    /// Every coordinate scaled about the origin by `factor` in both
    /// directions: `scale(scale_x=f, scale_y=f)` for positive `f`. See the
    /// module documentation for why the negative branches are not here.
    fn scaled(&self, factor: f64) -> Self {
        Self {
            name: self.name.clone(),
            coordinates: self
                .coordinates
                .iter()
                .map(|&(x, y)| (x * factor, y * factor))
                .collect(),
        }
    }

    /// Every coordinate rotated counter-clockwise about the origin by
    /// `angle` radians: `rotate(angle)` at its default centre.
    fn rotated(&self, angle: f64) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self {
            name: self.name.clone(),
            coordinates: self
                .coordinates
                .iter()
                .map(|&(x, y)| (cos * x - sin * y, sin * x + cos * y))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wedge whose chord is obvious by inspection: leading edge at the
    /// origin, trailing edge at `(2, 0)`, so a chord of exactly 2.
    fn wedge() -> Airfoil {
        Airfoil::from_coordinates(
            "wedge",
            vec![
                (2.0, 0.05),
                (1.0, 0.1),
                (0.0, 0.0),
                (1.0, -0.1),
                (2.0, -0.05),
            ],
        )
    }

    #[test]
    fn a_section_already_in_the_standard_frame_is_left_alone() {
        let unit = Airfoil::from_coordinates(
            "unit",
            vec![(1.0, 0.0), (0.5, 0.1), (0.0, 0.0), (0.5, -0.1), (1.0, 0.0)],
        );
        let normalized = unit.normalize();
        assert_eq!(normalized.x_translation, 0.0);
        assert_eq!(normalized.y_translation, 0.0);
        assert_eq!(normalized.scale_factor, 1.0);
        assert_eq!(normalized.rotation_angle_deg, 0.0);
        assert_eq!(normalized.airfoil.coordinates, unit.coordinates);
    }

    #[test]
    fn the_scale_factor_is_the_reciprocal_of_the_chord() {
        assert!((wedge().normalize().scale_factor - 0.5).abs() < 1e-15);
    }

    #[test]
    fn normalizing_puts_the_leading_edge_at_the_origin_and_the_trailing_edge_at_one() {
        // Take the wedge somewhere else entirely (moved, grown and tilted)
        // and check the frame comes back, not just the numbers describing it.
        let tilted = wedge().translated(3.0, -1.0).scaled(4.0).rotated(0.3);
        let normalized = tilted.normalize();
        let points = &normalized.airfoil.coordinates;
        let last = points.len() - 1;

        assert!(points[2].0.abs() < 1e-14 && points[2].1.abs() < 1e-14);
        let x_te = (points[0].0 + points[last].0) / 2.0;
        let y_te = (points[0].1 + points[last].1) / 2.0;
        assert!((x_te - 1.0).abs() < 1e-14, "trailing edge x = {x_te}");
        assert!(y_te.abs() < 1e-14, "trailing edge y = {y_te}");
    }

    #[test]
    fn the_rotation_angle_undoes_the_tilt_it_was_given() {
        // Rotating by +0.3 rad requires -0.3 rad to undo, and the reported
        // angle is the required change, in degrees.
        let tilted = wedge().rotated(0.3);
        let reported = tilted.normalize().rotation_angle_deg;
        assert!(
            (reported + 0.3_f64.to_degrees()).abs() < 1e-12,
            "reported {reported} deg"
        );
    }

    #[test]
    fn the_leading_edge_is_the_vertex_furthest_from_the_trailing_edge_midpoint() {
        // Not the vertex of least x: a section whose nose droops below the
        // chord line has its leading edge off the axis, and taking the
        // leftmost point instead would find a different vertex on a section
        // tilted far enough.
        let drooped = Airfoil::from_coordinates(
            "drooped",
            vec![
                (1.0, 0.0),
                (0.05, 0.02),
                (0.0, -0.30),
                (0.5, -0.4),
                (1.0, 0.0),
            ],
        );
        let normalized = drooped.normalize();
        // Vertex 2 is sqrt(1.09) = 1.0440 from the trailing-edge midpoint
        // (1, 0); vertex 1, the next-leftmost, is only 0.9502. The chord is
        // the longer of the two.
        assert!((1.0 / normalized.scale_factor - 1.09_f64.sqrt()).abs() < 1e-12);
    }

    #[test]
    fn a_tie_for_the_leading_edge_takes_the_first_vertex() {
        // `np.argmax` returns the first maximum, which puts a tie on the
        // upper surface. Both candidates are exactly 1.0 from (1, 0) here.
        let blunt = Airfoil::from_coordinates(
            "blunt",
            vec![(1.0, 0.0), (0.0, 0.0), (0.0, 0.0), (1.0, 0.0)],
        );
        // The tie is broken toward vertex 1, whose y is what `y_translation`
        // reports; both are zero here, so the observable is that this does
        // not panic and reports the shared chord of 1.
        assert_eq!(blunt.normalize().scale_factor, 1.0);
    }

    #[test]
    fn an_empty_section_normalizes_to_an_identity_rather_than_panicking() {
        let empty = Airfoil::from_coordinates("empty", Vec::new());
        let normalized = empty.normalize();
        assert_eq!(normalized.scale_factor, 1.0);
        assert!(normalized.airfoil.coordinates.is_empty());
    }
}
