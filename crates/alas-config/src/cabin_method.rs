// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Selection of the cabin-equipment and operating-item mass method.

use serde::{Deserialize, Serialize};

use crate::{Kind, Leaf};

/// Which method prices the cabin equipment and the occupant-driven operating
/// items.
///
/// These two groups together are where the FLOPS transport equations depart
/// furthest from a modern aircraft, and they depart in **both** directions,
/// which is why the choice is a method selection rather than a coefficient.
///
/// The comparison that establishes it uses Airbus' own accounting boundary
/// (Fuchte 2013, Table 1: passenger seats are ATA 60-3 and galley structure
/// ATA 60-2, both **operational items**, not furnishings), so the like-for-like
/// quantity is FLOPS `WFURN` plus the occupant-driven part of `WOPIT`:
///
/// | aircraft | Airbus accounting | LTH relations | FLOPS equations |
/// |---|---:|---:|---:|
/// | A320-200, 150 seats | 56.3 kg/seat | 55.7 kg/seat | 54.2 kg/seat |
/// | A340-300, 290-295 seats | 98.0 kg/seat | 99.3 kg/seat | 59.0 kg/seat |
///
/// On a single-aisle the three agree to within four percent. On a long-haul
/// three-class widebody the two independent sources agree with each other and
/// FLOPS is 40 % below both - about 11.7 t on the A340-300, which is two
/// thirds of that aircraft's whole operating-empty-mass deficit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CabinEquipmentMethod {
    /// NASA/TM-2017-219627 Vol. I equation 110 for the furnishings and
    /// equations 119-126 for the operating items, as published.
    ///
    /// This is the default and the auditable baseline: it reproduces the
    /// source equation set exactly. Its published validation population is
    /// *"commercial transport and military aircraft developed between the
    /// 1940s and 1970s"* (Horvath & Wells, NASA NTRS 20190000431), which
    /// contains no three-class long-haul cabin of the modern kind.
    #[default]
    FlopsTransportV1,
    /// The Luftfahrttechnisches Handbuch civil-transport relations for the
    /// furnishings and the operating items, MA 401 12-01 B (Dorbath, 2013),
    /// as reproduced with their coefficients by Pape (2018) equations 2.14 to
    /// 2.16, pp. 22-23:
    ///
    /// * furnishings, **excluding** passenger seats:
    ///   `m_fur = 200 + 3.35 (l_fus d_fus)^1.3368`, metres and kilograms;
    /// * operating items, **including** passenger seats:
    ///   `m_opp = 32.907 n_pax^1.021` on a short/medium-haul aircraft and
    ///   `m_opp = 35.782 n_pax^1.1141` on a long-haul one.
    ///
    /// Stated validity, in full: *"bezieht sich ausschliesslich auf zivile
    /// Verkehrsflugzeuge"*, restricted to those for which *"die maximale
    /// Abflugmasse (MTOW) mindestens 40 Tonnen betraegt bzw. sich mindestens
    /// 70 Passagiersitze an Bord befinden"* - a civil transport with a maximum
    /// takeoff mass of **at least 40 t or at least 70 passenger seats**.
    /// [`Self::for_civil_transport_size`] applies exactly that statement, both
    /// clauses, and nothing else.
    ///
    /// The author's own operating-empty-mass errors are +2.2 % (A320-200),
    /// +0.8 % (A330-200), +3.4 % (A340-300) and +7.5 % (B737-200), and the
    /// author states they are **in-sample**: the relations were fitted
    /// retroactively on the same four aircraft they are validated against
    /// (Pape 2018, Ausblick p. 41).
    ///
    /// **Three limits this method does not escape.** Its furnishings term is a
    /// single-tube `length x diameter` proxy exactly as FLOPS equation 110 is,
    /// so it is no more in domain on a double-deck fuselage than FLOPS; its
    /// operating-item exponent `n_pax^1.1141` is superlinear and its fitted
    /// seat range is 130-295, so a 525-seat cabin is an extrapolation of
    /// 78-88 % beyond the population; and that fitted population is four
    /// turbofan aircraft of 52-233 t, which contains no turboprop even though
    /// the seat clause of the domain statement admits one.
    LthCivilTransportV1,
    /// Torenbeek's systems-and-furnishings group for a twin-engine propeller
    /// transport, `W_sys = 0.11 MTOW + 0.768 k_fc MTOW^(2/3)` with
    /// `k_fc = 0.88`, plus 15 kg per seat, with the FLOPS operating items kept
    /// (D. Scholz, HAW Hamburg lecture notes, Sect. 8.2, eq. 8.2.18 and
    /// 8.2.20, a secondary source after E. Torenbeek, *Synthesis of Subsonic
    /// Airplane Design*, 1982; the primary source was not verified).
    ///
    /// Selected for a shaft-power installation below 40 t, where neither the
    /// FLOPS fit population nor the four-turbofan LTH population contains a
    /// comparable cabin. It complements the LTH mass clause: the 40 t boundary
    /// is an engineering estimate that follows the LTH statement's own mass
    /// figure, not a fitted threshold. Shaft-power aircraft of 40 t or more
    /// keep the LTH selection.
    RegionalTurbopropV1,
    /// Torenbeek's furnishing-and-equipment group for transport and executive
    /// aircraft, item by item, with the occupant operating items of the same
    /// book: E. Torenbeek, *Synthesis of Subsonic Airplane Design* (1982),
    /// Table 8-12 (p. 291), the executive seats of Table 3-2 (p. 76) and
    /// Table 8-13 (p. 292). It replaces FLOPS equation 110 (furnishings) and
    /// equation 124 (passenger service) and nothing else.
    ///
    /// Selected by the declared aircraft class, never by name: a preset that
    /// declares [`crate::presets::AircraftClass::BusinessJet`] takes it. FLOPS
    /// prices every passenger as an airline tourist seat (44 lb, eq. 110) with
    /// an airline service allowance (eq. 124); its fit population contains no
    /// executive cabin. The interior it prices is declared in
    /// [`ExecutiveCabinDeclaration`]; without one the evaluation is refused.
    ExecutiveJetV1,
}

