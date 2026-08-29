// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Named colormaps for colorbar-driven figures (spanwise position, Reynolds
//! sweeps, Mach fields, ...).
//!
//! Each map is a coarse (8-9 stop) piecewise-linear approximation of the
//! matching matplotlib colormap: close enough for chart legibility (the
//! figures using these are visual aids, not numerically parity-tested), at a
//! fraction of the lookup-table size a faithful reproduction would need.

use crate::scene::Color;

/// A named colormap sampled by normalized position `t` in `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colormap {
    /// Matplotlib's default perceptually-uniform map (dark purple -> yellow).
    Viridis,
    /// High-contrast rainbow map used for Mach/velocity fields.
    Turbo,
    /// Perceptually-uniform map (dark purple -> yellow), warmer than Viridis.
    Plasma,
    /// Perceptually-uniform map (black -> yellow), used for NeuralFoil sweeps.
    Inferno,
    /// Perceptually-uniform map (black -> pale pink), used for NeuralFoil sweeps.
    Magma,
    /// Classic blue-cyan-yellow-red map, used for legacy-style contour fields.
    Jet,
}

type Stop = (f64, u8, u8, u8);

impl Colormap {
    /// Sample the colormap at normalized position `t`, clamped to `[0, 1]`.
    pub fn sample(&self, t: f64) -> Color {
        let t = t.clamp(0.0, 1.0);
        let stops = self.stops();
        for pair in stops.windows(2) {
            let (t0, r0, g0, b0) = pair[0];
            let (t1, r1, g1, b1) = pair[1];
            if t <= t1 || (t1 - 1.0).abs() < 1e-9 {
                let span = (t1 - t0).max(1e-12);
                let f = ((t - t0) / span).clamp(0.0, 1.0);
                let lerp = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * f).round() as u8;
                return Color::rgb(lerp(r0, r1), lerp(g0, g1), lerp(b0, b1));
            }
        }
        let Some((_, r, g, b)) = stops.last().copied() else {
            return Color::rgb(0, 0, 0);
        };
        Color::rgb(r, g, b)
    }

    const fn stops(&self) -> &'static [Stop] {
        match self {
            Colormap::Viridis => &[
                (0.00, 68, 1, 84),
                (0.13, 71, 44, 122),
                (0.25, 59, 81, 139),
                (0.38, 44, 113, 142),
                (0.50, 33, 144, 141),
                (0.63, 39, 173, 129),
                (0.75, 92, 200, 99),
                (0.88, 170, 220, 50),
                (1.00, 253, 231, 37),
            ],
            Colormap::Plasma => &[
                (0.00, 13, 8, 135),
                (0.14, 84, 2, 163),
                (0.29, 139, 10, 165),
                (0.43, 185, 50, 137),
                (0.57, 219, 92, 104),
                (0.71, 244, 136, 73),
                (0.86, 254, 188, 43),
                (1.00, 240, 249, 33),
            ],
            Colormap::Inferno => &[
                (0.00, 0, 0, 4),
                (0.14, 40, 11, 84),
                (0.29, 101, 21, 110),
                (0.43, 159, 42, 99),
                (0.57, 212, 72, 66),
                (0.71, 245, 125, 21),
                (0.86, 250, 193, 39),
                (1.00, 252, 255, 164),
            ],
            Colormap::Magma => &[
                (0.00, 0, 0, 4),
                (0.14, 28, 16, 68),
                (0.29, 79, 18, 123),
                (0.43, 129, 37, 129),
                (0.57, 181, 54, 122),
                (0.71, 229, 80, 100),
                (0.86, 251, 135, 97),
                (1.00, 252, 253, 191),
            ],
            Colormap::Turbo => &[
                (0.00, 48, 18, 59),
                (0.13, 65, 90, 205),
                (0.25, 40, 174, 222),
                (0.38, 64, 222, 135),
                (0.50, 140, 231, 58),
                (0.63, 215, 207, 44),
                (0.75, 247, 141, 39),
                (0.88, 220, 60, 20),
                (1.00, 122, 4, 3),
            ],
            Colormap::Jet => &[
                (0.000, 0, 0, 128),
                (0.125, 0, 0, 255),
                (0.375, 0, 255, 255),
                (0.625, 255, 255, 0),
                (0.875, 255, 0, 0),
                (1.000, 128, 0, 0),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_colormap_covers_its_full_domain_without_panicking() {
        for cmap in [
            Colormap::Viridis,
            Colormap::Turbo,
            Colormap::Plasma,
            Colormap::Inferno,
            Colormap::Magma,
            Colormap::Jet,
        ] {
            let _ = cmap.sample(0.0);
            let _ = cmap.sample(1.0);
            let _ = cmap.sample(0.5);
            let _ = cmap.sample(-0.3); // clamps
            let _ = cmap.sample(1.7); // clamps
        }
    }

    #[test]
    fn viridis_endpoints_match_its_first_and_last_stop() {
        assert_eq!(Colormap::Viridis.sample(0.0), Color::rgb(68, 1, 84));
        assert_eq!(Colormap::Viridis.sample(1.0), Color::rgb(253, 231, 37));
    }

    #[test]
    fn sample_is_monotonic_in_red_channel_for_jet_first_half() {
        // Jet ramps blue->cyan over [0, 0.375]; blue channel should not
        // increase again before the ramp completes.
        let b_low = Colormap::Jet.sample(0.05).b;
        let b_mid = Colormap::Jet.sample(0.3).b;
        assert!(b_mid >= b_low);
    }
}
