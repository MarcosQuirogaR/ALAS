// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Normal-stress recovery and combined section-material margins.

use super::SparStressResult;
use crate::sizing::{section::stress_utilization, WingboxSizing};
use alas_config::materials::MaterialSpec;

// Resultants and three distinct materials are necessary to recover a mixed
// section; the boolean retains only the explicit frozen fixture convention.
#[allow(clippy::too_many_arguments)]
pub(super) fn recover(
    sizing: &WingboxSizing,
    moments: &[f64],
    shears: &[f64],
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
    product: bool,
) -> Vec<SparStressResult> {
    sizing
        .spars
        .iter()
        .enumerate()
        .map(|(index, spar)| {
            let values: Vec<_> = moments
                .iter()
                .enumerate()
                .map(|(station, moment)| {
                    if product {
                        stress_utilization(
                            sizing,
                            station,
                            index,
                            *moment,
                            shears[station],
                            skin,
                            web,
                            cap,
                        )
                    } else {
                        let stress = (spar.frac_moment[station] * moment).abs()
                            / (spar.a_cap[station] * spar.h[station] * 0.85).max(1.0e-12);
                        let ratio = if (spar.frac_moment[station] * moment).abs() > 1.0 {
                            stress.max(1.0e-9) / cap.f_allow_pa
                        } else {
                            0.0
                        };
                        (stress, ratio)
                    }
                })
                .collect();
            SparStressResult {
                chord_fraction: spar.chord_fraction,
                stress_pa: values.iter().map(|(stress, _)| *stress).collect(),
                margin_of_safety: values
                    .iter()
                    .map(|(_, ratio)| {
                        if *ratio > 0.0 {
                            1.0 / ratio - 1.0
                        } else {
                            f64::INFINITY
                        }
                    })
                    .collect(),
            }
        })
        .collect()
}
