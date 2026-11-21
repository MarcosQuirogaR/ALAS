// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// The main wing is found by name, and everything else falls back to the first
/// wing: upstream's `next((w for w in plane.wings if w.name == "Main Wing"),
/// plane.wings[0])`.
const MAIN_WING: &str = "Main Wing";

/// Quarter chord, which is where upstream leaves `aerodynamic_center`'s
/// default and therefore what `x_lemac` is measured back from.
const AERODYNAMIC_CENTER_CHORD_FRACTION: f64 = 0.25;

/// Minimum clear deck-to-deck band, as a fraction of inner half-height.
///
/// This reserves approximately 0.15 to 0.20 m in a widebody for the floor beam,
/// panels and systems instead of letting the hold ceiling touch the cabin
/// floor geometrically.
pub(super) const MIN_DECK_SEPARATION_FRAC: f64 = 0.06;

/// An airplane [`CabinGeometry`] cannot be built from.
///
/// Upstream indexes `plane.fuselages[0]` and `plane.wings[0]` and raises an
/// `IndexError` on either; this crate does not panic (`CONTRIBUTING.md`), so
/// the same two conditions are a typed error. `AircraftBuilder::build` always
/// supplies three wings and at least one fuselage, so neither is reachable
/// from this program's own inputs.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CabinGeometryError {
    /// The airplane has no fuselage to lay an interior out inside.
    #[error("the airplane has no fuselage, so there is no cabin to lay out")]
    NoFuselage,
    /// The airplane has no wing, so there is no mean aerodynamic chord to
    /// report a centre of gravity against.
    #[error("the airplane has no wing, so there is no MAC to measure the payload CG against")]
    NoWings,
    /// The declared floor zones or point masses are not physical (an inverted
    /// extent, a duplicate id, a non-positive mass); the configuration
    /// validation names the field.
    #[error(
        "the declared payload items are invalid; see the cabin.payload_items validation message"
    )]
    InvalidPayloadItems,
    /// The drawn cabin section does not fit the outer section it was drawn
    /// for: a depth outside it, or a hold wider than the skin there.
    #[error("the drawn cabin section does not fit the outer section: {0}")]
    SectionOutsideSkin(String),
}

/// Why an installed item cannot be evaluated or contained by the cabin.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum InteriorEnvelopeError {
    /// One or more dimensions are negative or a coordinate is not finite.
    #[error("the interior item has a non-finite coordinate or a negative extent")]
    InvalidExtent,
    /// A corner of the item lies outside the usable, wall-inset fuselage.
    #[error(
        "the interior item protrudes through the cabin envelope at ({x:.3}, {y:.3}, {z:.3}) m"
    )]
    OutsideEnvelope {
        /// Longitudinal station, in metres.
        x: f64,
        /// Lateral coordinate, in metres; positive to starboard.
        y: f64,
        /// Vertical coordinate, in metres; positive upward.
        z: f64,
    },
}

/// A horizontal deck: its floor, its ceiling, and its usable width.
///
/// `width_factor` scales the internal width to the usable floor width, which
/// accounts for a deck sitting off the section centre where the cross-section
/// is narrower: a lower hold in a circular body is a chord of the circle,
/// not its diameter.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckSpec {
    /// [`crate::layout::MAIN`], [`crate::layout::UPPER`] or
    /// [`crate::layout::LOWER`].
    pub name: &'static str,
    /// Floor height as a fraction of the internal half-height, +up.
    pub floor_frac: f64,
    /// Ceiling height as a fraction of the internal half-height, +up.
    pub ceil_frac: f64,
    /// Internal width to usable floor width.
    pub width_factor: f64,
    /// Whether this deck seats people.
    pub is_passenger: bool,
}

/// A stretch of one passenger deck that a declared floor zone (an office, a
/// bedroom, an operations room) takes out of the seat layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReservedZone {
    /// The deck's name: [`crate::layout::MAIN`] or [`crate::layout::UPPER`].
    pub deck: &'static str,
    /// Forward end, m.
    pub x0: f64,
    /// Aft end, m.
    pub x1: f64,
}

