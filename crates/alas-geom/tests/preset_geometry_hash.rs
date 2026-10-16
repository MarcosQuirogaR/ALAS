// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Every registered preset builds the same main wing, tail and fuselages (a wingtip device, wings[3], is not hashed).
//!
//! The pinned values are FNV-1a 64 hashes of the `Debug` text of the built
//! wings and fuselages (flight shape, engines included), captured on the tree
//! before the wingtip-device code existed (`loft/integration-2026-10-11`,
//! 1c951d9f). They must change only with an intended preset geometry change,
//! never with the device code. `ALAS_PRINT_GEOMETRY_HASHES=1` prints them.
//!
//! Re-pinned at geometry-phase integration step A (merge of the tail and
//! wingtip lanes plus the wave-2 and step-A fuselage loft conversions): AVE,
//! A380-800, ATR72-600, DC-10, B747-400 and A400M moved because their
//! fuselages are now lofted (B747-400 re-fitted, ATR72-600 and A400M with new
//! section fields); the other six presets kept their wingtip-lane hashes bit
//! for bit through the merge. Old -> new: AVE 610396278507951406 ->
//! 12024561024133631184, A380-800 8806501741221392139 -> 4450530403763678112,
//! ATR72-600 10417752919893658181 -> 5494413409939299389, DC-10
//! 3996217396689968392 -> 1154073452045089683, B747-400 16948506290996217650
//! -> 5995345093849992242, A400M 14797070025799154466 -> 18080284452760807515.
//!
//! Re-pinned at step B group n (narrow/regional preset values; the surface-count check allows a
//! real wingtip device): A320-200 13570755900607715252 -> 1272364271151679578, A220-300
//! 4028105955763757499 -> 5286615536373777387, ATR72-600 5494413409939299389 ->
//! 8318721444817932842, E195-E2 15965626458766967783 -> 6313165651666490003 (C2 spline only; the drawn chords were tried and reverted), C919
//! 1715998082900220027 -> 4846241248265170370.
//! Re-pinned at step B group w1 (A340-300: C2 wing profile splines and the drawn inboard planform (pin 8036627499525510511 was splines only); A380-800: splines,
//! tailplane root chord 10.0 m, inboard LE crank; A400M: engine height). Old -> new: A340-300
//!  16064759066258661050 -> 12254688967645973724, A380-800 4450530403763678112 ->
//! 6965575696955779563 (wings[..3] only; the fence is wings[3]), A400M 18080284452760807515 ->
//! 10878223092227262083. Final merge: the A380-800 hash now covers its fence (wings[3]): 6965575696955779563 (wings[..3]) -> 4293850580232913853.
//! Re-pinned at step B group w2 (wing, engine and tail preset values; the B747-400 now carries its
//! winglet as a fourth wing): AVE 12024561024133631184 -> 16552819746635769174, B787-9
//! 1124098503249085974 -> 16483365955246698810, DC-10 1154073452045089683 -> 13054788834961571329,
//! B747-400 5995345093849992242 -> 7151870025735671571; the other eight are unchanged.
//! Final merge (reference trapezoid for B747-400 and DC-10, V2 nacelle model for the presets with a V2 set, ATR72-600 fin area, A340-300 inboard TE reverted): every
//! preset hash except AVE and A400M moved; the new values are the PINNED table below (old values in the step B notes above).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::presets;
use alas_geom::builder::AircraftBuilder;

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

