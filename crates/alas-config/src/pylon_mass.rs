// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The engine-pylon mass method of the FLOPS propulsion buildup.

use serde::{Deserialize, Serialize};

use crate::{Kind, Leaf};

/// Which method prices the engine pylons of a podded installation.
///
/// **FLOPS has no pylon term at all.** Every propulsion mass in
/// NASA/TM-2017-219627 Vol. I sections 5.2.9 and 5.3 is an engine, a nacelle,
/// a reverser, a control, a starter or a fuel system; searching the published
/// equation set for a strut or pylon returns nothing, and equation 137 sums
/// only those groups. The structure that carries a podded engine to the wing
/// is therefore outside the published empty-weight boundary, not estimated at
/// zero by it.
///
/// That gap is not small. Inverting the published computed masses and
/// deviations of Fernandes da Moura (2001) against three independent methods
/// gives an actual pylon mass of **469 kg per pylon on the A320-200** and
/// **724 kg per pylon on the A340-300**, i.e. **2.27 % and 2.23 % of operating
/// empty weight**: the whole of ALAS's A320 deficit and about a sixth of the
/// A340's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PylonMassMethod {
    /// Charge no pylon, which is the published FLOPS boundary exactly.
    ///
    /// This is the auditable baseline: it reproduces the transport equation
    /// set as printed, and it is also the physically right answer for an
    /// installation with no pylon at all, such as a wing-faired turboprop
    /// nacelle.
    #[default]
    None,
    /// The LTH box-beam pylon relation, `m = n x 0.2648 x SLST^0.6517` with
    /// the sea-level static thrust of one engine in newtons and the mass in
    /// kilograms.
    ///
    /// Source: Luftfahrttechnisches Handbuch, Masseanalyse MA 401 12-01 B
    /// (Dorbath, 2013), whose stated validity is *"grosse zivile
    /// Verkehrsflugzeuge (MTOM > 40 t)"* and *"bezieht sich ausschliesslich
    /// auf zivile Verkehrsflugzeuge"*. Against the two pylon masses derived
    /// above it returns 515 kg (+9.8 %) and 625 kg (-13.7 %) per pylon.
    ///
    /// It prices the wing pylons of a podded installation and nothing else: a
    /// tail-mounted centre engine is carried by fuselage and fin structure
    /// that this relation was not fitted on, so it is not charged one.
    LthBoxBeamV1,
}

impl PylonMassMethod {
    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::LthBoxBeamV1 => "lth_box_beam_v1",
        }
    }
}

impl Leaf for PylonMassMethod {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}
