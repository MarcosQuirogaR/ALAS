// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Operating-empty centre-of-gravity references, one record per registered
//! aircraft.
//!
//! No manufacturer document at hand states a weighed OEW CG. What the Airbus
//! aircraft-characteristics documents publish are light-weight states with a
//! named CG: the ground-clearance and rescue figures draw the empty or
//! maintenance aircraft at a forward and/or an aft CG ("The dimensions are
//! given for: - A light weight, for an aircraft in maintenance configuration
//! with a FWD CG and an AFT CG", A380 AC 2-3-0 page 1). A forward state bounds
//! the empty CG from ahead and an aft state from behind, so where they exist
//! they are the reference, with the document, page, mass and state recorded.
//! Reading a clearance state as a bound on the OEW CG is this module's
//! interpretation, not something the documents say: the states are at 41, 45,
//! 125 and 300 t, not at the model OEW, and the operating items, residual fuel
//! or ballast that separate a drawn state from OEW move its CG; the A320 aft
//! state is the aft-CG configuration of a clearance drawing.
//!
//! Every other aircraft keeps Torenbeek's class band for its engine
//! arrangement, read on the page in *Synthesis of Subsonic Airplane Design*
//! (1982), Sec. 8.5.3: wing-podded engines "The c.g. corresponding to the OEW
//! is usually located at 25-30% MAC" (p. 299); rear-fuselage engines "very
//! near the rear limit (approximately 35-40% MAC)" (p. 299); engines or
//! propellers far forward "at about 20-25% MAC" (p. 300). A band is a class
//! statistic, not a measurement of the aircraft: every record names in
//! `evidence_missing` the document that would replace it and where to get it.
//!
//! Like the mass registry this is something the model is compared with,
//! never fitted to.

use serde::Serialize;

use super::sources::{A320_ACAP_REV46, A340_ACAP_REV33, A380_ACAP_REV20};
use super::OewSource;

/// Which side of the empty-aircraft CG a published light-weight state bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LightCgBound {
    /// A state drawn at a forward CG: the empty CG is not ahead of it.
    Forward,
    /// A state drawn at an aft CG: the empty CG is not behind it.
    Aft,
}

/// One light-weight state a manufacturer document draws at a stated CG.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PublishedLightCgState {
    /// The state as the document names it.
    pub state: &'static str,
    /// Aircraft mass of the state, kg.
    pub mass_kg: f64,
    /// CG of the state, % MAC in the document's frame (the preset's
    /// published reference wing).
    pub cg_pct_mac: f64,
    /// The side of the empty CG it bounds.
    pub bound: LightCgBound,
    /// Document, page and quote.
    pub source: OewSource,
}

/// Torenbeek's three general arrangements (Sec. 8.5.3, Fig. 8-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OewCgArrangement {
    /// Configuration 1: engines podded under the wing.
    WingPodded,
    /// Configuration 2: engines on the rear fuselage.
    RearEngined,
    /// Configuration 3: engines (in practice propellers) far forward.
    ForwardEngines,
}

impl OewCgArrangement {
    /// Torenbeek's OEW CG band for the arrangement, % MAC, and its page.
    pub fn class_band(self) -> (f64, f64, &'static str) {
        match self {
            Self::WingPodded => (
                25.0,
                30.0,
                "Torenbeek 1982 Sec. 8.5.3 p. 299, configuration 1",
            ),
            Self::RearEngined => (
                35.0,
                40.0,
                "Torenbeek 1982 Sec. 8.5.3 p. 299, configuration 2",
            ),
            Self::ForwardEngines => (
                20.0,
                25.0,
                "Torenbeek 1982 Sec. 8.5.3 p. 300, configuration 3",
            ),
        }
    }
}

/// The OEW CG reference of one registered aircraft.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OewCgReference {
    /// Registered preset name.
    pub preset: &'static str,
    /// Published light-weight states; empty when none is published.
    pub published: &'static [PublishedLightCgState],
    /// Torenbeek arrangement whose band applies when `published` is empty.
    pub arrangement: OewCgArrangement,
    /// The document that would give a weighed OEW CG (or the state the
    /// published ones lack) and where to get it.
    pub evidence_missing: &'static str,
}

