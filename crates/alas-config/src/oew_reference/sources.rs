// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The revision-locked document locators the OEW reference records cite.

use super::{OewInclusionList, OewSource, OewSourceTier};

pub(super) const RETRIEVED: &str = "2026-09-12";

pub(super) const fn unknown() -> OewInclusionList {
    OewInclusionList::unknown()
}

pub(super) const A320_ACAP_REV46: OewSource = OewSource {
    document: "A320 Aircraft Characteristics - Airport and Maintenance Planning",
    publisher: "Airbus",
    revision: "Revision 46",
    date: "2026-07-01",
    locator: "",
    url: "https://mediaassets.airbus.com/pm_38_916_916266-iujedqawwy.pdf?fileName=aca32001-jul-2026-2.pdf",
    local_path: ".agent/evidence/manufacturer/airbus-a320-ac-2026-07.pdf",
    retrieved: RETRIEVED,
    quote: "",
    tier: OewSourceTier::ManufacturerPlanningDocument,
};

pub(super) const A320_F_HDRF_SHEET: OewSource = OewSource {
    document: "A320-214 operator specification sheet (F-HDRF)",
    publisher: "operator / aviapages listing",
    revision: "undated sheet",
    date: "",
    locator: "weights table",
    url: "",
    local_path: ".agent/data/a320-flops-audit-evidence/sources/operator-f-hdrf-a320-spec.pdf",
    retrieved: "2026-09-11",
    quote: "MTOW 77,000 kg; MLW 64,500 kg; MZFW 61,000 kg; max fuel 19,476 kg; empty weight 41,052 kg; max payload 19,087 kg; 180 seats",
    tier: OewSourceTier::OperatorRecord,
};

pub(super) const A220_ARP: OewSource = OewSource {
    document: "A220 Aircraft Recovery Publication, data module BD500-A-J08-41-02-01AAA-030A-A (weight and balance)",
    publisher: "Airbus Canada",
    revision: "BD500-3AB48-10400-00",
    date: "2020-06-17 (data module); publication May/August 2026",
    locator: "Table 2 Design weights, page 3; Table 3 Operating items; section 2 weight definitions",
    url: "",
    local_path: ".agent/data/flops-refinement-evidence/sources/airbus-a220-arp-aug-2026.pdf (PDF pages 99-102)",
    retrieved: RETRIEVED,
    quote: "OEW 81,900 lb 37,149 kg; Max Payload 41,100 lb 18,643 kg. The OEW includes the MEW plus the weight of standard and operational items such as: unusable fuel, engine oil, seats, crew and their baggage, galley equipment, consumables, potable water, waste tank pre-charge, manuals, etc.",
    tier: OewSourceTier::ManufacturerPlanningDocument,
};

pub(super) const A220_ACP: OewSource = OewSource {
    document: "A220 Aircraft Characteristics - Airport and Maintenance Planning, DM BD500-A-J06-10-00-00AAA-030A-A",
    publisher: "Airbus Canada",
    revision: "Issue 013",
    date: "2025-11-27",
    locator: "general characteristics table (PDF pages 29-31)",
    url: "https://www.aircraft.airbus.com/sites/g/files/jlcbta126/files/2025-12/A220-ACP-Issue013-00-27Nov2025.pdf",
    local_path: ".agent/evidence/manufacturer/airbus-a220-acp-2025-11.pdf",
    retrieved: RETRIEVED,
    quote: "Standard seating capacity 140; Operating Weight Empty (OWE) 81,750 lb (37 081 kg); Maximum Zero Fuel Weight (MZFW) 123,000 lb (55 792 kg)",
    tier: OewSourceTier::ManufacturerPlanningDocument,
};

