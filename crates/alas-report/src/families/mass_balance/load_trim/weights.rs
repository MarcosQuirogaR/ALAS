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
    /// The registered maximum takeoff mass (the requirement), which can lie
    /// above the heaviest loading the tanks allow.
    RegisteredMtow,
    /// The aircraft's maximum zero-fuel mass (the registered certified value
    /// where the preset has one, else the analyzed zero-fuel mass).
    MaximumZeroFuel,
}

impl MassRole {
    /// Short display label without units.
    pub fn label(self) -> &'static str {
        match self {
            Self::SizedTakeoff => "Sized TOW",
            Self::AnalyzedTakeoff => "Analyzed TOW",
            Self::DesignLanding => "Design LW",
            Self::RegisteredMtow => "MTOW",
            Self::AnalyzedZeroFuel => "Analyzed ZFW",
            Self::MaximumZeroFuel => "MZFW",
        }
    }
}
