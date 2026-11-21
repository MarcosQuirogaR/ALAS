// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Aerodromes deliberately corrected away from the frozen reference table.
//!
//! The parity fixtures (`golden/config/airports.json`, `golden/mission/*.json`,
//! `golden/route/*.json`) record the values of the reference implementation and
//! are never edited. Where the product table
//! (`crates/alas-config/data/airports.json`) corrects a reference value, the
//! correction is declared here once, with the value it replaced and the cause,
//! and every parity test that reads airports shares this table. A test must
//! first check that the reference still reads what `reference` says, then
//! compare the product against `corrected`.

/// `(elevation_m, toda_m, lda_m)` of one aerodrome.
pub type Field = [f64; 3];

/// One declared correction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirportCorrection {
    /// Four-letter ICAO code.
    pub icao: &'static str,
    /// The value the reference table carries.
    pub reference: Field,
    /// The value the product table carries.
    pub corrected: Field,
    /// Why the reference value is replaced.
    pub why: &'static str,
}

/// Every declared airport correction.
pub const AIRPORT_CORRECTIONS: [AirportCorrection; 3] = [
    AirportCorrection {
        icao: "EDDF",
        reference: [364.0, 4000.0, 4000.0],
        corrected: [111.0, 4000.0, 4000.0],
        why: "the reference 364 is the field's 364 ft entered as metres (111 m); source OurAirports, not the DFS AIP",
    },
    AirportCorrection {
        icao: "SEQM",
        reference: [2813.0, 3970.0, 3970.0],
        corrected: [2411.0, 4098.0, 4098.0],
        why: "the reference 2813 m is the closed old Quito airport; the row's coordinates are the new Mariscal Sucre (7910 ft = 2411 m, the runway 36 threshold, the highest point of the landing area; OurAirports gives 7841 ft = 2390 m); LDA 13445 ft = 4098 m, TODA = LDA by convention (no declared TODA found); sources are secondary, not the Ecuador AIP",
    },
    AirportCorrection {
        icao: "LEMD",
        reference: [610.0, 4100.0, 4100.0],
        corrected: [609.0, 4179.0, 3365.0],
        why: "AIP Spain AD 2.3 / 2.13 (AIRAC 09/26): elevation 609 m; the best usable balanced field is 36L, TORA 4179 = ASDA 4179 (TODA 4609 includes a 430 m clearway that a rejected take-off cannot use, so the table keeps min(TORA, ASDA), D47); best LDA 3365 m (18R)",
    },
];

/// The correction declared for `icao`, if any.
#[must_use]
pub fn correction_for(icao: &str) -> Option<&'static AirportCorrection> {
    AIRPORT_CORRECTIONS
        .iter()
        .find(|correction| correction.icao == icao)
}
