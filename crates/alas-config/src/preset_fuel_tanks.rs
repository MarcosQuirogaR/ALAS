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
//! and the estimation method cite EASA.A.064 III.9, EASA.A.110, the Airbus A220
//! operator WBM Table 3-1, the Boeing 787 ACAP Rev Q, the DC-10 ACAP and EASA.A.084.

use crate::{
    AuxiliaryTankConfig, CenterTankConfig, FeedTankConfig, FuelTankLayoutConfig, TrimTankConfig,
    WingTankConfig,
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
        feed: FeedTankConfig::default(),
    }
}

/// `host` with an engine feed tank of `published_l` (both sides) over
/// `[start, end]`, one end shared with the host cell.
fn with_feed(host: WingTankConfig, start: f64, end: f64, published_l: f64) -> WingTankConfig {
    WingTankConfig {
        feed: FeedTankConfig {
            enabled: true,
            span_start_fraction: start,
            span_end_fraction: end,
            published_usable_volume_l: Some(published_l),
        },
        ..host
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
        // A380-800: no centre tank. Certified per-tank usable volumes from
        // EASA TCDS EASA.A.110 Issue 17, 2026-08-05, section 3.3 "Fluid
        // Capacities", p.14 of 20, lumped onto the cells this layout
        // declares (litres, at the sheet's own 0.800 kg/L convention):
        //   inner_wing = Feed 2 + Feed 3 + Inner L + Inner R
        //              = 29,349 + 29,349 + 46,142 + 46,142 = 150,982 L
        //   mid_wing   = Feed 1 + Feed 4 + Mid L + Mid R
        //              = 27,632 + 27,632 + 36,461 + 36,461 = 128,186 L
        //   outer_wing = Outer L + Outer R = 10,340 + 10,340 = 20,680 L
        //   trim       = Trim                                = 23,698 L
        // Tank total 323,546 L, 258,836.8 kg at 0.800 kg/L. These supersede
        // the secondary Airbus AC-derived cells this entry carried
        // (149,298 / 127,264 / 19,048 L, 319,308 L), which were 4,238 L
        // (1.33 %) short of the certified tanks; the trim tank was already
        // certified-exact.
        //
        // The preset declares 324,339 L (`presets::widebody::a380_800`),
        // which is the same table's *aeroplane* total: the extra 793 L is its
        // "Systems" row, usable fuel held in lines and engines rather than in
        // a tank, so it has no tank station and is deliberately not modelled
        // here. The declared total is kept as the certified aeroplane figure
        // and the residual between it and these cells is exactly that
        // 793 L (0.245 %), where before it was an unexplained 5,031 L.
        // `calibrate_to_published_capacity` cannot close it in either case:
        // every A380 cell carries its own published volume, so the
        // calibration has no geometric cell to absorb the difference and is
        // the identity.
        //
        // The same table gives unusable fuel 1,086 L (869 kg at 0.800 kg/L),
        // 0.00335 of the usable total, against the generic 0.007 in
        // `FuelPolicyConfig::unusable_fuel_fraction`. That fraction is a
        // global study default with no per-aircraft seam, and on the default
        // pure-FLOPS architecture the operating-items unusable fuel comes
        // from FLOPS equation 121 rather than from it, so the certified
        // figure is recorded and not wired here.
        //
        // The four feed tanks are separate tanks inside those cells, so they
        // fill first and burn last (`alas_mass::tanks::order`). Their
        // spanwise order is the one the TCDS table lists tip to tip and the
        // Airbus A380 AC Dec 01/25, FIGURE-10-0-0-991-002-A01 (hazardous
        // materials) draws: from the root, Feed 2/3, Inner, Mid, Feed 1/4,
        // Outer. The figure is an oblique view, so each feed boundary is
        // placed where the model's own spar box splits its cell in the
        // certified volume ratio (Feed 2 + 3 = 58,698 L of the inner cell,
        // Feed 1 + 4 = 55,264 L of the mid cell); the figure reads about
        // 0.22 and 0.47 of the semispan against the 0.16 and 0.50 used.
        // Feed 2/3 also sit in the aft part of the root bays, which a
        // full-chord spanwise cell cannot represent.
        //
        // The group boundaries remain estimates, not published. Transfer
        // burn order as the Airbus A380 AC fuel subject describes it: inner
        // first, then mid, and the outers last; the trim tank is emptied
        // first in every layout.
        "A380-800" => FuelTankLayoutConfig {
            inner_wing: with_feed(wing(0.09, 0.33, 1, 150_982.0), 0.09, 0.16, 58_698.0),
            mid_wing: with_feed(wing(0.33, 0.72, 2, 128_186.0), 0.50, 0.72, 55_264.0),
            outer_wing: wing(0.72, 0.85, 4, 20_680.0),
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
        // EASA.A.151 Issue 33 III.2.2, mod 110211: 166,488 L usable. Airbus A350 Aircraft
        // Characteristics, Feb 01/26, FIGURE-10-0-0-991-003-A01 (A350-900): wing tanks
        // 2 x 29,924 L; centre tank 80,947 L, 107,039 L with the increased capacity (note 01)
        // that mod 110211 certifies. Wings 59,848 L as printed; centre 106,640 L = the TCDS
        // total minus the wings (the figure's 107,039 L is 0.37 % above it). The figure draws
        // the centre tank out along the inboard wing to a step near the pylon, where the wing
        // tank starts; the 0.30 station of that step is an estimate (oblique view).
        "A350-900" => FuelTankLayoutConfig {
            inner_wing: wing(0.30, 0.85, 2, 59_848.0),
            center: center(1, 106_640.0),
            ..base
        },
        // DC-10-30: Boeing, DC-10 Airplane Characteristics for Airport
        // Planning, sec 5.4.4 "Ground Service Connection Data, Series
        // 30/30CF/40/40CF", printed p.111, usable US gal: tanks 1 and 3
        // (wing) 6,090 each, tank 2 9,654, centre-wing tank 12,731,
        // under-wing auxiliary tank 1,957; total 36,522 US gal (138,251 L at
        // 3.785411784 L/gal). The page lists no fuselage or cargo-hold tank.
        // Mapping: wing = 2 x 6,090 + 1,957 = 14,137 gal = 53,514 L (the
        // under-wing auxiliary tank is counted in the wing cell); centre =
        // 12,731 + 9,654 = 22,385 gal = 84,736 L (tank 2 is the wing-root /
        // centre-section tank, so it is a centre-section tank here, as the
        // inboard-extending centre tanks of the A220, 787 and A350). The page
        // gives no tank stations: the wing span stations (0.12-0.85) and the
        // burn priorities are the existing estimates, unchanged.
        "DC-10" => FuelTankLayoutConfig {
            inner_wing: wing(0.12, 0.85, 3, 53_514.0),
            center: center(2, 84_736.0),
            ..base
        },
        // Boeing 747-400 Airplane Characteristics for Airport Planning, D6-58326-1 Rev F (December 2024),
        // section 5.4.3, p. 5-14, tank capacities for the CF6-80C2 airplane: reserve 2 and 3 5,004 L each,
        // mains 1 and 4 16,548 L each, mains 2 and 3 47,486 L each, centre wing 64,960 L, horizontal
        // stabiliser 12,490 L (3,300 US gal, the trim tank here: filled last, emptied first); total usable
        // 216,199 L. Every main tank feeds its own engine, so mains 1-4 are the feed tanks (filled first,
        // emptied last, together in proportion to capacity); the centre and reserve tanks are transfer
        // tanks (centre emptied first, reserves into the inboard mains after it).
        // DERIVED: the span stations. No public source gives the rib stations (FAA TCDS A20WE refers fuel
        // to the Weight and Balance Manual), so the boundaries are where the preset's own spar-box volume
        // (chord squared times the Obert Fig. 24.82 t/c along the drawn planform) splits in the published
        // ratio between the side of body (0.10) and 0.85 (ESTIMATE, the reserve tanks' outboard end):
        // mains 2/3 0.10-0.3907, mains 1/4 0.3907-0.6863, reserves 0.6863-0.85.
        // NOT PUBLISHED: the refuel sequence inside the main group; the model fills the four mains in
        // proportion to capacity.
        "B747-400" => FuelTankLayoutConfig {
            inner_wing: with_feed(wing(0.10, 0.3907, 3, 94_972.0), 0.10, 0.3907, 94_972.0),
            outer_wing: with_feed(
                wing(0.3907, 0.85, 4, 33_096.0 + 10_008.0),
                0.3907,
                0.6863,
                33_096.0,
            ),
            center: center(2, 64_960.0),
            trim: trim(1, 12_490.0),
            ..base
        },
        // ATR 72-600 (EASA.A.084): two integral wing tanks, 5,000 kg usable
        // in total at 0.794 kg/L, each with a 160 kg feeder cell at the
        // inboard end; no centre tank.
        "ATR72-600" => FuelTankLayoutConfig {
            inner_wing: wing(0.11, 0.85, 1, 6_300.0),
            ..base
        },
        // COMAC C919: 24,917 L (19,560 kg) from the Wikipedia specification
        // table (secondary), in the wing integral tanks with no centre tank
        // (the table lists no auxiliary tank; the ER variant "retains the same
        // fuel capacity"). ESTIMATE: the tank spans, from the side of the body
        // (0.118 of the semispan) to 0.85, are not published.
        "C919" => FuelTankLayoutConfig {
            inner_wing: wing(0.12, 0.85, 1, 24_917.0),
            ..base
        },
        // Learjet 60, EASA TCDS IM.A.212 III.7: usable 2,898 lb in the wings
        // and 5,012 lb in the fuselage tank; volumes at 0.803 kg/L. Fuselage
        // tank centroid from the III.7 arm, F.S. 427.1 in (PTM fig 19-2 frame,
        // 10.52 m of 17.12 m). ESTIMATE: the wing tank spans, side of body to
        // 0.85, are not published; PTM ch 5: fuselage fuel transfers to the wings.
        "LJ60" => FuelTankLayoutConfig {
            inner_wing: wing(0.16, 0.85, 2, 1_637.0),
            auxiliary: AuxiliaryTankConfig {
                enabled: true,
                usable_volume_l: 2_831.0,
                x_position_fraction: 0.6146,
                burn_priority: 1,
            },
            ..base
        },
        // Embraer E195-E2 specification sheet (April 2025): 13,690 kg usable
        // at 0.803 kg/L, 17,049 L, in the wing integral tanks (the E190-E2
        // APM table 2.1 states the same arrangement as 16,800 L for 13,500
        // kg). No centre tank is listed. ESTIMATE: the tank spans, from the
        // side of the body (0.086 of the semispan) to 0.85, are not
        // published.
        "E195-E2" => FuelTankLayoutConfig {
            inner_wing: wing(0.09, 0.85, 1, 17_049.0),
            ..base
        },
        // A400M, EASA TCDS A.169 III.9 (normal fill, 0.785 kg/L): centre
        // 14,566 L; inner wing left + right 17,143 + 17,050 = 34,193 L; feed
        // tanks 1-4 sum 7,726 + 5,782 = 13,508 L, modelled inside the inner
        // cell (total 62,267 L). The semispan stations are estimates.
        "A400M" => FuelTankLayoutConfig {
            inner_wing: wing(0.10, 0.85, 2, 34_193.0 + 13_508.0),
            center: center(1, 14_566.0),
            ..base
        },
        // C-295, EASA TCDS A.186 Issue 08 III.9, 0.80 kg/L: main 1,605 L and auxiliary
        // 2,161 L per wing, 7,532 L (the printed 8,532 L total does not add up). Band edges are
        // DERIVED, see `band_edge_provenance`: 0.1095 side of body, 0.271 nacelle station
        // (3.50 m of the 12.905 m semispan), 0.90 tip end ESTIMATE. Main cell inboard of the
        // nacelle, auxiliary outboard (Jane's CN-235 entry). The published volumes are kept;
        // the model's spar box holds 2,386 L (both wings) in the inner band and 7,197 L to the
        // tip, short of the 7,532 L TCDS / 7,650 L Jane's totals.
        "C295" => FuelTankLayoutConfig {
            inner_wing: wing(0.1095, 0.271, 2, 3_210.0),
            outer_wing: wing(0.271, 0.90, 1, 4_322.0),
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

/// Where a preset's declared tank spanwise band comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TankBandSource {
    /// Read from a printed source (rib stations or span fractions).
    Published(&'static str),
    /// Computed from sourced inputs by the formula stated in the entry.
    Derived(&'static str),
    /// Not published: volume-consistent or geometry-derived estimate, with
    /// the basis of the estimate.
    Estimate(&'static str),
}

/// The provenance of the spanwise band(s) [`layout_for`] declares for the
/// named preset; `None` for a preset with no registered layout.
///
/// Only the A320 band is printed (AMM ATA 28 rib boundaries, cited in
/// [`layout_for`]); every other declared band is an ESTIMATE. C-295: EASA TCDS
/// A.186 Issue 08 and Jane's (CN-235 entry) give volumes and the main/auxiliary
/// split (main in the centre section inboard of the nacelles, auxiliary in the
/// outer panels) but no rib stations or span fractions.
pub fn band_provenance(preset_name: &str) -> Option<TankBandSource> {
    use TankBandSource::{Derived, Estimate, Published};
    Some(match preset_name {
        "A320-200" => Published("AMM ATA 28 tank boundaries, ribs 2-15 and 15-22 (EASA.A.064 III.9)"),
        "C295" => Derived("inner 0.1095-0.271 and outer 0.271-0.90 (tip end ESTIMATE), per-edge entries in band_edge_provenance; published volumes kept, box volume short of the totals"),
        "A380-800" => Estimate("feed and group boundaries from the model's spar-box volume split; AC figure is oblique"),
        "A220-300" | "A340-300" | "A350-900" | "B787-9" | "DC-10" | "B747-400" | "ATR72-600" | "C919"
        | "E195-E2" | "LJ60" | "A400M" | "AVE" => {
            Estimate("spans not published; volume-consistent estimate")
        }
        _ => return None,
    })
}

/// Per-edge provenance of the C-295 band: (edge, source).
///
/// 0.1095 = `side_of_body_span_fraction` of the C295 preset geometry (fuselage
/// half width 1.41 m over the 12.905 m semispan). 0.271 = nacelle station
/// y 3.50 m (half the 7.00 m propeller spacing, Jane's All the World's
/// Aircraft 2008-09, CN-235 entry) over the semispan 25.81 m / 2 (EASA TCDS
/// A.186 Issue 08, Section 5 III, span; PDF p.11). The tip end 0.90 is an
/// ESTIMATE. The model's spar box (alas-mass, usable fraction 0.92) holds
/// 2,386 L in the inner band and 7,197 L to eta 1.0, short of the 7,532 L
/// (TCDS III.9) and 7,650 L (Jane's 2008-09 pp.544-545) totals, so the
/// outer edge cannot be solved from volume; the published volumes are kept.
pub const C295_BAND_EDGES: [(&str, TankBandSource); 3] = [
    ("0.1095 inboard", TankBandSource::Derived("side_of_body_span_fraction of the C295 preset geometry")),
    ("0.271 split", TankBandSource::Derived("y 3.50 m / 12.905 m: half the 7.00 m propeller spacing (Jane's 2008-09, CN-235 entry) over half the 25.81 m span (EASA TCDS A.186 Issue 08, PDF p.11)")),
    ("0.90 tip end", TankBandSource::Estimate("not published; volume cannot fix it, box holds 7,197 L to the tip against 7,532/7,650 L")),
];

/// The per-edge provenance of a preset's band; empty when only the
/// whole-band [`band_provenance`] entry exists.
pub fn band_edge_provenance(preset_name: &str) -> &'static [(&'static str, TankBandSource)] {
    if preset_name == "C295" {
        &C295_BAND_EDGES
    } else {
        &[]
    }
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
    fn every_registered_wing_band_declares_its_provenance() {
        for preset in crate::presets::registry() {
            let layout = layout_for(preset.name).expect("registered arrangement");
            let has_band =
                layout.inner_wing.enabled || layout.mid_wing.enabled || layout.outer_wing.enabled;
            if has_band {
                assert!(
                    band_provenance(preset.name).is_some(),
                    "{}: tank band has no provenance entry",
                    preset.name
                );
            }
        }
        assert!(matches!(
            band_provenance("C295"),
            Some(TankBandSource::Derived(_))
        ));
        let edges = band_edge_provenance("C295");
        assert_eq!(edges.len(), 3);
        assert!(matches!(edges[2].1, TankBandSource::Estimate(_)));
        let c295 = layout_for("C295").expect("registered arrangement");
        assert_eq!(c295.outer_wing.span_end_fraction, 0.90);
        assert_eq!(
            c295.inner_wing.span_end_fraction,
            c295.outer_wing.span_start_fraction
        );
        // 3.50 m nacelle station over the 25.81 m span's semispan.
        assert!((c295.inner_wing.span_end_fraction - 3.50 / (25.81 / 2.0)).abs() < 5.0e-4);
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

        // The A380-800 has no centre tank, so the loop above cannot reach it,
        // and its trim tank had no guard anywhere. Pin the whole order: it is
        // the only ordering this layout carries, and
        // `alas_mass::tanks::FuelTankLayout::distribute` derives the ground
        // fill order from it by reversal, so the trim tank's value decides
        // where a partial load sits. This records the registered order (the
        // Airbus A380 AC fuel subject's transfer order) so that changing it
        // is a deliberate edit; it does not endorse the reversal rule, which
        // is unsourced and documented as such in that module.
        let a380 = layout_for("A380-800").expect("registered arrangement");
        assert!(!a380.center.enabled, "the A380-800 has no centre tank");
        assert!(a380.trim.enabled);
        assert_eq!(a380.inner_wing.burn_priority, 1);
        assert_eq!(a380.mid_wing.burn_priority, 2);
        assert_eq!(a380.trim.burn_priority, 3);
        assert_eq!(a380.outer_wing.burn_priority, 4);
    }

    #[test]
    fn the_a380_cells_carry_the_certified_tank_volumes() {
        // EASA TCDS EASA.A.110 Issue 17, 2026-08-05, section 3.3 "Fluid
        // Capacities", p.14 of 20, at the sheet's 0.800 kg/L convention. Each
        // cell is the sum of the certified tanks it lumps, written out so a
        // future edit has to restate which tanks it is claiming.
        let a380 = layout_for("A380-800").expect("registered arrangement");
        let published = |cell: &WingTankConfig| {
            cell.published_usable_volume_l
                .expect("published wing cell volume")
        };
        assert_eq!(
            published(&a380.inner_wing),
            29_349.0 + 29_349.0 + 46_142.0 + 46_142.0,
            "Feed 2 + Feed 3 + Inner Left + Inner Right"
        );
        assert_eq!(
            published(&a380.mid_wing),
            27_632.0 + 27_632.0 + 36_461.0 + 36_461.0,
            "Feed 1 + Feed 4 + Mid Left + Mid Right"
        );
        assert_eq!(
            published(&a380.outer_wing),
            10_340.0 + 10_340.0,
            "Outer Left + Outer Right"
        );
        assert_eq!(a380.trim.published_usable_volume_l, Some(23_698.0));
        assert_eq!(
            a380.inner_wing.feed.published_usable_volume_l,
            Some(29_349.0 + 29_349.0),
            "Feed 2 + Feed 3, the root end of the inner cell"
        );
        assert_eq!(
            a380.mid_wing.feed.published_usable_volume_l,
            Some(27_632.0 + 27_632.0),
            "Feed 1 + Feed 4, the outboard end of the mid cell"
        );

        // The tanks sum to the certified tank total, which is also the
        // Airbus A380 AC usable fuel capacity (323,546 L, sec. 2-1-1 p.2) the
        // preset now declares; the TCDS "Systems" row (793 L of line and
        // engine fuel) is not a tank and is not part of that pair. Litres are
        // exact integers here, so these are exact comparisons.
        let tanks_l = published(&a380.inner_wing)
            + published(&a380.mid_wing)
            + published(&a380.outer_wing)
            + a380.trim.published_usable_volume_l.expect("published trim");
        assert_eq!(tanks_l, 323_546.0);
        let declared_l = crate::presets::get("A380-800")
            .expect("registered preset")
            .reference
            .usable_fuel_volume_l
            .expect("declared usable volume");
        assert_eq!(declared_l, 323_546.0);
        assert_eq!(declared_l, tanks_l);
    }
}
