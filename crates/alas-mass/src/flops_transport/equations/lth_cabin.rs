// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! LTH civil-transport furnishings and operating-item relations, selected by
//! [`alas_config::CabinEquipmentMethod::LthCivilTransportV1`].

/// LTH MA 401 12-01 B furnishings, **excluding** passenger seats, kg.
///
/// `m_fur = 200 + 3.35 (l_fus d_fus)^1.3368`, both lengths in metres, as
/// reproduced with its coefficients by Pape (2018) equation 2.14 p. 22. It is
/// the alternative to FLOPS equation 110 selected by
/// [`alas_config::CabinEquipmentMethod::LthCivilTransportV1`]; the seats it
/// leaves out are inside [`lth_operating_items_kg`], which is why the two are
/// only ever evaluated together.
///
/// `d_fus` is the **average** fuselage diameter `D_av = (width + depth) / 2`,
/// not the maximum width. The source does not define the symbol in words, so
/// it is decided here by reproducing its own two worked examples (Pape 2018
/// Tab. 3.24 p. 31), both to the tenth of a kilogram:
///
/// | case | `l_fus` m | candidate `d_fus` m | this relation kg | Pape kg |
/// |---|---:|---|---:|---:|
/// | A320-200 | 37.57 | width 3.95 | 2,878.1 | n/a |
/// | A320-200 | 37.57 | depth 4.14 | 3,051.7 | n/a |
/// | A320-200 | 37.57 | `sqrt(w d)` 4.0439 | 2,963.5 | n/a |
/// | **A320-200** | 37.57 | **`(w + d)/2` 4.045** | **2,964.5** | **2,964.5** |
/// | **A340-300** | 62.47 | **circular 5.64** | **8,707.6** | **8,707.6** |
///
/// Only the arithmetic mean reproduces the published A320-200 value, and the
/// circular A340-300 case, where every candidate coincides, reproduces
/// exactly as well, which confirms the coefficients and the exponent
/// independently of the convention. It is also the same `D_av` that FLOPS
/// equation 56 uses in this crate (`structure.rs`), so the two methods read
/// the same quantity from the same geometry.
///
/// Feeding the maximum width instead understates a non-circular fuselage:
/// 1,723.8 kg on the A380-800 (7.14 m wide, 8.41 m deep) and 86.4 kg on the
/// A320-200. Every other registered preset has a circular section and is
/// unaffected.
pub(super) fn lth_furnishings_kg(fuselage_length_m: f64, average_fuselage_diameter_m: f64) -> f64 {
    200.0 + 3.35 * (fuselage_length_m * average_fuselage_diameter_m).powf(1.336_8)
}

/// LTH MA 401 12-01 B operating items, **including** passenger seats, kg.
///
/// `m_opp = 32.907 n_pax^1.021` short/medium-haul and
/// `m_opp = 35.782 n_pax^1.1141` long-haul (Pape 2018 equations 2.15 and
/// 2.16, p. 23). It replaces the occupant-driven FLOPS operating items (the
/// cabin crew and their baggage, the flight crew and theirs, and the passenger
/// service items), including unusable fuel and engine/APU oil: these fluids
/// are listed in Pape section 2.11, pp. 22-23. Their explicit rows are allocated
/// within this total. Cargo container tare remains separate.
///
/// The relation describes a current cabin. Beside `m_fur` it gives 8,499 kg on
/// Boeing's 2008-technology 154-seat narrow-body, whose published furnishings
/// and operational items are 8,259 kg (Bradley and Droney, NASA/CR-2011-216847,
/// Table 5.9, p. 61; `examples/sugar_free_statement.rs`), and FLOPS equations
/// 110 and 124 give 8,129 kg on the same aircraft. A sum of 1970s and 1980s
/// unit masses (Torenbeek 1982 Table 8-13, NASA CR-144935 Table VI) gives
/// 6,423 kg there: it reproduces the 1980s A320 statement (Obert 2009, Fig.
/// 42.25: 6,028 kg at 73.5 t) and is 1.8 t light on a present-day cabin, so it
/// is not used.
pub(super) fn lth_operating_items_kg(passengers: usize, long_haul: bool) -> f64 {
    let count = passengers as f64;
    if count <= 0.0 {
        return 0.0;
    }
    if long_haul {
        35.782 * count.powf(1.114_1)
    } else {
        32.907 * count.powf(1.021)
    }
}
