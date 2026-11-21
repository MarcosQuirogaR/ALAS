// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Published nose-gear shares at the most-aft centre of gravity, per preset.
//!
//! Airbus section 7-3-0 rows give the per-strut static main-gear loads at
//! the most-aft CG of each weight variant at its own maximum ramp weight;
//! the nose share is one minus the summed main-gear loads over that weight.
//! The lighter weight variants of the same airframe are read as points of
//! the airframe's ground aft limit at their weights only where they lie on
//! one aft-limit line (the share moves monotonically with weight, as the
//! Airbus rows do); where one weight carries several CG options the
//! most-aft one is taken (the least restrictive the manufacturer certifies
//! there). Rows that do not lie on one line are separate CG options of
//! different variants and only one variant's line is entered (E195-E2 enters the Figure 7.6 aft boundary). Chart points (Boeing and
//! A220 section 7.4 ground CG envelopes, percent of weight on the main gear
//! against weight) are the envelope vertices read off the aft boundary; the
//! chart read uncertainty is stated per source.

use super::{AftCgNoseLoadPoint, PublishedAftCgNoseLoad};

const fn point(mass_kg: f64, nose_gear_fraction: f64, aft_cg_pct_mac: f64) -> AftCgNoseLoadPoint {
    AftCgNoseLoadPoint {
        mass_kg,
        nose_gear_fraction,
        aft_cg_pct_mac: Some(aft_cg_pct_mac),
    }
}

const fn chart_point(mass_kg: f64, nose_gear_fraction: f64) -> AftCgNoseLoadPoint {
    AftCgNoseLoadPoint {
        mass_kg,
        nose_gear_fraction,
        aft_cg_pct_mac: None,
    }
}

/// Airbus A320-200, two main-gear struts.
pub(crate) const A320_200: PublishedAftCgNoseLoad = PublishedAftCgNoseLoad {
    points: &[
        point(66_400.0, 1.0 - 2.0 * 31_540.0 / 66_400.0, 43.0),
        point(67_400.0, 1.0 - 2.0 * 32_020.0 / 67_400.0, 43.0),
        point(68_400.0, 1.0 - 2.0 * 32_500.0 / 68_400.0, 43.0),
        point(70_400.0, 1.0 - 2.0 * 33_400.0 / 70_400.0, 42.61),
        point(71_900.0, 1.0 - 2.0 * 33_970.0 / 71_900.0, 41.5),
        point(73_900.0, 1.0 - 2.0 * 34_720.0 / 73_900.0, 40.0),
        point(75_900.0, 1.0 - 2.0 * 35_490.0 / 75_900.0, 38.7),
        point(77_400.0, 1.0 - 2.0 * 36_030.0 / 77_400.0, 37.5),
        point(78_400.0, 1.0 - 2.0 * 36_410.0 / 78_400.0, 36.8),
    ],
    source: "Airbus A320 Aircraft Characteristics, Jun 01/24, Figure 7-3-0-991-010-A01 sheets 1-4 (V(MG) per strut static at the most-aft CG, at MRW): WV006 66,400 kg 31,540 kg 43 %; WV005 (CG 43 %) 67,400 kg 32,020 kg 43 %; WV001 68,400 kg 32,500 kg 43 %; WV019 (CG 42.61 %) 70,400 kg 33,400 kg 42.61 %; WV004 71,900 kg 33,970 kg 41.5 %; WV000 73,900 kg 34,720 kg 40 %; WV003 (CG 38.7 %) 75,900 kg 35,490 kg 38.7 %; WV007 (CG 37.5 %) 77,400 kg 36,030 kg 37.5 %; WV017 78,400 kg 36,410 kg 36.8 % MAC",
};

/// Airbus A340-300, two wing-gear struts and one centre gear.
pub(crate) const A340_300: PublishedAftCgNoseLoad = PublishedAftCgNoseLoad {
    points: &[
        point(254_400.0, 1.0 - (2.0 * 100_230.0 + 40_510.0) / 254_400.0, 38.18),
        point(257_900.0, 1.0 - (2.0 * 101_640.0 + 40_910.0) / 257_900.0, 38.05),
        point(260_900.0, 1.0 - (2.0 * 102_950.0 + 41_120.0) / 260_900.0, 38.0),
    ],
    source: "Airbus A340-200/-300 Aircraft Characteristics Rev 33, 2025-12-01, Figure 7-3-0-991-007-A01 (A340-300 static loads at the most-aft CG, at MRW; wing gear per strut and centre gear): sheet 1 WV000 254,400 kg 100,230 / 40,510 kg 38.18 %; WV001 257,900 kg 101,640 / 40,910 kg 38.05 %; sheet 2 WV029 260,900 kg 102,950 / 41,120 kg 38 % MAC",
};

