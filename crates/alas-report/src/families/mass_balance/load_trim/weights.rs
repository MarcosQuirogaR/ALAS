// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Semantic roles of the model masses shown on the load-and-trim sheet.

/// Provenance of an analyzed or design mass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MassRole {
    /// Mission-sized takeoff mass.
    SizedTakeoff,
    /// Analyzed loading's takeoff mass.
    AnalyzedTakeoff,
    /// Landing mass used for design.
    DesignLanding,
    /// Analyzed loading's zero-fuel mass.
    AnalyzedZeroFuel,
}

impl MassRole {
    /// Short display label without units.
    pub fn label(self) -> &'static str {
        match self {
            Self::SizedTakeoff => "Sized TOW",
            Self::AnalyzedTakeoff => "Analyzed TOW",
            Self::DesignLanding => "Design LW",
            Self::AnalyzedZeroFuel => "Analyzed ZFW",
        }
    }

    /// Characters to reserve for the label in any shipped language: the
    /// longer of the English label and its Spanish catalog translation, which
    /// for every role is the Spanish one (16 characters for the sized takeoff
    /// mass, 13 for the analyzed takeoff and zero-fuel masses, 12 for the
    /// design landing mass). The sheet is localized after it is drawn, so its
    /// label backplates are sized for the longer text.
    pub(super) fn reserved_chars(self) -> usize {
        match self {
            Self::SizedTakeoff => 16,
            Self::AnalyzedTakeoff | Self::AnalyzedZeroFuel => 13,
            Self::DesignLanding => 12,
        }
    }
}
