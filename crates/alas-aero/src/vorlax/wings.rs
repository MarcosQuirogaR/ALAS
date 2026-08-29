// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Aerodynamics/Common/Fidelity_Zero/Lift/
// make_VLM_wings.py and
// mission analysis model/Methods/Flight_Dynamics/Static_Stability/Approximations/
// Supporting_Functions/convert_sweep.py.
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! Turning a wing into the span breaks the panelizer discretizes between.
//!
//! A *span break* is a spanwise station where the planform is allowed to
//! change: the root, the tip, a segment boundary, or -- upstream -- either
//! edge of a control surface. Between two of them the surface is a trapezoid,
//! and that trapezoid is what
//! [`super::distribution`] lays panels on.
//!
//! # Scope
//!
//! `make_VLM_wings` is 768 lines, and about six hundred of them exist to turn
//! control surfaces into wings of their own: a three-way merge sort over
//! leading-edge, trailing-edge and segment breaks, a scan that copies a
//! control surface's chord cut across any segment boundary it crosses, and a
//! quaternion hinge rotation in the panelizer for each resulting surface.
//! None of it runs here. `Fidelity_Zero.__defaults__` sets
//! `discretize_control_surfaces = False` and the mission runner overrides
//! nothing, so `populate_control_sections` is never called, no segment ever
//! acquires a control surface, and both `LE_breaks` and `TE_breaks` are empty
//! at the merge -- which reduces the merge to "the segment breaks, in order"
//! and the cut arrays to their `[[0, 0], [1, 1]]` defaults.
//!
//! What remains is the other path: a wing with no `Segments` at all, which is
//! every wing the mission runner builds. `convert_to_segmented_wing` gives it
//! exactly two -- root and tip -- and this module produces the two span
//! breaks that follow from them.
//!
//! Left untranslated for the same reason: the `All_Moving_Surface` branch (no
//! surface here is one), the airfoil lookup on a segment (no wing the runner
//! builds carries an `Airfoil`, so the camber line is identically zero), and
//! `make_span_break_tag`, whose only consumer is the container key of a
//! collection this port indexes by position.

/// One spanwise station where the planform is allowed to change.
///
/// The `cuts` array upstream carries is absent: with no control surface
/// discretized it is `[[0, 0], [1, 1]]` on every break, which makes the
/// leading-edge cut 0 and the trailing-edge cut 1 -- that is, no cut -- and
/// the panelizer's `np.interp` between them the identity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpanBreak {
    /// Fraction of the (half-)span this break sits at.
    pub span_fraction: f64,
    /// The local chord here, before any control-surface cut.
    pub local_chord_m: f64,
    /// Incidence here.
    pub twist_rad: f64,
    /// Dihedral of the section running outboard from here.
    pub dihedral_outboard_rad: f64,
    /// Leading-edge sweep of the section running outboard from here.
    pub sweep_outboard_le_rad: f64,
    /// Accumulated streamwise offset of the leading edge at this station.
    pub x_offset_m: f64,
    /// Accumulated offset from dihedral at this station: `z` for a
    /// horizontal surface, `y` for a vertical one.
    pub dih_offset_m: f64,
}

