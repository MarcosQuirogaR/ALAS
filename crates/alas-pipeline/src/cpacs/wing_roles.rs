// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Resolves which imported CPACS wing is the aircraft's main lifting
//! surface, and which (if any) are its horizontal and vertical
//! stabilizers, for [`super::aircraft::CpacsDocument::to_airplane`]
//! and the reference-chord/LEMAC decoupling.
//!
//! CPACS carries no explicit "this is the main wing" tag reachable from
//! this crate's reader, so the rule mirrors the one this module already
//! used for `s_ref`/`c_ref`: the wing with the greatest *projected*
//! planform area is the main wing. Every downstream consumer that reads
//! `airplane.wings[0]` or falls back to it by name (`alas-mass`,
//! `alas-stab`, `alas-report`, ...) needs that wing at index 0, so
//! [`resolve_main_wing`] reorders `wings` in place rather than only
//! picking a reference.
//!
//! Naming follows the same convention the native builder uses
//! (`alas-geom::builder`'s `"Main Wing"`/`"Horizontal Stabilizer"`/
//! `"Vertical Stabilizer"`): among the remaining wings, a symmetric one
//! (mirrored about the aircraft XZ plane, like every product horizontal
//! tail) is renamed the horizontal stabilizer and an asymmetric one (like
//! every product vertical fin) is renamed the vertical stabilizer, but
//! only when exactly one candidate of that kind exists. A document with
//! more than one symmetric or asymmetric non-main wing (canards, twin
//! fins, winglets modeled as separate wings) is outside this heuristic's
//! scope: those wings keep their CPACS names rather than risk mislabeling
//! one of several candidates.

use alas_geom::aircraft::wing::Wing;

/// The name product geometry uses for the main lifting surface.
const MAIN_WING_NAME: &str = "Main Wing";
/// The name product geometry uses for a symmetric tail.
const HSTAB_NAME: &str = "Horizontal Stabilizer";
/// The name product geometry uses for an asymmetric tail.
const VSTAB_NAME: &str = "Vertical Stabilizer";
/// Relative difference, in the resolved main wing's own MAC, above which
/// [`warn_on_reference_chord_mismatch`] flags a CPACS document's declared
/// reference chord: half a percent.
const REFERENCE_CHORD_WARNING_FRACTION: f64 = 0.005;

/// Move the largest-projected-area wing to `wings[0]` and rename it, and
/// where unambiguous, its tails, to the product convention. See the module
/// doc for the resolution rule and its documented scope.
pub(super) fn resolve_main_wing(wings: &mut [Wing]) {
    let Some(main_index) = wings
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.projected_area().total_cmp(&b.projected_area()))
        .map(|(index, _)| index)
    else {
        return;
    };
    wings.swap(0, main_index);
    wings[0].name = MAIN_WING_NAME.to_owned();

    let tails = &mut wings[1..];
    let symmetric_count = tails.iter().filter(|wing| wing.symmetric).count();
    let asymmetric_count = tails.len() - symmetric_count;
    for wing in tails {
        if wing.symmetric && symmetric_count == 1 {
            wing.name = HSTAB_NAME.to_owned();
        } else if !wing.symmetric && asymmetric_count == 1 {
            wing.name = VSTAB_NAME.to_owned();
        }
    }
}

/// Warn when a CPACS document's own declared reference chord
/// differs from the resolved main wing's physical MAC by more than
/// [`REFERENCE_CHORD_WARNING_FRACTION`].
///
/// Both quantities remain in use: `c_ref` keeps the file's declared
/// reference chord (the manufacturer's own `%MAC` convention, which import
/// must not silently override), while every `%MAC` frame computed from
/// [`alas_geom::aircraft::airplane::Airplane::mac_frame`] resolves
/// `x_LEMAC` from the wing's own geometry, so the two values never get
/// algebraically mixed the way the retired `AC(0.25) - 0.25 * c_ref`
/// reconstruction did.
pub(super) fn warn_on_reference_chord_mismatch(aircraft_name: &str, c_ref_m: f64, wing_mac_m: f64) {
    if wing_mac_m <= 0.0 {
        return;
    }
    let relative_difference = (c_ref_m - wing_mac_m).abs() / wing_mac_m;
    if relative_difference > REFERENCE_CHORD_WARNING_FRACTION {
        tracing::warn!(
            aircraft_name,
            c_ref_m,
            wing_mac_m,
            relative_difference,
            "CPACS reference chord differs from the main wing's own MAC by more than 0.5%; \
             %MAC figures use c_ref while x_LEMAC uses the wing's own geometry (see Airplane::mac_frame)"
        );
    }
}

#[cfg(test)]
// Test fixtures intentionally use expect/unwrap so a broken fixture panics
// at the assertion site instead of being converted into a silent fallback.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::WingXSec;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn wing(name: &str, root_chord: f64, span: f64, symmetric: bool) -> Wing {
        Wing::new(
            name,
            vec![
                WingXSec::new([0.0, 0.0, 0.0], root_chord, 0.0, naca("naca0012")),
                WingXSec::new([1.0, span, 0.0], root_chord * 0.4, 0.0, naca("naca0012")),
            ],
            symmetric,
        )
    }

    #[test]
    fn a_tailplane_listed_first_is_moved_to_index_zero_and_renamed() {
        let mut wings = vec![
            wing("Tailplane", 2.0, 5.0, true),
            wing("Wing", 4.0, 20.0, true),
            wing("Fin", 3.0, 4.0, false),
        ];
        resolve_main_wing(&mut wings);
        assert_eq!(wings[0].name, MAIN_WING_NAME);
        assert!((wings[0].xsecs[0].chord - 4.0).abs() < 1e-12);
        let hstab = wings
            .iter()
            .find(|w| w.name == HSTAB_NAME)
            .expect("exactly one symmetric tail resolves");
        assert!((hstab.xsecs[0].chord - 2.0).abs() < 1e-12);
        let vstab = wings
            .iter()
            .find(|w| w.name == VSTAB_NAME)
            .expect("exactly one asymmetric tail resolves");
        assert!((vstab.xsecs[0].chord - 3.0).abs() < 1e-12);
    }

    #[test]
    fn an_already_first_main_wing_keeps_its_position() {
        let mut wings = vec![wing("Wing", 4.0, 20.0, true), wing("Tail", 1.0, 5.0, true)];
        resolve_main_wing(&mut wings);
        assert!((wings[0].xsecs[0].chord - 4.0).abs() < 1e-12);
        assert_eq!(wings[0].name, MAIN_WING_NAME);
    }

    #[test]
    fn ambiguous_symmetric_tails_keep_their_original_names() {
        let mut wings = vec![
            wing("Wing", 4.0, 20.0, true),
            wing("Canard", 1.0, 5.0, true),
            wing("Tailplane", 1.2, 6.0, true),
        ];
        resolve_main_wing(&mut wings);
        assert_eq!(wings[0].name, MAIN_WING_NAME);
        assert_eq!(wings[1].name, "Canard");
        assert_eq!(wings[2].name, "Tailplane");
    }

    #[test]
    fn a_reference_chord_within_tolerance_of_the_wing_mac_does_not_warn() {
        // Exercised for coverage of the non-warning branch; `tracing`
        // has no in-crate assertion surface here, so this only proves the
        // guard does not panic or divide by zero.
        warn_on_reference_chord_mismatch("Probe", 4.19, 4.193423);
        warn_on_reference_chord_mismatch("Probe", 7.5, 6.27126);
        warn_on_reference_chord_mismatch("Probe", 1.0, 0.0);
    }
}