const WBM: &str = "a weighed basic-empty CG from the type's Weight and Balance Manual (operator document): ask an operator or the manufacturer";
const BOEING: &str = "the Boeing airport-planning document states CGs only at maximum design taxi weight (section 7.3); needs the Weight and Balance Manual (operator document): ask an operator or Boeing";

const A320: &[PublishedLightCgState] = &[
    PublishedLightCgState {
        state: "empty weight for maintenance, forward CG",
        mass_kg: 45_000.0,
        cg_pct_mac: 17.0,
        bound: LightCgBound::Forward,
        source: OewSource {
            locator: "2-3-0 Ground Clearances, page 3, Figure 2-3-0-991-004-A01 (A320-200, PDF page 52)",
            quote: "45 000 kg (99 208 lb) EMPTY WEIGHT FOR MAINTENANCE FORWARD CG (17%)",
            ..A320_ACAP_REV46
        },
    },
    PublishedLightCgState {
        state: "aircraft configuration for clearances, aft CG",
        mass_kg: 41_000.0,
        cg_pct_mac: 26.5,
        bound: LightCgBound::Aft,
        source: OewSource {
            locator: "2-3-0 Ground Clearances, page 3, Figure 2-3-0-991-004-A01 (A320-200, PDF page 52); mass from 10-0-0 page 15, Figure 10-0-0-991-041-A01 (PDF page 434)",
            quote: "A/C CONFIGURATION CG (26.5%); 41 000 kg (90 389 lb) AFT CG (26.5%) A/C CONFIGURATION",
            ..A320_ACAP_REV46
        },
    },
];

const A340: &[PublishedLightCgState] = &[PublishedLightCgState {
    state: "empty weight for maintenance, aft CG (rescue and fire-fighting clearances)",
    mass_kg: 125_000.0,
    cg_pct_mac: 31.9,
    bound: LightCgBound::Aft,
    source: OewSource {
        locator: "10-0-0 Aircraft Rescue and Fire Fighting, page 15, Figure 10-0-0-991-038-A01 (A340-200/-300, PDF page 396)",
        quote: "125 000 kg (275 578 lb) AFT CG (31.9%)",
        ..A340_ACAP_REV33
    },
}];

const A380: &[PublishedLightCgState] = &[
    PublishedLightCgState {
        state: "light weight in maintenance configuration, forward CG",
        mass_kg: 300_000.0,
        cg_pct_mac: 29.0,
        bound: LightCgBound::Forward,
        source: OewSource {
            locator: "2-3-0 Ground Clearances, pages 1-2 (PDF pages 36-37)",
            quote: "A light weight, for an aircraft in maintenance configuration with a FWD CG and an AFT CG; 300 t FWD CG (29%)",
            ..A380_ACAP_REV20
        },
    },
    PublishedLightCgState {
        state: "light weight in maintenance configuration, aft CG",
        mass_kg: 300_000.0,
        cg_pct_mac: 44.0,
        bound: LightCgBound::Aft,
        source: OewSource {
            locator: "2-3-0 Ground Clearances, page 2 (PDF page 37); 10-0-0 page 15, Figure 10-0-0-991-012-A01 (PDF page 319)",
            quote: "300 t (44%) AFT CG; 300 000 kg (661 387 lb) AFT CG (44%)",
            ..A380_ACAP_REV20
        },
    },
];

const fn band(
    preset: &'static str,
    arrangement: OewCgArrangement,
    evidence_missing: &'static str,
) -> OewCgReference {
    OewCgReference {
        preset,
        published: &[],
        arrangement,
        evidence_missing,
    }
}

