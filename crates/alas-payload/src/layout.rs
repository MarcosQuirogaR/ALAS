// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/payload.py (`DeckItem`, `PayloadLayout`)
// Reference: alas @ rust-port-baseline.

//! What a laid-out interior is: an ordered list of physical things, each with
//! a size, a place and a mass, and the mass properties that fall out of them.
//!
//! Both layout engines return this one type, because all three consumers --
//! the weight-and-balance analysis, the deck-plan drawings and the report --
//! want the same thing from a cabin and from a hold full of freight. The
//! engines differ in what they put in the list, not in what the list is.
//!
//! The order of [`PayloadLayout::items`] is part of the result and not an
//! implementation detail. It is the order the engine placed things in, front
//! to back and deck by deck, and a drawing that walked it differently would
//! stack monuments over seats.
//!
//! # Two dictionaries that became types
//!
//! Upstream gives every item a `meta: Dict` and every layout a
//! `summary: Dict`, both of which carry different keys depending on what kind
//! of item or layout it is. Here they are [`ItemMeta`] and [`LayoutSummary`],
//! one variant per shape, for the reason `alas-mass::breakdown` gives for the
//! same change: a key that is spelled wrong in one of the two implementations
//! is a compile error here and a silently missing value there. The variants
//! carry exactly the keys upstream writes, including the ones that differ
//! between two kinds that look alike -- a containerised bag records the fill
//! fraction it achieved, a main-deck container records that plus the net load
//! inside it.

/// The main passenger deck.
pub const MAIN: &str = "main";
/// The upper passenger deck of a double-deck body.
pub const UPPER: &str = "upper";
/// The lower-deck holds, which carry freight and checked baggage.
pub const LOWER: &str = "lower";

/// Whether the interior was laid out as a cabin or as a freight deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Seats, monuments, exits and checked baggage.
    Passenger,
    /// Unit load devices on whichever decks carry them.
    Cargo,
}

impl Mode {
    /// The name upstream writes into `PayloadLayout.mode`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passenger => "passenger",
            Self::Cargo => "cargo",
        }
    }
}

/// What one placed item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// One transverse row of seats.
    SeatRow,
    /// A unit load device on a freighter deck.
    Uld,
    /// A galley bay.
    Galley,
    /// A lavatory bay.
    Lav,
    /// A wheelchair-accessible lavatory bay.
    AccessibleLav,
    /// The dedicated in-cabin wheelchair stowage provision.
    WheelchairStowage,
    /// A sidewall or centre overhead stowage bin.
    OverheadBin,
    /// An emergency exit cutout. Carries no mass.
    Exit,
    /// Checked baggage or belly freight in a lower hold.
    Bag,
}

impl ItemKind {
    /// The name upstream writes into `DeckItem.kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SeatRow => "seat_row",
            Self::Uld => "uld",
            Self::Galley => "galley",
            Self::Lav => "lav",
            Self::AccessibleLav => "accessible_lav",
            Self::WheelchairStowage => "wheelchair_stowage",
            Self::OverheadBin => "overhead_bin",
            Self::Exit => "exit",
            Self::Bag => "bag",
        }
    }
}

/// A seat row's `meta`: what it seats and how the row is divided.
#[derive(Debug, Clone, PartialEq)]
pub struct SeatMeta {
    /// Which class this row belongs to.
    pub cls: &'static str,
    /// Seats the row would hold at this station's floor width.
    pub abreast: i64,
    /// Seats actually occupied, which is fewer in the row that exhausts a
    /// class's count or runs into the exit-derived capacity ceiling.
    pub filled: i64,
    /// Which deck the row is on, repeated inside the item as upstream does.
    pub deck: &'static str,
    /// One aisle or two.
    pub aisles: i64,
    /// Seats per lateral block, outboard to outboard.
    pub blocks: Vec<i64>,
    /// Lateral footprint of one seat in this class.
    pub seat_w: f64,
    /// The aisle width the row was laid out against.
    pub aisle_w: f64,
}

