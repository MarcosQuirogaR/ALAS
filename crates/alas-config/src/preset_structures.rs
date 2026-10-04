// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The structural configuration of each registered aircraft.
//!
//! Kept beside the presets rather than inside them so the aircraft registry,
//! which is held at its reviewed size, does not grow a field for every new
//! discipline. [`config_for`] is the one seam the configuration loader reads.
//!
//! Material family and the declared longitudinal cap proxy travel here.
//! Spar stations and structural gauges remain with the study/defaults, because
//! no manufacturer document on file establishes them and the spars bound the
//! fuel tank box (which is already calibrated against
//! published usable volumes in [`crate::preset_fuel_tanks`]).

use crate::StructuresConfig;

/// Declared transport cap assumption, not an aircraft-specific ply schedule.
/// The source accompanies both composite presets without changing their
/// manufacturer material-family evidence tier.
pub const TRANSPORT_CAP_SOURCE: &str =
    "Declared symmetric balanced 60/30/10 (0/+45/-45/90=60/15/15/10%) cap laminate; \
         AS4/3501-6 lamina E1=135 GPa, E2=11 GPa, G12=5.8 GPa, nu12=0.301: \
         NASA-TM-104055 (1991), Table 1; NASA-RP-1351 (1994), Sec. V-B, CLT. \
         CLT Ex=A11-A12^2/A22=91.595391044 GPa (A normalized by thickness). \
         Ultimate design strain 0.004 is a declared damage-tolerance assumption: \
         Niu, Composite Airframe Structures (1992), design allowables; \
         CMH-17-3G (2012), damage tolerance. No manufacturer ply schedule is claimed.";

/// Source of the declared cap material for each composite transport preset.
pub fn cap_material_source(preset_name: &str) -> Option<&'static str> {
    matches!(preset_name, "B787-9" | "A220-300").then_some(TRANSPORT_CAP_SOURCE)
}