const RECORDS: &[OewCgReference] = &[
    OewCgReference {
        preset: "A320-200",
        published: A320,
        arrangement: OewCgArrangement::WingPodded,
        evidence_missing: WBM,
    },
    OewCgReference {
        preset: "A340-300",
        published: A340,
        arrangement: OewCgArrangement::WingPodded,
        evidence_missing: "a forward state: 2-3-0 draws the 125 t maintenance aircraft at a mid CG not printed in the text layer (read the drawing of AC Rev 33); else the Weight and Balance Manual (operator document)",
    },
    OewCgReference {
        preset: "A380-800",
        published: A380,
        arrangement: OewCgArrangement::WingPodded,
        evidence_missing: WBM,
    },
    band("A220-300", OewCgArrangement::WingPodded, "the A220 ACP Issue 013 states CGs only for its ACN/LCN loading cases; needs the Weight and Balance Manual (operator document): ask an operator or Airbus Canada"),
    band("B747-400", OewCgArrangement::WingPodded, BOEING),
    band("B787-9", OewCgArrangement::WingPodded, BOEING),
    band("DC-10", OewCgArrangement::WingPodded, "the DC/MD-10 airport-planning document states no OEW CG; needs the DC-10 Weight and Balance Manual (operator document, archive copies with former operators or Boeing). A tri-jet is none of Torenbeek's three arrangements; the wing-podded band is the nearest"),
    band("C919", OewCgArrangement::WingPodded, "no public COMAC airport-planning CG state found; needs the C919 Weight and Balance Manual (COMAC, not public)"),
    band("E195-E2", OewCgArrangement::WingPodded, "the Embraer E195-E2 Airport Planning Manual (public, Embraer website) is still to be read for a light-weight CG state; else the Weight and Balance Manual (operator document)"),
    band("A350-900", OewCgArrangement::WingPodded, "the A350 aircraft characteristics document states CGs only at its loading and clearance cases; needs the Weight and Balance Manual (operator document): ask an operator or Airbus"),
    band("C295", OewCgArrangement::ForwardEngines, "no public Airbus Defence and Space CG state for the empty C295 (TCDS EASA.A.186 prints the flight CG limits only); needs the C295 Weight and Balance Manual (operator or manufacturer document). Wing propellers ahead of the leading edge (Torenbeek p. 300)"),
    band("AVE", OewCgArrangement::WingPodded, "none obtainable: a concept aircraft defined only by its preset, so no weighed CG can exist"),
    band("ATR72-600", OewCgArrangement::ForwardEngines, "the ATR 72 load-and-balance worked example (13,700 kg at 27 % MAC, seen in a secondary citation, not read on a page): ATR 72 Weight and Balance Manual (operator document). Wing propellers ahead of the leading edge (Torenbeek p. 300: configuration 3 in propeller aircraft)"),
    band("LJ60", OewCgArrangement::RearEngined, "the Learjet 60 Weight and Balance Manual (operator document); the FlightSafety PTM fig 19-1 prints only the loading envelope"),
    band("A400M", OewCgArrangement::ForwardEngines, "the A400M Weight and Balance Manual (Airbus Defence and Space, restricted). Wing propellers ahead of the leading edge (Torenbeek p. 300)"),
];

/// Every OEW CG record.
pub fn oew_cg_references() -> &'static [OewCgReference] {
    RECORDS
}

/// The OEW CG record of a registered aircraft.
pub fn oew_cg_reference(preset: &str) -> Option<&'static OewCgReference> {
    RECORDS.iter().find(|record| record.preset == preset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_preset_has_exactly_one_record() {
        for name in crate::presets::available() {
            let count = RECORDS
                .iter()
                .filter(|record| record.preset == name)
                .count();
            assert_eq!(count, 1, "{name}");
        }
        assert_eq!(RECORDS.len(), crate::presets::available().len());
    }

    #[test]
    fn published_states_carry_their_page_and_quote_and_brackets_are_ordered() {
        for record in RECORDS {
            assert!(!record.evidence_missing.is_empty(), "{}", record.preset);
            for state in record.published {
                assert!(!state.source.locator.is_empty() && !state.source.quote.is_empty());
                assert!(state
                    .source
                    .quote
                    .contains(&format!("{}%", state.cg_pct_mac)));
            }
            let forward = record
                .published
                .iter()
                .filter(|s| s.bound == LightCgBound::Forward);
            let aft = record
                .published
                .iter()
                .filter(|s| s.bound == LightCgBound::Aft);
            for f in forward {
                for a in aft.clone() {
                    assert!(f.cg_pct_mac < a.cg_pct_mac, "{}", record.preset);
                }
            }
        }
    }
}
