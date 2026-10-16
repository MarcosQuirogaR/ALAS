# UavSpec schema (frozen v1)

`alas_uav::spec::UavSpec` is the single serialisable source of truth of a UAV
design. It lives outside `AlasConfig` (no parity ledger, no per-field Spanish
catalogue entry, no frozen-module budget churn) and has its own field
inventory below.

## Conventions

- Frame: origin at the fuselage nose reference point, `x` aft, `y` towards the
  right wing tip, `z` up (the `alas-geom` convention).
- Lengths in metres, areas in square metres, masses in kilograms. Angles of
  design parameters are in degrees (what a CAD parameter table holds); solvers
  convert. Ratios and fractions are dimensionless.
- A wing is defined for the right half (1 to 4 panels, root outwards) and
  mirrored. Tail surfaces are either `driven` (tail volume coefficients and arm
  choose the areas, used by the optimiser) or `explicit` (typed in, as in a
  team CAD model).
- Parts never store absolute coordinates. A tree node names a publication
  (`parent_frame`) and an offset in that frame, so changing a master parameter
  regenerates bays and centre-of-gravity points.
- Derived quantities (wetted areas, MAC, volumes, tail volume coefficients) are
  computed, never stored.
- Provenance: `origins` maps a concrete parameter path (`panels[1].span_m`) to
  `user | cad_import | calibrated | derived | default` plus a note. A parameter
  without an entry is `user`. Component rows carry their own per-field
  provenance in the component database (`docs/uav-db-schema.md`).
- Mission: a `PresetRef` (id, data version, variant) selects a competition
  preset; presets are versioned data with rule gates, course parameters, a
  segment template and a score definition. The same types cover Air Cargo
  Challenge, XtraChallenge, SAE Aero Design and AIAA DBF. Hand launch is not
  simulated in v1 (`SegmentKind::Launch` keeps the interface).
- Mission outputs available to score and rule expressions are listed below;
  names are stable.

## Scalar parameters and mission outputs (generated)