const PINNED: &[(&str, u64)] = &[
    ("AVE", 11095587164708946354), // provenance paths made repo-relative 2026-10-16 (was 13468479781278576821);  // geo/close 2026-10-12 re-pin (was 16552819746635769174)
    ("A340-300", 11701508811301204286), // provenance paths made repo-relative 2026-10-16 (was 17208589000048430206);  // geo/freeze 2026-10-12 re-pin (was 14291678859939318320; Published reference wing, A340 wing TE) // geo/close 2026-10-12 re-pin (was 5588297983029544011)
    ("A380-800", 4835624319668717555), // provenance paths made repo-relative 2026-10-16 (was 9834228720352499627);  // geo/a380fix r2 2026-10-12 re-pin (was 3591507440088974704; unloaded W2 = drawn fence bottom 6.59 m, flight tip held) // geo/a380fix 2026-10-12 re-pin (was 7779591518429099096; W2 = fence bottom with flight tip held, nacelle 3.88 m, per-pair engine z) // geo/tailcone 2026-10-12 re-pin (was 5098761908960540685; aft-body loft refit) // geo/freeze 2026-10-12 re-pin (was 12425867495771478454; Published reference wing, A340 wing TE) // geo/close 2026-10-12 re-pin (was 391850771658620363)
    ("B787-9", 1774353630593138562), // provenance paths made repo-relative 2026-10-16 (was 15214518997608092550);  // geo/freeze 2026-10-12 re-pin (was 5808532977760312359; Published reference wing, A340 wing TE) // geo/close 2026-10-12 re-pin (was 15699978998198924369)
    ("A320-200", 3198985799769475364), // provenance paths made repo-relative 2026-10-16 (was 4227799242590834772);  // fix/reference-coupling 2026-10-12 re-pin (was 5959803520183508482; A320 published LEMAC 15.2615 replaces the trapezoid MAC LE 15.547) // geo/freeze 2026-10-12 re-pin (was 3871000397321575707; Published reference wing, A340 wing TE) // geo/close 2026-10-12 re-pin (was 14789297901454117527)
    ("A220-300", 16608563276810553131), // provenance paths made repo-relative 2026-10-16 (was 2565875564314783171);  // geo/freeze 2026-10-12 re-pin (was 17474995212154995751; Published reference wing, A340 wing TE) // geo/close 2026-10-12 re-pin (was 5590247412936984494)
    ("ATR72-600", 13407197985806009788), // provenance paths made repo-relative 2026-10-16 (was 9363827761913373628);  // geo/freeze 2026-10-12 re-pin (was 10700935699241596872; Published reference wing, A340 wing TE) // geo/close 2026-10-12 re-pin (was 10212099904811567149)
    ("DC-10", 14739660932387965408), // provenance paths made repo-relative 2026-10-16 (was 12556811726260817772);  // final re-pin 2026-10-14 (was 13881591976244895501; printed Obert Fig. 24.21 wing thickness stations, fuel-chain lane) // geo/freeze 2026-10-12 re-pin (was 3797858947093887165; Published reference wing, A340 wing TE) // geo/close 2026-10-12 re-pin (was 725558010824249599)
    ("E195-E2", 7347306179553344861), // provenance paths made repo-relative 2026-10-16 (was 11207091119743141893);  // fix/reference-coupling 2026-10-12 re-pin (was 13698981231447496369; derived LEMAC 17.958) // geo/freeze 2026-10-12 re-pin (was 3048971105713279233; Published reference wing, A340 wing TE) // geo/close 2026-10-12 re-pin (was 15048045877647627820)
    ("C919", 9792897282796501957), // geo/freeze 2026-10-12 re-pin (was 3274790824617544882; Published reference wing, A340 wing TE) // geo/close 2026-10-12 re-pin (was 13220576745914729849)
    ("B747-400", 15492356647656025083), // provenance paths made repo-relative 2026-10-16 (was 10062655502215309929);  // final re-pin 2026-10-14 (was 2883493161930035400, then 61288880130442329 before the dihedral stations were moved onto the thickness nodes; printed Obert Fig. 24.82 wing thickness stations, fuel-chain lane) // geo/freeze 2026-10-12 re-pin (was 13562575350236679593; Published reference wing, A340 wing TE) // geo/close 2026-10-12 re-pin (was 12065180943941677486)
    ("A400M", 8243637745584233548), // provenance paths made repo-relative 2026-10-16 (was 7323503594127204010);  // integration round 4 2026-10-12 re-pin (was 15403258036913307423 on reference-coupling, 3791940184766590783 on tailcone; merged LEMAC 15.415 + aft-body refit) // fix/reference-coupling 2026-10-12 re-pin (was 6908957092296681392; LEMAC 15.415) // geo/freeze 2026-10-12 re-pin (was 16620310613232825262; Published reference wing, A340 wing TE) // geo/close 2026-10-12 re-pin (was 10878223092227262083)
];

#[test]
fn presets_without_a_wingtip_device_are_bit_identical() {
    let print = std::env::var_os("ALAS_PRINT_GEOMETRY_HASHES").is_some();
    for name in presets::available() {
        let preset = presets::get(name).unwrap();
        let plane = AircraftBuilder::new(Some(preset.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .unwrap();
        // Step B: a preset with a real device (sharklet, winglet, fence) builds it as a fourth surface.
        let surfaces = 3 + usize::from(preset.geometry.wing.wingtip_device.is_some());
        assert_eq!(plane.wings.len(), surfaces, "{name}");
        let hash = fnv1a(&format!("{:?}{:?}", plane.wings, plane.fuselages));
        if print {
            println!("HASH {name} {hash}");
        }
        if !print {
            let key = name.to_string();
            let pinned = PINNED
                .iter()
                .find(|entry| entry.0 == key)
                .map(|entry| entry.1);
            assert_eq!(pinned, Some(hash), "{name}: geometry hash moved");
        }
    }
}
