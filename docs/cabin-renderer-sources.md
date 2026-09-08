# Cabin renderer evidence register

Last evidence review: 2026-08-30

## Conclusion and use policy

The renderer may use public manufacturer material for representative geometry, regulations for safety context, and software documentation for implementation behaviour. None of those sources, by itself, supplies a certified, station-indexed cabin/hold definition. A dimension or contour must therefore carry its provenance and fidelity (`authoritative`, `manufacturer_publication`, `derived`, or `placeholder`) through the scene schema. The renderer must not silently turn an envelope, airport-planning drawing, marketing illustration, or engineering inference into an exact production contour.

The machine-readable companion is [`tools/cabin_renderer/data/bibliography.json`](../tools/cabin_renderer/data/bibliography.json). Source IDs below are stable and intended for use in scene metadata, validation findings, and generated-figure captions.

## Manufacturer facts

| ID | Source | Supported use | Limitation |
|---|---|---|---|
| `airbus-a320-acap-2026` | Airbus, *A320 Aircraft Characteristics — Airport and Maintenance Planning*, 1 July 2026 | Public A320-family planning dimensions and the cargo/servicing arrangements actually shown in that publication. | Airport- and maintenance-planning data are not a cabin CAD model or Weight and Balance Manual. Do not infer unseen liner, window, bin, seat, or ULD cross-section coordinates. |
| `airbus-fast-39` | Airbus, *FAST magazine 39*, December 2006 | The article explicitly identifies A320-family cargo options and depicts named NAS 3610/IATA ULD contours and dimensions, including LD3-45W; it states an A320 arrangement of three forward and four aft ULD positions. | A fleet-upgrade article, not a current aircraft-specific loading manual. Its drawings are reference illustrations and must not be traced as certified station geometry without scale verification. |
| `airbus-safety-first-windows` | Airbus Safety First, *Under the Spotlights* | A typical passenger window assembly has inner and outer stretched-acrylic panes in a seal, a cabin-side transparent lining, a retainer and attachment to the frame; it is a plug-type pressure component. This supports a visibly layered window asset. | Describes typical construction and a heat-damage event, not aperture dimensions, belt height, pane curvature, pitch, or aircraft-specific installation coordinates. |
| `boeing-acap-library` | Boeing, *Airplane Characteristics for Airport Planning — Plan Manuals* | Locates current public Boeing planning manuals, including 787 and legacy DC/MD types, and defines their airport-planning scope. | The library itself provides no universal cabin/hold contour. Each model/revision must be cited separately if numerical data are extracted; coordinate-level cabin geometry normally requires stronger configuration data. |

## Regulation and advisory material

| ID | Source | Supported use | Limitation |
|---|---|---|---|
| `easa-cs25-775` | EASA, *Easy Access Rules for Large Aeroplanes (CS-25)*, CS 25.775 and AMC 25.775(d) | Establishes structural and high-altitude design considerations for windows. AMC material describes typical multi-pane cabin-window construction. | A certification requirement/acceptable means, not a drawing standard. It does not prescribe the visual position, dimensions, or curvature needed by the renderer. |
| `faa-ac25-17a-chg1` | FAA, *AC 25-17A Change 1 — Transport Airplane Cabin Interiors Crashworthiness Handbook* | Safety and certification context for transport-airplane cabin interiors and occupant/restraint installations. | Advisory guidance is not mandatory and is not a source of aircraft-specific seat, bin, liner, or aisle geometry. |
| `faa-ac25-21-cancelled` | FAA, *AC 25-21 — Certification of Transport Airplane Structure* | Historical structural-certification context and a pointer to applicable Part 25 subjects. | Cancelled on 2017-03-21. It must not be treated as current guidance or used to derive cabin geometry. Retained only to prevent accidental reliance on an earlier research lead. |

## Industry standard and identification sources

| ID | Source | Supported use | Limitation |
|---|---|---|---|
| `iata-uldr` | IATA, *ULD Regulations (ULDR)* | Authoritative catalogue route for registered ULD type codes, diagrams, characteristics, aircraft acceptability, pallet/net compatibility, markings, and operational limitations. | Full normative data are licensed. The public landing page is evidence of scope, not permission to invent or reproduce unavailable contour coordinates. Authoritative implementation requires licensed ULDR data and the applicable aircraft Weight and Balance Manual. |
| `iata-uld-identification` | IATA, *What is Aircraft ULD in Air Transport?* | Public explanation that ULD markings follow IATA Cargo Services Conference Resolution 685 and the ULDR; suitable for label semantics. | Educational summary, not a dimensional or compatibility dataset. A plausible code rendered on a diagram must be marked synthetic unless it comes from the solved load record. |

## Software documentation

| ID | Source | Supported use | Limitation |
|---|---|---|---|
| `shapely-manual` | Shapely project, *The Shapely User Manual* | Defines Cartesian constructive and set-theoretic geometry operations used for offsets, intersections, containment and clearance/collision checks. | Shapely does not validate aircraft physics or source fidelity and does not perform coordinate-system transformations. Buffer curves are polygonal approximations whose resolution/tolerance must be controlled. |
| `svg-clippath-mdn` | MDN Web Docs, *&lt;clipPath&gt; — SVG* | Defines SVG clipping used to keep decorative geometry inside resolved liner/hold regions while preserving semantic vector elements. | Clipping changes painting, not inherent geometry. It must never replace collision/containment validation or hide invalid solved geometry. |
| `cairosvg-docs` | CairoSVG project, *Documentation* | Documents deterministic command-line/Python conversion from SVG 1.1 to PNG/PDF/PS/SVG for review and export. | CairoSVG is an output converter, not the geometry engine. SVG/CSS feature support and font availability must be checked by rendering golden fixtures on the deployment platform. |

