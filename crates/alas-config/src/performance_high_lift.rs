// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Chord extension of the deployed high-lift devices, carried in the
//! performance block (see `presets::high_lift_chord` for the preset values).

use serde::{Deserialize, Serialize};

/// Trailing-edge flap family, the row of the flap-efficiency charts of
/// Torenbeek, *Synthesis of Subsonic Airplane Design*, 1982, App. G,
/// Figs. G-5 and G-6, that the landing flap's lift reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlapSlotKind {
    /// Single-slotted Fowler flap (Fig. G-5, Fowler line).
    FowlerSingle,
    /// Double-slotted flap, tracked or on offset hinges (Fig. G-6, double
    /// slotted fixed-vane average line).
    Double,
    /// Triple-slotted flap (Fig. G-6, triple-slotted line).
    Triple,
}

/// Chord extension `c'/c - 1` each deployed high-lift device adds on the wing.
/// The flap value is read from Jenkinson Fig. 8.11 by device type and landing
/// angle and the slat value from Fig. 8.10 (see `presets::high_lift_chord`,
/// which also gives the Krueger evidence: a Krueger flap takes no extension),
/// with the landing deflections and flap family the landing pitching moment
/// reads.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HighLiftChordExtension {
    /// Trailing-edge device, `c'/c - 1`.
    pub flap: f64,
    /// Leading-edge device (slat share only; Krueger adds none), `c'/c - 1`.
    pub slat: f64,
    /// Landing trailing-edge flap deflection, degrees.
    #[serde(default = "default_flap_landing_deg")]
    pub flap_landing_deg: f64,
    /// Trailing-edge flap family.
    #[serde(default = "default_flap_kind")]
    pub flap_kind: FlapSlotKind,
    /// Landing slat deflection, degrees (zero without slats).
    #[serde(default = "default_slat_landing_deg")]
    pub slat_landing_deg: f64,
    /// Share of the leading-edge device span that is slat (the rest Krueger).
    #[serde(default = "default_slat_span_share")]
    pub slat_span_share: f64,
}

fn default_flap_landing_deg() -> f64 {
    HighLiftChordExtension::CLASS_DEFAULT.flap_landing_deg
}

fn default_flap_kind() -> FlapSlotKind {
    HighLiftChordExtension::CLASS_DEFAULT.flap_kind
}

fn default_slat_landing_deg() -> f64 {
    HighLiftChordExtension::CLASS_DEFAULT.slat_landing_deg
}

fn default_slat_span_share() -> f64 {
    HighLiftChordExtension::CLASS_DEFAULT.slat_span_share
}

impl HighLiftChordExtension {
    /// Class default: tracked 2-slot flap at 35 deg landing (Fig. 8.11) and slat
    /// `c'/c` 1.075 (Fig. 8.10), the slat at 27 deg (the A320's largest
    /// setting, Jane's 2008-09), all leading-edge span slatted.
    pub const CLASS_DEFAULT: Self = Self {
        flap: 0.2625,
        slat: 0.075,
        flap_landing_deg: 35.0,
        flap_kind: FlapSlotKind::Double,
        slat_landing_deg: 27.0,
        slat_span_share: 1.0,
    };

    /// The flap's `c'/c - 1` at deflection `flap_deg`, degrees: its Jenkinson
    /// Fig. 8.11 family curve through [`Self::flap`] at
    /// [`Self::flap_landing_deg`] (see `presets::high_lift_chord`), e.g. at a
    /// take-off setting.
    #[must_use]
    pub fn flap_extension_at(&self, flap_deg: f64) -> f64 {
        crate::presets::flap_extension_at(self, flap_deg)
    }
}

impl Default for HighLiftChordExtension {
    fn default() -> Self {
        Self::CLASS_DEFAULT
    }
}

pub(crate) fn is_class_default(extension: &HighLiftChordExtension) -> bool {
    *extension == HighLiftChordExtension::CLASS_DEFAULT
}
