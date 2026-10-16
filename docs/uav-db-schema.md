# UAV component database, schema v2

Crate: `alas-uav-db`. Code is AGPL-3.0-or-later; the curated data under
`crates/alas-uav-db/data/kit` is licensed CC BY 4.0 (see
`crates/alas-uav-db/data/LICENSE-DATA`). Third-party measurement sets are never
relicensed.

## Rules

- SI at rest. The vendor text is kept in `raw`. The one non-SI unit is the
  vendor C rating, stored in `1/h` (current in A = C rating x capacity in Ah).
- Unknown stays unknown: a member that is not published is `null`, never a
  default. Models declare the members they need (`mandatory for` below) and
  report what is missing.
- Every number carries provenance (a `Field`): `value`, `unit_si`, `raw`,
  `source_id`, `retrieved_at` (null = unknown, caps confidence at 0.5),
  `method` (`measured`, `datasheet`, `computed`, `retailer`, `user`),
  `test` (`voltage_v`, `temp_k`, `load`) and `confidence` in [0, 1]. Default
  confidence: measured 0.9, user 0.8, datasheet 0.7, computed 0.6, retailer 0.5.
- Tabulated relations are `Curve`s (strictly increasing abscissa, one
  provenance per table, optional fixed parameter such as temperature). They are
  never extrapolated.
- Prices are never inside a spec row. `Offer` rows (component, vendor, price,
  basis, currency, taxes, stock, ship-from, observation date) form a separate
  time series and are never part of the shipped bundle.
- Compatibility (cells versus voltage, prop versus motor) is computed by the
  consumer, never stored.
- Spec corrections are append-only revisions: same `id`, later `valid_from`,
  the earlier row closed with `valid_to`. Intervals of one id must not overlap.
- Sources carry a `redistribution` class. A bundle that ships in the repository
  may cite only `facts_with_citation` or `permission_granted` sources
  (`Database::validate_for_shipping`).
- Rm is the phase-to-phase resistance; its test temperature goes in `test`.
- Motor `i_max_cont_a` is a continuous rating; a vendor "maximum current"
  without duration maps to `i_max_burst_a`.

## Files and tools

- Shards: JSON files of the bundle shape `{schema_version, sources, components,
  offers, prop_perf}`; shards are merged and validated together.
- CSV import: one category per file, header `stem [vendor unit]`
  (`kv [rpm/V]`, `mass [g]`, `dims [mm]` with cell `100x33x31`),
  `<stem>.test_voltage [V]`, `<stem>.test_temp [degC]`. Curves and blade
  geometry are JSON only. Rows that fail are rejected with their row number.
- User overlay: the same schema layered over the shipped bundle
  (`overlay::apply`, `overlay::load_dir`); never edits the base.
- v1 converter: `v1::convert_v1` lifts per-record provenance of the legacy
  `alas-uav/data/*.json` catalogues to per-field provenance (confidence 0.5,
  sources `unreviewed`, therefore not shippable).

## Field inventory (generated from `alas_uav_db::inventory`)

Ranges are sanity bands for the 0.5-25 kg fixed-wing scope (a value outside
them is almost certainly a unit mistake), not product limits.

<!-- BEGIN GENERATED INVENTORY -->
#### `motor`

