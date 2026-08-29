# Cabin layout overhaul: focused source register

**Accessed:** 2026-08-29  
**Scope:** primary sources used for the current three-class presets and cabin
screening requirements. The fuller domain review remains
[cabin-interiors-cargo.md](cabin-interiors-cargo.md).

## Conclusions

- Airline class mixes are configuration-specific seat counts, not universal
  airline percentages and not cabin-floor shares.
- The three public ALAS slots require an explicit surrogate for Iberia's
  published Premium Economy cabin: fold it into the Economy target bucket.
- Cabin rules depend on certification basis, operating jurisdiction, aircraft
  capacity/configuration, and relevant dates. A preliminary geometry checker
  cannot make a certification or accessibility-compliance finding.
- Neither EASA CS-25 nor FAA Part 25 supplies a universal minimum seat pitch or
  seat width. Operator geometry is calibration evidence, not regulation.

## Airline configuration anchors

Percentages below are calculated from the cited whole-seat counts and rounded to
two decimals. They are target **seat shares**. Conversion to ALAS floor-length
shares requires the class geometry and available cabin section.

| Preset | Primary airline source | Published configuration | Three-slot target |
| --- | --- | --- | --- |
| Ryanair | [Ryanair fleet](https://corporate.ryanair.com/about-us/our-fleet/) | Boeing 737-8200: 197 seats. Ryanair also lists 189-seat 737-800s and 180-seat A320s, so this is explicitly the 737-8200 anchor. | First 0/197 = 0.00%; Business 0/197 = 0.00%; Economy 197/197 = 100.00%. |
| Iberia | [Iberia A350 leaflet](https://megustavolar.iberia.com/wp-content/uploads/mgv/D%C3%ADptico-A350.pdf) | A350-900: 348 seats = 31 Business + 24 Premium Economy + 293 Economy. The leaflet describes Iberia's first A350 and is a dated configuration, not a current fleet-wide average. | First 0/348 = 0.00%; Business 31/348 = 8.91%; Economy bucket (24 + 293)/348 = 91.09%. Premium Economy is a declared surrogate inside Economy, not relabeled First. |
| Emirates | [Emirates A380 Beirut configuration](https://www.emirates.com/media-centre/emirates-to-operate-first-ever-a380-to-beirut) | Dated February 2018 three-class A380: 14 First + 76 Business + 429 Economy = 519 seats. | First 14/519 = 2.70%; Business 76/519 = 14.64%; Economy 429/519 = 82.66%. |

The [current Emirates A380 fleet page](https://www.emirates.com/us/english/experience/our-fleet/a380/)
lists several capacities (including 4-, 3-, and 2-class aircraft) and warns that
models/configurations vary. This confirms that the 519-seat table is a named
historical three-class archetype, not a current fleet average. If the product
later adopts another A380 variant, its full class counts must be sourced anew.

## Regulatory layers and applicability

### Large-aeroplane type design

- [EASA CS-25 Amendment 28](https://www.easa.europa.eu/en/document-library/certification-specifications/cs-25-amendment-28)
  is the current official source selected for European large-aeroplane
  certification screening. Relevant sections include CS 25.785 (seats,
  berths, safety belts and harnesses), 25.803 (emergency evacuation), 25.807--
  25.813 (exits and access), 25.815 (aisle width), 25.817 (maximum seats
  abreast), 25.787 (stowage), and 25.853/Appendix F (interior fireworthiness).
- The [FAA eCFR Part 25](https://www.ecfr.gov/current/title-14/chapter-I/subchapter-C/part-25)
  is the corresponding US type-design source. A rule profile must select the
  applicable certification basis/amendment; similarity between CS-25 and Part
  25 is not permission to merge them without traceability.
- CS/FAR 25.803's evacuation requirement and exit-rating rules are not proven
  by an exit count alone. A 3-D layout and geometric egress screen remain
  preliminary until the accepted demonstration/analysis and supporting tests
  exist.
- CS/FAR 25.815 measures aisle width at different heights above the floor, and
  25.817 limits a single aisle to no more than three seats on either side.
  Therefore a floor-plan-only width is insufficient; vertical seat, bin,
  monument, and passenger-clearance geometry matters.
- CS/FAR 25.787 requires approved stowage strength/retention behavior. A bin
  volume or rendered shell does not establish attachment loads, placarded load,
  critical distribution, door retention, or head-injury acceptability.

### Accessibility and operations

- [EU Regulation (EC) 1107/2006](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=celex%3A32006R1107)
  concerns nondiscrimination and assistance for disabled persons/persons with
  reduced mobility. Annex II includes reasonable seating efforts, assistance
  to toilet facilities, and carriage of mobility equipment subject to the
  stated conditions. It does not supply a complete cabin-dimensional design
  code.
- [US DOT 14 CFR Part 382 overview](https://www.transportation.gov/airconsumer/passengers-disabilities)
  distinguishes accessible-lavatory, on-board-wheelchair, seating, and route/
  carrier obligations. Applicability depends on aircraft and operations; it is
  not a universal CS-25 design requirement.
- The [2023 US DOT single-aisle accessible-lavatory final rule](https://www.transportation.gov/airconsumer/final-rule-accessible-lavatories-single-aisle-aircraft-PDF)
  adds phased requirements for new single-aisle aircraft. For the long-term
  enlarged-lavatory provision, the source covers aircraft with an FAA-certified
  maximum seating capacity of at least 125 and uses order, delivery, and new
  type-design dates. The implementation must evaluate those dates rather than
  applying the rule indiscriminately to every cabin.

## Industry evidence boundary

Airline publications are appropriate for preset class-count anchors. They do
not disclose the complete approved LOPA or enough dimensional, structural,
human-factors, and operational data to reproduce it. Class pitch/width,
monument footprints and quantities, crew stations, door/exit clearances,
overhead-bin/PSU integration, wheelchair maneuvering, baggage demand, catering,
water/waste, and evacuation evidence need separate sourced assumptions.

Published standards and approved manufacturer/operator data may ultimately be
needed for certification-quality work. Where standards are paywalled or not in
the project evidence corpus, ALAS must state `evidence-required`; it must not
invent a value or present a familiar industry heuristic as a regulatory limit.
