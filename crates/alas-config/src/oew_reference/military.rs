// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The OEW reference record of the registered military transport.

use super::sources::unknown;
use super::{
    OewApplicability, OewReference, OewReferenceConfiguration, OewSource, OewSourceTier,
    PublishedOewValue,
};

const BUNDESWEHR_A400M: OewSource = OewSource {
    document: "A400M transport aircraft (German Air Force operator page)",
    publisher: "Bundeswehr",
    revision: "web page, undated",
    date: "",
    locator: "technical data: empty weight",
    url: "https://www.bundeswehr.de/en/organization/german-air-force/a400m",
    local_path: "",
    retrieved: "2026-10-04",
    quote: "EMPTY WEIGHT: 78,6 t",
    tier: OewSourceTier::OperatorRecord,
};

pub(super) static A400M: OewReference = OewReference {
    preset: "A400M",
    applicability: OewApplicability::ConditionalMismatch,
    reference_oew_kg: Some(78_600.0),
    definition_label: "empty weight, operator page; manufacturer empty weight, basic empty weight and operating empty weight are not distinguished",
    reference_configuration: OewReferenceConfiguration {
        model: "A400M (military standard)",
        weight_variant: "141,000 kg MTOW military (Airbus brochure TMMA0026/01/2025 p024)",
        mtow_kg: Some(141_000.0),
        engine: "TP400-D6",
        modification_state: "series production military standard, role equipment not stated",
        cabin: "military cargo hold, no passenger seats",
    },
    differences_from_preset: &[
        "the operator page does not say whether 78.6 t is a manufacturer empty weight or an operating empty weight, and states no inclusion list",
        "optional role kits (refuelling pods, hose drum unit, defensive aids, armour) are not itemised by the source and are not modelled; the rear ramp is inside the FLOPS military-cargo-floor fuselage (CARGF 1, a door of the body group by the CR-151970 pp4-3 and 4-4 definition); the cargo-handling system and the handling gear are separate items",
        "older secondary sources give 76,500 kg",
        "class evidence, group by group: the FLOPS wing equation on the printed inputs of the military transports of Beltramo et al., NASA CR-151970 (1977), Table 5.2 p5-6, gives 0.96-1.44 of the printed wing weights (C-130A, C-130E, KC-135A, C-133B, C-141A, C-5A) and 0.88-1.33 of the tail weights; against the same report the preset gear is 3.2 percent of MTOW (C-130E and C-141A 3.3 percent, Table 5.4 p5-19), the fuselage about 6.5 lb/ft2 of wetted area (C-141A 5.6, C-130E 4.3, Table 5.2) and systems plus furnishings about 10.7 t (C-141A about 8.5 t, Tables 5.12-5.29 pp5-62 to 5-114)",
        "empty-mass fraction: the preset manufacturer empty mass over design gross mass is 0.442 (2026-11-07 model state: OEW 63,175 kg less 867 kg operating items, at 141,000 kg); the CR-151970 class (C-141A, C-133B, C-130E, from the percent-of-MEW rows of section 5) is 0.41-0.45; the A400M is 0.56 (Jane's All the World's Aircraft 2008-09, A400M entry, p298: operating weight empty 76,500 kg at 136,500 kg logistic MTOW)",
        "a press report gives the A400M as about 12 t over its target weight (Financial Times Deutschland, 12 January 2009, via AFP): the difference is the type's own excess over its class, which no class statistic predicts, not a missing mass group",
        "programme weight history from primary programme sources: K. Wieland (A400M Chief Engineer, Airbus Deutschland), DGLR/HAW Hamburg lecture 29 Nov 2001, slides 25 and 43: OWE 66,500 kg at 130,000 kg MTOW (2.25 g), primary structure 38,800 kg (wing 13,000, fuselage 19,000, horizontal tail 2,200, vertical tail 1,800, four nacelles 2,800 at 98 percent titanium, the tubular engine mountings of slide 42, without the aluminium and CFRP cowlings, for the original TP400 later replaced by the TP400-D6); G. Morsch (BWB A400M project leader), VDI/DGLR Hamburg lecture 24 Nov 2005, slide 23: empty weight 76,850 kg at 136,500 kg MTOW, contractual payload 32 t logistic (from 37 t); the preset sits near the 2001 design OWE, and the 10-13 t growth to the production standard is not allocated to groups in any public source",
    ],
    inclusion: unknown(),
    source: Some(BUNDESWEHR_A400M),
    uncertainty_kg: Some(2_100.0),
    case_anchor: None,
    other_published_values: &[PublishedOewValue {
        label: "EASA TCDS A.169 minimum flight weight (civil WV001)",
        value_kg: 78_000.0,
        is_operating_empty: false,
        source: OewSource {
            document: "EASA Type-Certificate Data Sheet EASA.A.169 Airbus A400M",
            publisher: "EASA",
            revision: "Issue 07",
            date: "2025-11-28",
            locator: "III.13 weight variant WV001, minimum weight",
            url: "https://www.easa.europa.eu/en/downloads/7276/en",
            local_path: "",
            retrieved: "2026-10-04",
            quote: "Minimum weight 78 000 kg",
            tier: OewSourceTier::CertificationDocument,
        },
        note: "a minimum flight weight of the civil type design, not an empty mass",
    }],
    structural_payload_basis_oew_kg: None,
    notes: "Reference mismatch: the comparison stays visible and enters no validation metric. Single operator-page value with no stated definition; the uncertainty is the 2,100 kg spread to the older 76,500 kg secondary figure. The class evidence in the differences shows the model at the top of the historical military-transport class and the A400M about 25 percent above it; the gap is not closed by a factor or a fitted item.",
};