/// The two span breaks of an unsegmented trapezoidal wing.
///
/// This is `convert_to_segmented_wing` followed by `make_VLM_wings`' own
/// reformatting loop and `make_span_break_from_segment`, collapsed: with two
/// segments there are only two breaks and each one's fields come from a
/// single place.
///
/// Two details are upstream's and are reproduced rather than tidied. The tip
/// break's local chord is `taper * chords.root` and not `chords.tip` -- the
/// reformatting loop overwrites the chord `convert_to_segmented_wing` had
/// just set from `chords.tip`, so a wing whose taper and tip chord disagree
/// is panelized on the taper. And the tip break's outboard sweep is set to
/// `1e-8` and then never read, because the panelizer replaces the last
/// break's sweep with zero; the value is carried anyway so the two
/// implementations hold the same object.
pub fn span_breaks(wing: &super::types::VlmWing) -> [SpanBreak; 2] {
    let root_chord = wing.chord_root_m;
    let tip_chord = wing.taper * root_chord;

    let half_span = if wing.symmetric {
        wing.span_projected_m * 0.5
    } else {
        wing.span_projected_m
    };

    // The root segment's leading-edge sweep. A caller that supplied one has
    // it used directly; otherwise it is converted from the quarter-chord
    // sweep of the section between the two segments, which for a two-segment
    // wing is the whole wing.
    let sweep_le = wing.sweep_leading_edge_rad.unwrap_or_else(|| {
        convert_sweep_segments(
            wing.sweep_quarter_chord_rad,
            SweepSection {
                root_chord_m: root_chord,
                tip_chord_m: tip_chord,
                span_m: if wing.symmetric {
                    wing.span_projected_m
                } else {
                    wing.span_projected_m * 2.0
                },
                span_fraction: 1.0,
            },
            0.25,
            0.0,
        )
    });

    [
        SpanBreak {
            span_fraction: 0.0,
            local_chord_m: root_chord,
            twist_rad: wing.twist_root_rad,
            dihedral_outboard_rad: wing.dihedral_rad,
            sweep_outboard_le_rad: sweep_le,
            x_offset_m: 0.0,
            dih_offset_m: 0.0,
        },
        SpanBreak {
            span_fraction: 1.0,
            local_chord_m: tip_chord,
            twist_rad: wing.twist_tip_rad,
            dihedral_outboard_rad: 0.0,
            sweep_outboard_le_rad: TIP_SWEEP_PLACEHOLDER,
            x_offset_m: half_span * sweep_le.tan(),
            dih_offset_m: half_span * wing.dihedral_rad.tan(),
        },
    ]
}

/// What `make_VLM_wings` writes onto the outboard sweep of the last segment.
///
/// It is not a small sweep angle; it is a marker that stands in for "there is
/// nothing outboard of here". The panelizer never reads it -- `break_sweep`
/// is set to zero on the last break -- so the value has no effect and is
/// carried only so the two implementations hold the same object.
const TIP_SWEEP_PLACEHOLDER: f64 = 1e-8;

/// The trapezoid a sweep conversion is referred to.
///
/// Upstream passes two `Segment`s and the owning `Wing` and reads four
/// numbers off the three of them. Taking those four directly keeps this
/// function from needing a wing type it does not otherwise use, the same
/// scoping `alas-aero::analysis`'s `TrimPoint` uses.
pub struct SweepSection {
    /// Chord at the inboard end of the section.
    pub root_chord_m: f64,
    /// Chord at the outboard end.
    pub tip_chord_m: f64,
    /// The *full* span of the wing this section belongs to, both sides.
    pub span_m: f64,
    /// The section's span as a fraction of that.
    pub span_fraction: f64,
}

