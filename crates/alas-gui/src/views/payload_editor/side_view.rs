// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The payload side view: the built fuselage section by station, the level
//! floors, the cabin and hold volumes, and the placed items, all in aircraft
//! body axes (x aft from the nose, z up, metres).
//!
//! [`SideView`] is plain sampled data so the placement rules can be tested
//! without building an aircraft; [`build`] fills it from the same geometry the
//! payload layout is built on.

use alas_config::AlasConfig;
use alas_payload::{
    build::{build_payload_layout, product_cabin_geometry},
    layout::{DeckItem, ItemKind, ItemMeta},
    CabinGeometry,
};

use crate::state::AppState;

/// Samples along the body.
const STATIONS: usize = 160;

/// One placed item as drawn.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ViewItem {
    pub kind: ItemKind,
    pub label: String,
    /// Declared point-mass id or zone name; empty for built items.
    pub declared: String,
    pub x: f64,
    pub z: f64,
    pub length: f64,
    pub height: f64,
    pub mass: f64,
    /// Lateral centre and extent, m (the front view draws the seats from them).
    pub y: f64,
    pub width: f64,
    /// A seat row's lateral division.
    pub seat: Option<SeatBlocks>,
}

/// How a seat row is divided across the cabin.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SeatBlocks {
    /// Seats per lateral block, outboard to outboard.
    pub blocks: Vec<i64>,
    pub seat_w: f64,
    pub aisle_w: f64,
    pub floor_z: f64,
    /// The class the row belongs to (picks its legend colour).
    pub cls: &'static str,
}

/// What the payload auto-balance did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BalanceView {
    pub target_x: f64,
    pub reached_x: f64,
    pub saturated: bool,
    pub settled: bool,
}

/// The generated side view.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct SideView {
    pub stations: Vec<f64>,
    pub top: Vec<f64>,
    pub bottom: Vec<f64>,
    pub main_floor: Vec<f64>,
    pub main_ceiling: Vec<f64>,
    /// Hold floor and ceiling; equal to each other where there is no hold.
    pub hold_floor: Vec<f64>,
    pub hold_ceiling: Vec<f64>,
    /// The stations the cabin occupies, forward to aft.
    pub cabin_x: [f64; 2],
    /// The wing box gap the hold cannot occupy.
    pub hold_gap_x: Option<[f64; 2]>,
    /// Declared lower-deck compartments (`cabin.cargo.hold_compartments`),
    /// which replace the derived hold when the list is not empty.
    pub hold_spans: Vec<[f64; 2]>,
    pub declared_holds: bool,
    pub has_hold: bool,
    pub items: Vec<ViewItem>,
    pub cg_x: f64,
    pub x_lemac: f64,
    pub mac: f64,
    pub balance: Option<BalanceView>,
    /// The cabin's target CG, % MAC, when it sets one.
    pub target_pct: Option<f64>,
    /// Zero-fuel centre of gravity (operating empty mass plus payload), m;
    /// `None` when the mass estimate is unavailable.
    pub zero_fuel_cg: Option<f64>,
    /// Operating empty mass estimate, kg, shown beside the total payload;
    /// `None` when the mass estimate is unavailable.
    pub oew_kg: Option<f64>,
    /// Whether the payload is the custom one (zones and point masses only).
    pub custom_only: bool,
    /// Whether the masses are the pre-run estimate, not the last run's.
    pub estimate: bool,
    /// Door pair stations on the main deck, m.
    pub doors: Vec<f64>,
    /// Main-deck stretches reserved by declared compartments, m.
    pub reserved: Vec<[f64; 2]>,
    /// Outer fuselage width at each station, m.
    pub widths: Vec<f64>,
    /// Usable main-deck floor width at each station, m.
    pub main_width: Vec<f64>,
    /// Main wing leading-edge points `[x, y, z]`, root to tip, `y >= 0`, m.
    pub wing_le: Vec<[f64; 3]>,
    /// Main wing root chord, m.
    pub wing_chord: f64,
    /// Payload mass of the layout, kg.
    pub payload_mass: f64,
    /// Mass an occupant and baggage carry (`requirements.passenger_mass_kg`), kg.
    pub occupant_kg: f64,
    /// Forward and aft centre-of-gravity limits, m: the aerodynamic aft limit
    /// (neutral point less the target static margin) and the forward one a
    /// CG-envelope width ahead of it. `None` without a neutral point.
    pub cg_limits: Option<[f64; 2]>,
    /// Highest leading-edge point of any lifting surface (the fin's tip), m:
    /// the front view frames the whole aircraft up to it. Not an envelope.
    pub tail_top_z: Option<f64>,
    /// Every lifting-surface panel in side projection, `[x, z]` leading edge then trailing edge at
    /// the inboard section, then trailing then leading edge at the outboard one, m: the silhouette
    /// a label placed above the body must keep clear of (the fin).
    pub surfaces: Vec<[[f64; 2]; 4]>,
    /// The airframe this view was sampled from, so the front view's render
    /// is the same aircraft as the section drawn over it.
    pub airframe: Option<std::sync::Arc<Airframe>>,
}