/// An emergency exit's `meta`: its regulatory class and minimum cutout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExitMeta {
    /// The FAR/CS-25.807 exit type letter.
    pub exit_type: &'static str,
    /// The type's minimum door width.
    pub door_w: f64,
    /// The type's minimum door height.
    pub door_h: f64,
}

/// A containerised load's `meta`: which container, and how full.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContainerMeta {
    /// The container's IATA code.
    pub uld: &'static str,
    /// Fraction of the container's net capacity used.
    pub fill: f64,
    /// The colour the deck plan draws this container family in.
    pub color: &'static str,
    /// Net load inside the container, excluding its tare. Recorded only for
    /// freighter containers; a hold full of checked bags does not carry it.
    pub net: Option<f64>,
}

/// The two overhead-stowage architectures represented by the cabin model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverheadBinType {
    /// Pivot bin following the curved sidewall crown.
    Sidewall,
    /// Hinge bin suspended over a centre seat block between two aisles.
    Center,
}

impl OverheadBinType {
    /// Stable display and serialization token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sidewall => "sidewall",
            Self::Center => "center",
        }
    }
}

/// Type-specific information for one overhead-bin run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverheadBinMeta {
    /// Whether this is a sidewall pivot bin or a centre hinge bin.
    pub bin_type: OverheadBinType,
}

/// The kind-specific extras upstream carries in `DeckItem.meta`.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemMeta {
    /// A galley or lavatory, which records nothing.
    None,
    /// A seat row.
    Seat(SeatMeta),
    /// An emergency exit.
    Exit(ExitMeta),
    /// Baggage or freight in a container, or a freighter container.
    Container(ContainerMeta),
    /// An overhead-bin run.
    OverheadBin(OverheadBinMeta),
    /// The loose bulk block that carries whatever the containers could not.
    BulkBag,
}

/// One physical payload element placed in the aircraft.
///
/// Coordinates are absolute aircraft coordinates in metres; `length` is along
/// x, `width` along y, `height` along z. `mass` is zero for the monuments and
/// exits, which occupy floor and are accounted for in the furnishings weight
/// rather than here.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckItem {
    /// What this item is.
    pub kind: ItemKind,
    /// Which deck it sits on: [`MAIN`], [`UPPER`] or [`LOWER`].
    pub deck: &'static str,
    /// Longitudinal centre.
    pub x: f64,
    /// Lateral centre.
    pub y: f64,
    /// Vertical centre.
    pub z: f64,
    /// Extent along x.
    pub length: f64,
    /// Extent along y.
    pub width: f64,
    /// Mass, kg.
    pub mass: f64,
    /// Extent along z, clamped to the deck it sits in.
    pub height: f64,
    /// What a deck plan writes on it.
    pub label: String,
    /// Kind-specific extras.
    pub meta: ItemMeta,
}