| member | kind | SI unit | x unit | range | mandatory for | description |
|---|---|---|---|---|---|---|
| `kv_rad_s_per_v` | Num | rad/s/V | - | 0.5 .. 3000 | motor_circuit | Speed constant (vendor KV x 2 pi / 60) |
| `rm_ohm` | Num | Ohm | - | 0.00001 .. 10 | motor_circuit | Phase-to-phase winding resistance |
| `i0_a` | Num | A | - | 0.01 .. 100 | motor_circuit | No-load current |
| `i0_test_v` | Num | V | - | 1 .. 200 | motor_circuit | Voltage at which i0 was stated |
| `i_max_cont_a` | Num | A | - | 0.1 .. 500 | motor_circuit | Maximum continuous current |
| `i_max_burst_a` | Num | A | - | 0.1 .. 1000 | - | Maximum burst current |
| `burst_s` | Num | s | - | 0.1 .. 600 | - | Burst duration |
| `p_max_w` | Num | W | - | 1 .. 50000 | - | Maximum power |
| `n_poles` | Count | 1 | - | 2 .. 100 | - | Magnetic pole count |
| `mass_kg` | Num | kg | - | 0.001 .. 50 | mass_budget | Mass |
| `diameter_m` | Num | m | - | 0.005 .. 0.5 | packaging | Can diameter |
| `length_m` | Num | m | - | 0.005 .. 0.5 | packaging | Length without shaft |
| `shaft_d_m` | Num | m | - | 0.001 .. 0.05 | - | Shaft diameter |
| `mount_pattern` | Text | 1 | - | - | - | Mounting pattern text |
| `omega_max_rad_s` | Num | rad/s | - | 10 .. 20000 | - | Maximum shaft speed |
| `thermal_r_k_per_w` | Num | K/W | - | 0.01 .. 100 | - | Winding-to-ambient thermal resistance |
| `winding_temp_max_k` | Num | K | - | 330 .. 520 | - | Maximum winding temperature |
| `cells_min` | Count | 1 | - | 1 .. 60 | - | Minimum LiPo series count |
| `cells_max` | Count | 1 | - | 1 .. 60 | - | Maximum LiPo series count |
| `thrust_static_max_n` | Num | N | - | 0.1 .. 2000 | - | Vendor-claimed maximum static thrust |
| `rec_prop_diameter_m` | Num | m | - | 0.05 .. 1.5 | - | Vendor-recommended propeller diameter |
| `rec_prop_pitch_m` | Num | m | - | 0.02 .. 1.5 | - | Vendor-recommended propeller pitch |
| `i0_vs_omega` | Curve | A | rad/s | - | - | No-load current versus shaft speed |

#### `propeller`

| member | kind | SI unit | x unit | range | mandatory for | description |
|---|---|---|---|---|---|---|
| `diameter_m` | Num | m | - | 0.02 .. 3 | prop_map, bemt | Diameter |
| `pitch_m` | Num | m | - | 0.005 .. 3 | prop_map | Geometric pitch |
| `blades` | Count | 1 | - | 1 .. 12 | prop_map, bemt | Blade count |
| `family` | Text | 1 | - | - | - | Product family text |
| `mass_kg` | Num | kg | - | 0.0001 .. 5 | mass_budget | Mass |
| `hub_bore_m` | Num | m | - | 0.001 .. 0.05 | - | Hub bore |
| `folding` | Bool | 1 | - | - | - | Blades fold |
| `inertia_kg_m2` | Num | kg*m2 | - | 0.000000001 .. 1 | - | Polar moment of inertia |
| `electric_compatible` | Bool | 1 | - | - | - | Vendor permits electric propulsion |
| `geometry` | Geometry | m | m | - | bemt | Blade geometry table (r, chord, twist, airfoil) |

#### `esc`

| member | kind | SI unit | x unit | range | mandatory for | description |
|---|---|---|---|---|---|---|
| `i_cont_a` | Num | A | - | 0.5 .. 500 | esc_limits | Continuous output current |
| `i_burst_a` | Num | A | - | 0.5 .. 1000 | - | Burst output current |
| `v_min_v` | Num | V | - | 1 .. 200 | - | Minimum input voltage |
| `v_max_v` | Num | V | - | 1 .. 200 | esc_limits | Maximum input voltage |
| `cells_min` | Count | 1 | - | 1 .. 60 | - | Minimum LiPo series count |
| `cells_max` | Count | 1 | - | 1 .. 60 | - | Maximum LiPo series count |
| `bec_v_min_v` | Num | V | - | 1 .. 30 | - | Minimum adjustable BEC voltage |
| `bec_v_max_v` | Num | V | - | 1 .. 30 | - | Maximum adjustable BEC voltage |
| `bec_a_cont_a` | Num | A | - | 0.01 .. 50 | - | Continuous BEC current |
| `bec_a_peak_a` | Num | A | - | 0.01 .. 50 | - | Peak BEC current |
| `protocols` | TextList | 1 | - | - | - | Supported signal protocols |
| `mass_kg` | Num | kg | - | 0.0005 .. 5 | mass_budget | Mass |
| `dims_m` | Dims3 | m | - | 0.0001 .. 5 | packaging | Bounding box length x width x height |
| `efficiency_vs_current` | Curve | 1 | A | - | - | Efficiency versus output current |

#### `battery`

