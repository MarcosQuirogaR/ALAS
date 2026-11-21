// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Certified tires with rated loads from public tire data books.
//!
//! Each entry is keyed by the designation (size and ply rating) that an
//! aircraft's airport-planning document or type-certificate data sheet
//! prints, never by an aircraft name. Its [`RatingBasis`] says how the rated
//! load was established:
//!
//! - `Exact`: that size and ply rating is rated in a data book.
//! - `LowerBound`: no book rates the fitted designation, so the rated load is
//!   a lower bound from a rated tire of the same size at a lower ply rating,
//!   or of a dominated envelope (outside diameter, section width, rim
//!   diameter and ply rating all no larger). A larger rim is not dominated:
//!   in every Goodyear Data Book 2022 pair of equal outside diameter, width
//!   and ply rating, the larger rim rates higher (41x15.0-18 against
//!   H41x15.0-19 at 24 ply, 31,400 against 33,650 lb; 49x19.0-20 against
//!   H49x19.0-22 at 32 ply, 51,900 against 56,600 lb; 54x21.0-23 against
//!   H54x21.0-24 at 36 ply, 68,500 against 72,200 lb; ten pairs in all). The
//!   bound assumes the larger envelope at an equal
//!   or higher ply rating carries at least that load, which the rated loads
//!   of every same-family pair in the books support. It never exceeds the
//!   fitted tire's true rating, so a tire check on it cannot hide an overload
//!   the certified tire would carry, but it can flag one the certified tire
//!   would not have.
//!
//! Diameter and width are the data book's grown outside diameter and section
//! width where the book rates the size, else the fitted nominal size or the
//! grown size of the same-size rated tire (each entry's doc says which).
//! A dominated envelope also has an air-volume proxy (OD^2 - rim^2) x width no
//! larger than the fitted tire's (H42x16.0-19 24 ply, 34,400 lb, fails it for the
//! E2 tire: 22,448 against 21,824, so the 33,650 lb H41x15.0-19 entry is used).

use super::{RatingBasis, TireSpec};

/// Kilograms per pound-mass (exact definition).
const LB_KG: f64 = 0.453_592_37;
/// Metres per inch (exact definition).
const IN_M: f64 = 0.0254;

/// Goodyear Aviation, *Aircraft Tire Data Book*, Data Section 2022,
/// Sec. 4A Bias Three-Part Tire Specifications (PDF p.6): H49x19.0-22, 32 ply,
/// TL, 235 mph, rated load 56,600 lb, inflation 205 psi, max braking load
/// 84,900 lb (part numbers 499K29-3, 499K29T1). The Boeing 747-400 ACAP
/// D6-58326-1 Rev F sec. 7.2 prints this size and ply on the main gear.
pub const H49X19_0_22_32PR: TireSpec = TireSpec {
    code: "H49x19.0-22/32PR",
    name: "H49x19.0-22 32PR (Goodyear, 747-400 main)",
    rated_load_kg: 56_600.0 * LB_KG,
    diameter_m: 49.0 * IN_M,
    width_m: 19.0 * IN_M,
    ply_rating: 32,
    rated_speed_mph: 235.0,
    rated_inflation_psi: 205.0,
    rating_basis: RatingBasis::Exact,
    max_braking_load_kg: Some(84900.0 * LB_KG),
    source: "Goodyear Aviation Data Book 2022, Sec. 4A, PDF p.6 (also Michelin Aircraft Tire Engineering Data application chart: 747-400 H49x19.0-22 32 PR)",
};