/// The passenger cabin's summary line.
#[derive(Debug, Clone, PartialEq)]
pub struct PassengerSummary {
    /// Seats asked for, across every class.
    pub total_pax: i64,
    /// Seats the layout could actually place.
    pub seated_pax: i64,
    /// Requested passengers that have no seat in this layout.
    pub unseated_pax: i64,
    /// Seats placed per class, in cabin order.
    pub classes: Vec<(&'static str, i64)>,
    /// Lavatories installed.
    pub lavatories: i64,
    /// Galleys installed.
    pub galleys: i64,
    /// Wheelchair-accessible lavatories installed.
    pub accessible_lavatories: i64,
    /// Dedicated wheelchair stowage positions installed.
    pub wheelchair_stowages: i64,
    /// Which FAR/CS-25.807 exit type the fuselage takes.
    pub exit_type: &'static str,
    /// Exit pairs installed, summed over the passenger decks.
    pub exit_pairs: i64,
    /// What those pairs are rated to evacuate.
    pub exit_capacity: i64,
    /// The realistic capacity ceiling, which is what caps the seating.
    pub max_certifiable_capacity: i64,
    /// Total payload, tonnes.
    pub payload_t: f64,
    /// Occupants and their carry-on, tonnes.
    pub seat_mass_t: f64,
    /// Checked baggage, tonnes.
    pub bag_mass_t: f64,
    /// Revenue freight in the belly, tonnes.
    pub belly_cargo_t: f64,
    /// What the holds could take, tonnes.
    pub hold_capacity_t: f64,
    /// What went into them, tonnes.
    pub hold_used_t: f64,
    /// Containers used in the holds.
    pub hold_ulds: i64,
    /// The aisle width laid out against.
    pub aisle_width_m: f64,
    /// The widest row placed.
    pub max_abreast: i64,
    /// The most aisles any row needed.
    pub n_aisles: i64,
    /// Floor length used, per deck, as a percentage.
    pub deck_utilization: Vec<(&'static str, f64)>,
    /// Where the payload's centre of gravity landed, as a percentage of MAC.
    pub cg_pct_mac: f64,
    /// Whether the body has two passenger decks.
    pub double_deck: bool,
}

/// The freighter deck's summary line.
#[derive(Debug, Clone, PartialEq)]
pub struct CargoSummary {
    /// Gross carried cargo mass, tonnes, including ULD tare.
    pub payload_t: f64,
    /// Net cargo requested by the load case, tonnes, excluding ULD tare.
    pub requested_net_payload_t: f64,
    /// Net cargo actually loaded, tonnes, excluding ULD tare.
    pub loaded_net_payload_t: f64,
    /// Tare mass of the loaded ULDs, tonnes.
    pub tare_mass_t: f64,
    /// Containers loaded.
    pub n_ulds: i64,
    /// How many of them are on the main deck.
    pub n_main_deck: i64,
    /// How many are in the lower holds.
    pub n_lower_deck: i64,
    /// Positions available, loaded or not.
    pub n_slots: usize,
    /// What those positions could hold, tonnes.
    pub capacity_t: f64,
    /// The requested payload as a percentage of that capacity. Exceeds a
    /// hundred when more was asked for than there are positions to hold it,
    /// which upstream reports rather than clamping.
    pub fill_pct: f64,
    /// Nominal internal volume of every position, loaded or not.
    pub volume_m3: f64,
    /// Which container the lower holds ended up taking, after the fit check.
    pub lower_uld: &'static str,
    /// The centre of gravity asked for, as a percentage of MAC.
    pub target_cg_pct_mac: f64,
    /// The one the trim reached.
    pub achieved_cg_pct_mac: f64,
    /// Which loading strategy produced this.
    pub strategy: String,
}

/// The summary line, whichever engine produced the layout.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutSummary {
    /// A passenger cabin.
    Passenger(Box<PassengerSummary>),
    /// A freighter deck.
    Cargo(Box<CargoSummary>),
}

/// The complete interior layout and its mass properties.
#[derive(Debug, Clone, PartialEq)]
pub struct PayloadLayout {
    /// Which engine produced it.
    pub mode: Mode,
    /// Everything placed, in placement order.
    pub items: Vec<DeckItem>,
    /// Total mass over the mass-bearing items, kg.
    pub total_mass: f64,
    /// Mass-weighted longitudinal centre of gravity, m.
    pub cg_x: f64,
    /// Mass-weighted lateral centre of gravity, m.
    pub cg_y: f64,
    /// The summary line.
    pub summary: LayoutSummary,
}

/// Total mass and the two centre-of-gravity coordinates of a set of items --
/// `PayloadLayout.recompute_cg`, as a function so a builder can compute them
/// before it has a summary to construct the layout with.
///
/// Only items with positive mass contribute, so the monuments and exits place
/// themselves without moving the balance.
pub fn mass_properties(items: &[DeckItem]) -> (f64, f64, f64) {
    let mut mass = 0.0;
    let mut moment_x = 0.0;
    let mut moment_y = 0.0;
    for item in items {
        if item.mass > 0.0 {
            mass += item.mass;
            moment_x += item.mass * item.x;
            moment_y += item.mass * item.y;
        }
    }
    if mass > 0.0 {
        (mass, moment_x / mass, moment_y / mass)
    } else {
        (mass, 0.0, 0.0)
    }
}

impl PayloadLayout {
    /// Every item on one deck, in placement order.
    pub fn by_deck(&self, deck: &str) -> Vec<&DeckItem> {
        self.items.iter().filter(|it| it.deck == deck).collect()
    }