/// Airbus A380-800, two wing-gear and two body-gear struts.
pub(crate) const A380_800: PublishedAftCgNoseLoad = PublishedAftCgNoseLoad {
    points: &[
        point(
            512_000.0,
            1.0 - (2.0 * 97_410.0 + 2.0 * 146_110.0) / 512_000.0,
            43.0,
        ),
        point(
            562_000.0,
            1.0 - (2.0 * 106_920.0 + 2.0 * 160_380.0) / 562_000.0,
            43.0,
        ),
    ],
    source: "Airbus A380 Aircraft Characteristics Rev 20, 2025-12-01, Figure 7-3-0-991-006-A01 sheet 1 (static loads per strut at the most-aft CG, at MRW): WV001 512,000 kg wing gear 97,410 kg, body gear 146,110 kg, 43 %; WV000 562,000 kg 106,920 / 160,380 kg, 43 % MAC",
};

/// Airbus A220-300 (BD-500-1A11).
pub(crate) const A220_300: PublishedAftCgNoseLoad = PublishedAftCgNoseLoad {
    points: &[
        point(36_287.0, 1.0 - 0.9312, 29.0),
        point(45_359.0, 1.0 - 0.9485, 35.8),
        point(60_781.0, 1.0 - 0.9487, 35.8),
        point(68_039.0, 1.0 - 0.9350, 30.3),
    ],
    source: "Airbus A220 ACP Issue 013, 2025-11-27, DM BD500-A-J00-00-00-11AAB-030A-A Figure 14 (A220-300 ground CG envelope, percent of weight on the main gear; aft-boundary vertices read 93.12 % at 80,000 lb, 94.85 % at 100,000 lb, 94.87 % at 134,000 lb, 93.50 % at 150,000 lb, read +-0.02 %); aft CG from Airbus A220 ARP BD500-3AB48-10400-00, DM BD500-A-J08-41-03-01AAA-030A-A Table 3 (ground aft limit 29.0 / 35.8 / 35.8 / 30.3 % MAC at the same weights)",
};

/// Learjet 60, one nose wheel and two main-gear struts: the static nose share
/// at the certified ground-handling aft limit, between the PTM fig 19-2 static
/// wheel stations (nose F.S. 92.42 in, main F.S. 396.82 in).
pub(crate) const LJ60: PublishedAftCgNoseLoad = PublishedAftCgNoseLoad {
    points: &[
        point(10_427.0, (396.82 - 391.92) / (396.82 - 92.42), 33.5),
        point(10_433.0, (396.82 - 391.75) / (396.82 - 92.42), 33.3),
    ],
    source: "EASA TCDS IM.A.212 Learjet M60 Issue 04 (12 Nov 2024) III.5, ground handling limit: aft limit F.S. 391.92 in (33.5 % MAC) up to 22,987 lb (10,427 kg), tapering to F.S. 391.75 in (33.3 % MAC) at 23,000 lb (10,433 kg); nose share by static balance between the FlightSafety Learjet 60 PTM fig 19-2 static wheel stations, nose F.S. 92.42 in and main F.S. 396.82 in",
};

/// Boeing 787-9, MTW 563,000 lb, two main-gear struts.
pub(crate) const B787_9: PublishedAftCgNoseLoad = PublishedAftCgNoseLoad {
    points: &[
        chart_point(110_900.0, 1.0 - 0.9366),
        chart_point(155_700.0, 1.0 - 0.9567),
        chart_point(226_400.0, 1.0 - 0.9567),
        chart_point(249_400.0, 1.0 - 0.9443),
        chart_point(255_372.0, 1.0 - 2.0 * 259_574.0 / 563_000.0),
    ],
    source: "Boeing 787 ACAP D6-58333 Rev Q, October 2025, Figure 7.4.2 (787-9, MTW 563,000 lb: aft boundary of the ground CG envelope read 93.66 % on the main gear at 244,500 lb, 95.67 % at 343,300 lb and 499,100 lb, 94.43 % at 549,800 lb, read +-0.1 % and +-1,000 lb) and section 7.3 (259,574 lb static per main-gear strut at the most-aft CG at 563,000 lb)",
};

/// McDonnell Douglas DC-10-30, two wing-gear struts and one centre gear.
pub(crate) const DC_10_30: PublishedAftCgNoseLoad = PublishedAftCgNoseLoad {
    points: &[
        chart_point(118_500.0, 1.0 - 0.9336),
        chart_point(123_000.0, 1.0 - 0.943),
        chart_point(209_600.0, 1.0 - 0.943),
        chart_point(264_445.0, 1.0 - (2.0 * 218_767.0 + 94_746.0) / 583_000.0),
    ],
    source: "Boeing DC/MD-10 ACAP DAC-67803A Rev A, Figure 7.4.2 (DC-10 Series 30/30CF/40/40CF, maximum ramp weight 583,000 lb: aft boundary of the ground CG envelope read 93.36 % on the main gear at 261,200 lb, 94.3 % from 271,300 lb to 462,000 lb, read +-0.15 % and +-2,000 lb) and section 7.3.2 (583,000 lb: wing gear 218,767 lb per strut and centre gear 94,746 lb static at the most-aft CG)",
};