/// The inputs the aircraft was built from (cabin preset applied).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Airframe {
    pub geometry: alas_config::GeometryConfig,
    pub design: alas_config::DesignVector,
    /// Imported airfoils change the loft without changing their names.
    pub airfoil_generation: u64,
}

impl SideView {
    pub(super) fn at(&self, series: &[f64], x: f64) -> f64 {
        let n = self.stations.len();
        if n == 0 || series.len() != n {
            return 0.0;
        }
        if x <= self.stations[0] {
            return series[0];
        }
        if x >= self.stations[n - 1] {
            return series[n - 1];
        }
        let upper = self.stations.partition_point(|&s| s < x).clamp(1, n - 1);
        let (x0, x1) = (self.stations[upper - 1], self.stations[upper]);
        let t = if x1 > x0 { (x - x0) / (x1 - x0) } else { 0.0 };
        series[upper - 1] + t * (series[upper] - series[upper - 1])
    }

    pub(super) fn floor_z(&self, x: f64) -> f64 {
        self.at(&self.main_floor, x)
    }

    pub(super) fn ceiling_z(&self, x: f64) -> f64 {
        self.at(&self.main_ceiling, x)
    }

    pub(super) fn hold_floor_z(&self, x: f64) -> f64 {
        self.at(&self.hold_floor, x)
    }

    pub(super) fn hold_ceiling_z(&self, x: f64) -> f64 {
        self.at(&self.hold_ceiling, x)
    }

    #[cfg(test)]
    pub(super) fn top_z(&self, x: f64) -> f64 {
        self.at(&self.top, x)
    }

    #[cfg(test)]
    pub(super) fn bottom_z(&self, x: f64) -> f64 {
        self.at(&self.bottom, x)
    }

    /// The percent MAC of station `x`.
    pub(super) fn pct_mac(&self, x: f64) -> f64 {
        100.0 * (x - self.x_lemac) / self.mac.max(1.0e-6)
    }

    /// The x range of the whole body.
    pub(super) fn x_range(&self) -> [f64; 2] {
        match (self.stations.first(), self.stations.last()) {
            (Some(&a), Some(&b)) => [a, b],
            _ => [0.0, 1.0],
        }
    }

    /// The z range of the whole body.
    pub(super) fn z_range(&self) -> [f64; 2] {
        let low = self.bottom.iter().copied().fold(f64::MAX, f64::min);
        let high = self.top.iter().copied().fold(f64::MIN, f64::max);
        if low > high {
            [-1.0, 1.0]
        } else {
            [low, high]
        }
    }

    /// The z range the side view frames: the body, and with the airframe
    /// behind it the fin above and the engines hanging below.
    pub(super) fn render_z_range(&self) -> [f64; 2] {
        let [z0, z1] = self.z_range();
        if self.airframe.is_none() {
            return [z0, z1];
        }
        let top = self.tail_top_z.map_or(z1, |tail| z1.max(tail));
        [z0 - 1.5, top + 0.3]
    }
}

