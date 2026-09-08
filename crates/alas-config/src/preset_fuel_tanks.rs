// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The tank arrangement of each registered aircraft.
//!
//! Kept beside the presets rather than inside them so the aircraft registry,
//! which is held at its reviewed size, does not grow a field for every new
//! discipline. [`layout_for`] is the one seam the configuration loader reads.
//!
//! Per-tank usable volumes are the manufacturer's, at the density the source
//! states; the semispan stations are published only for the A320 (rib
//! boundaries in the AMM) and are volume-consistent estimates elsewhere,
//! flagged as such beside each entry. The A220, B787 and 777-class "centre"
//! tanks extend into the inboard wing and carry more than half the fuel;
//! they are declared as centre tanks with their published volume, so the
//! capacity is exact while the centroid is the carry-through box's. Sources
//! and the estimation method are in the 2026-09-05 fuel-tank-layout research
//! note (`.agent/reports/research-2026-09-05-fuel-tank-layouts.md`), which
//! cites EASA.A.064 III.9, EASA.A.110, the Airbus A220 operator WBM Table
//! 3-1, the Boeing 787 ACAP Rev Q, the DC-10 ACAP and EASA.A.084.

use crate::{
    AuxiliaryTankConfig, CenterTankConfig, FuelTankLayoutConfig, TrimTankConfig, WingTankConfig,
};

/// An integral wing cell with a published two-side volume.
fn wing(start: f64, end: f64, burn_priority: i64, published_l: f64) -> WingTankConfig {
    WingTankConfig {
        enabled: true,
        span_start_fraction: start,
        span_end_fraction: end,
        usable_fraction: 0.92,
        burn_priority,
        published_usable_volume_l: Some(published_l),
    }
}

/// A centre tank with a published volume.
fn center(burn_priority: i64, published_l: f64) -> CenterTankConfig {
    CenterTankConfig {
        enabled: true,
        usable_fraction: 0.80,
        burn_priority,
        published_usable_volume_l: Some(published_l),
    }
}

/// A trim tank with a published volume.
fn trim(burn_priority: i64, published_l: f64) -> TrimTankConfig {
    TrimTankConfig {
        enabled: true,
        burn_priority,
        published_usable_volume_l: Some(published_l),
        ..TrimTankConfig::default()
    }
}

fn no_wing_cell() -> WingTankConfig {
    WingTankConfig {
        enabled: false,
        ..WingTankConfig::default()
    }
}

fn no_center() -> CenterTankConfig {
    CenterTankConfig {
        enabled: false,
        ..CenterTankConfig::default()
    }
}

