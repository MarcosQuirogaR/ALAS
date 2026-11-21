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

use alas_geom::aircraft::wing::{Wing, WingReference};

use super::model::CpacsMainWingRecord;

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

/// How close, in metres, degrees and square metres, a document must be to an ALAS
/// toolspecific record for the record to describe that wing.
const RECORD_FIT_TOLERANCE: f64 = 1.0e-6;

/// How a document compares with the record it carries.
#[derive(Debug, Clone, PartialEq)]
enum RecordFit {
    /// Every section and the document reference are the recorded ones, the sections
    /// after this rigid translation of the wing, metres.
    Fits([f64; 3]),
    /// The record describes another wing, for this reason.
    Mismatch(String),
}

fn record_fit(
    wing: &Wing,
    record: &CpacsMainWingRecord,
    document_reference: Option<(f64, f64)>,
) -> RecordFit {
    let close = |a: f64, b: f64| (a - b).abs() <= RECORD_FIT_TOLERANCE;
    match (record.document_reference, document_reference) {
        (Some(recorded), Some(declared))
            if close(recorded.0, declared.0) && close(recorded.1, declared.1) => {}
        (recorded, declared) => {
            return RecordFit::Mismatch(format!(
                "the document's reference (area m^2, length m) {declared:?} is not the \
                 recorded {recorded:?}"
            ))
        }
    }
    let Some(first) = record.sections.first() else {
        return RecordFit::Mismatch(
            "the record carries no section list to check the document against".to_owned(),
        );
    };
    if record.sections.len() != wing.xsecs.len() {
        return RecordFit::Mismatch(format!(
            "the document has {} wing sections and the record {}",
            wing.xsecs.len(),
            record.sections.len()
        ));
    }
    let origin = wing.xsecs[0].xyz_le;
    let delta = [
        origin[0] - first.le_m[0],
        origin[1] - first.le_m[1],
        origin[2] - first.le_m[2],
    ];
    for (index, (xsec, recorded)) in wing.xsecs.iter().zip(&record.sections).enumerate() {
        let on_translation =
            (0..3).all(|axis| close(xsec.xyz_le[axis], recorded.le_m[axis] + delta[axis]));
        if !(on_translation
            && close(xsec.chord, recorded.chord_m)
            && close(xsec.twist, recorded.twist_deg))
        {
            return RecordFit::Mismatch(format!(
                "wing section {index} of the document (LE {:?} m, chord {} m, twist {} deg) \
                 is not the recorded one (LE {:?} m, chord {} m, twist {} deg) after a rigid \
                 translation of {delta:?} m",
                xsec.xyz_le,
                xsec.chord,
                xsec.twist,
                recorded.le_m,
                recorded.chord_m,
                recorded.twist_deg
            ));
        }
    }
    RecordFit::Fits(delta)
}

/// Put an ALAS export's reference wing and design planform back on its main wing,
/// so that the import reads the masses, tanks, sweep and `%MAC` the export was
/// made with. A document whose wing is the recorded one after a rigid translation
/// keeps the record, translated with it. Any other difference from the recorded
/// sections or document reference means the document was edited after the export and
/// the record describes another wing: it is dropped whole and the reason returned, and
/// the wing is read as a document with no record is. `true` with the record on the wing.
pub(super) fn restore_main_wing_record(
    wing: &mut Wing,
    record: &CpacsMainWingRecord,
    document_reference: Option<(f64, f64)>,
    warnings: &mut Vec<String>,
) -> bool {
    match record_fit(wing, record, document_reference) {
        RecordFit::Fits(delta) => {
            wing.reference = record
                .reference
                .map(|reference| reference.translate(delta[0]));
            wing.planform_datum = record.datum.map(|datum| datum.translate(delta));
            wing.tip_te_flare_m = record.tip_te_flare_m;
            true
        }
        RecordFit::Mismatch(reason) => {
            warnings.push(format!(
                "the ALAS main-wing record does not fit this document and was ignored, so the \
                 sections are read as the planform: {reason}"
            ));
            false
        }
    }
}

/// Make a document's declared reference the one area every reader of the wing sees:
/// FLOPS states its wing terms on the reference area, as the native path gives it
/// them, so `Wing::reference_area` must return what the coefficients are quoted on.
/// The `%MAC` datum stays where the sections put it.
pub(super) fn apply_declared_reference(wing: &mut Wing, area_m2: f64, span_m: f64, mac_m: f64) {
    if (wing.reference_area() - area_m2).abs() <= RECORD_FIT_TOLERANCE {
        return;
    }
    wing.reference = Some(WingReference {
        area_m2,
        span_m,
        mac_m,
        mac_le_x_m: wing.reference_mac_station_le_x(),
    });
}

