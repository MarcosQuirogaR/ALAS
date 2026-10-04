// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Splitting a configured wing cell into the spanwise bays that become tank
//! pairs: the whole cell, or its transfer remainder and the engine feed tank
//! the cell declares at one of its ends.

use alas_config::WingTankConfig;

use super::types::TankKind;

/// One spanwise bay of a wing cell that becomes a tank pair.
pub(super) struct WingBay {
    pub(super) kind: TankKind,
    pub(super) id_stem: String,
    /// Fractions of the semispan from the root section.
    pub(super) span_start_fraction: f64,
    pub(super) span_end_fraction: f64,
    pub(super) usable_fraction: f64,
    pub(super) burn_priority: i64,
    /// Both sides together, L.
    pub(super) published_usable_volume_l: Option<f64>,
}

/// The bays of one enabled wing cell. A feed tank takes its own interval
/// and published volume; the transfer tank keeps the rest of the cell's
/// span and the rest of its published volume (the configuration validates
/// that the feed tank shares one end of the cell and is a strict part of
/// its volume).
pub(super) fn wing_bays(kind: TankKind, cell: &WingTankConfig) -> Vec<WingBay> {
    let whole = WingBay {
        kind,
        id_stem: kind.id_prefix().to_owned(),
        span_start_fraction: cell.span_start_fraction,
        span_end_fraction: cell.span_end_fraction,
        usable_fraction: cell.usable_fraction,
        burn_priority: cell.burn_priority,
        published_usable_volume_l: cell.published_usable_volume_l,
    };
    let feed = &cell.feed;
    if !feed.enabled {
        return vec![whole];
    }
    let (transfer_start, transfer_end) = if feed.span_start_fraction <= cell.span_start_fraction {
        (feed.span_end_fraction, cell.span_end_fraction)
    } else {
        (cell.span_start_fraction, feed.span_start_fraction)
    };
    let host = kind.id_prefix().trim_start_matches("wing_");
    vec![
        WingBay {
            span_start_fraction: transfer_start,
            span_end_fraction: transfer_end,
            published_usable_volume_l: cell
                .published_usable_volume_l
                .zip(feed.published_usable_volume_l)
                .map(|(cell_l, feed_l)| cell_l - feed_l),
            ..whole
        },
        WingBay {
            kind: TankKind::WingFeed,
            id_stem: format!("{}_{host}", TankKind::WingFeed.id_prefix()),
            span_start_fraction: feed.span_start_fraction,
            span_end_fraction: feed.span_end_fraction,
            usable_fraction: cell.usable_fraction,
            burn_priority: cell.burn_priority,
            published_usable_volume_l: feed.published_usable_volume_l,
        },
    ]
}
