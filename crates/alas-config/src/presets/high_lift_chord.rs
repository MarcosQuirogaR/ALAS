// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Chord extension of each preset's deployed trailing-edge flap, carried in the
//! performance block so the neutral-point high-lift condition reads the design
//! it is analysing rather than looking a registered name up.
//!
//! The extension `c'_TE/c - 1` is read from Jenkinson, *Civil Jet Aircraft
//! Design*, Fig. 8.11 (printed p. 169, PDF p. 184, "Typical values of effective
//! chord for trailing-edge flaps (fully extended)"), digitised from the
//! rendered page by pixel position at flap angles 10-45 deg:
//!
//! | flap angle | 10 | 20 | 30 | 40 | 45 |
//! |---|---|---|---|---|---|
//! | tracked (Fowler), 3 slots | 1.207 | 1.258 | 1.297 | 1.324 | 1.337 |
//! | tracked (Fowler), 2 slots | 1.179 | 1.222 | 1.252 | 1.273 | 1.282 |
//! | offset hinge, 2 slots | 1.119 | 1.140 | 1.154 | 1.164 | 1.170 |
//!
//! (reading uncertainty about 0.01). The figure has no single-slot tracked
//! curve; a single-slotted Fowler flap takes the 2-slot tracked curve, its
//! nearest (the result, 1.2625 at 35 deg, lies inside Jenkinson's text range
//! of 1.25-1.30 for Fowler flaps, p. 168). A flap angle outside 10-45 deg is
//! clamped to the nearest point.
//!
//! - Declared systems ([`super::declared_system`]): the curve of the declared
//!   trailing-edge device at its landing flap angle (the declared one where the
//!   reference states it, else 35 deg, the largest setting of the A320 and of
//!   the ATR 72, Jane's 2008-09).
//! - ATR 72-600: "two-segment double-slotted flaps on offset hinges", flaps
//!   0/15/25/35 deg, no slats (Jane's 2008-09 ATR 72 entry, PDF p. 344):
//!   offset-hinge 2-slot curve at 35 deg.
//! - A400M: "two-section fixed-vane flaps on each wing ... No slats" (Jane's
//!   2008-09 A400M entry, PDF p. 340): hinged vane flaps. Slot count and
//!   landing setting are not printed; the offset-hinge 2-slot curve at 35 deg
//!   is taken (that curve varies by 0.02 over 20-45 deg). If the hinge were
//!   the plain offset type (the figure's lowest curve, about 1.10 at 35 deg) the
//!   extension and the high-lift slope rise would be about 0.06 and 0.03 lower;
//!   the A400M flap kinematics are not in the library.
//! - Any other design keeps the class default (tracked 2 slots at 35 deg, slat).
//!
//! The landing deflections and the flap family go with the extension, for the
//! landing pitching moment: the declared landing flap angle (else 35 deg), the
//! declared device's row of the flap-efficiency charts (offset-hinge flaps
//! take the double-slotted row), and the landing slat angle where Jane's
//! 2008-09 states one (A320 18/22/27 deg, PDF p. 315; A330/A340 21-24 deg,
//! PDF p. 323), else 27 deg, the A320's largest setting.
//!
//! Leading-edge devices:
//!
//! - **Slats** extend the chord by `c'_LE/c`; Jenkinson Fig. 8.10 (printed p. 168,
//!   PDF p. 183, "Typical values of effective chord for leading-edge high-lift
//!   devices") rises from 1.045 at 10 % exposed span to 1.11 at 100 % (unweighted
//!   mean over the usual slat run about 1.077; weighting by the larger inboard
//!   chords gives about 1.07, inside the reading uncertainty), the text range
//!   1.05-1.10 (printed pp. 167-168); the single value is
//!   [`super::SLAT_CHORD_RATIO`] (1.075).
//!   Raymer (6th ed. p. 413) states that a slat, which increases wing area,
//!   increases the slope of the lift curve much as a Fowler flap does, so the
//!   area-ratio slope term applies to the slat share, with the added lift at
//!   the quarter chord of the extended section (a geometric construction:
//!   neither a measured nor a sourced slat pitching-moment method exists in
//!   the library, so slatted-wing NP shifts are not validated).
//! - **Krueger flaps** take no chord extension: Jenkinson (printed p. 167, PDF
//!   p. 182): "Krueger flaps increase lift in a similar way to slats but without
//!   increasing the wing chord" (Table 8.2 likewise gives no `c'/c`), and the one measured deployed
//!   configuration, NASA CR-2144 (B-747 landing; the 747 carries only Krueger
//!   leading-edge devices, Rudolph NASA CR-4746 PDF p. 61), shows a slope rise
//!   (1.16-1.18) and a neutral-point shift (0 to -2 points) that the flap
//!   extension alone reproduces. That is evidence for a Krueger term of zero,
//!   not for slats.