/// Goodyear Aviation, *Aircraft Tire Data Book* 2022, Sec. 4A (PDF p.6):
/// 49x17, 32 ply, TL, 235 mph, rated load 50,400 lb, inflation 210 psi,
/// outside diameter 48.75 in, width 17.25 in; the Michelin Aircraft tire
/// Engineering Data bias table prints the same load and inflation. The Boeing
/// 747-400 ACAP D6-58326-1 Rev F sec. 7.2 prints 49x17 32PR on the nose gear.
pub const B49X17_32PR: TireSpec = TireSpec {
    code: "49x17/32PR",
    name: "49x17 32PR (Goodyear, 747-400 nose)",
    rated_load_kg: 50_400.0 * LB_KG,
    diameter_m: 48.75 * IN_M,
    width_m: 17.25 * IN_M,
    ply_rating: 32,
    rated_speed_mph: 235.0,
    rated_inflation_psi: 210.0,
    rating_basis: RatingBasis::Exact,
    max_braking_load_kg: Some(75600.0 * LB_KG),
    source: "Goodyear Aviation Data Book 2022, Sec. 4A bias table (49x17 32 ply, 235 mph); Michelin Aircraft tire Engineering Data bias table (49 x 17 32 ply)",
};

/// The DC-10-30/40 main tire size 52x20.5-23. Michelin's application chart
/// prints it at 26, 28 or 30 ply (Goodyear's chart, Sec. 4A PDF p.17, at 28 or
/// 30) and no retrieved DC-10 document prints the ply, so the rated load is the
/// lowest charted ply: 26 ply, 55,000 lb, 165 psi (Michelin Engineering Data
/// bias table, no speed printed), outside diameter 52 in, width 20.5 in. The
/// 28 and 30 ply ratings are 59,500 and 63,700 lb.
pub const B52X20_5_23: TireSpec = TireSpec {
    code: "52x20.5-23",
    name: "52x20.5-23, ply not printed (DC-10-30/40 main; 26 ply lower bound)",
    rated_load_kg: 55_000.0 * LB_KG,
    diameter_m: 52.0 * IN_M,
    width_m: 20.5 * IN_M,
    ply_rating: 0,
    rated_speed_mph: 0.0,
    rated_inflation_psi: 0.0,
    rating_basis: RatingBasis::LowerBound,
    max_braking_load_kg: None,
    source: "LOWER BOUND: Michelin Aircraft tire Engineering Data bias table, 52 x 20.5 - 23 at 26 ply (55,000 lb); Michelin application chart prints DC-10-30/40 at 26/28/30 ply; Goodyear Data Book 2022 Sec. 4A application chart PDF p.17 (28/30 ply)",
};

/// The DC-10-30/40 nose tire size 40x15.5-16, charted at 26 or 28 ply
/// (Goodyear Data Book 2022 application chart PDF p.17). The rated load is
/// the lower ply: 26 ply, 36,300 lb, 180 psi, 235 mph (Michelin Engineering
/// Data bias table); the 28 ply rating is 39,500 lb (Goodyear 2022 Sec. 4A).
pub const B40X15_5_16: TireSpec = TireSpec {
    code: "40x15.5-16",
    name: "40x15.5-16, ply not printed (DC-10-30/40 nose; 26 ply lower bound)",
    rated_load_kg: 36_300.0 * LB_KG,
    diameter_m: 40.0 * IN_M,
    width_m: 15.5 * IN_M,
    ply_rating: 0,
    rated_speed_mph: 235.0,
    rated_inflation_psi: 0.0,
    rating_basis: RatingBasis::LowerBound,
    max_braking_load_kg: None,
    source: "LOWER BOUND: Michelin Aircraft tire Engineering Data bias table, 40 x 15.5 - 16 at 26 ply (36,300 lb); Goodyear Data Book 2022 application chart PDF p.17 (DC-10 nose 26/28 ply), Sec. 4A 28 ply 39,500 lb",
};

