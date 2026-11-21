# Mission plan (`mission.plan`)

A mission plan states the flown mission as a list of stages. An empty plan (the default, and
omitted from saved files) flies the standard mission, so every saved configuration and every
registered aircraft behaves as before. The example `mission-plan.example.json` is loaded by a
test of `alas-config`.

```
mission.plan = {
  name?: string,
  stages: [ { id, kind, events?, reserve?, extra_drag_area_m2?, extra_drag_basis? } ],
  contingency_fraction?: 0..1,       // fuel kept at the end, of the fuel of non-reserve stages
  takeoff_fuel_kg?: kg,              // absent: closed to the plan
  balance_target_pct_mac?: % MAC,    // payload auto-balance target (zero-fuel CG)
  lock_layout?: bool                 // Lock Layout: the auto-balance never moves payload
}
```

Stage `kind` is tagged by `type`: `taxi {time_min}`, `takeoff {field?: {name, elevation_ft, toda_m,
isa_deviation_k?}}`, `climb {altitude_ft, agl?, rate_m_s?}`, `cruise {extent, altitude_ft?, agl?,
speed_m_s?}` with `extent` one of `{distance_km}`, `{time_min}` or `"remainder"` (the route less the
other stages), `loiter {time_min, altitude_ft?, agl?, speed_m_s?}`, `combat {time_min, load_factor,
...}`, `descent {altitude_ft, agl?}`, `approach {decision_height_ft, field_elevation_ft?}`,
`go_around {altitude_ft, agl?}`, `diversion {distance_km, cruise_altitude_ft, field_elevation_ft?}`,
`landing {field_elevation_ft?}`. Altitudes are feet above mean sea level, or above the current field
with `agl` (flat terrain). Speeds default to the aircraft's own profile schedule; a hold flies the
speed of minimum fuel flow.

Events (`{at: "start" | "end", action}`), `action` tagged by `type`: `payload_release {items}` (empty:
every releasable item), `store_add {id, mass_kg, x_m, y_m?, z_m, drag_area_m2?}`, `aerial_refuel
{mass_kg}` (negative offloads), `fuel_transfer {from, to, mass_kg}` (tank ids), `trim_tank_control
{target_min_pct_mac, target_max_pct_mac}`, `trim_tank_off`. Events belong to their stage and move with it.

Added drag (`extra_drag_area_m2`, an open ramp, a deployed probe) needs its `extra_drag_basis`: a
source, or "study input" when it is the author's own figure; the plan refuses an added area with no
basis. A hold without a `speed_m_s` flies the speed of minimum fuel flow of the aircraft's own drag table
at the mean of the masses it starts and ends with (CS-25 Amdt 27 AMC to 25.21(g) 6.9.2 sets the 1.3 V_SR
floor); an aircraft without that table needs the speed stated.

What a flown plan reports: per stage the fuel, time, distance, mass and centre of gravity (tank by tank
at every instant); the fuel loaded at the ramp (`ramp_fuel_kg`) and at the first brake release (the ramp
fuel less the taxi fuel before it); the closure state; the trim drag of the centre-of-gravity movement
(the increment of the vortex-lattice-trimmed induced drag at the leg's centre of gravity, re-trimmed at
the stations the plan reaches); the critical-field result of a take-off stage that names a field.
Findings (error unless stated): centre of gravity outside the model envelope at any instant (per stage,
worst instant); an operation the tanks could not carry out (offload, refuelling, transfer, trim move,
burn); mass above the maximum take-off mass after brake release (fuel taken on in flight); a fuel
closure that did not settle; a payload auto-balance that ran out of range or passes (warning); a trim
drag that could not be followed (an external-solver polar: warning; a station the stabiliser cannot
trim: error).

Order is validated: the aircraft takes off first and lands last, taxi only on the ground, a go-around
only after an approach. Payload items gain `movable` and `x_range_m` for the auto-balance (point
masses of payload category; zones without furnishing).

Templates: `MissionPlan::raymer(profile, inputs)` builds the four example profiles of Raymer's
Fig. 3.2; `MissionPlan::from_profile` and `with_policy_reserves` build the standard sequence and its
reserves as stages.
