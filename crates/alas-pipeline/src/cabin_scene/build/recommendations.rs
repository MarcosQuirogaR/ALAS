// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Section stations recommended from the laid-out deck items.
//!
//! A cabin station is ranked by how many seat rows and overhead runs it
//! cuts through, then by how few monuments and exits, then by closeness to
//! the occupied extent's midpoint; a hold station by occupied cargo overlap.

use alas_payload::layout::{DeckItem, ItemKind};

use crate::cabin_scene::RecommendedSection;

pub(super) fn recommended_sections(items: &[DeckItem]) -> Vec<RecommendedSection> {
    let mut result = Vec::new();
    for deck in [alas_payload::layout::MAIN, alas_payload::layout::UPPER] {
        if let Some(x) = best_cabin_station(items, deck) {
            // A deck the fitting solver gave no overhead runs cannot offer a
            // station that intersects them; the purpose says what the station
            // actually maximizes rather than claiming an overhead overlap.
            let has_overhead = items
                .iter()
                .any(|item| item.deck == deck && item.kind == ItemKind::OverheadBin);
            let (purpose, selection) = if has_overhead {
                (
                    "occupied_cabin_with_overhead",
                    "maximizes simultaneous seat-row and overhead-run interval overlap",
                )
            } else {
                (
                    "occupied_cabin",
                    "maximizes seat-row interval overlap; the deck has no overhead runs",
                )
            };
            result.push(RecommendedSection {
                station_id: String::new(),
                purpose: purpose.into(),
                deck_id: deck.into(),
                x_m: x,
                intersects: intersecting_kinds(items, deck, x),
                selection: selection.into(),
            });
        }
    }
    if let Some(x) = best_cargo_station(items) {
        result.push(RecommendedSection {
            station_id: String::new(),
            purpose: "occupied_hold".into(),
            deck_id: alas_payload::layout::LOWER.into(),
            x_m: x,
            intersects: intersecting_kinds(items, alas_payload::layout::LOWER, x),
            selection: "maximizes occupied cargo-item interval overlap".into(),
        });
    }
    result.sort_by(|a, b| a.x_m.total_cmp(&b.x_m));
    result
}

fn best_cabin_station(items: &[DeckItem], deck: &str) -> Option<f64> {
    let candidates: Vec<f64> = items
        .iter()
        .filter(|item| item.deck == deck && item.kind == ItemKind::SeatRow)
        .map(|item| item.x)
        .collect();
    let midpoint = extent_midpoint(items, deck, |kind| kind == ItemKind::SeatRow)?;
    candidates.into_iter().min_by(|a, b| {
        cabin_rank(items, deck, *a, midpoint)
            .partial_cmp(&cabin_rank(items, deck, *b, midpoint))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn best_cargo_station(items: &[DeckItem]) -> Option<f64> {
    let candidates: Vec<f64> = items
        .iter()
        .filter(|item| {
            item.deck == alas_payload::layout::LOWER
                && matches!(item.kind, ItemKind::Uld | ItemKind::Bag)
        })
        .map(|item| item.x)
        .collect();
    let midpoint = extent_midpoint(items, alas_payload::layout::LOWER, |kind| {
        matches!(kind, ItemKind::Uld | ItemKind::Bag)
    })?;
    candidates.into_iter().min_by(|a, b| {
        cargo_rank(items, *a, midpoint)
            .partial_cmp(&cargo_rank(items, *b, midpoint))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn cabin_rank(items: &[DeckItem], deck: &str, x: f64, midpoint: f64) -> (i64, i64, i64) {
    let seats = overlap_count(items, deck, x, |kind| kind == ItemKind::SeatRow);
    let bins = overlap_count(items, deck, x, |kind| kind == ItemKind::OverheadBin);
    let obstacles = overlap_count(items, deck, x, |kind| {
        matches!(
            kind,
            ItemKind::Exit
                | ItemKind::Galley
                | ItemKind::Lav
                | ItemKind::AccessibleLav
                | ItemKind::WheelchairStowage
        )
    });
    (
        -((seats + bins) as i64),
        obstacles as i64,
        ((x - midpoint).abs() * 1_000_000.0).round() as i64,
    )
}

fn cargo_rank(items: &[DeckItem], x: f64, midpoint: f64) -> (i64, i64) {
    let overlap = overlap_count(items, alas_payload::layout::LOWER, x, |kind| {
        matches!(kind, ItemKind::Uld | ItemKind::Bag)
    });
    (
        -(overlap as i64),
        ((x - midpoint).abs() * 1_000_000.0).round() as i64,
    )
}

fn extent_midpoint(items: &[DeckItem], deck: &str, kind: impl Fn(ItemKind) -> bool) -> Option<f64> {
    let mut x0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    for item in items
        .iter()
        .filter(|item| item.deck == deck && kind(item.kind))
    {
        x0 = x0.min(item.x - item.length * 0.5);
        x1 = x1.max(item.x + item.length * 0.5);
    }
    (x0.is_finite() && x1.is_finite()).then_some((x0 + x1) * 0.5)
}

fn overlap_count(items: &[DeckItem], deck: &str, x: f64, kind: impl Fn(ItemKind) -> bool) -> usize {
    items
        .iter()
        .filter(|item| {
            item.deck == deck && kind(item.kind) && (item.x - x).abs() <= item.length * 0.5 + 1e-9
        })
        .count()
}

fn intersecting_kinds(items: &[DeckItem], deck: &str, x: f64) -> Vec<String> {
    let mut kinds: Vec<String> = items
        .iter()
        .filter(|item| item.deck == deck && (item.x - x).abs() <= item.length * 0.5 + 1e-9)
        .map(|item| item.kind.as_str().to_owned())
        .collect();
    kinds.sort();
    kinds.dedup();
    kinds
}