/// The installed executive interior that
/// [`CabinEquipmentMethod::ExecutiveJetV1`] prices: published internal
/// dimensions and declared monuments, never a mass.
///
/// Every field is a Torenbeek (1982) Table 8-12 / 8-13 input: `S_cf`, `V_pc`
/// and `V_ch` and the galley, toilet, oxygen and overwater categories.
///
/// `S_cf` and `V_pc` are published for one cabin length,
/// [`Self::passenger_compartment_length_m`]; the method scales both with the
/// built passenger-compartment length (constant cabin section), so a design or
/// optimisation run that moves the fuselage moves them too, and a baseline
/// whose built cabin is the published one reads them unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutiveCabinDeclaration {
    /// Cabin floor area, galleys and toilets included, flight deck excluded,
    /// m^2 (Torenbeek `S_cf`), at [`Self::passenger_compartment_length_m`].
    pub cabin_floor_area_m2: f64,
    /// Passenger cabin volume, galleys and toilets included, flight deck
    /// excluded, m^3 (Torenbeek `V_pc`), at
    /// [`Self::passenger_compartment_length_m`].
    pub passenger_cabin_volume_m3: f64,
    /// Cabin length, flight deck excluded, that the published floor area and
    /// volume describe, m.
    pub passenger_compartment_length_m: f64,
    /// Total baggage and cargo compartment volume, m^3 (Torenbeek `V_ch`).
    pub cargo_hold_volume_m3: f64,
    /// The part of [`Self::cargo_hold_volume_m3`] that is a baggage
    /// compartment inside the published passenger cabin volume (pressurised
    /// and accessible in flight), m^3. It is removed from `V_pc` so that
    /// `V_pc + V_ch` counts it once. `None` when the source places such a
    /// compartment inside the cabin volume but does not publish its share;
    /// the linings term then counts that share twice (an upper bound).
    #[serde(default)]
    pub in_cabin_baggage_volume_m3: Option<f64>,
    /// Maximum passenger count for certification: Torenbeek's `N_pax` of the
    /// Table 8-12 oxygen row, not the installed seat count.
    pub certified_passenger_count: u32,
    /// Main-meal galleys installed.
    pub main_meal_galleys: u32,
    /// Snack pantries (refreshment centres) installed.
    pub snack_pantries: u32,
    /// Coffee bars installed.
    pub coffee_bars: u32,
    /// Toilets (water closets) installed, Torenbeek `N_wc`.
    pub lavatories: u32,
    /// Certified maximum operating altitude, m. Above 7,620 m (25,000 ft)
    /// Table 8-12 prices the high-altitude oxygen system.
    pub maximum_operating_altitude_m: f64,
    /// Whether the aircraft is equipped for extended overwater flights.
    pub extended_overwater: bool,
}