/// Goodyear Aviation, *Aircraft Tire Data Book* 2022, Sec. 4A bias table:
/// 450x190-5, 10 ply, TL, 190 mph, rated load 3,600 lb, inflation 75 psi,
/// outside diameter 18.31 in, width 7.68 in (part 459M08-2). The Michelin
/// application chart lists 450 x 190-5 10 ply as the ATR 72 nose tire.
pub const B450X190_5_10PR: TireSpec = TireSpec {
    code: "450x190-5/10PR",
    name: "450x190-5 10PR (ATR 72 nose)",
    rated_load_kg: 3_600.0 * LB_KG,
    diameter_m: 18.31 * IN_M,
    width_m: 7.68 * IN_M,
    ply_rating: 10,
    rated_speed_mph: 190.0,
    rated_inflation_psi: 75.0,
    rating_basis: RatingBasis::Exact,
    max_braking_load_kg: Some(5400.0 * LB_KG),
    source: "Goodyear Aviation Data Book 2022, Sec. 4A bias table: 450x190-5 10 ply 190 mph; Michelin Aircraft tire Engineering Data application chart (ATR 72 nose 450 x 190-5 10 ply)",
};

/// Michelin, *Aircraft tire Engineering Data*, Radial Civil Three Part (Metric
/// Code), printed p.49: 1400x530R23, 36 ply, 235 mph, max static load
/// 68,500 lb, inflation 223 psi, grown OD 56.85 in, width 21.70 in. The
/// Michelin application chart (printed p.63) lists 36 PR for the A340-300 at
/// maximum ramp weights of 260.9 t and above, and Airbus A340-200/-300
/// Aircraft Characteristics sec. 7-2-0 prints 1400x530R23 as the main tire.
pub const R1400X530R23_36PR: TireSpec = TireSpec {
    code: "1400x530R23/36PR",
    name: "1400x530R23 36PR (Michelin, A330/A340 main)",
    rated_load_kg: 68_500.0 * LB_KG,
    diameter_m: 56.85 * IN_M,
    width_m: 21.70 * IN_M,
    ply_rating: 36,
    rated_speed_mph: 235.0,
    rated_inflation_psi: 223.0,
    rating_basis: RatingBasis::Exact,
    max_braking_load_kg: None,
    source: "Michelin Aircraft tire Engineering Data, Radial Civil Three Part (Metric Code), printed p.49; application chart printed p.63",
};

/// Michelin, *Aircraft tire Engineering Data*, printed p.49: 1400x530R23,
/// 40 ply, 235 mph, max static load 74,950 lb, inflation 249 psi, grown OD
/// 56.85 in, width 21.70 in. The Goodyear application chart (PDF p.17)
/// lists 1400x530R23 40 PR for the A380 main gear.
pub const R1400X530R23_40PR: TireSpec = TireSpec {
    code: "1400x530R23/40PR",
    name: "1400x530R23 40PR (Michelin, A380 and A340-500/-600 main)",
    rated_load_kg: 74_950.0 * LB_KG,
    diameter_m: 56.85 * IN_M,
    width_m: 21.70 * IN_M,
    ply_rating: 40,
    rated_speed_mph: 235.0,
    rated_inflation_psi: 249.0,
    rating_basis: RatingBasis::Exact,
    max_braking_load_kg: None,
    source: "Michelin Aircraft tire Engineering Data, Radial Civil Three Part (Metric Code), printed p.49; Goodyear application chart PDF p.17 (A380 main 1400x530R23 40 PR)",
};

/// Goodyear Aviation, *Aircraft Tire Data Book* 2022, Sec. 4F Radial (PDF
/// p.11): H34x10.0R16, 14 ply, TL, 190 mph, rated load 13,400 lb, inflation
/// 130 psi (part numbers 346Q48-1, 346Q48-4); Michelin's data book prints
/// the same load and inflation. The UK CAA TCDS A.159 sec. 22 lists
/// H 34 x 10.0 R16 as the main tyre for all ATR 72 models.
pub const H34X10_0R16_14PR: TireSpec = TireSpec {
    code: "H34x10.0R16/14PR",
    name: "H34x10.0R16 14PR (ATR 72 main)",
    rated_load_kg: 13_400.0 * LB_KG,
    diameter_m: 34.85 * IN_M,
    width_m: 10.40 * IN_M,
    ply_rating: 14,
    rated_speed_mph: 190.0,
    rated_inflation_psi: 130.0,
    rating_basis: RatingBasis::Exact,
    max_braking_load_kg: Some(20100.0 * LB_KG),
    source: "Goodyear Aviation Data Book 2022, Sec. 4F, PDF p.11; Michelin Aircraft tire Engineering Data application chart (ATR 72 H34x10.0R16 14 PR)",
};