/// The MAC a wing's reference chord is compared with: the reference wing's, the
/// design planform's, or its sections' when it has neither.
pub(super) fn quoted_mac_m(wing: &Wing) -> f64 {
    wing.reference
        .map(|reference| reference.mac_m)
        .unwrap_or_else(|| {
            wing.planform_datum
                .map_or_else(|| wing.mac_station().chord_m, |datum| datum.gross_mac_m)
        })
}

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
    use super::super::model::CpacsRecordedSection;
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::WingXSec;

    /// The record an export of `wing` writes, with a reference wing and no datum.
    fn record_of(wing: &Wing) -> CpacsMainWingRecord {
        CpacsMainWingRecord {
            reference: Some(WingReference {
                area_m2: 100.0,
                span_m: 40.0,
                mac_m: 3.0,
                mac_le_x_m: 2.0,
            }),
            datum: None,
            sections: wing
                .xsecs
                .iter()
                .map(|xsec| CpacsRecordedSection {
                    le_m: xsec.xyz_le,
                    chord_m: xsec.chord,
                    twist_deg: xsec.twist,
                })
                .collect(),
            document_reference: Some((100.0, 3.0)),
            tip_te_flare_m: wing.tip_te_flare_m,
        }
    }

    #[test]
    fn the_record_is_kept_on_its_own_wing_and_translated_with_a_moved_one() {
        let mut same = wing("Wing", 4.0, 20.0, true);
        let record = record_of(&same);
        let mut warnings = Vec::new();
        assert!(restore_main_wing_record(
            &mut same,
            &record,
            Some((100.0, 3.0)),
            &mut warnings
        ));
        assert_eq!(same.reference, record.reference);

        let mut moved = wing("Wing", 4.0, 20.0, true);
        for xsec in &mut moved.xsecs {
            xsec.xyz_le[0] += 1.5;
            xsec.xyz_le[2] -= 0.25;
        }
        assert!(restore_main_wing_record(
            &mut moved,
            &record,
            Some((100.0, 3.0)),
            &mut warnings
        ));
        let reference = moved.reference.expect("kept");
        assert!((reference.mac_le_x_m - 3.5).abs() < 1e-12);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn any_other_difference_drops_the_record_with_a_warning() {
        let original = wing("Wing", 4.0, 20.0, true);
        let record = record_of(&original);
        let mut tip = original.clone();
        tip.xsecs[1].chord *= 1.3;
        let mut twist = original.clone();
        twist.xsecs[1].twist += 1.0;
        let mut count = original.clone();
        count.xsecs.push(count.xsecs[1].clone());
        let unlisted = CpacsMainWingRecord {
            sections: Vec::new(),
            ..record.clone()
        };
        for (mut edited, record, declared) in [
            (tip, &record, Some((100.0, 3.0))),
            (twist, &record, Some((100.0, 3.0))),
            (count, &record, Some((100.0, 3.0))),
            (original.clone(), &record, Some((600.0, 3.0))),
            (original.clone(), &record, None),
            (original.clone(), &unlisted, Some((100.0, 3.0))),
        ] {
            let mut warnings = Vec::new();
            assert!(!restore_main_wing_record(
                &mut edited,
                record,
                declared,
                &mut warnings
            ));
            assert_eq!(edited.reference, None);
            assert_eq!(warnings.len(), 1, "{warnings:?}");
        }
    }

    #[test]
    fn a_declared_reference_area_is_the_area_the_wing_reads() {
        let mut declared = wing("Wing", 4.0, 20.0, true);
        let datum_x = declared.reference_mac_station_le_x();
        apply_declared_reference(&mut declared, 100.0, 40.0, 3.0);
        assert!((declared.reference_area() - 100.0).abs() < 1e-12);
        assert!((declared.reference_mac_station_le_x() - datum_x).abs() < 1e-12);

        let mut same = wing("Wing", 4.0, 20.0, true);
        let area = same.projected_area();
        apply_declared_reference(&mut same, area, 40.0, 3.0);
        assert_eq!(same.reference, None);
    }

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
}