impl CabinEquipmentMethod {
    /// Every serialized name, in declaration order.
    pub const OPTIONS: &'static [&'static str] = &[
        "flops_transport_v1",
        "lth_civil_transport_v1",
        "regional_turboprop_v1",
        "executive_jet_v1",
    ];

    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FlopsTransportV1 => "flops_transport_v1",
            Self::LthCivilTransportV1 => "lth_civil_transport_v1",
            Self::RegionalTurbopropV1 => "regional_turboprop_v1",
            Self::ExecutiveJetV1 => "executive_jet_v1",
        }
    }

    /// Whether this selection declares the regional turboprop class: a
    /// shaft-power transport below the 40 t LTH mass figure.
    ///
    /// The class is decided once, by [`Self::for_civil_transport_size`], and
    /// other methods of the same class (the pressurized fuselage of the FLOPS
    /// structural group) read it here, so one discriminator selects them all
    /// and none switches while a search moves the takeoff mass.
    pub const fn is_regional_turboprop_class(self) -> bool {
        matches!(self, Self::RegionalTurbopropV1)
    }

    /// The LTH relations' own stated validity domain, applied as a method
    /// selection: MTOM **at least** 40 t **or at least** 70 passenger seats.
    ///
    /// This is the one selection rule in the product, used for a registered
    /// preset and for a configuration built without one alike, so the same
    /// aircraft cannot be priced by two different accounting systems depending
    /// on how its configuration was produced. It reads only size; it never
    /// reads a resulting error, and there is no per-aircraft exception.
    ///
    /// Both clauses of the LTH statement are applied for a jet. A shaft-power
    /// aircraft is outside the population the relations were fitted on, so
    /// below the mass figure it takes the regional turboprop relation rather
    /// than the seat clause.
    ///
    /// `mtom_kg` is the maximum takeoff mass in kilograms and
    /// `passenger_seats` the installed seat count; either may be absent, and
    /// an absent clause simply cannot admit the aircraft. With both absent the
    /// result is the published FLOPS baseline.
    ///
    /// `shaft_power` is true for a shaft-power (turboprop) installation. Such
    /// an aircraft with a known mass below the LTH mass figure takes
    /// [`Self::RegionalTurbopropV1`], whatever its seat count, because the LTH
    /// seat clause is a domain statement about jets: its fitted population
    /// contains no turboprop. A shaft-power aircraft at or above that mass
    /// follows the LTH rule unchanged.
    pub fn for_civil_transport_size(
        mtom_kg: Option<f64>,
        passenger_seats: Option<i64>,
        shaft_power: bool,
    ) -> Self {
        /// "mindestens 40 Tonnen", kg.
        const LTH_MINIMUM_TAKEOFF_MASS_KG: f64 = 40_000.0;
        /// "mindestens 70 Passagiersitze".
        const LTH_MINIMUM_PASSENGER_SEATS: i64 = 70;
        let by_mass =
            mtom_kg.is_some_and(|mass| mass.is_finite() && mass >= LTH_MINIMUM_TAKEOFF_MASS_KG);
        let light =
            mtom_kg.is_some_and(|mass| mass.is_finite() && mass < LTH_MINIMUM_TAKEOFF_MASS_KG);
        let by_seats = passenger_seats.is_some_and(|seats| seats >= LTH_MINIMUM_PASSENGER_SEATS);
        if shaft_power && light {
            Self::RegionalTurbopropV1
        } else if by_mass || by_seats {
            Self::LthCivilTransportV1
        } else {
            Self::FlopsTransportV1
        }
    }
}

impl Leaf for CabinEquipmentMethod {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}