<!-- BEGIN GENERATED INVENTORY -->
| path | unit | range | meaning |
|---|---|---|---|
| `root_chord_m` | m | 0.03 .. 1.5 | Root chord of the first panel |
| `panels[].span_m` | m | 0.02 .. 2.5 | Panel half-span along y |
| `panels[].taper_ratio` | 1 | 0.05 .. 1.5 | Tip chord over root chord of the panel |
| `panels[].sweep_le_deg` | deg | -30 .. 60 | Leading-edge sweep (positive aft) |
| `panels[].dihedral_deg` | deg | -20 .. 40 | Dihedral (positive tip up) |
| `panels[].twist_tip_deg` | deg | -10 .. 10 | Tip twist relative to root (positive nose-up) |
| `winglet.cant_deg` | deg | 0 .. 90 | Cant from vertical |
| `winglet.length_m` | m | 0.01 .. 0.8 | Winglet length |
| `winglet.root_chord_ratio` | 1 | 0.05 .. 1 | Winglet root chord over last-panel tip chord |
| `winglet.taper_ratio` | 1 | 0.05 .. 1 | Winglet tip chord over its root chord |
| `winglet.toe_deg` | deg | -10 .. 10 | Toe-in (positive nose-in) |
| `wing_xyz_m` | m | -5 .. 5 | Wing root leading-edge position component |
| `fuselage_xyz_m` | m | -5 .. 5 | Fuselage reference point component |
| `tail_xyz_m` | m | -5 .. 5 | Empennage mount position component |
| `tail_boom.length_m` | m | 0.1 .. 3 | Boom length |
| `tail_boom.outer_diameter_m` | m | 0.004 .. 0.08 | Boom outer diameter |
| `tail_boom.wall_m` | m | 0.0003 .. 0.01 | Boom wall thickness |
| `controls[].span_start` | 1 | 0 .. 1 | Inboard end, fraction of host half-span |
| `controls[].span_end` | 1 | 0 .. 1 | Outboard end, fraction of host half-span |
| `controls[].chord_fraction` | 1 | 0.05 .. 0.5 | Hinged chord fraction |
| `controls[].max_down_deg` | deg | 0 .. 60 | Maximum trailing-edge-down deflection |
| `controls[].max_up_deg` | deg | 0 .. 60 | Maximum trailing-edge-up deflection |
| `empennage.horizontal_volume` | 1 | 0.1 .. 1.5 | Horizontal tail volume coefficient (driven mode) |
| `empennage.vertical_volume` | 1 | 0.01 .. 0.2 | Vertical tail volume coefficient (driven mode) |
| `empennage.arm_m` | m | 0.1 .. 4 | Wing-to-tail aerodynamic-centre arm (driven mode) |
| `empennage.span_m` | m | 0.05 .. 2.5 | Tail surface span (explicit mode) |
| `empennage.root_chord_m` | m | 0.02 .. 0.8 | Tail surface root chord (explicit mode) |
| `empennage.taper_ratio` | 1 | 0.05 .. 1.5 | Tail surface taper ratio (explicit mode) |
| `empennage.sweep_le_deg` | deg | -20 .. 60 | Tail surface leading-edge sweep (explicit mode) |
| `empennage.incidence_deg` | deg | -10 .. 10 | Tail surface incidence (explicit mode) |
| `empennage.dihedral_deg` | deg | -60 .. 60 | Tail surface dihedral (explicit mode) |
| `layout.fuselage.length_m` | m | 0.1 .. 2.5 | Pod length from the nose |
| `layout.fuselage.width_m` | m | 0.03 .. 0.6 | Pod outer width |
| `layout.fuselage.height_m` | m | 0.03 .. 0.6 | Pod outer height |
| `layout.fuselage.wall_m` | m | 0.0002 .. 0.01 | Pod wall thickness |
| `layout.fuselage.bays[].x0_frac` | 1 | 0 .. 1 | Bay forward end, fraction of pod length |
| `layout.fuselage.bays[].x1_frac` | 1 | 0 .. 1 | Bay aft end, fraction of pod length |
| `layout.gear.main_x_m` | m | -1 .. 4 | Main axle x |
| `layout.gear.main_half_track_m` | m | 0.02 .. 1 | Main wheel lateral position (each side) |
| `layout.gear.main_wheel_diameter_m` | m | 0.015 .. 0.3 | Main wheel diameter |
| `layout.gear.main_wheel_width_m` | m | 0.005 .. 0.1 | Main tyre width |
| `layout.gear.aux_x_m` | m | -1 .. 4 | Tail or nose wheel x |
| `layout.gear.aux_wheel_diameter_m` | m | 0.01 .. 0.2 | Auxiliary wheel diameter |
| `layout.gear.aux_wheel_width_m` | m | 0.003 .. 0.08 | Auxiliary tyre width |
| `layout.gear.ground_z_m` | m | -1 .. -0.01 | Ground plane z (below the datum) |
| `layout.motor_mount.prop_plane_offset_m` | m | 0 .. 0.6 | Propeller plane offset from the wing leading edge or pod end |
| `layout.motor_mount.z_offset_m` | m | -0.2 .. 0.3 | Motor height above the wing leading-edge chord line |

| mission output | SI unit | meaning |
|---|---|---|
| `payload_mass` | kg | Payload mass carried |
| `payload_count` | 1 | Cans, bottles or bags carried |
| `payload_volume` | m3 | Liquid payload volume |
| `distance_120s` | m | Ground-projected distance in the 120 s window |
| `distance_90s` | m | Ground-projected distance in the 90 s window |
| `time_1km` | s | Time over the 1 km timed cruise |
| `energy` | J | Battery energy used over the scored segment |
| `energy_per_distance` | J/m | Energy per ground distance |
| `current_integral_over_limit` | A*s | Integral of max(0, I - limit) dt |
| `peak_current` | A | Maximum battery-side current |
| `peak_voltage` | V | Maximum battery voltage |
| `peak_power` | W | Maximum battery-side electrical power |
| `takeoff_distance` | m | Ground roll to liftoff (expected value) |
| `climb_time` | s | Time of the climb segment |
| `altitude_at_60s` | m | Altitude 60 s after liftoff |
| `glide_time` | s | Unpowered glide duration |
| `lap_count` | 1 | Laps completed in the flight window |
| `loading_time` | s | Loading plus unloading time |
| `empty_mass` | kg | Empty (no payload) mass |
| `mtow` | kg | Take-off mass |
| `span` | m | Wing span |
| `wing_area` | m2 | Wing reference area |
| `cost` | EUR | Bill-of-materials cost |
| `battery_energy` | J | Rated pack energy |
| `static_margin_min` | 1 | Smallest static margin over payload states |
| `static_margin_max` | 1 | Largest static margin over payload states |
<!-- END GENERATED INVENTORY -->