/// The built fuselage, sampled: cabin extent, per-station section, and the
/// decks inside it.
#[derive(Debug, Clone, PartialEq)]
pub struct CabinGeometry {
    /// Inset per side from the outer skin to the usable cabin wall.
    pub wall: f64,
    /// The forwardmost station.
    pub x_min: f64,
    /// The aftmost station.
    pub x_max: f64,
    /// Overall length.
    pub fus_len: f64,
    /// The widest cross-section's diameter, from the configuration.
    pub diameter_m: f64,
    /// Where the nose taper ends.
    pub cabin_start_x: f64,
    /// How much of the length is aft taper.
    pub tailcone_len: f64,
    /// Where the constant section ends.
    pub cabin_end_x: f64,
    /// Whether this body gets two full-length passenger decks.
    pub is_double_deck: bool,
    /// The partial upper deck under an upper-deck hump (747 type), on the
    /// product path only; see `hump_deck`.
    pub upper_deck: Option<alas_config::UpperDeck>,
    /// The hump's crown rise at each station (empty without an upper deck),
    /// taken off the sampled sections to find the main lobe the main deck
    /// and the hold are laid out in.
    pub(super) crown_rises: Vec<f64>,
    /// The level main-deck floor height of a body with a partial upper deck,
    /// m (z up); see `hump_deck`. `None` without an upper deck.
    pub(super) level_main_floor_z: Option<f64>,
    /// The level floor of each passenger deck of a body without a partial
    /// upper deck, m (z up), in `passenger_decks` order; empty on the frozen
    /// reference frame and under a hump, which has its own level floor.
    pub(super) level_deck_floors: Vec<f64>,
    /// Mean aerodynamic chord, which the centre of gravity is reported against.
    pub mac: f64,
    /// The main wing's aerodynamic centre, longitudinally.
    pub x_wing_ac: f64,
    /// The leading edge of the MAC, the origin of the percent-MAC frame.
    pub x_lemac: f64,
    /// The main wing root's leading edge, the front of the centre wing box.
    pub x_wing_le: f64,
    /// The main wing root chord, the length of that box.
    pub wing_root_chord: f64,
    /// The passenger decks, forward-lowest first.
    pub passenger_decks: Vec<DeckSpec>,
    /// The lower-deck holds.
    pub lower_deck: DeckSpec,
    /// Whether product analyses enforce the physical inner-envelope contract.
    /// Frozen Python parity construction leaves this false deliberately.
    pub(super) strict_envelope: bool,
    /// The main deck's declared door pairs on this body, forward to aft, or
    /// empty when the cabin is bounded by the generic nose and tail-cone
    /// lengths instead.
    pub door_stations: Vec<crate::cabin::DoorStation>,
    /// Floor that declared zones take away from the seat layout (empty
    /// unless the configuration declares zones).
    pub reserved_zones: Vec<ReservedZone>,
    /// The underfloor hold's polygon on a body whose section is drawn or
    /// classed; `None` keeps the baseline hold.
    pub hold: Option<HoldSection>,
    /// Whether the passenger floor stands at a drawn or class depth below the
    /// crown; seat rows are then contained at armrest and shoulder height.
    /// A double-decker and a hump body keep their deck tables for now.
    pub(super) floor_datum_applied: bool,
    /// Ratio of the drawn (or class) lining width to the lofted lining's chord
    /// at the armrests and at the shoulders; one without a drawn section.
    pub lining_scale: [f64; 2],
    /// Whether the body has an underfloor hold; false only where the section
    /// leaves no room for one (a turboprop). Bodies with their own deck tables
    /// (double-decker, hump) keep it.
    pub underfloor_hold: bool,
    /// Station coordinates, ascending. The three sampled series below are in
    /// this same order, which is what lets one interpolation index serve all.
    pub(super) x_stations: Vec<f64>,
    widths: Vec<f64>,
    pub(super) shapes: Vec<f64>,
    heights: Vec<f64>,
    zcs: Vec<f64>,
    /// `(centres, heights)` of the loadable interior of a container hold, in the
    /// order of `x_stations`: it keeps the cabin keel (a flat structural floor)
    /// where the outer belly line rises (`FuselageConfig::hold_interior_section`).
    /// `None` where it is the outer loft. The lower deck and the hold's lining
    /// read it; the passenger deck never does.
    hold_interior: Option<(Vec<f64>, Vec<f64>)>,
}