use crate::performance_high_lift::{FlapSlotKind, HighLiftChordExtension};

use super::{declared_system, AircraftPreset, TrailingEdgeDevice};

/// Flap angles of the digitised Fig. 8.11 points, degrees.
const FLAP_DEG: [f64; 5] = [10.0, 20.0, 30.0, 40.0, 45.0];
const TRACKED_3: [f64; 5] = [1.207, 1.258, 1.297, 1.324, 1.337];
const TRACKED_2: [f64; 5] = [1.179, 1.222, 1.252, 1.273, 1.282];
const OFFSET_2: [f64; 5] = [1.119, 1.140, 1.154, 1.164, 1.170];
/// Slat chord extension `c'_LE/c - 1`, from the one slat chord ratio of the presets.
pub(crate) const SLAT_EXTENSION: f64 = super::SLAT_CHORD_RATIO - 1.0;
/// Landing flap angle where a reference does not state one, degrees.
pub(crate) const DEFAULT_LANDING_FLAP_DEG: f64 = 35.0;
/// Landing slat angle where a reference does not state one, degrees: the
/// A320's largest setting (Jane's 2008-09, PDF p. 315).
pub(crate) const DEFAULT_LANDING_SLAT_DEG: f64 = 27.0;

/// Landing slat angle of the preset named `name` where Jane's 2008-09 states
/// one, degrees.
fn declared_landing_slat_deg(name: &str) -> f64 {
    match name {
        "A320-200" => 27.0,
        "A340-300" => 24.0,
        _ => DEFAULT_LANDING_SLAT_DEG,
    }
}

/// `c'_TE/c - 1` of a curve of Fig. 8.11 at `flap_deg` (linear between points).
pub(crate) fn flap_extension(curve: &[f64; 5], flap_deg: f64) -> f64 {
    let angle = flap_deg.clamp(FLAP_DEG[0], FLAP_DEG[4]);
    let upper = FLAP_DEG
        .iter()
        .position(|&a| a >= angle)
        .unwrap_or(4)
        .max(1);
    let t = (angle - FLAP_DEG[upper - 1]) / (FLAP_DEG[upper] - FLAP_DEG[upper - 1]);
    curve[upper - 1] + t * (curve[upper] - curve[upper - 1]) - 1.0
}

/// Trailing-edge chord extension `c'/c - 1` at flap angle `flap_deg` of the
/// flap whose landing extension and angle `devices` carries: the Fig. 8.11
/// curve that passes nearest the landing point, scaled through it, so a
/// preset's own curve is read exactly at `flap_deg` and a user-set landing
/// extension keeps its ratio to the curve. The chart gives the extension
/// fully deployed at each flap angle (see the module note).
#[must_use]
pub fn flap_extension_at(devices: &HighLiftChordExtension, flap_deg: f64) -> f64 {
    let landing = devices.flap_landing_deg;
    let curve = [&TRACKED_3, &TRACKED_2, &OFFSET_2]
        .into_iter()
        .min_by(|a, b| {
            let gap = |curve: &[f64; 5]| (flap_extension(curve, landing) - devices.flap).abs();
            gap(a).total_cmp(&gap(b))
        })
        .unwrap_or(&TRACKED_2);
    let at_landing = flap_extension(curve, landing);
    if at_landing > 0.0 {
        devices.flap * flap_extension(curve, flap_deg) / at_landing
    } else {
        flap_extension(curve, flap_deg)
    }
}

/// Class default: tracked 2-slot flap at the default landing angle.
#[cfg(test)]
pub(crate) fn class_default_flap() -> f64 {
    flap_extension(&TRACKED_2, DEFAULT_LANDING_FLAP_DEG)
}

