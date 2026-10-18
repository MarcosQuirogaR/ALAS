// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Chart data of Roskam, *Airplane Design Part VI* for the flap and aileron
//! lift increments of [`crate::torque_sources`], read off the figures.

/// Single-slotted flap lift effectiveness `alpha_delta`, Roskam Part VI
/// Fig. 8.17 (book p. 230): rows are `c_f/c` = 0.15, 0.20, 0.25, 0.30, 0.40,
/// columns the deflection 0, 10, 20, 30, 40, 50, 60, 70, 80 deg. Read from the
/// chart, about +-0.02.
pub(crate) fn alpha_delta(chord_fraction: f64, deflection_deg: f64) -> f64 {
    const CF: [f64; 5] = [0.15, 0.20, 0.25, 0.30, 0.40];
    const DEG: [f64; 9] = [0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0];
    const TABLE: [[f64; 9]; 5] = [
        [
            0.370, 0.370, 0.350, 0.310, 0.270, 0.195, 0.150, 0.145, 0.140,
        ],
        [
            0.450, 0.445, 0.425, 0.385, 0.330, 0.250, 0.190, 0.175, 0.170,
        ],
        [
            0.515, 0.510, 0.490, 0.455, 0.400, 0.305, 0.215, 0.200, 0.195,
        ],
        [
            0.555, 0.550, 0.530, 0.490, 0.440, 0.340, 0.245, 0.220, 0.205,
        ],
        [
            0.600, 0.600, 0.585, 0.545, 0.495, 0.400, 0.290, 0.250, 0.230,
        ],
    ];
    interp2(&CF, &DEG, &TABLE, chord_fraction, deflection_deg)
}

/// Plain-flap non-linearity `k'`, Roskam Part VI Fig. 8.13 (book p. 228):
/// rows `c_f/c` = 0.10, 0.20, 0.30, 0.50, columns 10, 15, 20, 30, 40, 50, 60
/// deg; 1.0 at or below 10 deg. Chart-read, about +-0.02.
pub(crate) fn k_prime(chord_fraction: f64, deflection_deg: f64) -> f64 {
    const CF: [f64; 4] = [0.10, 0.20, 0.30, 0.50];
    const DEG: [f64; 7] = [10.0, 15.0, 20.0, 30.0, 40.0, 50.0, 60.0];
    const TABLE: [[f64; 7]; 4] = [
        [1.0, 0.985, 0.91, 0.715, 0.655, 0.600, 0.565],
        [1.0, 0.960, 0.86, 0.670, 0.600, 0.540, 0.505],
        [1.0, 0.930, 0.78, 0.610, 0.545, 0.500, 0.460],
        [1.0, 0.880, 0.70, 0.545, 0.490, 0.455, 0.425],
    ];
    if deflection_deg <= 10.0 {
        return 1.0;
    }
    interp2(&CF, &DEG, &TABLE, chord_fraction, deflection_deg)
}

fn interp1(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    let x = x.clamp(xs[0], xs[xs.len() - 1]);
    let i = xs.partition_point(|&v| v <= x).clamp(1, xs.len() - 1) - 1;
    let t = (x - xs[i]) / (xs[i + 1] - xs[i]);
    ys[i] * (1.0 - t) + ys[i + 1] * t
}

fn interp2<const R: usize, const C: usize>(
    rows: &[f64; R],
    cols: &[f64; C],
    table: &[[f64; C]; R],
    row: f64,
    col: f64,
) -> f64 {
    let along: Vec<f64> = table.iter().map(|r| interp1(cols, r, col)).collect();
    interp1(rows, &along, row)
}