/// Re-refer a sweep angle from one chord fraction to another.
///
/// The wing is treated as the equivalent trapezoid with the same reference
/// sweep, aspect ratio and taper, which is exact here because every section
/// this reaches *is* a trapezoid.
///
/// The two-step form is upstream's and is kept: an old fraction of exactly
/// zero means the input already is a leading-edge sweep and is taken as-is,
/// and anything else is converted to one first. The composition therefore
/// evaluates `atan(tan(x))` when the new fraction is zero, which is not the
/// identity in floating point and is where a "simplified" version would
/// start disagreeing in the last digits.
pub fn convert_sweep_segments(
    old_sweep_rad: f64,
    section: SweepSection,
    old_ref_chord_fraction: f64,
    new_ref_chord_fraction: f64,
) -> f64 {
    if old_ref_chord_fraction == new_ref_chord_fraction {
        return old_sweep_rad;
    }

    let taper = section.tip_chord_m / section.root_chord_m;
    let section_span = section.span_m * section.span_fraction;
    let chord_mean_geometric = 0.5 * (section.root_chord_m + section.tip_chord_m);
    let aspect_ratio = section_span / chord_mean_geometric;

    let sweep_le = if old_ref_chord_fraction == 0.0 {
        old_sweep_rad
    } else {
        (old_sweep_rad.tan()
            + 4.0 * old_ref_chord_fraction * (1.0 - taper) / (aspect_ratio * (1.0 + taper)))
            .atan()
    };

    (sweep_le.tan() - 4.0 * new_ref_chord_fraction * (1.0 - taper) / (aspect_ratio * (1.0 + taper)))
        .atan()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vorlax::types::VlmWing;

    fn trapezoidal_wing() -> VlmWing {
        VlmWing {
            tag: "main_wing".to_string(),
            symmetric: true,
            vertical: false,
            vortex_lift: false,
            span_projected_m: 60.0,
            chord_root_m: 10.0,
            chord_tip_m: 3.0,
            taper: 0.3,
            aspect_ratio: 9.0,
            sweep_quarter_chord_rad: 0.5,
            sweep_leading_edge_rad: None,
            twist_root_rad: 0.05,
            twist_tip_rad: -0.02,
            dihedral_rad: 0.08,
            area_reference_m2: 400.0,
            origin_m: [20.0, 0.0, -1.0],
        }
    }

    #[test]
    fn a_quarter_chord_sweep_converts_to_a_larger_leading_edge_sweep_on_a_tapered_wing() {
        let wing = trapezoidal_wing();
        let breaks = span_breaks(&wing);
        assert!(
            breaks[0].sweep_outboard_le_rad > wing.sweep_quarter_chord_rad,
            "a wing that tapers has its leading edge swept further back than its quarter chord"
        );
    }

    #[test]
    fn converting_a_sweep_to_the_fraction_it_is_already_referred_to_returns_it_unchanged() {
        let section = SweepSection {
            root_chord_m: 10.0,
            tip_chord_m: 3.0,
            span_m: 60.0,
            span_fraction: 1.0,
        };
        assert_eq!(convert_sweep_segments(0.4, section, 0.25, 0.25), 0.4);
    }

    #[test]
    fn an_untapered_section_has_the_same_sweep_at_every_chord_fraction() {
        let section = SweepSection {
            root_chord_m: 5.0,
            tip_chord_m: 5.0,
            span_m: 40.0,
            span_fraction: 1.0,
        };
        let converted = convert_sweep_segments(0.3, section, 0.25, 0.0);
        assert!(
            (converted - 0.3).abs() < 1e-15,
            "the correction term carries a factor of (1 - taper), which is zero here"
        );
    }

    #[test]
    fn the_tip_break_takes_its_chord_from_the_taper_and_not_from_the_tip_chord_field() {
        let mut wing = trapezoidal_wing();
        // A tip chord that disagrees with the taper is what makes the choice
        // observable; upstream's reformatting loop overwrites the chord it
        // had just read from this field.
        wing.chord_tip_m = 99.0;
        let breaks = span_breaks(&wing);
        assert_eq!(breaks[1].local_chord_m, wing.taper * wing.chord_root_m);
    }

    #[test]
    fn the_tip_break_accumulates_the_root_sections_sweep_and_dihedral_over_the_half_span() {
        let wing = trapezoidal_wing();
        let breaks = span_breaks(&wing);
        let half_span = wing.span_projected_m * 0.5;
        assert_eq!(
            breaks[1].x_offset_m,
            half_span * breaks[0].sweep_outboard_le_rad.tan()
        );
        assert_eq!(breaks[1].dih_offset_m, half_span * wing.dihedral_rad.tan());
    }

    #[test]
    fn an_asymmetric_wing_accumulates_over_its_whole_projected_span() {
        let mut wing = trapezoidal_wing();
        wing.symmetric = false;
        let breaks = span_breaks(&wing);
        assert_eq!(
            breaks[1].dih_offset_m,
            wing.span_projected_m * wing.dihedral_rad.tan()
        );
    }

    #[test]
    fn a_supplied_leading_edge_sweep_is_used_rather_than_derived() {
        let mut wing = trapezoidal_wing();
        wing.sweep_leading_edge_rad = Some(0.9);
        assert_eq!(span_breaks(&wing)[0].sweep_outboard_le_rad, 0.9);
    }
}
