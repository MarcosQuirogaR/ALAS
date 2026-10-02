// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing masses one run produced, in one record shared by the findings
//! panel and the structures sizing figure.

/// Which model produced the primary-structure mass of a [`WingMassComparison`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryMassSource {
    /// Shell and bar material written to the finite-element (Nastran) deck.
    FiniteElement,
    /// The native beam sizing of the wingbox, used when no FE mass exists.
    NativeBeam,
}

/// Wing masses of the complete symmetric wing (both semi-wings), kg.
///
/// The primary-structure masses (native beam, FE deck) cover the wingbox only;
/// the FLOPS estimate covers the complete wing group (primary box plus leading
/// and trailing edges, movables and fittings). FLOPS exposes no primary-box
/// share (`W1` bending material is only one term of the box, and `W2` mixes
/// shear material with control surfaces), so the two scopes are never
/// differenced into a single error figure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingMassComparison {
    /// Native beam primary-structure mass, kg.
    pub native_primary_kg: f64,
    /// FE deck primary-structure mass, kg, when the mesh was built.
    pub fe_primary_kg: Option<f64>,
    /// FLOPS complete-wing estimate from the mass ledger, kg.
    pub flops_complete_wing_kg: f64,
}

impl WingMassComparison {
    /// Record the three masses of one run; none is altered.
    pub fn new(
        native_primary_kg: f64,
        fe_primary_kg: Option<f64>,
        flops_complete_wing_kg: f64,
    ) -> Self {
        Self {
            native_primary_kg,
            fe_primary_kg,
            flops_complete_wing_kg,
        }
    }

    /// The primary-structure mass to present: the FE mass when it is a
    /// positive finite number, otherwise the native beam mass.
    pub fn primary(&self) -> (PrimaryMassSource, f64) {
        match self.fe_primary_kg.filter(|m| m.is_finite() && *m > 0.0) {
            Some(fe) => (PrimaryMassSource::FiniteElement, fe),
            None => (PrimaryMassSource::NativeBeam, self.native_primary_kg),
        }
    }

    /// Every primary-structure inventory with its label, for comparison with
    /// the FLOPS complete wing.
    pub fn primary_inventories(&self) -> [(&'static str, Option<f64>); 2] {
        [
            ("native primary structure", Some(self.native_primary_kg)),
            ("FE primary material", self.fe_primary_kg),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fe_mass_is_preferred_and_beam_is_the_fallback() {
        let with_fe = WingMassComparison::new(20_000.0, Some(30_000.0), 44_000.0);
        assert_eq!(
            with_fe.primary(),
            (PrimaryMassSource::FiniteElement, 30_000.0)
        );
        for absent in [None, Some(f64::NAN), Some(0.0)] {
            let without = WingMassComparison::new(20_000.0, absent, 44_000.0);
            assert_eq!(without.primary(), (PrimaryMassSource::NativeBeam, 20_000.0));
        }
    }
}