/// Goodyear Aviation, *Aircraft Tire Data Book* 2022, Sec. 4F Radial (PDF
/// p.11): 46x17.0R20, 30 ply, TL, 225 mph, rated load 46,000 lb, inflation
/// 222 psi, max braking load 69,000 lb, outside diameter 47.50 in, width
/// 17.70 in (part numbers 467Q02-3 Amdt A, 467Q02-6); the Michelin data book
/// prints the same load. COMAC C919 ACAP Table 1 prints 46x17R20/30PR as the
/// main tire.
pub const R46X17_0R20_30PR: TireSpec = TireSpec {
    code: "46x17.0R20/30PR",
    name: "46x17.0R20 30PR (C919 main)",
    rated_load_kg: 46_000.0 * LB_KG,
    diameter_m: 47.50 * IN_M,
    width_m: 17.70 * IN_M,
    ply_rating: 30,
    rated_speed_mph: 225.0,
    rated_inflation_psi: 222.0,
    rating_basis: RatingBasis::Exact,
    max_braking_load_kg: Some(69000.0 * LB_KG),
    source: "Goodyear Aviation Data Book 2022, Sec. 4F Radial, PDF p.11: 46x17.0R20 30 ply; COMAC C919 ACAP Table 1 prints 46x17R20/30PR",
};

/// A320-family 46x17.0R20: the Airbus A320 Aircraft Characteristics sec. 7-2-0
/// prints the size without a ply rating (Michelin's application chart lists
/// the A319/A320 at 32 PR; Goodyear's at 30 ply). The rated load is the
/// 30 ply rating (46,000 lb) of the same size, a lower bound for any higher
/// ply rating fitted.
pub const R46X17_0R20: TireSpec = TireSpec {
    code: "46x17.0R20",
    name: "46x17.0R20, ply not printed (A320 main; 30 ply lower bound)",
    rated_load_kg: 46_000.0 * LB_KG,
    diameter_m: 47.50 * IN_M,
    width_m: 17.70 * IN_M,
    ply_rating: 0,
    rated_speed_mph: 225.0,
    rated_inflation_psi: 0.0,
    rating_basis: RatingBasis::LowerBound,
    max_braking_load_kg: Some(69000.0 * LB_KG),
    source: "LOWER BOUND: Goodyear Aviation Data Book 2022, Sec. 4F, PDF p.11, same size at 30 ply (46,000 lb); Airbus A320 AC sec. 7-2-0 prints 46x17R20 without ply, Michelin application chart lists 32 PR",
};

/// Boeing 787 ACAP D6-58333 Rev Q sec. 7.2 prints 54 x 21.0 R23 38PR as the
/// 787-9 main tire (Michelin catalogue Nov 2022: M19501, 235 mph). Neither
/// data book rates 38 ply of this size; Goodyear 2022 Sec. 4F (PDF p.11) rates
/// 54x21.0R23 at 30 ply: 71,200 lb, 236 psi, 235 mph, outside diameter
/// 55.85 in, width 21.85 in. Same size at a lower ply is a strict lower bound
/// for the 38 ply load (the 1400x530R23 pair rates 68,500 lb at 36 ply and
/// 74,950 lb at 40 ply, above this bound).
pub const R54X21_0R23_38PR: TireSpec = TireSpec {
    code: "54x21.0R23/38PR",
    name: "54x21.0R23 38PR (787-9 main; 30 ply lower bound)",
    rated_load_kg: 71_200.0 * LB_KG,
    diameter_m: 55.85 * IN_M,
    width_m: 21.85 * IN_M,
    ply_rating: 38,
    rated_speed_mph: 235.0,
    rated_inflation_psi: 0.0,
    rating_basis: RatingBasis::LowerBound,
    max_braking_load_kg: Some(106800.0 * LB_KG),
    source: "LOWER BOUND: Goodyear Aviation Data Book 2022, Sec. 4F, PDF p.11, 54x21.0R23 at 30 ply (71,200 lb); 787 ACAP D6-58333 Rev Q sec. 7.2 prints 38PR; Michelin catalogue Nov 2022 M19501",
};