fn view_item(item: &DeckItem) -> ViewItem {
    let declared = match &item.meta {
        ItemMeta::Declared(meta) => meta.id.clone(),
        _ => String::new(),
    };
    ViewItem {
        kind: item.kind,
        label: item.label.clone(),
        declared,
        x: item.x,
        z: item.z,
        length: item.length,
        height: item.height,
        mass: item.mass,
        y: item.y,
        width: item.width,
        seat: match &item.meta {
            ItemMeta::Seat(seat) => Some(SeatBlocks {
                blocks: seat.blocks.clone(),
                seat_w: seat.seat_w,
                aisle_w: seat.aisle_w,
                floor_z: seat.floor_z,
                cls: seat.cls,
            }),
            _ => None,
        },
    }
}

/// Sample the geometry of `geometry` and the placed `items` into a view.
pub(super) fn from_geometry(geometry: &CabinGeometry, items: &[DeckItem], cg_x: f64) -> SideView {
    let (x0, x1) = (geometry.x_min, geometry.x_max);
    let mut view = SideView {
        cabin_x: [geometry.cabin_start_x, geometry.cabin_end_x],
        has_hold: geometry.underfloor_hold,
        x_lemac: geometry.x_lemac,
        mac: geometry.mac,
        cg_x,
        items: items.iter().map(view_item).collect(),
        ..SideView::default()
    };
    let gap = geometry.lower_hold_gap_x_range();
    view.hold_gap_x = (gap.1 > gap.0).then_some([gap.0, gap.1]);
    view.doors = geometry.door_stations.iter().map(|door| door.x).collect();
    view.reserved = geometry
        .reserved_zones
        .iter()
        .filter(|zone| zone.deck == alas_payload::layout::MAIN)
        .map(|zone| [zone.x0, zone.x1])
        .collect();
    let main = geometry.passenger_decks.first();
    for step in 0..STATIONS {
        let x = x0 + (x1 - x0) * step as f64 / (STATIONS - 1) as f64;
        let centre = geometry.zc_at(x);
        let half = geometry.height_at(x) / 2.0;
        view.stations.push(x);
        view.top.push(centre + half);
        view.bottom.push(centre - half);
        view.widths.push(geometry.width_at(x));
        let (floor, ceiling) = main.map_or((centre, centre), |deck| {
            (geometry.floor_z(deck, x), geometry.ceil_z(deck, x))
        });
        view.main_width
            .push(main.map_or(0.0, |deck| geometry.usable_width(deck, x)));
        view.main_floor.push(floor);
        view.main_ceiling.push(ceiling);
        let lower = &geometry.lower_deck;
        let (hold_floor, hold_ceiling) = if geometry.underfloor_hold {
            (geometry.floor_z(lower, x), geometry.ceil_z(lower, x))
        } else {
            (floor, floor)
        };
        view.hold_floor.push(hold_floor);
        view.hold_ceiling.push(hold_ceiling);
    }
    view
}

/// The operating empty mass and its station from component masses and
/// centroids, through the payload crate's own `oew_and_cg` (the loader's and
/// the analysis's definition, not a copy of it).
fn operating_empty(
    masses: &std::collections::BTreeMap<String, f64>,
    coordinates: &std::collections::BTreeMap<String, [f64; 3]>,
) -> Option<(f64, f64)> {
    use alas_mass::breakdown::{
        MassBreakdown, MassCoordinates, FUEL, FURNISHINGS, FUSELAGE, GEAR, H_STAB, PAYLOAD,
        PROPULSION, SYSTEMS, V_STAB, WING,
    };
    let mass = |key: &str| masses.get(key).copied().unwrap_or(0.0);
    // Every operating-empty component needs its centroid: without them the
    // station would be wrong, so the view has none.
    for key in alas_mass::breakdown::OEW_KEYS {
        if mass(key) > 0.0 && !coordinates.contains_key(key) {
            return None;
        }
    }
    let at = |key: &str| -> [f64; 3] { coordinates.get(key).copied().unwrap_or([0.0; 3]) };
    let typed_masses = MassBreakdown {
        wing: mass(WING),
        h_stab: mass(H_STAB),
        v_stab: mass(V_STAB),
        fuselage: mass(FUSELAGE),
        gear: mass(GEAR),
        propulsion: mass(PROPULSION),
        systems: mass(SYSTEMS),
        furnishings: mass(FURNISHINGS),
        payload: mass(PAYLOAD),
        fuel: mass(FUEL),
    };
    let typed_coordinates = MassCoordinates {
        wing: at(WING),
        h_stab: at(H_STAB),
        v_stab: at(V_STAB),
        fuselage: at(FUSELAGE),
        gear: at(GEAR),
        propulsion: at(PROPULSION),
        systems: at(SYSTEMS),
        furnishings: at(FURNISHINGS),
        payload: at(PAYLOAD),
        fuel: at(FUEL),
    };
    let (oew, x_oew) = alas_payload::oew_and_cg(&typed_masses, &typed_coordinates);
    (oew > 0.0).then_some((oew, x_oew))
}