| member | kind | SI unit | x unit | range | mandatory for | description |
|---|---|---|---|---|---|---|
| `chemistry` | Text | 1 | - | - | battery_sag | Chemistry text |
| `cells_s` | Count | 1 | - | 1 .. 60 | battery_sag | Series cell count |
| `parallel_p` | Count | 1 | - | 1 .. 20 | - | Parallel string count |
| `capacity_c` | Num | C | - | 36 .. 1000000 | battery_sag | Rated capacity (1 Ah = 3600 C) |
| `v_nom_v` | Num | V | - | 1 .. 200 | battery_sag | Nominal pack voltage |
| `v_max_v` | Num | V | - | 1 .. 200 | - | Fully charged voltage |
| `v_min_v` | Num | V | - | 1 .. 200 | - | Cut-off voltage |
| `energy_j` | Num | J | - | 100 .. 100000000 | - | Stored energy (usually derived) |
| `r_dc_ohm` | Num | Ohm | - | 0.0001 .. 5 | battery_sag | Pack DC internal resistance |
| `i_cont_a` | Num | A | - | 0.1 .. 2000 | - | Continuous discharge current |
| `i_burst_a` | Num | A | - | 0.1 .. 4000 | - | Burst discharge current |
| `claimed_c_rating` | Num | 1/h | - | 0.5 .. 500 | - | Vendor C rating as printed (a claim) |
| `mass_kg` | Num | kg | - | 0.001 .. 50 | battery_sag, mass_budget | Pack mass |
| `dims_m` | Dims3 | m | - | 0.0001 .. 5 | packaging | Bounding box length x width x height |
| `connector` | Text | 1 | - | - | - | Main connector text |
| `voc_vs_soc` | Curve | V | 1 | - | - | Open-circuit voltage versus state of charge |
| `r_dc_vs_soc_t` | CurveList | Ohm | 1 | - | - | Resistance versus state of charge, one curve per temperature |

#### `servo`

| member | kind | SI unit | x unit | range | mandatory for | description |
|---|---|---|---|---|---|---|
| `torque_vs_voltage` | Curve | N*m | V | - | servo_sizing | Stall torque versus supply voltage |
| `speed_vs_voltage` | Curve | rad/s | V | - | - | No-load speed versus supply voltage |
| `i_stall_vs_voltage` | Curve | A | V | - | - | Stall current versus supply voltage |
| `v_min_v` | Num | V | - | 1 .. 50 | servo_sizing | Minimum supply voltage |
| `v_max_v` | Num | V | - | 1 .. 50 | servo_sizing | Maximum supply voltage |
| `mass_kg` | Num | kg | - | 0.0005 .. 2 | mass_budget | Mass |
| `dims_m` | Dims3 | m | - | 0.0001 .. 5 | packaging | Bounding box length x width x height |
| `gear` | Text | 1 | - | - | - | Gear type |

#### `receiver, flight_controller, electronics (shared avionics body)`

| member | kind | SI unit | x unit | range | mandatory for | description |
|---|---|---|---|---|---|---|
| `role` | Text | 1 | - | - | - | Functional role text |
| `mass_kg` | Num | kg | - | 0.0001 .. 5 | mass_budget | Mass |
| `v_in_min_v` | Num | V | - | 1 .. 200 | avionics_budget | Minimum supply voltage |
| `v_in_max_v` | Num | V | - | 1 .. 200 | avionics_budget | Maximum supply voltage |
| `i_draw_a` | Num | A | - | 0.0001 .. 20 | avionics_budget | Supply current draw |
| `i_max_a` | Num | A | - | 0.001 .. 200 | - | Rated maximum current of an output or limiter (not a draw) |
| `channels` | Count | 1 | - | 1 .. 64 | - | Output channel count |
| `protocols` | TextList | 1 | - | - | - | Supported protocols |
| `telemetry` | Bool | 1 | - | - | - | Provides telemetry |
| `dims_m` | Dims3 | m | - | 0.0001 .. 5 | packaging | Bounding box length x width x height |

#### `wheel`

| member | kind | SI unit | x unit | range | mandatory for | description |
|---|---|---|---|---|---|---|
| `diameter_m` | Num | m | - | 0.01 .. 0.6 | packaging | Outer diameter |
| `width_m` | Num | m | - | 0.002 .. 0.2 | - | Tyre width |
| `mass_kg` | Num | kg | - | 0.0005 .. 5 | mass_budget | Mass |
| `crr_grass` | Num | 1 | - | 0 .. 0.5 | - | Rolling-resistance coefficient on short grass |