/// Chord extension of the devices of the preset named `name`.
fn extension_for(name: &str) -> HighLiftChordExtension {
    if let Some(system) = declared_system(name) {
        let curve = match system.trailing_edge {
            TrailingEdgeDevice::TripleSlotted => &TRACKED_3,
            TrailingEdgeDevice::FowlerSingleSlotted | TrailingEdgeDevice::DoubleSlotted => {
                &TRACKED_2
            }
        };
        let landing = system
            .takeoff_flap
            .map_or(DEFAULT_LANDING_FLAP_DEG, |flap| flap.landing_deg);
        let slat_span_share = 1.0 - system.krueger_share.clamp(0.0, 1.0);
        return HighLiftChordExtension {
            flap: flap_extension(curve, landing),
            slat: slat_span_share * SLAT_EXTENSION,
            flap_landing_deg: landing,
            flap_kind: match system.trailing_edge {
                TrailingEdgeDevice::TripleSlotted => FlapSlotKind::Triple,
                TrailingEdgeDevice::DoubleSlotted => FlapSlotKind::Double,
                TrailingEdgeDevice::FowlerSingleSlotted => FlapSlotKind::FowlerSingle,
            },
            slat_landing_deg: if slat_span_share > 0.0 {
                declared_landing_slat_deg(name)
            } else {
                0.0
            },
            slat_span_share,
        };
    }
    if matches!(name, "ATR72-600" | "A400M" | "LJ60" | "C295") {
        // Hinged flaps, no leading-edge device (Jane's 2008-09).
        return HighLiftChordExtension {
            flap: flap_extension(&OFFSET_2, DEFAULT_LANDING_FLAP_DEG),
            slat: 0.0,
            flap_landing_deg: DEFAULT_LANDING_FLAP_DEG,
            flap_kind: FlapSlotKind::Double,
            slat_landing_deg: 0.0,
            slat_span_share: 0.0,
        };
    }
    HighLiftChordExtension::CLASS_DEFAULT
}

/// What a configuration states about the leading-edge devices of its wing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlatDeclaration {
    /// Slats on at least part of the leading edge.
    Slats,
    /// No leading-edge device.
    NoSlats,
    /// Krueger flaps only: a leading-edge device, but no slat.
    KruegerOnly,
    /// Nothing is stated: the chord extension is the class default and the
    /// preset declares no high-lift system.
    Undeclared,
}

/// Slat state of a configuration started from the preset named `preset`
/// (blank for none) whose performance block carries `extension`.
///
/// A chord extension that differs from the class default is a statement
/// (presets without a leading-edge device carry a zero slat extension), and so
/// is a declared high-lift system ([`declared_system`]), whose values may
/// coincide with the class default. Otherwise the class default is only a
/// placeholder and the state is [`SlatDeclaration::Undeclared`].
pub fn slat_declaration(preset: &str, extension: &HighLiftChordExtension) -> SlatDeclaration {
    let declared = declared_system(preset);
    if declared.is_none() && *extension == HighLiftChordExtension::CLASS_DEFAULT {
        return SlatDeclaration::Undeclared;
    }
    if extension.slat > 0.0 {
        SlatDeclaration::Slats
    } else if declared.is_some_and(|system| system.krueger_share >= 1.0) {
        SlatDeclaration::KruegerOnly
    } else {
        SlatDeclaration::NoSlats
    }
}