/// What the masses of a view come from.
struct MassSource {
    layout: Option<alas_payload::layout::PayloadLayout>,
    oew: Option<(f64, f64)>,
    /// Neutral point station of the same analysis, m.
    x_np: Option<f64>,
    estimate: bool,
}

/// The last run's layout and operating empty mass, when that run is of this
/// very form (same configuration and design values): the layout the analysis
/// balanced against its cabin-synchronized operating empty mass.
fn run_source(state: &AppState) -> Option<MassSource> {
    if !state.pipeline_result_complete {
        return None;
    }
    let result = state.pipeline_result.as_ref()?;
    let form = state.typed_config_arc();
    let unchanged = state.pipeline_result_design_values.as_ref() == Some(&state.design_values)
        && form.is_some()
        && (form.as_deref() == Some(&result.config)
            || form.as_deref() == state.pipeline_result_form_config.as_ref());
    if !unchanged {
        return None;
    }
    // The design the form shows is the baseline evaluation; the optimized
    // report is another aircraft (a different design vector, so a different
    // geometry and MAC) and is read only when no optimization changed it.
    if let Some(baseline) = result.baseline_report.as_ref() {
        return Some(MassSource {
            layout: baseline.payload_layout.clone(),
            oew: operating_empty(&baseline.component_masses, &baseline.mass_coordinates),
            x_np: Some(baseline.x_neutral_point),
            estimate: false,
        });
    }
    if result.optimization_result.is_some() {
        return None;
    }
    let report = result.optimized_report.as_ref()?;
    Some(MassSource {
        layout: report.payload_layout.clone(),
        oew: operating_empty(&report.component_masses, &report.mass_coordinates),
        x_np: Some(report.x_neutral_point),
        estimate: false,
    })
}