impl CabinGeometry {
    /// Whether this frame enforces the product physical-envelope contract.
    pub const fn enforces_physical_envelope(&self) -> bool {
        self.strict_envelope
    }

    /// Bound the main deck by the door stations `layout` declares, placed on
    /// this body (see [`crate::cabin`]'s station rules). A layout without
    /// usable stations, or none, leaves the generic frame in place, as does
    /// the frozen reference frame, whose fixtures predate declared doors.
    pub fn with_declared_doors(
        mut self,
        layout: Option<&alas_config::CertifiedExitLayout>,
    ) -> Self {
        self.door_stations = layout
            .filter(|_| self.strict_envelope)
            .and_then(|layout| crate::cabin::resolve_door_stations(&self, layout))
            .unwrap_or_default();
        self
    }

    /// Sample `plane`'s first fuselage into a cabin frame.
    ///
    /// # Errors
    ///
    /// [`CabinGeometryError`], for an airplane with no fuselage or no wing.
    pub fn new(
        plane: &Airplane,
        geometry_config: &GeometryConfig,
        wall_thickness_m: f64,
    ) -> Result<Self, CabinGeometryError> {
        Self::new_with_mode(plane, geometry_config, wall_thickness_m, true)
    }

    /// Rebuild the historical cabin frame for explicit Python parity paths.
    pub fn new_reference_compatibility(
        plane: &Airplane,
        geometry_config: &GeometryConfig,
        wall_thickness_m: f64,
    ) -> Result<Self, CabinGeometryError> {
        Self::new_with_mode(plane, geometry_config, wall_thickness_m, false)
    }