pub(super) const A340_ACAP_REV33: OewSource = OewSource {
    document: "A340-200/-300 Aircraft Characteristics - Airport and Maintenance Planning",
    publisher: "Airbus",
    revision: "Revision 33",
    date: "2025-12-01",
    locator: "2-14-0 Jacking for Maintenance, page 9, Figure 2-14-0-991-012-B01 (PDF page 143)",
    url: "https://www.aircraft.airbus.com/sites/g/files/jlcbta126/files/2025-12/AC_A340-200-300_20251201.pdf",
    local_path: ".agent/evidence/manufacturer/airbus-a340-200-300-ac-2025-12.pdf",
    retrieved: RETRIEVED,
    quote: "AIRCRAFT ON WHEELS WITH STANDARD TIRES, OEW 131 215 kg (279 279 lb)",
    tier: OewSourceTier::ManufacturerPlanningDocument,
};

pub(super) const A380_AGGREGATOR: OewSource = OewSource {
    document: "flugzeuginfo.net A380-800 data page and comparable compilations",
    publisher: "aggregator",
    revision: "accessed 2026-09-04",
    date: "",
    locator: "typical three-class operating empty weight",
    url: "",
    local_path: ".agent/validation/A380-800.json (mass.oew_kg)",
    retrieved: "2026-09-04",
    quote: "typical operating empty weight 277,000 kg (values consulted span 270.1 t to 285 t)",
    tier: OewSourceTier::Aggregator,
};

pub(super) const A380_ACAP_REV20: OewSource = OewSource {
    document: "A380 Aircraft Characteristics - Airport and Maintenance Planning",
    publisher: "Airbus",
    revision: "Revision 20",
    date: "2025-12-01",
    locator: "2-3-0 ground clearances (PDF page 37)",
    url:
        "https://www.aircraft.airbus.com/sites/g/files/jlcbta126/files/2025-12/AC_A380_20251201.pdf",
    local_path: ".agent/evidence/manufacturer/airbus-a380-ac-2025-12.pdf",
    retrieved: RETRIEVED,
    quote: "MAXIMUM JACKING WEIGHT = 333 700 kg (735 682 lb)",
    tier: OewSourceTier::ManufacturerPlanningDocument,
};

pub(super) const B787_ACAP_REV_L: OewSource = OewSource {
    document: "787 Airplane Characteristics for Airport Planning, D6-58333",
    publisher: "Boeing",
    revision: "Revision L (superseded; not hosted by Boeing)",
    date: "2015-12",
    locator: "page 2-3, typical two-class 290-seat operating empty weight",
    url: "",
    local_path: "",
    retrieved: "not retrieved; value carried from .agent/validation/B787-9.json",
    quote: "284,000 lb (128,850 kg) attributed to Rev L; Rev Q and Rev P print no operating-empty-weight row",
    tier: OewSourceTier::ManufacturerPlanningDocument,
};

pub(super) const DC10_ACAP: OewSource = OewSource {
    document: "DC/MD-10 Airplane Characteristics for Airport Planning, DAC-67803A",
    publisher: "McDonnell Douglas / Boeing",
    revision: "Revision A",
    date: "2004-04",
    locator: "Figure 2.1 General Airplane Characteristics, Model DC-10 Series 10, 30 and 40, document page 4 (PDF page 10), Series 30 passenger column and the 572,000-pound MTOGW footnote",
    url: "https://www.boeing.com/content/dam/boeing/v2/airports/acaps/DC10.pdf",
    local_path: ".agent/evidence/manufacturer/boeing-dc10.pdf",
    retrieved: RETRIEVED,
    quote: "OPERATING WEIGHT EMPTY POUNDS 266,191 KILOGRAMS 120,742 ... FOR 572,000-POUND MTOGW: ADD 379 POUNDS TO OWE AND SUBTRACT 379 POUNDS FROM MAXIMUM STRUCTURAL PAYLOAD; INCREASE LANDING WEIGHT TO 421,000 POUNDS",
    tier: OewSourceTier::ManufacturerPlanningDocument,
};

pub(super) const DC10_ACAP_30CF: OewSource = OewSource {
    locator: "Figure 2.1 General Airplane Characteristics, Model DC-10 Series 10CF, 30CF, 40CF and MD-10, document page 5 (PDF page 11), Series 30CF passenger-mode column",
    quote: "OPERATING WEIGHT EMPTY POUNDS 268,751 KILOGRAMS 121,904",
    ..DC10_ACAP
};