/// Build the side view of the case from the geometry the payload layout is
/// built on (declared doors, reserved stretches) and the layout the pipeline
/// would build, balanced against the operating empty mass of the mass
/// estimate. `None` when the aircraft cannot be built.
pub(super) fn build(state: &AppState, config: &AlasConfig) -> Option<SideView> {
    let design = state.current_design().unwrap_or_default();
    let mut config = config.clone();
    if config.requirements.cabin_preset != "Custom" {
        alas_payload::apply_cabin_preset(&mut config, Some(&design)).ok()?;
    }
    // A set of items that fails validation must not hide the view the user
    // corrects it on: the body is drawn without them.
    if config.cabin.payload_items.validate().is_err() {
        config.cabin.payload_items = alas_config::PayloadItemsConfig::default();
    }
    let airplane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .ok()?;
    let geometry = product_cabin_geometry(&airplane, &config).ok()?;
    let airframe = Airframe {
        geometry: config.geometry.clone(),
        design,
        airfoil_generation: alas_geom::airfoil_io::generation(),
    };
    // The analysis's own numbers when the last run is of this very form; before
    // a run (or after an edit) the pre-run estimate stands in, and the view
    // says so.
    let source = run_source(state).unwrap_or_else(|| {
        let report = alas_report::families::mass_balance::quick_preview_report(
            airplane.clone(),
            &config,
            design,
        )
        .ok();
        MassSource {
            layout: report.as_ref().and_then(|r| r.payload_layout.clone()),
            oew: report
                .as_ref()
                .and_then(|r| operating_empty(&r.component_masses, &r.mass_coordinates)),
            x_np: report.as_ref().map(|r| r.x_neutral_point),
            estimate: true,
        }
    });
    let layout = source
        .layout
        .or_else(|| build_payload_layout(&airplane, &config, 0.0, 0.0).ok())?;
    let mut view = from_geometry(&geometry, &layout.items, layout.cg_x);
    view.airframe = Some(std::sync::Arc::new(airframe));
    view.payload_mass = layout.total_mass;
    view.custom_only = config.cabin.payload_items.custom_only;
    view.occupant_kg = config.requirements.passenger_mass_kg;
    view.tail_top_z = airplane
        .wings
        .iter()
        .flat_map(|wing| wing.xsecs.iter())
        .map(|xsec| xsec.xyz_le[2])
        .filter(|z| z.is_finite())
        .max_by(f64::total_cmp);
    view.surfaces = airplane
        .wings
        .iter()
        .flat_map(|wing| {
            wing.xsecs.windows(2).map(|pair| {
                let (a, b) = (&pair[0], &pair[1]);
                [
                    [a.xyz_le[0], a.xyz_le[2]],
                    [a.xyz_le[0] + a.chord, a.xyz_le[2]],
                    [b.xyz_le[0] + b.chord, b.xyz_le[2]],
                    [b.xyz_le[0], b.xyz_le[2]],
                ]
            })
        })
        .filter(|quad| quad.iter().flatten().all(|v| v.is_finite()))
        .collect();
    if let Some(wing) = airplane.wings.first() {
        // The wing's reach over the holds is the box inside the body.
        let carried = wing.carried_box();
        view.wing_le = carried
            .xsecs
            .iter()
            .map(|xsec| [xsec.xyz_le[0], xsec.xyz_le[1].abs(), xsec.xyz_le[2]])
            .collect();
        view.wing_chord = carried.xsecs.first().map_or(0.0, |xsec| xsec.chord);
    }
    view.cg_limits = source.x_np.filter(|x| x.is_finite()).map(|x_np| {
        let aft = x_np - config.requirements.target_static_margin * geometry.mac;
        [
            aft - config.requirements.cg_range_pct_mac / 100.0 * geometry.mac,
            aft,
        ]
    });
    let declared = &config.cabin.cargo.hold_compartments;
    view.declared_holds = !declared.is_empty();
    view.hold_spans = declared
        .iter()
        .filter(|hold| hold.is_valid() && hold.deck == alas_config::HoldDeck::Lower)
        .map(|hold| [hold.x_start_m, hold.x_end_m])
        .collect();
    if view.declared_holds {
        view.has_hold = !view.hold_spans.is_empty();
    }
    view.estimate = source.estimate;
    view.target_pct = Some(config.cabin.cargo.target_cg_pct_mac).filter(|pct| *pct > 0.0);
    view.balance = layout.balance.as_ref().map(|outcome| BalanceView {
        target_x: outcome.target_x_m,
        reached_x: outcome.reached_x_m,
        saturated: outcome.saturated,
        settled: outcome.settled,
    });
    // One operating empty mass for the marker, balanced or not, with the
    // payload of the (balanced) layout.
    view.oew_kg = source.oew.map(|(oew, _)| oew).filter(|oew| oew.is_finite());
    view.zero_fuel_cg = source.oew.map(|(oew, x_oew)| {
        let payload = layout.total_mass;
        (oew * x_oew + payload * layout.cg_x) / (oew + payload)
    });
    Some(view)
}

/// A rectangular test body: 40 m long, a 4 m section centred on z = 0, a main
/// floor at z = 0 with 2 m of headroom, a hold from -1.6 to -0.2 m between
/// x = 10 and 36 m with a wing box gap at 18 to 22 m.
#[cfg(test)]
pub(super) fn test_body() -> SideView {
    let stations: Vec<f64> = (0..=80).map(|i| f64::from(i) * 0.5).collect();
    let n = stations.len();
    SideView {
        top: vec![2.0; n],
        bottom: vec![-2.0; n],
        main_floor: vec![0.0; n],
        main_ceiling: vec![1.9; n],
        hold_floor: vec![-1.6; n],
        hold_ceiling: vec![-0.2; n],
        stations,
        cabin_x: [5.0, 36.0],
        hold_gap_x: Some([18.0, 22.0]),
        hold_spans: Vec::new(),
        declared_holds: false,
        has_hold: true,
        items: Vec::new(),
        cg_x: 18.0,
        x_lemac: 14.0,
        mac: 4.0,
        balance: None,
        zero_fuel_cg: None,
        estimate: false,
        doors: Vec::new(),
        reserved: Vec::new(),
        ..SideView::default()
    }
}