pub(super) fn apply(preset: &mut AircraftPreset) {
    let extension = extension_for(preset.name);
    preset
        .performance
        .get_or_insert_with(Default::default)
        .high_lift_chord_extension = extension;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curves_read_back_at_their_digitised_points_and_interpolate() {
        assert!((flap_extension(&TRACKED_3, 30.0) - 0.297).abs() < 1e-12);
        assert!((flap_extension(&TRACKED_2, 35.0) - 0.2625).abs() < 1e-12);
        assert!((flap_extension(&OFFSET_2, 35.0) - 0.159).abs() < 1e-12);
        // Clamped outside the digitised range.
        assert_eq!(
            flap_extension(&OFFSET_2, 0.0),
            flap_extension(&OFFSET_2, 10.0)
        );
        assert_eq!(
            flap_extension(&OFFSET_2, 60.0),
            flap_extension(&OFFSET_2, 45.0)
        );
    }

    #[test]
    fn the_extension_at_another_angle_follows_the_family_curve() {
        let b747 = extension_for("B747-400");
        assert!((b747.flap_extension_at(30.0) - b747.flap).abs() < 1e-12);
        assert!((b747.flap_extension_at(20.0) - 0.258).abs() < 1e-12);
        let atr = extension_for("ATR72-600");
        assert!((atr.flap_extension_at(20.0) - 0.140).abs() < 1e-12);
        let class = HighLiftChordExtension::CLASS_DEFAULT;
        assert!((class.flap_extension_at(20.0) - 0.222).abs() < 1e-12);
        let user = HighLiftChordExtension {
            flap: 2.0 * class.flap,
            ..class
        };
        // A user-set landing extension is scaled through the curve nearest its
        // landing point: its own value at the landing angle, a smaller one at 20 deg.
        assert!((user.flap_extension_at(class.flap_landing_deg) - user.flap).abs() < 1e-12);
        let user_20 = user.flap_extension_at(20.0);
        assert!(user_20 > 0.222 && user_20 < user.flap, "{user_20}");
        let hinged = HighLiftChordExtension { flap: 0.0, ..class };
        assert_eq!(hinged.flap_extension_at(20.0), 0.0);
    }

    #[test]
    fn the_class_default_is_the_tracked_two_slot_flap_at_the_default_landing_angle() {
        assert!((class_default_flap() - HighLiftChordExtension::CLASS_DEFAULT.flap).abs() < 1e-12);
        assert!((SLAT_EXTENSION - HighLiftChordExtension::CLASS_DEFAULT.slat).abs() < 1e-12);
    }

    #[test]
    fn the_slat_state_is_a_statement_or_undeclared() {
        let state = |name: &str| slat_declaration(name, &extension_for(name));
        assert_eq!(state("A320-200"), SlatDeclaration::Slats);
        // Slats outboard, Kruegers inboard.
        assert_eq!(state("B787-9"), SlatDeclaration::Slats);
        assert_eq!(state("B747-400"), SlatDeclaration::KruegerOnly);
        assert_eq!(state("ATR72-600"), SlatDeclaration::NoSlats);
        assert_eq!(state("LJ60"), SlatDeclaration::NoSlats);
        // The program's own design declares the class-default devices: a statement.
        assert_eq!(state("AVE"), SlatDeclaration::Slats);
        let (ave, class) = (extension_for("AVE"), HighLiftChordExtension::CLASS_DEFAULT);
        assert!((ave.flap - class.flap).abs() < 1e-12 && (ave.slat - class.slat).abs() < 1e-12);
        // No declared system and the class default: nothing is stated.
        assert_eq!(
            slat_declaration("", &HighLiftChordExtension::CLASS_DEFAULT),
            SlatDeclaration::Undeclared
        );
        // A stated extension of a design without a preset is a statement.
        let stated = HighLiftChordExtension {
            slat: 0.0,
            ..HighLiftChordExtension::CLASS_DEFAULT
        };
        assert_eq!(slat_declaration("", &stated), SlatDeclaration::NoSlats);
    }

    #[test]
    fn the_hinged_turboprop_flaps_extend_less_than_the_tracked_jet_flaps() {
        let atr = extension_for("ATR72-600").flap;
        let a400m = extension_for("A400M").flap;
        let a320 = extension_for("A320-200").flap;
        let b747 = extension_for("B747-400").flap;
        assert!((atr - 0.159).abs() < 1e-9 && (a400m - atr).abs() < 1e-12);
        assert!(atr > 0.10 && atr < a320 && a320 < b747);
        // The 747 reads the 3-slot tracked curve at its declared 30 deg landing flap.
        assert!((b747 - 0.297).abs() < 1e-9);
        // Slats on the A320 (Jane's: five-segment slats), none on the all-Krueger 747.
        assert!((extension_for("A320-200").slat - SLAT_EXTENSION).abs() < 1e-12);
        assert_eq!(extension_for("B747-400").slat, 0.0);
        assert_eq!(extension_for("ATR72-600").slat, 0.0);
        assert_eq!(extension_for("A400M").slat, 0.0);
    }
}
