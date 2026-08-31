#![doc = "Audit the materialized passenger load case and cabin capacity for registered presets."]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::cabin::{cabin_deck_segments, max_certifiable_capacity, resolve_aisle_width};
use alas_payload::{
    apply_cabin_preset, build_payload_layout, CabinGeometry, ItemMeta, LayoutSummary,
};

fn main() {
    for name in std::env::args().skip(1).collect::<Vec<_>>() {
        audit(&name);
    }
}

fn audit(name: &str) {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
        .expect("registered preset loads");
    let preset = presets::get(name).expect("registered preset resolves");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry builds");
    let geometry = CabinGeometry::new(
        &plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .expect("cabin geometry builds");
    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).expect("layout builds");
    let mut materialized = config.clone();
    apply_cabin_preset(&mut materialized, Some(&preset.design_vector))
        .expect("cabin preset materializes");
    let materialized_counts = materialized
        .cabin
        .passenger
        .all_classes()
        .map(|(_, class)| class.count);
    let mut fixed = materialized.cabin.passenger.clone();
    fixed.set_fixed_passenger_count(config.requirements.num_passengers);
    let mut economy_only = materialized.cabin.passenger.clone();
    economy_only.first.count = 0;
    economy_only.business.count = 0;
    economy_only.premium.count = 0;
    economy_only.economy.count = config.requirements.num_passengers;
    let economy_layout =
        alas_payload::build_passenger_layout(&geometry, &economy_only, &config.requirements);
    let LayoutSummary::Passenger(economy_summary) = economy_layout.summary else {
        unreachable!();
    };
    let LayoutSummary::Passenger(summary) = layout.summary else {
        println!("{name}: cargo");
        return;
    };
    let seat_rows = layout
        .items
        .iter()
        .filter_map(|item| match &item.meta {
            ItemMeta::Seat(meta) => Some((item.x, meta.cls, meta.abreast, meta.filled)),
            _ => None,
        })
        .collect::<Vec<_>>();
    println!(
        "{name}: requested={} seated={} unseated={} classes={:?} materialized_counts={:?} fixed_counts={:?} economy_only_seated={} payload_kg={:.1} payload_cg_x={:.3} cg_pct_mac={:.3} geometry=(cabin {:.3}..{:.3}, fus {:.3}, diameter {:.3}, mac {:.3}, lemac {:.3}, wing_ac {:.3}) decks={:?} caps={:?} aisle={:.3} rows={} first_rows={:?}",
        summary.total_pax,
        summary.seated_pax,
        summary.unseated_pax,
        summary.classes,
        materialized_counts,
        fixed.all_classes().map(|(_, class)| class.count),
        economy_summary.seated_pax,
        layout.total_mass,
        layout.cg_x,
        summary.cg_pct_mac,
        geometry.cabin_start_x,
        geometry.cabin_end_x,
        geometry.fus_len,
        geometry.diameter_m,
        geometry.mac,
        geometry.x_lemac,
        geometry.x_wing_ac,
        cabin_deck_segments(&geometry)
            .iter()
            .map(|segment| (segment.deck.name, segment.x0, segment.x1))
            .collect::<Vec<_>>(),
        max_certifiable_capacity(&geometry, &config.cabin.passenger),
        resolve_aisle_width(&config.cabin.passenger, summary.total_pax),
        seat_rows.len(),
        &seat_rows[..seat_rows.len().min(12)],
    );
}