pub(super) const ATR_FACTSHEET: OewSource = OewSource {
    document: "ATR 72-600 Factsheet",
    publisher: "ATR",
    revision: "2020 product factsheet",
    date: "2020-07",
    locator: "weights table",
    url: "https://www.atr-aircraft.com/wp-content/uploads/2020/07/Factsheets_-_ATR_72-600.pdf",
    local_path: ".agent/evidence/manufacturer/atr72-600-factsheet-2020.pdf",
    retrieved: RETRIEVED,
    quote: "typical in-service operational empty weight 13,450 kg (29,652 lb)",
    tier: OewSourceTier::ManufacturerPlanningDocument,
};

/// Jenkinson, Simpkin & Rhodes, *Civil Jet Aircraft Design* companion site,
/// Appendix Data A Table 1 (weights).
///
/// A published textbook compilation, not a manufacturer document: the site's
/// own `word-of-caution` page states that the data *"still originates from
/// manufacturers sources"* and *"requires careful interpretation since each
/// manufacturer may define the data in a different way"*, and the row is
/// labelled *"Operational empty"* with **no inclusion list anywhere on the
/// site**. It is therefore registered at [`OewSourceTier::Aggregator`] — the
/// tier defined as a compilation with no primary document behind it — and only
/// ever as a secondary anchor in `other_published_values`, never as a record's
/// comparable value. All Data A quantities are SI.
///
/// The one row that must **not** be taken from this book is `A3XX-100`: it is a
/// pre-programme-launch projected configuration (817 m^2 wing, 540 t MTOW, 555
/// three-class seats) and is not the A380-800. It is deliberately absent here.
pub(super) const ELSEVIER_DATA_A: OewSource = OewSource {
    document: "Civil Jet Aircraft Design (Jenkinson, Simpkin & Rhodes), companion site Appendix Data A, Table 1",
    publisher: "Elsevier / Butterworth-Heinemann (ISBN 9780340741528)",
    revision: "companion site as published",
    date: "",
    locator: "data-a/table-1/table.htm, row 'Operational empty'",
    url: "https://booksite.elsevier.com/9780340741528/appendices/default.htm",
    local_path: "",
    retrieved: "2026-09-16",
    quote: "The data in these table has been validated where possible but it still originates from manufacturers sources. The information requires careful interpretation since each manufacturer may define the data in a different way.",
    tier: OewSourceTier::Aggregator,
};

pub(super) const B777X_ACAP_REV_G: OewSource = OewSource {
    document: "777-9 Airplane Characteristics for Airport Planning, D6-86073",
    publisher: "Boeing",
    revision: "Revision G",
    date: "2025-09",
    locator: "section 2 (weights) and 3 (payload/range)",
    url: "https://www.boeing.com/content/dam/boeing/v2/airports/acaps/777X_Rev_G.pdf",
    local_path: ".agent/data/flops-refinement-evidence/sources/boeing-777x-acap-rev-g.pdf",
    retrieved: "2026-09-11",
    quote: "no numeric operating empty weight; payload/range data will be provided at a later date",
    tier: OewSourceTier::ManufacturerPlanningDocument,
};

/// Early 777-9X projection repeated in an independent aviation report.
///
/// The current Boeing Rev. G planning document still gives no numeric OEW.
/// This value is retained only as a clearly secondary, pre-certification
/// context row; it is not the AVE reference and cannot enter validation.
pub(super) const B777X_SECONDARY_PROJECTION: OewSource = OewSource {
    document: "Airbus, Boeing in game of thrones for widebody dominance",
    publisher: "Aspire Aviation (reported by an independent aviation forum)",
    revision: "2014-07-11 report; secondary copy/quote",
    date: "2014-07-11",
    locator: "forum quotation of the Aspire Aviation 4-class, 300-seat 777-9X estimate",
    url: "https://www.aviazionecivile.it/threads/thread-airbus-330-neo.132876/",
    local_path: "",
    retrieved: RETRIEVED,
    quote: "a 4-class 300-seat 777-9X has an OEW of 188,241 kg (415,000 lbs), attributed to Aspire Aviation's multiple Boeing sources",
    tier: OewSourceTier::Aggregator,
};