/// Boeing 747-400, two wing-gear and two body-gear struts of four wheels.
pub(crate) const B747_400: PublishedAftCgNoseLoad = PublishedAftCgNoseLoad {
    points: &[
        chart_point(353_260.0, 1.0 - 0.963),
        // Section 7.3 rows without tail-tank fuel: 4 x 191,500 lb at 803,000 lb,
        // 4 x 197,300 lb at 836,000 lb, 4 x 200,300 lb at 853,000 lb.
        chart_point(364_223.0, 1.0 - 4.0 * 191_500.0 / 803_000.0),
        chart_point(379_203.0, 1.0 - 4.0 * 197_300.0 / 836_000.0),
        chart_point(386_914.0, 1.0 - 4.0 * 200_300.0 / 853_000.0),
        chart_point(395_986.0, 1.0 - 4.0 * 204_500.0 / 873_000.0),
        chart_point(397_800.0, 1.0 - 4.0 * 204_600.0 / 877_000.0),
    ],
    source: "Boeing 747-400 ACAP D6-58326-1 Rev F, December 2024, Figure 7.4.1 (p.7-9; 747-400, -400 Combi, -400 Domestic: aft boundary of the ground CG envelope read 96.3 % on the main gear up to 750,000 lb on the main gear, i.e. 778,800 lb total, read +-0.15 % and +-3,000 lb) and section 7.3 (p.7-8; maximum main-gear load at the most-aft CG per strut, four struts, rows without tail-tank fuel: 191,500 lb at 803,000 lb, 197,300 lb at 836,000 lb, 200,300 lb at 853,000 lb, 204,500 lb at 873,000 lb and 204,600 lb at the 877,000 lb maximum design taxi weight)",
};

/// Embraer E195-E2 (ERJ 190-400, SB 190E2-00-0058 variant), two main-gear
/// struts of two wheels.
///
/// The aft boundary of the 195-E2 ground CG envelope, APM 5824 Rev 25 Figure
/// 7.6 (section 07 page 9 of 40, rendered): percent of weight on the main
/// gear against weight on the main gear, vertices read +-0.1 % and +-500 kgf.
/// Aircraft weight is the weight on the mains over that percentage, matched to
/// the weight lines the chart prints. The same table 7.3 rows (62,700 kg
/// 46.4 % and 34,700 kg 46.0 % per leg) are the two ends of that boundary, so
/// they are entered as the same points. Between 46,660 and 53,980 kg the
/// boundary is the 40.0 % MAC aft CG line (94.3 % on the mains, a constant
/// share; the chart's %MAC is in the manufacturer's frame, so no model-frame
/// CG is entered); above it the boundary is curtailed forward to 92.8 % at 61,700 kg
/// (a load-limited edge of this reference aircraft, not a CG property).
pub(crate) const E195_E2: PublishedAftCgNoseLoad = PublishedAftCgNoseLoad {
    points: &[
        chart_point(34_700.0, 1.0 - 0.920),
        chart_point(41_060.0, 1.0 - 0.940),
        chart_point(46_660.0, 1.0 - 0.943),
        chart_point(53_980.0, 1.0 - 0.943),
        chart_point(61_700.0, 1.0 - 0.928),
        chart_point(62_700.0, 1.0 - 2.0 * 0.464),
    ],
    source: "Embraer E-Jets E2 Airport Planning Manual APM 5824 Rev 25 (29 Nov 2024) section 07 Figure 7.6 (page 9 of 40, EMBRAER 195-E2 landing gear loading on pavement, rendered): aft boundary vertices (percent of weight on main gear, weight on main gear kgf) 92.0 % 31,900 (34,700 kg line), 94.0 % 38,600, 94.3 % 44,000 and 50,900 (the 40.0 % MAC dashed aft line), 92.8 % 57,300 (61,700 kg line); section 7.10.2 Table 7.3 (ACR calculation for ERJ 190-400, MTOW 62,500 kg): maximum ramp weight 62,700 kg 46.4 % on one main leg and 34,700 kg 46.0 %, the same 92.8 % and 92.0 % as the chart ends. Fig 7.4 (section 7.3) prints 29,185 kg per strut (94.6 %) at 61,700 kg and 26,921 kg (89.6 %) at 60,100 kg for the same airframe, which do not agree with the chart vertex and are not entered",
};
