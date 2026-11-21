// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Span-fraction and feed-tank validation helpers for the fuel tank layout.

use super::WingTankConfig;

/// A feed tank must be a strict sub-interval of its host cell sharing one of
/// its ends, so the transfer remainder is one contiguous bay; a published
/// feed volume must be a strict part of a published host volume, and a
/// published host cannot be split without one.
pub(super) fn validate_feed(name: &str, cell: &WingTankConfig) -> Result<(), String> {
    let feed = &cell.feed;
    if !feed.enabled {
        return Ok(());
    }
    validate_interval(name, feed.span_start_fraction, feed.span_end_fraction)?;
    let at_inboard_end = feed.span_start_fraction == cell.span_start_fraction
        && feed.span_end_fraction < cell.span_end_fraction;
    let at_outboard_end = feed.span_end_fraction == cell.span_end_fraction
        && feed.span_start_fraction > cell.span_start_fraction;
    // A cell whose every bay feeds an engine (the 747-400 inboard mains).
    let whole_cell = feed.span_start_fraction == cell.span_start_fraction
        && feed.span_end_fraction == cell.span_end_fraction;
    if !(at_inboard_end || at_outboard_end || whole_cell) {
        return Err(format!(
            "{name} feed tank must lie inside the cell and share exactly one of its ends, or fill it"
        ));
    }
    match (feed.published_usable_volume_l, cell.published_usable_volume_l) {
        (None, None) => Ok(()),
        (Some(feed_l), Some(cell_l)) if whole_cell && feed_l.is_finite() && feed_l == cell_l => Ok(()),
        _ if whole_cell => Err(format!(
            "{name} feed tank that fills the cell must carry the cell's published volume"
        )),
        (Some(feed_l), Some(cell_l)) if feed_l.is_finite() && feed_l > 0.0 && feed_l < cell_l => {
            Ok(())
        }
        _ => Err(format!(
            "{name} feed tank published volume must be a positive part of the cell's published volume"
        )),
    }
}

pub(super) fn validate_interval(name: &str, start: f64, end: f64) -> Result<(), String> {
    if !start.is_finite() || !end.is_finite() || !(0.0..=1.0).contains(&start) || end > 1.0 {
        return Err(format!("{name} span fractions must lie in [0, 1]"));
    }
    if end <= start {
        return Err(format!("{name} span end must lie outboard of its start"));
    }
    Ok(())
}

pub(super) fn validate_fraction(name: &str, field: &str, value: f64) -> Result<(), String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(format!("{name} {field} must lie in [0, 1]"));
    }
    Ok(())
}