/// The structural configuration registered for the named preset, if it has one.
///
/// `None` means the preset has not declared a specialized structural
/// configuration and keeps [`StructuresConfig::default`].
pub fn config_for(preset_name: &str) -> Option<StructuresConfig> {
    match preset_name {
        // Material family verified locally: Airbus A320 AC Jul 01/26, FIGURE-10-0-0-991-033-A01, 10-0-0 Page 7.
        // CFRP confined to empennage, movables, nacelles, LE/TE devices and fairings; wing box uncoloured (metallic).
        // Alloy, temper and gauge are NOT sourced; the alloy below is the database's metallic default, not a source claim.
        "A320-200" => Some(StructuresConfig {
            spar_cap_material: "Al 7075-T6".to_owned(),
            ..StructuresConfig::default()
        }),

        // Material family verified locally: Airbus A340-200/-300 AC Dec 01/25, FIGURE-10-0-0-991-031-A01, 10-0-0 Page 8.
        // Same pattern; wing box uncoloured (metallic).
        // Alloy, temper and gauge are NOT sourced; the alloy below is the database's metallic default, not a source claim.
        "A340-300" => Some(StructuresConfig {
            spar_cap_material: "Al 7075-T6".to_owned(),
            ..StructuresConfig::default()
        }),

        // Material family verified locally: Airbus A380 AC Dec 01/25, FIGURE-10-0-0-991-003-A01, 10-0-0 Page 8.
        // Composite centre-wing-box region, metallic outer wing box, GLARE upper fuselage.
        // Only the OUTER wing box is modelled here.
        // Alloy, temper and gauge are NOT sourced; the alloy below is the database's metallic default, not a source claim.
        "A380-800" => Some(StructuresConfig {
            spar_cap_material: "Al 7075-T6".to_owned(),
            ..StructuresConfig::default()
        }),

        // Declared class assumption: no local primary source states this aircraft's wing box material.
        // 1970 design predating structural CFRP; the DC/MD-10 ACAP contains no composite reference.
        // No source states the alloy. Not a verified assignment.
        "DC-10" => Some(StructuresConfig {
            spar_cap_material: "Al 7075-T6".to_owned(),
            ..StructuresConfig::default()
        }),

        // Declared class assumption: no local primary source states this aircraft's wing box material.
        // The real outer wing box is understood to be composite, but neither local factsheet (2020, 2022)
        // contains the word composite or carbon. Metallic is the conservative declared choice pending a
        // real source. Not a verified assignment.
        "ATR72-600" => Some(StructuresConfig {
            spar_cap_material: "Al 7075-T6".to_owned(),
            ..StructuresConfig::default()
        }),

        // Composite family: Boeing 787 ARFF composite-content diagram (2013,
        // current 2025 file), p. 3. Aircraft ply schedule remains unavailable.
        // Assigned as an effective isotropic proxy, not a verified material.
        // Caps are 0-dominated, covers and webs QI. CLT modulus and the 0.004
        // strain assumption are declared in cap_material_source(), not fitted
        // to the empirical wing weight. No separate stringers are declared.
        "B787-9" => Some(StructuresConfig {
            skin_material: "CFRP QI".to_owned(),
            spar_web_material: "CFRP QI".to_owned(),
            spar_cap_material: "CFRP 60/30/10".to_owned(),
            ..StructuresConfig::default()
        }),

        // Composite family: Airbus FAST 63 (2019), Flying the A220, and
        // Airbus A220 airframe features (July 2025). Ply schedule unavailable.
        // Assigned as an effective isotropic proxy, not a verified material.
        // Same declared 0-dominated transport cap as B787; see
        // cap_material_source(). Webs/covers retain the QI proxy.
        "A220-300" => Some(StructuresConfig {
            skin_material: "CFRP QI".to_owned(),
            spar_web_material: "CFRP QI".to_owned(),
            spar_cap_material: "CFRP 60/30/10".to_owned(),
            ..StructuresConfig::default()
        }),

        // AVE has no entry: return None so it takes StructuresConfig::default()
        "AVE" => None,

        _ => None,
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ave_returns_none_so_it_takes_the_default() {
        assert_eq!(config_for("AVE"), None);
    }

    #[test]
    fn unknown_preset_returns_none() {
        assert_eq!(config_for("NonexistentAirplane"), None);
    }

    #[test]
    fn metallic_presets_replace_composite_spar_cap_with_al_7075() {
        for name in &["A320-200", "A340-300", "A380-800", "DC-10", "ATR72-600"] {
            let cfg = config_for(name).unwrap_or_else(|| panic!("{name} should be registered"));
            assert_eq!(cfg.spar_cap_material, "Al 7075-T6");
            assert_eq!(cfg.skin_material, "Al 7075-T6");
            assert_eq!(cfg.spar_web_material, "Al 7075-T6");
            assert_eq!(cfg.rib_material, "Al 7075-T6");
            assert_eq!(cfg.t_skin_min_m, StructuresConfig::default().t_skin_min_m);
            assert_eq!(
                cfg.spar_chord_fractions,
                StructuresConfig::default().spar_chord_fractions
            );
            assert_eq!(
                cfg.center_spar_enabled,
                StructuresConfig::default().center_spar_enabled
            );
        }
    }

    #[test]
    fn composite_presets_assign_strain_limited_longitudinal_caps_and_qi_panels() {
        for name in &["B787-9", "A220-300"] {
            let cfg = config_for(name).unwrap_or_else(|| panic!("{name} should be registered"));
            assert_eq!(cfg.skin_material, "CFRP QI");
            assert_eq!(cfg.spar_web_material, "CFRP QI");
            assert_eq!(cfg.spar_cap_material, "CFRP 60/30/10");
            assert!(cap_material_source(name)
                .unwrap()
                .contains("NASA-TM-104055"));
            assert_eq!(cfg.rib_material, "Al 7075-T6");
            assert_eq!(cfg.t_skin_min_m, StructuresConfig::default().t_skin_min_m);
            assert_eq!(
                cfg.spar_chord_fractions,
                StructuresConfig::default().spar_chord_fractions
            );
            assert_eq!(
                cfg.center_spar_enabled,
                StructuresConfig::default().center_spar_enabled
            );
        }
    }
}
