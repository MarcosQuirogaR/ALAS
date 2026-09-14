// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use crate::FlopsStructureConfig;

/// Structure and propulsion overrides for registered aircraft.
///
/// NASA/TM-2017-219627 Vol. I defines the engine term `WENGB` as a user
/// input, the weight of the baseline engine, and falls back to the transport
/// correlation `THRSO / 5.5` (equation 76) only when none is input. The
/// pinned Aviary implementation likewise takes the engine mass from the
/// engine deck. A certified dry weight from the engine type-certificate data
/// sheet is that input when its stated scope is the FLOPS engine term: the
/// basic engine with its accessories, with the nacelle, thrust reverser and
/// installation outside it (those are FLOPS equations 69, 86, 87 and 89).
/// [`certified_dry_engine_mass_kg`] declares it only for the engines whose
/// data sheet states that scope; every other preset keeps the equation 76
/// correlation and records the reason. No legacy fraction is inserted here.
pub(super) fn declared_structure(name: &str) -> FlopsStructureConfig {
    let mut config = FlopsStructureConfig::default();
    // No registered aircraft has a source-backed FLOPS FCOMP coefficient. A
    // literal composite percentage would be a different quantity, so retain
    // the published metallic baseline until a calibrated FLOPS coefficient is
    // obtained. This branch keeps the aircraft distinction visible for future
    // evidence without silently pretending it is known today.
    if matches!(name, "A220-300" | "B787-9") {
        config.composite_utilization = 0.0;
    }
    config.baseline_engine_mass_kg = certified_dry_engine_mass_kg(name);
    config
}

/// The certified dry engine mass declared as FLOPS `WENGB`, kg, where the
/// type-certificate data sheet states a scope that matches the FLOPS engine
/// term; `None` keeps the equation 76 correlation.
///
/// Declared:
///
/// * A320-200, CFM56-5B4/3 SAC: 2,454.8 kg, EASA TCDS E.003 Issue 06
///   (2023-01-09) p.11, "including basic engine, its accessories and
///   optional accessories, as well as engine condition monitoring
///   equipment"; the starter is engine type design (inside); the CFM56-5B
///   thrust reverser is aircraft equipment (only the -5C parts list carries
///   a reverser, p.17 note 9).
/// * A220-300, PW1521G-3: 2,177 kg, EASA TCDS IM.E.090 Issue 10
///   (2025-08-14) p.7, "applies to the basic engine and includes standard
///   equipment"; the thrust reverser "is not engine type design" (p.14
///   note 4).
/// * A380-800, Trent 970-84: 6,246 kg, EASA TCDS E.012 Issue 12
///   (2026-03-16) p.9, "Not including fluids and Nacelle EBU".
///
/// Kept on equation 76:
///
/// * A340-300, CFM56-5C3/F: the 2,644.4 kg dry weight (E.003 p.11) includes
///   an adapter kit with mixer, exhaust plug and thrust reverser, which
///   FLOPS prices separately (equation 86); the split is not published.
/// * B787-9, GEnx-1B74/75/P2: 6,147.1 kg (EASA TCDS for the GEnx, p.8) is
///   the basic engine with accessories, but the same data sheet lists the
///   787 fan reversers under the engine type design without stating whether
///   they are inside the dry weight; the scope is unresolved.
/// * DC-10, CF6-50C: no certified dry weight was retained.
/// * AVE (notional) and ATR72-600 (unsupported propulsion technology).
///
/// Known scope residuals of the declared values, recorded rather than
/// adjusted: the engine starter is inside the data-sheet mass and FLOPS
/// equation 89 prices a starting system separately (order 20 kg per engine
/// of overlap); the exhaust nozzle and plug are outside the data-sheet mass
/// and are not a separate FLOPS term on this branch (order 50 kg per engine
/// of omission). Both are far below the correlation's departure from the
/// certified value.
fn certified_dry_engine_mass_kg(name: &str) -> Option<f64> {
    match name {
        "A320-200" => Some(2_454.8),
        "A220-300" => Some(2_177.0),
        "A380-800" => Some(6_246.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certified_dry_engine_masses_are_declared_only_where_the_scope_is_stated() {
        assert_eq!(
            declared_structure("A320-200").baseline_engine_mass_kg,
            Some(2_454.8)
        );
        assert_eq!(
            declared_structure("A220-300").baseline_engine_mass_kg,
            Some(2_177.0)
        );
        assert_eq!(
            declared_structure("A380-800").baseline_engine_mass_kg,
            Some(6_246.0)
        );
        for name in ["A340-300", "B787-9", "DC-10", "AVE", "ATR72-600"] {
            assert_eq!(
                declared_structure(name).baseline_engine_mass_kg,
                None,
                "{name} keeps the equation 76 correlation"
            );
        }
        // A declared engine mass without a separate inlet or nozzle is the
        // equation 80 branch and validates as such.
        assert!(declared_structure("A320-200").validate().is_ok());
    }
}
