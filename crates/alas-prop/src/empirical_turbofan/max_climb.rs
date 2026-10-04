// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Class-level maximum-climb thrust at the deck's reference point.
//!
//! D. Howe, "Aircraft Conceptual Design Synthesis", Professional Engineering
//! Publishing (2000), p. 67, gives the maximum thrust of a turbofan relative
//! to its sea-level-static rating as
//!
//! `F / F_SLS = (k1 + k2 BPR + (k3 + k4 BPR) M) sigma^s`,
//!
//! with coefficient rows for bypass ratio 1, 3-6 and 8, and `s` applying
//! below 11 km. The coefficients used here are the 0.4 <= M <= 0.9 rows as
//! tabulated by O. Schulz, "Assessment of Numerical Models for Thrust and
//! Specific Fuel Consumption for Turbofan Engines", Diplomarbeit, HAW
//! Hamburg (2007), Table 2.1 / Eq. 2.9. Schulz compares the relation with
//! manufacturer climb-thrust data of two engines (Sec. 3.3.5, Fig. 3.13):
//! within 5 % to 14,000 ft, 11 % to 20,000 ft and 20 % to 35,000 ft, and he
//! names it the most accurate climb-thrust model of those he assessed that
//! needs only the static thrust (Sec. 4.1.7).
//!
//! Only the reference point (the cruise altitude and Mach of the engine
//! catalogue entry, 0.4 <= M <= 0.9 and at most 11 km) is evaluated here;
//! the deck carries the thrust from that anchor to other altitudes and
//! speeds with the Bartel-Young climb lapse, whose ratio is relative to the
//! maximum-climb thrust at exactly that point.

/// One of Howe's 0.4 <= M <= 0.9 coefficient rows and the bypass-ratio
/// band it is tabulated for.
#[derive(Debug, Clone, Copy)]
struct HoweRow {
    /// Lowest and highest bypass ratio of the row's class.
    bpr_band: (f64, f64),
    k1: f64,
    k2: f64,
    k3: f64,
    k4: f64,
    /// Density-ratio exponent below 11 km.
    s: f64,
}

/// Howe's high-speed rows for the classes BPR 1, BPR 3-6 and BPR 8.
const HOWE_HIGH_MACH_ROWS: [HoweRow; 3] = [
    HoweRow {
        bpr_band: (1.0, 1.0),
        k1: 0.856,
        k2: 0.062,
        k3: 0.16,
        k4: -0.23,
        s: 0.8,
    },
    HoweRow {
        bpr_band: (3.0, 6.0),
        k1: 0.88,
        k2: -0.016,
        k3: -0.3,
        k4: 0.0,
        s: 0.7,
    },
    HoweRow {
        bpr_band: (8.0, 8.0),
        k1: 0.89,
        k2: -0.014,
        k3: -0.3,
        k4: 0.005,
        s: 0.7,
    },
];

/// Lowest Mach number of Howe's high-speed coefficient rows.
pub(super) const HOWE_MINIMUM_MACH: f64 = 0.4;
/// Highest Mach number for which Howe gives the relation.
pub(super) const HOWE_MAXIMUM_MACH: f64 = 0.9;
/// Tropopause altitude bounding the `sigma^s` exponent Howe tabulates, m
/// (US Standard Atmosphere 1976).
pub(super) const HOWE_MAXIMUM_ALTITUDE_M: f64 = 11_000.0;

/// Howe's maximum-thrust ratio `F / F_SLS` of one coefficient row.
fn row_ratio(row: HoweRow, bpr: f64, mach: f64, sigma: f64) -> f64 {
    (row.k1 + row.k2 * bpr + (row.k3 + row.k4 * bpr) * mach) * sigma.powf(row.s)
}

/// Howe's maximum-thrust ratio `F / F_SLS` at `bpr`, `mach` and density
/// ratio `sigma` (0.4 <= M <= 0.9, below 11 km).
///
/// Inside a row's bypass-ratio band the row is used as tabulated (each row
/// is already linear in BPR). Howe gives no row between the bands (BPR 1-3
/// and 6-8); there the two neighbouring row results are blended linearly in
/// BPR, so the ratio is continuous in bypass ratio. Below BPR 1 and above
/// BPR 8 the nearest row is extrapolated through its own BPR terms.
pub(super) fn howe_maximum_thrust_ratio(bpr: f64, mach: f64, sigma: f64) -> f64 {
    let rows = HOWE_HIGH_MACH_ROWS;
    if bpr <= rows[0].bpr_band.1 {
        return row_ratio(rows[0], bpr, mach, sigma);
    }
    for pair in rows.windows(2) {
        let (low, high) = (pair[0], pair[1]);
        if bpr <= low.bpr_band.1 {
            return row_ratio(low, bpr, mach, sigma);
        }
        if bpr < high.bpr_band.0 {
            let weight = (bpr - low.bpr_band.1) / (high.bpr_band.0 - low.bpr_band.1);
            return (1.0 - weight) * row_ratio(low, bpr, mach, sigma)
                + weight * row_ratio(high, bpr, mach, sigma);
        }
    }
    row_ratio(rows[2], bpr, mach, sigma)
}
