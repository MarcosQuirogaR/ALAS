// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ice protection of the horizontal stabiliser of the registered types, from
//! the published type descriptions that state it. Only a document that says so
//! gives a status; a type without one carries none and is checked as
//! unprotected (`AircraftReferenceData::horizontal_tail_ice_protection`).
//!
//! Types with no document found (2026-11-16 search of the type certificate
//! data sheets, the manufacturers' aircraft characteristics and the public
//! accident and training literature): A220-300 (a student flashcard deck only),
//! A350-900 (an abstract listing the horizontal tail plane among "unprotected
//! parts", paper not read), A380-800, A400M, 747-400, 787-9, C295, C919,
//! DC-10-30, E195-E2, and the AVE, a clean-sheet design.

/// Whether the horizontal stabiliser of a type carries an ice-protection
/// system, as the type's published description states it. It selects the ice
/// accretion the landing balance checks the tail against (CS-25 Appendix C
/// Part II: the critical accretion on unprotected surfaces, and the accretion
/// of normal system operation on protected ones).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailIceProtection {
    /// No system on the stabiliser leading edge: holding ice.
    Unprotected,
    /// A pneumatic, hot-air or electrothermal system: inter-cycle ice.
    Protected,
}

/// A type's published tail ice-protection status and the page that states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcedTailIceProtection {
    /// The status the document states.
    pub protection: TailIceProtection,
    /// Document title, edition and page.
    pub source: &'static str,
}

/// Learjet 60: electrically heated horizontal stabiliser, from the airplane
/// flight manual approved by the FAA.
pub(crate) const LJ60: SourcedTailIceProtection = SourcedTailIceProtection {
    protection: TailIceProtection::Protected,
    source: "Learjet 60 Airplane Flight Manual FM-123, FAA approved 15 Jan 1993, Change 37, \
             Normal Procedures, Anti-Ice Systems, p. 2-47: electrically heated systems include \
             the horizontal stabilizer",
};

/// ATR 72: pneumatic de-icing boots on the horizontal stabiliser leading edge
/// (the vertical stabiliser's are optional). The document describes the
/// ATR 72-212 of the accident; the boots are the same on the -600.
pub(crate) const ATR72_600: SourcedTailIceProtection = SourcedTailIceProtection {
    protection: TailIceProtection::Protected,
    source: "NTSB/AAR-96/01, In-flight icing encounter and loss of control, American Eagle \
             flight 4184, ATR 72-212 N401AM, 31 Oct 1994, Vol. 1, 1996, sect. 1.6.6 (p. 43) \
             and p. 47: de-icing boots on the horizontal stabilizer leading edges",
};

/// The A320 family (A300, A310, A330 and A340 inside the document's scope):
/// the tailplane and fin carry no ice protection.
const AIRBUS_COLD_WEATHER: &str = "Airbus Industrie, Getting to grips with Cold Weather \
    Operations - A Flight Operations View, AI/ST-F 945.9843/99, sect. A3.1.1, p. 38: the \
    tailplane and the fin are not de-iced";

/// A320-200 (A320 family).
pub(crate) const A320_200: SourcedTailIceProtection = SourcedTailIceProtection {
    protection: TailIceProtection::Unprotected,
    source: AIRBUS_COLD_WEATHER,
};

/// A340-300 (the same Airbus document, generic to the family).
pub(crate) const A340_300: SourcedTailIceProtection = SourcedTailIceProtection {
    protection: TailIceProtection::Unprotected,
    source: AIRBUS_COLD_WEATHER,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_names_its_document_and_page() {
        for status in [LJ60, ATR72_600, A320_200, A340_300] {
            assert!(status.source.contains("p. "), "{}", status.source);
        }
        assert_eq!(LJ60.protection, TailIceProtection::Protected);
        assert_eq!(A320_200.protection, TailIceProtection::Unprotected);
    }
}