/// Airbus A220 ACP BD500-3AB48-13800-00 Table 29 prints H42 x 15.0 R21 26 PR
/// as the main tire (Michelin catalogue Nov 2022: M20201, 225 mph). No data
/// book rates this size. The rated load is the Goodyear 2022 Sec. 4A rating
/// of H40x14.5-19 at 26 ply, 36,800 lb (outside diameter 40 in and section
/// width 14.5 in against 42 in and 15.0 in, equal ply): a dominated-envelope
/// lower bound.
pub const H42X15_0R21_26PR: TireSpec = TireSpec {
    code: "H42x15.0R21/26PR",
    name: "H42x15.0R21 26PR (A220 main; H40x14.5-19 26 ply lower bound)",
    rated_load_kg: 36_800.0 * LB_KG,
    diameter_m: 42.0 * IN_M,
    width_m: 15.0 * IN_M,
    ply_rating: 26,
    rated_speed_mph: 225.0,
    rated_inflation_psi: 0.0,
    rating_basis: RatingBasis::LowerBound,
    max_braking_load_kg: Some(53360.0 * LB_KG),
    source: "LOWER BOUND: Goodyear Aviation Data Book 2022, Sec. 4A, H40x14.5-19 26 ply 225 mph (36,800 lb), a dominated envelope; A220 ACP Table 29 prints H42 x 15.0 R21 26 PR; Michelin catalogue Nov 2022 M20201",
};

/// Embraer E-Jets E2 APM Fig. 7.2 prints H42X16.0R20 24PR as the main tire
/// (Michelin catalogue Nov 2022: 190/195-E2, 225 mph, 24 ply). No data book
/// rates this size. The rated load is the Goodyear 2022 Sec. 4A rating of
/// H41x15.0-19 at 24 ply, 33,650 lb (41 in and 15.0 in against 42 in and
/// 16.0 in, equal ply): a dominated-envelope lower bound. The E195-E2 APM
/// Fig. 7.4 maximum static load at the most aft CG, 29,185 kg per main strut
/// (two wheels), is 32,171 lb per wheel, within the bound.
pub const H42X16_0R20_24PR: TireSpec = TireSpec {
    code: "H42x16.0R20/24PR",
    name: "H42x16.0R20 24PR (E2 main; H41x15.0-19 24 ply lower bound)",
    rated_load_kg: 33_650.0 * LB_KG,
    diameter_m: 42.0 * IN_M,
    width_m: 16.0 * IN_M,
    ply_rating: 24,
    rated_speed_mph: 225.0,
    rated_inflation_psi: 0.0,
    rating_basis: RatingBasis::LowerBound,
    max_braking_load_kg: Some(48800.0 * LB_KG),
    source: "LOWER BOUND: Goodyear Aviation Data Book 2022, Sec. 4A, H41x15.0-19 24 ply 225 mph (33,650 lb), a dominated envelope; E-Jets E2 APM Fig. 7.2 prints H42X16.0R20 24PR; Michelin catalogue Nov 2022",
};

