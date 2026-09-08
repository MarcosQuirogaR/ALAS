# Cargo capacity and correlation

`CargoLoadManager::capacity_summary()` counts every generated position,
including empty positions. Its ULD count excludes `BLK`, which represents
loose bulk and has no container tare. Dispatching a different mass changes
the loaded items and their CG, but does not change this capacity summary.

The nominal internal container volume is the sum of the selected ULD
catalogue volumes. It is not an estimate of total aircraft hold volume.
Published hold capacity can include bulk compartments and space outside
the container envelopes; comparison requires the same loading configuration
and volume definition on both sides. An unsupported hold-volume metric
must remain unavailable rather than treating container volume as hold volume.

Physical layouts reject the extra aft bulk position when its footprint
intersects another position. Bulk loads are reported as bulk items, and do
not increase loaded ULD counts. The explicit reference-compatibility path
retains the frozen overlapping-grid and bulk-as-container conventions for
Python parity; the product path enforces the physical envelope instead.

Passenger baggage uses `cabin.cargo` with main-deck freight disabled. The A220
planning preset selects `lower_deck_uld = "BLK"`, following the operator WBM
sections 1.6 and 1.6.1 (bulk compartments only, CLC not installed). An explicit
bulk-only selection cannot fall back to rigid ULDs even when one fits the
fuselage geometrically. The public layout-only helper without a cargo-config
argument retains the default format; use `build_payload_layout` for preset
and user-configured cargo systems.

This system selection does not supply A220 bay dimensions or certified mass
limits. The existing loose-baggage remainder represents carried baggage as a
contained bulk block; it is not a volumetric packing or floor-loading proof.
Nominal capacity of generated bulk slots can consequently remain zero in a
shallow hold while its passenger baggage remainder is carried. Do not equate
that placeholder with the WBM's 27.46 cubic metres of theoretical bulk space.

The three capacity quantities (positions, volume, net mass) are distinct.
Matching a manufacturer's position count does not validate hold structure,
floor loading, doors, fire zoning, or a certified ULD contour. The conservative
rectangular fit model can reject a contoured ULD that fits an actual aircraft.
The ATR's main-deck baggage compartments also require an explicit compartment
model; lower-hold container volume cannot validate those spaces.

Regression tests in `crates/alas-payload/tests/cargo_capacity.rs` verify that
an empty and loaded 787 hold report identical available capacity, a bulk-only
hold reports zero ULD volume/count while retaining its mass and bulk volume,
and the generated physical positions do not overlap across all presets.