#### `landing_gear`

| member | kind | SI unit | x unit | range | mandatory for | description |
|---|---|---|---|---|---|---|
| `form` | Text | 1 | - | - | - | Form text |
| `mass_kg` | Num | kg | - | 0.0005 .. 20 | mass_budget | Assembly mass |
| `max_aircraft_mass_kg` | Num | kg | - | 0.05 .. 200 | - | Maximum supported aircraft mass |
| `nominal_voltage_v` | Num | V | - | 1 .. 60 | - | Nominal mechanism voltage |
| `dims_m` | Dims3 | m | - | 0.0001 .. 5 | packaging | Bounding box length x width x height |

#### `material`

| member | kind | SI unit | x unit | range | mandatory for | description |
|---|---|---|---|---|---|---|
| `form` | Text | 1 | - | - | - | Form text |
| `density_kg_m3` | Num | kg/m3 | - | 10 .. 25000 | stock_mass | Density |
| `e_pa` | Num | Pa | - | 1000000 .. 1000000000000 | - | Young's modulus |
| `g_pa` | Num | Pa | - | 100000 .. 1000000000000 | - | Shear modulus |
| `strength_pa` | Num | Pa | - | 100000 .. 100000000000 | - | Failure or allowable strength |
| `stock_mass_kg` | Num | kg | - | 0.0001 .. 1000 | - | Purchased stock mass |
| `stock_dims_m` | Dims3 | m | - | 0.0001 .. 5 | - | Purchased stock bounding box |

#### `laminate`

| member | kind | SI unit | x unit | range | mandatory for | description |
|---|---|---|---|---|---|---|
| `ply_thickness_m` | Num | m | - | 0.00001 .. 0.005 | clt | Cured ply thickness |
| `areal_mass_kg_m2` | Num | kg/m2 | - | 0.01 .. 5 | clt | Ply areal mass |
| `ply_e1_pa` | Num | Pa | - | 1000000000 .. 1000000000000 | clt | Fibre-direction modulus E1 |
| `ply_e2_pa` | Num | Pa | - | 100000000 .. 100000000000 | clt | Transverse modulus E2 |
| `ply_g12_pa` | Num | Pa | - | 100000000 .. 100000000000 | clt | In-plane shear modulus G12 |
| `ply_nu12` | Num | 1 | - | 0.01 .. 0.6 | clt | Major Poisson ratio |
| `ply_xt_pa` | Num | Pa | - | 1000000 .. 10000000000 | - | Longitudinal tensile strength |
| `ply_xc_pa` | Num | Pa | - | 1000000 .. 10000000000 | - | Longitudinal compressive strength |
| `ply_yt_pa` | Num | Pa | - | 1000000 .. 1000000000 | - | Transverse tensile strength |
| `ply_yc_pa` | Num | Pa | - | 1000000 .. 1000000000 | - | Transverse compressive strength |
| `ply_s12_pa` | Num | Pa | - | 1000000 .. 1000000000 | - | In-plane shear strength |
| `knockdown` | Num | 1 | - | 0.05 .. 1 | - | Design knock-down from the coupon programme |
| `coupon_source` | Text | 1 | - | - | - | Coupon campaign identifier |

#### `kit`

| member | kind | SI unit | x unit | range | mandatory for | description |
|---|---|---|---|---|---|---|
| `mass_kg` | Num | kg | - | 0.05 .. 100 | mass_budget | Kit mass |
| `span_m` | Num | m | - | 0.2 .. 10 | - | Wing span |
| `area_m2` | Num | m2 | - | 0.01 .. 10 | - | Wing area |
| `intended_auw_kg` | Num | kg | - | 0.05 .. 100 | - | Intended all-up weight |

#### `connector`

| member | kind | SI unit | x unit | range | mandatory for | description |
|---|---|---|---|---|---|---|
| `i_cont_a` | Num | A | - | 1 .. 1000 | - | Continuous current rating |
| `r_contact_ohm` | Num | Ohm | - | 0.000001 .. 0.01 | - | Contact-pair resistance |
| `mass_kg` | Num | kg | - | 0.00001 .. 1 | mass_budget | Mass of a mated pair |

<!-- END GENERATED INVENTORY -->