/// EASA TCDS EASA.A.169 sec. 21 prints 43x15.5-17 22PR as the A400M main
/// tyre (12 main wheels). No data book rates this size. The rated load is the
/// Goodyear 2022 Sec. 4A rating of H37x14.0-15 at 22 ply, 24,100 lb at 165 psi
/// (outside diameter 37 in, width 14.0 in, rim 15 in, ply 22 against 43 in,
/// 15.5 in, rim 17 in, ply 22): a dominated-envelope lower bound, the highest
/// such rating in the book. H40x14.0-19 20 ply (27,100 lb) is not a bound: its
/// 19 in rim is larger. H42x16.0-19 22 ply (31,200 lb) is wider.
pub const B43X15_5_17_22PR: TireSpec = TireSpec {
    code: "43x15.5-17/22PR",
    name: "43x15.5-17 22PR (A400M main; H37x14.0-15 22 ply lower bound)",
    rated_load_kg: 24_100.0 * LB_KG,
    diameter_m: 43.0 * IN_M,
    width_m: 15.5 * IN_M,
    ply_rating: 22,
    rated_speed_mph: 0.0,
    rated_inflation_psi: 0.0,
    rating_basis: RatingBasis::LowerBound,
    max_braking_load_kg: Some(36150.0 * LB_KG),
    source: "LOWER BOUND: Goodyear Aviation Data Book 2022, Sec. 4A, H37x14.0-15 22 ply 235 mph (24,100 lb), a tire dominated in outside diameter, width, rim and ply; EASA TCDS A.169 sec. 21 prints 43x15.5-17 22PR",
};

/// Goodyear Aviation, *Aircraft Tire Data Book* 2022, Sec. 4F Radial (PDF
/// p.11): 40x16.0R16, 26 ply, TL, 235 mph, rated load 31,475 lb, inflation
/// 180 psi, outside diameter 41.45 in, width 16.65 in. The Boeing 787 ACAP
/// D6-58333 Rev Q sec. 7.2 prints 40 x 16.0 R16 26PR on the 787-9 nose gear.
pub const R40X16_0R16_26PR: TireSpec = TireSpec {
    code: "40x16.0R16/26PR",
    name: "40x16.0R16 26PR (787-9 nose)",
    rated_load_kg: 31_475.0 * LB_KG,
    diameter_m: 41.45 * IN_M,
    width_m: 16.65 * IN_M,
    ply_rating: 26,
    rated_speed_mph: 235.0,
    rated_inflation_psi: 180.0,
    rating_basis: RatingBasis::Exact,
    max_braking_load_kg: Some(47225.0 * LB_KG),
    source: "Goodyear Aviation Data Book 2022, Sec. 4F, PDF p.11: 40x16.0R16 26 ply; 787 ACAP D6-58333 Rev Q sec. 7.2",
};

/// Goodyear Aviation, *Aircraft Tire Data Book* 2022, Sec. 4F Radial (PDF
/// p.11): 30x8.8R15, 16 ply, TL, 225 mph, rated load 14,200 lb, inflation
/// 199 psi, outside diameter 31.10 in, width 9.30 in (part 309Q62-1). COMAC
/// C919 ACAP Table 1 prints 30x8.8R15/16PR on the nose gear and the Airbus
/// A320 AC sec. 7-2-0 prints 30x8.8R15; the data books rate this size at
/// 16 ply only.
pub const R30X8_8R15_16PR: TireSpec = TireSpec {
    code: "30x8.8R15/16PR",
    name: "30x8.8R15 16PR (A320 and C919 nose)",
    rated_load_kg: 14_200.0 * LB_KG,
    diameter_m: 31.10 * IN_M,
    width_m: 9.30 * IN_M,
    ply_rating: 16,
    rated_speed_mph: 225.0,
    rated_inflation_psi: 199.0,
    rating_basis: RatingBasis::Exact,
    max_braking_load_kg: Some(21300.0 * LB_KG),
    source: "Goodyear Aviation Data Book 2022, Sec. 4F, PDF p.11: 30x8.8R15 16 ply; Michelin Engineering Data 30 x 8.8 R 15 16 ply; COMAC C919 ACAP Table 1",
};

