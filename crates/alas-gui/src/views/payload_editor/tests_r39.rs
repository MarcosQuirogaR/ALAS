// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The zone properties on the form's label-over-box grid, rendered at the widths of the
//! review (the page's right column and a wide window).

use alas_config::{FloorZoneConfig, PayloadItemsConfig, ZoneDeck, ZoneKind};

use super::canvas::Target;
use crate::test_raster::{overlapping_texts, render, write_in};

#[test]
fn zone_properties_share_equal_boxes_and_never_overlap() {
    for width in [330.0_f32, 560.0, 1_100.0] {
        let mut items = PayloadItemsConfig::default();
        items.zones.push(FloorZoneConfig {
            name: "Zone 1".to_owned(),
            kind: ZoneKind::Custom,
            deck: ZoneDeck::Main,
            x_start_m: 57.4,
            x_end_m: 62.72,
            width_m: None,
            y_center_m: 0.0,
            furnishing_kg_per_m2: 0.0,
            total_mass_kg: None,
            occupants: 0,
            carve_seat_floor: true,
            movable: false,
            x_range_m: None,
        });
        let shot = render(width, 420.0, 4, 0, |ui| {
            crate::theme::card_frame(ui).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                super::props::show(ui, &mut items, None, Some(&Target::Zone(0)), false);
            });
        });
        write_in(
            &crate::test_raster::render_dir(),
            &format!("zone-editor-{width:.0}.png"),
            &shot,
        );
        assert!(
            overlapping_texts(&shot).is_empty(),
            "{width}: {:?}",
            overlapping_texts(&shot)
        );
    }
}