## Cabin-system research

| ID | Source | Supported use | Limitation |
|---|---|---|---|
| `fuchs-2023-cabin-variants` | Fuchs et al., “An Approach for Linking Heterogenous and Domain-Specific Models to Investigate Cabin System Variants,” *INCOSE International Symposium* 33(1), 1418–1434 (2023), DOI 10.1002/iis2.13090 | Supports persistent object IDs across SysML/Matlab/Blender/Unity, multi-fidelity asset substitution, object-level requirement checks, and the regular/large/extra-large OHSC topology taxonomy. | It provides qualitative OHSC silhouettes but no dimensioned profile, attachment geometry, liner curve, or center-bin formulation. It cannot establish renderer dimensions or certified installation geometry. |

## Engineering inferences — not external facts

These are explicit ALAS design decisions. They are testable hypotheses, not claims made by the sources above.

1. Build the cabin and hold as independent, station-indexed containment polygons. Place seats, occupants, bins, windows, and ULDs in SI coordinates and validate them before drawing.
2. Construct side bins, center bins, valances, PSU strips, rails and ceiling panels as a connected interior system. A purely visual bridge is unacceptable if the resolved components collide or lack declared attachment topology.
3. Derive bin profiles from the available liner region minus structural/installation margins, head/shoulder keep-outs and opening-sweep envelopes. If inputs are missing, render a conspicuous provisional asset and emit a fidelity warning.
4. Preserve solved seat dimensions and positions. Do not shrink seats or move outboard blocks merely to make a cross-section look plausible.
5. Preserve each ULD's standard contour and orientation. Empty hold area is a reported utilization result; it is not removed by resizing either the hold or the ULD. Show empty slots when the solver exports them.
6. Use SVG clipping only for decorative layers. Geometric acceptance uses Shapely predicates and quantitative clearances on the unclipped model.

## Evidence still required for high-fidelity aircraft presets

- Station-indexed outer mould line, structural inner boundary, cabin liner and cargo-liner contours for each aircraft/configuration.
- Deck and cargo-floor heights, frames of reference, and local coordinate transforms.
- Window aperture outline, belt height, pitch/phase, local surface normal, reveal depth, and exclusion zones.
- Seat model envelopes and solved per-seat positions, including occupant/egress keep-outs.
- Side and center OHCP profiles, common rails/attachments, valances, PSU geometry, door sweep, clearances, and permissible station ranges.
- Cargo slot inventory (occupied and empty), ULD type code, contour polygon, orientation/mirroring, restraint points, compatibility, and source revision.
- Applicable aircraft Weight and Balance Manual or equivalent configuration-controlled source for operational cargo compatibility.

Until those data exist, generated figures are engineering visualizations and must not be described as manufacturer-accurate, certified, or suitable for loading/airworthiness decisions.

## Bibliography

- Airbus. “Aircraft Characteristics.” https://www.aircraft.airbus.com/en/customer-care/fleet-wide-care/airport-operations-and-aircraft-characteristics/aircraft-characteristics
- Airbus. *FAST magazine 39*. https://www.aircraft.airbus.com/sites/g/files/jlcbta126/files/2022-04/FAST39.pdf
- Airbus Safety First. “Under the Spotlights.” https://safetyfirst.airbus.com/under-the-spotlights/?airbus-iframe=true&airbus-post=2055
- Boeing. “Plan Manuals.” https://www.boeing.com/commercial/airports/plan-manuals
- EASA. “Easy Access Rules for Large Aeroplanes (CS-25), CS 25.775.” https://www.easa.europa.eu/en/document-library/easy-access-rules/online-publications/easy-access-rules-large-aeroplanes-cs-25?page=26
- FAA. “AC 25-17A — Transport Airplane Cabin Interiors Crashworthiness Handbook.” https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentid/74596
- FAA. “AC 25-21 — Certification of Transport Airplane Structure (Cancelled).” https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentid/22649
- IATA. “ULD Regulations.” https://www.iata.org/en/publications/manuals/uld-regulations/
- IATA. “What is Aircraft ULD in Air Transport?” https://www.iata.org/en/publications/newsletters/iata-knowledge-hub/what-is-aircraft-uld-in-air-transport/
- Shapely project. “The Shapely User Manual.” https://shapely.readthedocs.io/en/stable/manual.html
- MDN Web Docs. “&lt;clipPath&gt; — SVG.” https://developer.mozilla.org/en-US/docs/Web/SVG/Reference/Element/clipPath
- CairoSVG project. “Documentation.” https://cairosvg.org/documentation/
- Fuchs, M., Y. Ghanjaoui, J. Biedermann, and B. Nagel. “An Approach for Linking Heterogenous and Domain-Specific Models to Investigate Cabin System Variants.” *INCOSE International Symposium* 33(1), 1418–1434, 2023. https://doi.org/10.1002/iis2.13090