/// Michelin, *Aircraft tire Engineering Data*, Radial Civil Three Part
/// (Metric Code), printed p.49 (read on the page): 1050x395R16, 28 ply,
/// 235 mph, max static load 34,200 lb, inflation 190 psi, grown OD 42.65 in,
/// width 16.15 in. The Airbus A340-200/-300 Aircraft Characteristics sec.
/// 7-2-0 prints 1050x395R16 as the nose tire; the Goodyear and Michelin
/// application charts list it at 28 ply.
pub const R1050X395R16_28PR: TireSpec = TireSpec {
    code: "1050x395R16/28PR",
    name: "1050x395R16 28PR (Michelin, A330/A340 nose)",
    rated_load_kg: 34_200.0 * LB_KG,
    diameter_m: 42.65 * IN_M,
    width_m: 16.15 * IN_M,
    ply_rating: 28,
    rated_speed_mph: 235.0,
    rated_inflation_psi: 190.0,
    rating_basis: RatingBasis::Exact,
    max_braking_load_kg: None,
    source: "Michelin Aircraft tire Engineering Data, Radial Civil Three Part (Metric Code), printed p.49; application charts (A340 nose 1050x395R16 28 PR)",
};

/// Michelin, *Aircraft tire Engineering Data*, printed p.49 (read on the
/// page): 1270x455R22, 32 ply, 225 mph, max static load 54,800 lb, inflation
/// 235 psi, grown OD 51.55 in, width 18.75 in (30 ply: 50,900 lb, 219 psi).
/// The Michelin Air X catalogue Nov 2022 lists 1270X455R22 32 PR as the A380
/// nose tire, at 235 mph against the databook's 225 mph (the databook speed
/// is kept), and also lists 50X20.0R22 34 PR (M20301) as an A380 nose-gear
/// option. The 32 PR tire, the lower rating of the two, is the conservative
/// choice for the nose check.
pub const R1270X455R22_32PR: TireSpec = TireSpec {
    code: "1270x455R22/32PR",
    name: "1270x455R22 32PR (Michelin, A380 nose)",
    rated_load_kg: 54_800.0 * LB_KG,
    diameter_m: 51.55 * IN_M,
    width_m: 18.75 * IN_M,
    ply_rating: 32,
    rated_speed_mph: 225.0,
    rated_inflation_psi: 235.0,
    rating_basis: RatingBasis::Exact,
    max_braking_load_kg: None,
    source: "Michelin Aircraft tire Engineering Data, Radial Civil Three Part (Metric Code), printed p.49; Michelin Air X catalogue Nov 2022 (A380 NLG 1270X455R22 32 PR)",
};

/// Airbus A220 ACP Table 29 and the E-Jets E2 APM Figs 7.1-7.2 print 27 x 8.5
// Needs data: Michelin product data for M20001 (27x8.5R12 16 PR) is not on disk;
// the Goodyear 2022 book only bounds it, so the rating stays a lower bound until that sheet is read.
/// R12 16 PR as the nose tire (Michelin catalogue Nov 2022: M20001, 225 mph).
/// No data book rates this size. The rated load is the Goodyear Aviation Data
/// Book 2022 rating of H25x8.0-12, 16 ply, 210 mph, 11,300 lb at 187 psi
/// (Sec. 4A, PDF p.5): outside diameter 25 in, section width 8.0 in, rim 12 in
/// and ply 16 are each no larger than the fitted tire's (27 in, 8.5 in, rim 12
/// in, 16 ply), and its air-volume proxy (25^2 - 12^2) x 8.0 = 3,848 is below
/// the fitted tire's (27^2 - 12^2) x 8.5 = 4,972. A dominated-envelope lower
/// bound; the larger H27x8.5-14 (13,300 lb) is not used because its rim is
/// larger, which raises the rated load at equal pressure.
pub const R27X8_5R12_16PR: TireSpec = TireSpec {
    code: "27x8.5R12/16PR",
    name: "27x8.5R12 16PR (A220 and E2 nose; H25x8.0-12 16 ply lower bound)",
    rated_load_kg: 11_300.0 * LB_KG,
    diameter_m: 27.0 * IN_M,
    width_m: 8.5 * IN_M,
    ply_rating: 16,
    rated_speed_mph: 225.0,
    rated_inflation_psi: 0.0,
    rating_basis: RatingBasis::LowerBound,
    max_braking_load_kg: Some(16950.0 * LB_KG),
    source: "LOWER BOUND: Goodyear Aviation Data Book 2022, Sec. 4A, H25x8.0-12 16 ply (11,300 lb), a tire dominated in outside diameter, width, rim and ply; A220 ACP Table 29 and E2 APM Figs 7.1-7.2 print 27x8.5R12 16PR; Michelin catalogue Nov 2022 M20001",
};