    fn new_with_mode(
        plane: &Airplane,
        geometry_config: &GeometryConfig,
        wall_thickness_m: f64,
        strict_envelope: bool,
    ) -> Result<Self, CabinGeometryError> {
        let fus = plane
            .fuselages
            .first()
            .ok_or(CabinGeometryError::NoFuselage)?;
        // Upstream sorts with `np.argsort`, whose default is not a stable
        // sort. Every fuselage this program builds has strictly increasing
        // stations, so no tie exists for the two to disagree about, and a
        // stable sort is the conservative choice where one ever did.
        let mut order: Vec<usize> = (0..fus.xsecs.len()).collect();
        order.sort_by(|&a, &b| fus.xsecs[a].xyz_c[0].total_cmp(&fus.xsecs[b].xyz_c[0]));

        let mut x_stations: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].xyz_c[0]).collect();
        let mut widths: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].width).collect();
        let mut shapes: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].shape).collect();
        let mut heights: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].height).collect();
        let mut zcs: Vec<f64> = order.iter().map(|&i| fus.xsecs[i].xyz_c[2]).collect();
        let mut hold_interior: Option<(Vec<f64>, Vec<f64>)> = None;
        if strict_envelope {
            // The interior's keel steps back to the outer loft at the hold end,
            // so the loft needs a definition station there: without one the
            // floor is interpolated from the last station before the end to the
            // first station after it, lifting it under the last container
            // position of a hold that ends between two loft stations.
            if let Some(end) = geometry_config
                .fuselage
                .hold_interior_end_x_m
                .filter(|end| end.is_finite())
            {
                let at = x_stations.partition_point(|&x| x < end);
                let inside = at > 0 && at < x_stations.len();
                if inside && x_stations[at] > end && end > geometry_config.fuselage.cabin_start_x_m
                {
                    let (w, s, h, z) = (
                        crate::numeric::interp(end, &x_stations, &widths),
                        crate::numeric::interp(end, &x_stations, &shapes),
                        crate::numeric::interp(end, &x_stations, &heights),
                        crate::numeric::interp(end, &x_stations, &zcs),
                    );
                    x_stations.insert(at, end);
                    widths.insert(at, w);
                    shapes.insert(at, s);
                    heights.insert(at, h);
                    zcs.insert(at, z);
                }
            }
            // The loadable interior keeps the cabin keel up to the hold end,
            // whatever the outer belly line does (unset: the outer loft). It
            // is a second series read only for the lower deck and for the
            // hold's lining; the series above stay the outer loft the
            // passenger deck stands in, so declaring a lower hold cannot move
            // a seat row.
            let interior: (Vec<f64>, Vec<f64>) = x_stations
                .iter()
                .enumerate()
                .map(|(i, x)| {
                    geometry_config
                        .fuselage
                        .hold_interior_section(*x, zcs[i], heights[i])
                })
                .unzip();
            if interior.0 != zcs || interior.1 != heights {
                hold_interior = Some(interior);
            }
        }

        let x_min = x_stations
            .first()
            .copied()
            .ok_or(CabinGeometryError::NoFuselage)?;
        let x_max = x_stations
            .last()
            .copied()
            .ok_or(CabinGeometryError::NoFuselage)?;

        let fg = &geometry_config.fuselage;
        let diameter_m = fg.diameter_m;
        let tailcone_len = fg.tailcone_length_m;

        let wing = plane
            .wings
            .iter()
            .find(|w| w.name == MAIN_WING)
            .or_else(|| plane.wings.first())
            .ok_or(CabinGeometryError::NoWings)?;
        wing.xsecs.first().ok_or(CabinGeometryError::NoWings)?;
        // The holds stop at the centre wing box, which the side-of-body section
        // sets; the plan of the wing around it inside the body is not structure.
        let (box_le_m, box_chord_m) = wing.carried_root();

        let mac = plane.c_ref;
        let x_wing_ac = wing
            .carried_box()
            .aerodynamic_center(AERODYNAMIC_CENTER_CHORD_FRACTION)[0];
        // Canonical MAC frame (MAC-06) on the product path only: the main
        // wing's own `mac_station()` leading edge, never reconstructed from
        // `aerodynamic_center(0.25) - 0.25 * c_ref`. The frozen
        // `reference_compatibility` path (`strict_envelope` false) keeps
        // that reconstruction bit-identical.
        let x_lemac = if strict_envelope {
            plane.mac_frame().map_or(
                x_wing_ac - AERODYNAMIC_CENTER_CHORD_FRACTION * mac,
                |frame| frame.x_lemac_m,
            )
        } else {
            x_wing_ac - AERODYNAMIC_CENTER_CHORD_FRACTION * mac
        };

        // A partial upper deck exists only on a single-deck body: a
        // full-length double decker already has its upper deck.
        let upper_deck = fg
            .upper_deck()
            .filter(|_| strict_envelope && !is_double_deck(fg.height_m, diameter_m));
        let (passenger_decks, lower_deck) = if upper_deck.is_some() {
            hump_decks()
        } else if strict_envelope {
            decks(fg.height_m, diameter_m)
        } else {
            reference_decks(fg.height_m, diameter_m)
        };

        let mut geometry = Self {
            wall: wall_thickness_m,
            x_min,
            x_max,
            fus_len: x_max - x_min,
            diameter_m,
            cabin_start_x: fg.cabin_start_x_m,
            tailcone_len,
            cabin_end_x: x_max - tailcone_len,
            is_double_deck: is_double_deck(fg.height_m, diameter_m),
            crown_rises: upper_deck
                .map(|_| {
                    x_stations
                        .iter()
                        .map(|&x| fg.hump_crown_rise_m(x))
                        .collect()
                })
                .unwrap_or_default(),
            level_main_floor_z: None,
            level_deck_floors: Vec::new(),
            upper_deck,
            mac,
            x_wing_ac,
            x_lemac,
            x_wing_le: box_le_m[0],
            wing_root_chord: box_chord_m,
            passenger_decks,
            lower_deck,
            strict_envelope,
            door_stations: Vec::new(),
            reserved_zones: Vec::new(),
            hold: None,
            floor_datum_applied: false,
            lining_scale: [1.0, 1.0],
            underfloor_hold: true,
            x_stations,
            widths,
            shapes,
            heights,
            zcs,
            hold_interior,
        };
        geometry.level_main_floor_z = geometry.level_main_floor();
        if strict_envelope && geometry.upper_deck.is_none() {
            if !geometry.is_double_deck {
                geometry.apply_cabin_section(fg.cabin_section)?;
            }
            geometry.level_deck_floors = geometry.level_floors();
        }
        Ok(geometry)
    }

    /// The section width at station `x`, clamped to the end sections outside
    /// the body.
    pub fn width_at(&self, x: f64) -> f64 {
        interp(x, &self.x_stations, &self.widths)
    }

    /// The section height at station `x`.
    pub fn height_at(&self, x: f64) -> f64 {
        interp(x, &self.x_stations, &self.heights)
    }

    /// The section centre's height at station `x`, which follows the body's
    /// droop and tail upsweep.
    pub fn zc_at(&self, x: f64) -> f64 {
        interp(x, &self.x_stations, &self.zcs)
    }

    /// The loadable interior's `(centre z, height)` at station `x`: the keel of
    /// a container hold where one is declared, else the outer loft.
    pub(super) fn interior_section_at(&self, x: f64) -> (f64, f64) {
        match &self.hold_interior {
            Some((zcs, heights)) => (
                interp(x, &self.x_stations, zcs),
                interp(x, &self.x_stations, heights),
            ),
            None => (self.zc_at(x), self.height_at(x)),
        }
    }

    /// The section `(centre z, height)` that `deck` stands in at `x`: the outer
    /// loft for a passenger deck, the loadable interior for a deck that carries
    /// cargo (the lower holds, a freight hold on the main deck).
    fn deck_section_at(&self, deck: &DeckSpec, x: f64) -> (f64, f64) {
        if deck.is_passenger {
            (self.zc_at(x), self.height_at(x))
        } else {
            self.interior_section_at(x)
        }
    }

    /// `internal_half_height` of the section `deck` stands in.
    fn deck_half_height(&self, deck: &DeckSpec, x: f64) -> f64 {
        let rise = self.crown_rise_m(x);
        ((self.deck_section_at(deck, x).1 - rise) / 2.0 - self.wall).max(0.1)
    }

    /// Half the internal height at station `x`, the unit every deck fraction
    /// is measured in.
    ///
    /// Floored at 0.1 m so that the tapered ends, where the wall thickness
    /// exceeds the half-section, still divide into decks rather than
    /// inverting.
    /// Under an upper-deck hump this is the main lobe's, the hump's crown
    /// rise taken off.
    pub fn internal_half_height(&self, x: f64) -> f64 {
        let rise = self.crown_rise_m(x);
        ((self.height_at(x) - rise) / 2.0 - self.wall).max(0.1)
    }

    /// Where `deck`'s floor sits at station `x`.
    ///
    /// An underfloor cargo floor is level, not a fixed fraction of the local
    /// section: aft of the middle of the constant section, where the tail's
    /// upsweep lifts the section centre, the fractional rule would lift the
    /// floor with it and squeeze the hold below a container's height while the
    /// belly is still far below. The hold floor there stays at the level it has
    /// at the middle of the constant section, and rises only when the lining
    /// keel, plus the floor-to-keel clearance (`FLOOR_KEEL_CLEARANCE_M`), forces
    /// it up. It never goes below the fractional rule's own floor.
    pub fn floor_z(&self, deck: &DeckSpec, x: f64) -> f64 {
        let rule = self.rule_floor_z(deck, x);
        if deck.name != crate::layout::LOWER || !self.strict_envelope {
            return rule;
        }
        let x_ref = 0.5 * (self.cabin_start_x + self.cabin_end_x);
        if x <= x_ref {
            return rule;
        }
        let level = self.rule_floor_z(deck, x_ref);
        let (keel_zc, keel_height) = self.interior_section_at(x);
        let keel =
            keel_zc - 0.5 * self.crown_rise_m(x) - 0.5 * (keel_height - self.crown_rise_m(x))
                + self.wall;
        // Where the crown falls aft while the keel is held (the DC-10 from
        // 32 m), the fractional rule sinks below the level floor: the floor
        // stays level there and does not follow it down.
        if rule < level {
            return level;
        }
        level
            .max(keel + super::hold::FLOOR_KEEL_CLEARANCE_M)
            .min(rule)
    }

    /// The floor the section rule gives: the hump rule, a declared level floor,
    /// or the deck's fraction of the local internal half-height.
    fn rule_floor_z(&self, deck: &DeckSpec, x: f64) -> f64 {
        if self.upper_deck.is_some() {
            return self.hump_floor_z(deck, x);
        }
        if let Some(level) = self.level_floor_of(deck) {
            return level;
        }
        self.deck_section_at(deck, x).0 + deck.floor_frac * self.deck_half_height(deck, x)
    }

    /// Where `deck`'s ceiling sits at station `x`.
    pub fn ceil_z(&self, deck: &DeckSpec, x: f64) -> f64 {
        if self.upper_deck.is_some() {
            return self.hump_ceil_z(deck, x);
        }
        let section =
            self.deck_section_at(deck, x).0 + deck.ceil_frac * self.deck_half_height(deck, x);
        match self.level_deck_floors.first() {
            // Under a level main floor the hold ceiling is the underside of
            // that floor's beams: a plane at the depth it has at the middle of
            // the constant section, not a fraction of the local section (which
            // drops it wherever the crown falls). It never rises through the
            // level main floor where the tail's upsweep lifts the section.
            Some(&main_floor) if deck.name == crate::layout::LOWER => {
                let ceiling = if self.strict_envelope {
                    let x_ref = 0.5 * (self.cabin_start_x + self.cabin_end_x);
                    self.deck_section_at(deck, x_ref).0
                        + deck.ceil_frac * self.deck_half_height(deck, x_ref)
                } else {
                    section
                };
                ceiling.min(main_floor - MIN_DECK_SEPARATION_FRAC * self.deck_half_height(deck, x))
            }
            _ => section,
        }
    }

    /// The headroom on `deck` at station `x`, floored at 0.3 m for the same
    /// reason [`Self::internal_half_height`] has a floor.
    pub fn deck_height(&self, deck: &DeckSpec, x: f64) -> f64 {
        // Under a hump or a level floor the floor is not the section rule's, so
        // the headroom is measured between the planes the decks really have.
        if self.upper_deck.is_some() || !self.level_deck_floors.is_empty() {
            return (self.ceil_z(deck, x) - self.floor_z(deck, x)).max(0.3);
        }
        if deck.name == crate::layout::LOWER {
            return (self.ceil_z(deck, x) - self.floor_z(deck, x)).max(0.3);
        }
        ((deck.ceil_frac - deck.floor_frac) * self.deck_half_height(deck, x)).max(0.3)
    }

    /// An item height cut down to what `deck` has room for at station `x`.
    pub fn clamp_height(&self, deck: &DeckSpec, x: f64, h: f64) -> f64 {
        h.min(self.deck_height(deck, x))
    }

    /// The vertical centre of an item of height `h` resting on `deck`'s floor.
    pub fn item_z(&self, deck: &DeckSpec, x: f64, h: f64) -> f64 {
        self.floor_z(deck, x) + self.clamp_height(deck, x, h) / 2.0
    }

    /// A station as a percentage of MAC.
    ///
    /// The MAC is floored at a micrometre so that a degenerate wing reports a
    /// large percentage rather than a division by zero, which is upstream's
    /// guard and matters because the optimizer evaluates infeasible designs.
    pub fn x_to_pct_mac(&self, x: f64) -> f64 {
        (x - self.x_lemac) / self.mac.max(1e-6) * 100.0
    }

    /// A percentage of MAC as a station.
    pub fn pct_mac_to_x(&self, pct: f64) -> f64 {
        self.x_lemac + (pct / 100.0) * self.mac
    }

    /// The longitudinal span of the centre wing box, which is what the lower
    /// holds are split forward and aft of.
    pub fn wing_box_x_range(&self) -> (f64, f64) {
        (self.x_wing_le, self.x_wing_le + self.wing_root_chord)
    }

    /// The longitudinal stretch of the lower fuselage the lower holds cannot
    /// use because the centre wing box and the main-gear bay occupy it.
    ///
    /// The box begins at the front spar, not at the root leading edge: ahead
    /// of the front spar the belly is free hold floor, the leading-edge
    /// structure sits above the keel. The front spar is the first entry of
    /// the model's own spar list ([`alas_config::StructuresConfig`], 25 % of
    /// the local chord by default). The bay behind the rear spar runs to about
    /// the root trailing edge, so the stretch ends there. The frozen Python
    /// parity frame keeps the whole root chord, as it always has.
    pub fn lower_hold_gap_x_range(&self) -> (f64, f64) {
        let (start, end) = self.wing_box_x_range();
        if !self.strict_envelope {
            return (start, end);
        }
        let front_spar = alas_config::StructuresConfig::default()
            .spar_chord_fractions
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        if !front_spar.is_finite() || !(0.0..1.0).contains(&front_spar) {
            return (start, end);
        }
        (start + front_spar * self.wing_root_chord, end)
    }
}