    /// Every item of one kind, in placement order.
    pub fn by_kind(&self, kind: ItemKind) -> Vec<&DeckItem> {
        self.items.iter().filter(|it| it.kind == kind).collect()
    }

    /// The decks this layout uses, in the order items first appear on them.
    pub fn decks(&self) -> Vec<&'static str> {
        let mut seen: Vec<&'static str> = Vec::new();
        for item in &self.items {
            if !seen.contains(&item.deck) {
                seen.push(item.deck);
            }
        }
        seen
    }

    /// Recompute the mass and centre of gravity from the items.
    ///
    /// The engines have already done this for the layouts they return; this is
    /// for a consumer that has added or removed an item of its own.
    pub fn recompute_cg(&mut self) {
        let (mass, cg_x, cg_y) = mass_properties(&self.items);
        self.total_mass = mass;
        self.cg_x = cg_x;
        self.cg_y = cg_y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(deck: &'static str, kind: ItemKind, x: f64, mass: f64) -> DeckItem {
        DeckItem {
            kind,
            deck,
            x,
            y: 0.0,
            z: 0.0,
            length: 1.0,
            width: 1.0,
            mass,
            height: 1.0,
            label: String::new(),
            meta: ItemMeta::None,
        }
    }

    #[test]
    fn a_massless_monument_places_itself_without_moving_the_balance() {
        // A galley at the nose has no mass here -- it is counted in the
        // furnishings weight -- so it must not drag the payload CG forward.
        let items = vec![
            item(MAIN, ItemKind::Galley, 0.0, 0.0),
            item(MAIN, ItemKind::SeatRow, 10.0, 100.0),
            item(MAIN, ItemKind::SeatRow, 20.0, 100.0),
        ];
        let (mass, cg_x, _cg_y) = mass_properties(&items);
        assert_eq!(mass, 200.0);
        assert_eq!(cg_x, 15.0);
    }

    #[test]
    fn an_empty_layout_reports_no_centre_of_gravity_rather_than_dividing_by_zero() {
        // The optimizer's first evaluation can produce a cabin with nothing in
        // it, and a NaN centre of gravity there would fail an envelope check
        // for the wrong reason.
        let (mass, cg_x, cg_y) = mass_properties(&[]);
        assert_eq!((mass, cg_x, cg_y), (0.0, 0.0, 0.0));
    }

    #[test]
    fn the_decks_are_listed_in_the_order_the_items_reach_them() {
        let layout = PayloadLayout {
            mode: Mode::Passenger,
            items: vec![
                item(MAIN, ItemKind::SeatRow, 1.0, 1.0),
                item(LOWER, ItemKind::Bag, 2.0, 1.0),
                item(MAIN, ItemKind::SeatRow, 3.0, 1.0),
                item(UPPER, ItemKind::SeatRow, 4.0, 1.0),
            ],
            total_mass: 0.0,
            cg_x: 0.0,
            cg_y: 0.0,
            summary: LayoutSummary::Cargo(Box::new(CargoSummary {
                payload_t: 0.0,
                requested_net_payload_t: 0.0,
                loaded_net_payload_t: 0.0,
                tare_mass_t: 0.0,
                n_ulds: 0,
                n_main_deck: 0,
                n_lower_deck: 0,
                n_slots: 0,
                capacity_t: 0.0,
                fill_pct: 0.0,
                volume_m3: 0.0,
                lower_uld: "",
                target_cg_pct_mac: 0.0,
                achieved_cg_pct_mac: 0.0,
                strategy: String::new(),
            })),
        };
        assert_eq!(layout.decks(), vec![MAIN, LOWER, UPPER]);
        assert_eq!(layout.by_deck(MAIN).len(), 2);
        assert_eq!(layout.by_kind(ItemKind::Bag).len(), 1);
    }
}