// Needs data: no book rates 37x14.0-14 at 22 PR (Goodyear 2022 Sec. 4A gives 24 PR only,
// 25,000 lb); the 22 PR rating needs the Goodyear/Michelin product sheet or the A400M ACAP gear loads.
/// EASA TCDS EASA.A.169 sec. 21 prints 37x14.0-14 22PR as the A400M nose
/// tyre (two wheels). Both the Goodyear Aviation Data Book 2022 (part
/// 374F42-4) and the Michelin Engineering Data rate 37x14.0-14 only at 24
/// ply (25,000 lb, 160 psi); no book rates 22 ply. The rated load is the
/// Goodyear 2022 rating of H31x13.0-12, 20 ply, 225 mph, 17,200 lb at 155 psi
/// (Sec. 4A, PDF p.5): outside diameter 31 in, width 13.0 in, rim 12 in and
/// ply 20 are each no larger than the fitted tire's (37 in, 14.0 in, rim 14
/// in, 22 ply) and its air-volume proxy is 10,621 against 16,422. A
/// dominated-envelope lower bound; the 24 ply rating is not used because it
/// belongs to a different (higher) ply rating. The nominal size gives the
/// dimensions.
pub const B37X14_0_14_22PR: TireSpec = TireSpec {
    code: "37x14.0-14/22PR",
    name: "37x14.0-14 22PR (A400M nose; H31x13.0-12 20 ply lower bound)",
    rated_load_kg: 17_200.0 * LB_KG,
    diameter_m: 37.0 * IN_M,
    width_m: 14.0 * IN_M,
    ply_rating: 22,
    rated_speed_mph: 225.0,
    rated_inflation_psi: 0.0,
    rating_basis: RatingBasis::LowerBound,
    max_braking_load_kg: Some(25800.0 * LB_KG),
    source: "LOWER BOUND: Goodyear Aviation Data Book 2022, Sec. 4A, H31x13.0-12 20 ply 225 mph (17,200 lb), a tire dominated in outside diameter, width, rim and ply; EASA TCDS A.169 sec. 21 prints 37x14.0-14 22PR; the books rate 37x14.0-14 only at 24 ply (25,000 lb)",
};

/// The certified tires, searched by designation after the tire classes.
pub const CERTIFIED_TIRES: [TireSpec; 20] = [
    H49X19_0_22_32PR,
    B49X17_32PR,
    B52X20_5_23,
    B40X15_5_16,
    B450X190_5_10PR,
    R1400X530R23_36PR,
    R1400X530R23_40PR,
    H34X10_0R16_14PR,
    R46X17_0R20_30PR,
    R46X17_0R20,
    R54X21_0R23_38PR,
    H42X15_0R21_26PR,
    H42X16_0R20_24PR,
    B43X15_5_17_22PR,
    R40X16_0R16_26PR,
    R30X8_8R15_16PR,
    R1050X395R16_28PR,
    R1270X455R22_32PR,
    R27X8_5R12_16PR,
    B37X14_0_14_22PR,
];
