// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The class mix of a cabin flown without cabin crew.

use alas_config::AlasConfig;

/// `mix` as the cabin of `config` is laid out. Its seats are the registered
/// source capacity `source_capacity_cap` where one is declared, otherwise the
/// requested passengers. At most 19 seats fly without cabin crew (EASA
/// ORO.CC.100(a)), so there are no classes to serve apart: every seat takes
/// the class with the largest share. A class boundary also costs a 0.95 m
/// monument bay, which so short a cabin cannot spare: the 5.38 m LJ60 cabin
/// seated one of its eight passengers on the default 15/85 business/economy
/// shares.
pub(super) fn single_class_without_cabin_crew(
    mix: Vec<(&'static str, f64)>,
    config: &AlasConfig,
    source_capacity_cap: Option<i64>,
) -> Vec<(&'static str, f64)> {
    let seats = source_capacity_cap
        .filter(|cap| *cap > 0)
        .unwrap_or(config.requirements.num_passengers);
    let crewless = usize::try_from(seats).is_ok_and(|seats| {
        (1..=alas_config::presets::MAX_SEATS_WITHOUT_CABIN_CREW).contains(&seats)
    });
    if !crewless || mix.len() < 2 {
        return mix;
    }
    let largest = mix.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1));
    largest.map_or(mix, |(name, _)| vec![(name, 1.0)])
}
