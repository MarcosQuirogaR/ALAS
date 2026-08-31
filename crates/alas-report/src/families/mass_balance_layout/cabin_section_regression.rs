use super::*;
use alas_config::presets;
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::build_payload_layout;

#[test]
fn preset_geometry_regression_for_seats_windows_and_hold() {
    for name in ["A220-300", "A320-200", "A380-800"] {
        let preset = presets::get(name).expect("preset");
        let cfg = AlasConfig::from_value(&serde_json::json!({ "preset": name })).expect("config");
        let plane = AircraftBuilder::new(Some(cfg.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("aircraft");
        let cabin = CabinGeometry::new(&plane, &cfg.geometry, cfg.cabin.passenger.wall_thickness_m)
            .expect("cabin");
        let layout = build_payload_layout(&plane, &cfg, 0.0, 0.0).expect("layout");
        let mid = (cabin.cabin_start_x + cabin.cabin_end_x) * 0.5;
        let x = representative_cabin_station(&layout, mid);
        let available_bin_stations = layout
            .items
            .iter()
            .filter(|row| {
                row.kind == ItemKind::SeatRow
                    && layout.items.iter().any(|bin| {
                        bin.kind == ItemKind::OverheadBin
                            && bin.deck == row.deck
                            && asset_for_item(bin).is_some_and(|asset| asset.intersects(row.x))
                    })
            })
            .count();
        if available_bin_stations > 0 {
            assert!(
                layout.items.iter().any(|bin| {
                    bin.kind == ItemKind::OverheadBin
                        && asset_for_item(bin).is_some_and(|asset| asset.intersects(x))
                }),
                "{name} representative station missed installed bins"
            );
        }
        for deck in &cabin.passenger_decks {
            let floor = cabin.floor_z(deck, x);
            let wz = art::window_center_z(floor, cabin.height_at(x), cabin.zc_at(x));
            assert!(
                (wz - floor - 0.95).abs() < 1e-9
                    || (wz - cabin.zc_at(x)).abs() <= cabin.height_at(x) * 0.40 + 1e-9,
                "{name} window band"
            );
            if let Some(item) = row(&layout, deck.name, x) {
                if let ItemMeta::Seat(meta) = &item.meta {
                    let half = [floor + 0.59, floor + 1.10]
                        .into_iter()
                        .map(|z| half_width(&cabin, x, z, z) - 0.025)
                        .fold(f64::INFINITY, f64::min);
                    let placed = constrained_centers(meta, half);
                    assert!(placed.side_gap >= 0.025 - 1e-9, "{name} side gap");
                    assert!(
                        placed
                            .seats
                            .iter()
                            .all(|y| y.abs() + placed.seat_width * 0.504 <= half + 1e-9),
                        "{name} containment"
                    );
                }
            }
        }
        let hx = hold_station(&layout, x);
        let lower = &cabin.lower_deck;
        let hold_half =
            cabin.usable_width_at_z(hx, cabin.floor_z(lower, hx)) * lower.width_factor * 0.5;
        for item in layout
            .items
            .iter()
            .filter(|i| i.deck == LOWER && intersects(i, hx))
        {
            if let Some(asset) = asset_for_item(item) {
                assert!(
                    asset
                        .profile_yz
                        .iter()
                        .all(|p| p[0].abs() <= hold_half + 1e-6),
                    "{name} ULD exceeds own-station hold width"
                );
            }
        }
    }
}