/// The tank arrangement registered for the named preset, if it has one.
///
/// `None` means the preset has not declared its tanks and keeps the default
/// twin arrangement; a registered layout is the aircraft's own.
pub fn layout_for(preset_name: &str) -> Option<FuelTankLayoutConfig> {
    let base = FuelTankLayoutConfig {
        inner_wing: no_wing_cell(),
        mid_wing: no_wing_cell(),
        outer_wing: no_wing_cell(),
        center: no_center(),
        trim: TrimTankConfig::default(),
        auxiliary: AuxiliaryTankConfig::default(),
        calibrate_to_published_capacity: true,
    };
    Some(match preset_name {
        // EASA.A.064 III.9 with MOD 37331: centre 8,250 L, wing 15,959 L
        // (inner cells rib 2-15, outer cells rib 15-22 from the AMM ATA 28
        // tank boundaries); the outer cells gravity-feed the inner ones at
        // 750 kg and are the last fuel burned.
        "A320-200" => FuelTankLayoutConfig {
            inner_wing: wing(0.12, 0.60, 2, 14_199.0),
            outer_wing: wing(0.60, 0.85, 3, 1_760.0),
            center: center(1, 8_250.0),
            ..base
        },
        // Airbus A220 operator WBM Table 3-1: centre 13,968 L, mains
        // 2 x 3,770 L at 0.809 kg/L. The centre tank runs into the inboard
        // wing (an estimated 0.32 of the semispan); the mains are the engine
        // feed tanks and are kept near full while the centre transfers.
        "A220-300" => FuelTankLayoutConfig {
            inner_wing: wing(0.32, 0.85, 2, 7_540.0),
            center: center(1, 13_968.0),
            ..base
        },
        // EASA.A.064 III.9, A340-312 three-tank plus trim: centre 42,420 L,
        // inner 2 x 42,775 L, outer 2 x 3,650 L, trim 6,230 L (141,500 L).
        // The trim tank is filled aft in the climb and emptied forward
        // before landing; the outers are held for bending relief.
        "A340-300" => FuelTankLayoutConfig {
            inner_wing: wing(0.10, 0.70, 3, 85_550.0),
            outer_wing: wing(0.70, 0.85, 4, 7_300.0),
            center: center(2, 42_420.0),
            trim: trim(1, 6_230.0),
            ..base
        },
        // A380-800: no centre tank. Feed 2/3 plus inner (2 x 74,649 L),
        // feed 1/4 plus mid (2 x 63,632 L), outer 2 x 9,524 L and the
        // 23,698 L trim tank, from the Airbus A380 AC fuel table; stations
        // are volume-consistent estimates. Inner feeds first, then mid,
        // trim, and the outers last.
        "A380-800" => FuelTankLayoutConfig {
            inner_wing: wing(0.09, 0.33, 1, 149_298.0),
            mid_wing: wing(0.33, 0.72, 2, 127_264.0),
            outer_wing: wing(0.72, 0.85, 4, 19_048.0),
            trim: trim(3, 23_698.0),
            ..base
        },
        // Boeing 787 ACAP Rev Q / ARFF: centre 22,340 US gal (84,566 L),
        // mains 2 x 5,520 US gal (20,895 L). The centre tank runs into the
        // inboard wing to an estimated 0.30 of the semispan and is emptied
        // first by its override pumps.
        "B787-9" => FuelTankLayoutConfig {
            inner_wing: wing(0.30, 0.85, 2, 41_790.0),
            center: center(1, 84_566.0),
            ..base
        },
        // DC-10-30 ACAP total 36,652 US gal. The per-tank split is an
        // estimate: tanks 1 and 3 about 7,850 US gal each in the wings,
        // tank 2 about 6,060 US gal in the centre wing box, and the centre-
        // section auxiliary tank about 14,890 US gal, transferred to the
        // mains first.
        "DC-10" => FuelTankLayoutConfig {
            inner_wing: wing(0.12, 0.85, 3, 59_430.0),
            center: center(2, 22_940.0),
            auxiliary: AuxiliaryTankConfig {
                enabled: true,
                usable_volume_l: 56_365.0,
                x_position_fraction: 0.47,
                burn_priority: 1,
            },
            ..base
        },
        // ATR 72-600 (EASA.A.084): two integral wing tanks, 5,000 kg usable
        // in total at 0.794 kg/L, each with a 160 kg feeder cell at the
        // inboard end; no centre tank.
        "ATR72-600" => FuelTankLayoutConfig {
            inner_wing: wing(0.11, 0.85, 1, 6_300.0),
            ..base
        },
        // AVE is a 777-9-class notional twin: 52,136 US gal primary
        // capacity split as a centre tank of about 27,600 US gal (104,500 L)
        // extending into the inboard wing and mains of about 12,270 US gal
        // (46,450 L) each, an estimate from the 777-300ER shares.
        "AVE" => FuelTankLayoutConfig {
            inner_wing: wing(0.26, 0.85, 2, 92_900.0),
            center: center(1, 104_500.0),
            ..base
        },
        _ => return None,
    })
}

#[cfg(test)]
// A test asserts on arrangements it registered itself, so a failed expect
// there is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_preset_keeps_the_default_arrangement() {
        assert_eq!(layout_for("Concorde"), None);
    }

    #[test]
    fn every_registered_preset_declares_a_valid_arrangement() {
        for preset in crate::presets::registry() {
            let layout = layout_for(preset.name)
                .unwrap_or_else(|| panic!("{} has no tank arrangement", preset.name));
            assert!(
                layout.validate().is_ok(),
                "{}: {:?}",
                preset.name,
                layout.validate()
            );
        }
    }

    #[test]
    fn published_tank_volumes_sum_to_the_registered_usable_volume_within_two_percent() {
        // Every tank of a registered aircraft carries its published volume,
        // so their sum must reproduce the preset's total usable volume; the
        // 2 percent band covers modification states and rounding between
        // the AC, TCDS and WBM sources named in the module doc.
        for preset in crate::presets::registry() {
            let Some(published_total_l) = preset.reference.usable_fuel_volume_l else {
                continue;
            };
            let layout = layout_for(preset.name).expect("registered arrangement");
            let wing_cells = [&layout.inner_wing, &layout.mid_wing, &layout.outer_wing];
            let mut sum_l = 0.0;
            for cell in wing_cells.into_iter().filter(|cell| cell.enabled) {
                sum_l += cell.published_usable_volume_l.expect("published wing cell");
            }
            if layout.center.enabled {
                sum_l += layout
                    .center
                    .published_usable_volume_l
                    .expect("published centre");
            }
            if layout.trim.enabled {
                sum_l += layout
                    .trim
                    .published_usable_volume_l
                    .expect("published trim");
            }
            if layout.auxiliary.enabled {
                sum_l += layout.auxiliary.usable_volume_l;
            }
            let deviation = (sum_l - published_total_l).abs() / published_total_l;
            assert!(
                deviation < 0.02,
                "{}: tanks sum to {sum_l} L against {published_total_l} L published",
                preset.name
            );
        }
    }

    #[test]
    fn outer_and_trim_tanks_are_burned_after_the_centre_tank() {
        for name in ["A320-200", "A340-300"] {
            let layout = layout_for(name).expect("registered arrangement");
            assert!(layout.outer_wing.burn_priority > layout.center.burn_priority);
            assert!(layout.outer_wing.burn_priority > layout.inner_wing.burn_priority);
        }
        let a340 = layout_for("A340-300").expect("registered arrangement");
        assert!(a340.trim.enabled);
    }
}
