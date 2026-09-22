# Glossary

Terms used across the documentation, with enough context to be useful
rather than just expanded.

## Geometry

**Aspect ratio (AR)**
: Span squared divided by wing area. A long, narrow wing has a high aspect
  ratio and less induced drag, but weighs more and bends further. Most of
  the tension in wing design sits in this one number.

**Chord**
: The distance from leading edge to trailing edge at a given point along
  the span. Root chord is at the fuselage, tip chord at the wingtip.

**Mean aerodynamic chord (MAC)**
: A single reference chord standing in for the whole wing. Centre-of-gravity
  and neutral-point positions are quoted as a percentage of it, which makes
  them comparable between aircraft of different sizes.

**Planform**
: The wing seen from directly above: its outline, sweep and taper.

**Span**
: Wingtip to wingtip.

**Sweep**
: How far back the wing angles from the fuselage. Sweep delays the drag
  rise near the speed of sound, and costs weight and low-speed lift.

**Taper ratio**
: Tip chord divided by root chord. Affects both how efficiently lift is
  distributed along the span and how heavy the structure has to be.

**Wingbox**
: The load-carrying structure inside the wing: spars, ribs and skin. It
  carries the bending and twisting; the outer shape is aerodynamic
  fairing over the top of it.

## Aerodynamics

**Angle of attack (α)**
: The angle between the wing and the oncoming air. Lift rises with it until
  the flow separates.

**Drag polar**
: Drag coefficient plotted against lift coefficient. It is the compact
  summary of an aircraft's aerodynamic efficiency across its whole
  operating range.

**Induced drag**
: The drag that comes as a by-product of producing lift. It falls with span
  and rises steeply at low speed.

**Korn equation**
: An empirical relation predicting the Mach number at which drag begins to
  rise sharply, from thickness, sweep and lift coefficient. It is how a
  fast model accounts for compressibility without solving for the shock.

**Lift and drag coefficients (C_L, C_D)**
: Force divided by dynamic pressure and reference area. Removing size and
  speed from the numbers makes shapes directly comparable.

**Lift-to-drag ratio (L/D)**
: How much lift you get per unit of drag. The single most quoted measure of
  cruise efficiency.

**Oswald efficiency (e)**
: How close a wing's lift distribution comes to the theoretical
  minimum-induced-drag ideal. One would be perfect; real swept, tapered
  wings sit somewhat below.

**Parasite drag**
: Everything that is not induced drag: skin friction and the pressure drag
  from the shape itself. Roughly constant with lift coefficient.

**Vortex-lattice method (VLM)**
: The aerodynamic method behind most of ALAS's results. It represents
  lifting surfaces as a grid of vortices and solves for the flow around
  them. Fast and dependable, but it cannot see viscosity or shocks.

**Wave drag**
: The drag penalty from shock waves once the flow over part of the aircraft
  goes supersonic. It sets the practical cruise speed of a transport.

## Structures and loads

**Load case**
: A flight condition the structure has to survive: a pull-up, a push-over,
  steady flight. The one that demands the most is what the structure is
  sized against.

**Margin of safety**
: The headroom between applied stress and what the material allows. Zero
  means exactly at the limit; below zero means it fails.

**Natural modes**
: The shapes and frequencies at which a structure prefers to vibrate. They
  matter because a structure excited near one of them responds far more
  strongly than elsewhere.

**Ultimate and limit load**
: Limit load is the most the aircraft should ever see in service; ultimate
  is limit multiplied by a safety factor, and is what the structure is
  actually built to withstand.

**V-n diagram**
: The envelope of load factor against airspeed within which the aircraft is
  cleared to operate. Its corners are where the structural design is
  decided.

## Weight and balance

**Centre of gravity (CG)**
: Where the aircraft's mass balances. Quoted as a percentage of mean
  aerodynamic chord, and it moves as fuel burns and payload changes.

**MTOW, MZFW, OEW**
: Maximum take-off weight, maximum zero-fuel weight, and operating empty
  weight. Empty weight plus payload gives zero-fuel weight; add fuel to
  reach take-off weight.

**Neutral point**
: The centre-of-gravity position at which the aircraft becomes
  indifferent to pitch disturbances. Ahead of it is stable, behind it is
  not.

**Payload-range diagram**
: How far the aircraft can fly against how much it carries. Its shape shows
  where payload starts having to be traded for fuel.

**Static margin**
: The distance between centre of gravity and neutral point, as a fraction
  of mean aerodynamic chord. More margin means more stability and less
  manoeuvrability.

**Torenbeek method**
: A set of empirical mass equations fitted to real aircraft data, used to
  estimate component weights before a structure exists to weigh.

## Propulsion

**Bypass ratio (BPR)**
: Air routed around the engine core divided by air through it. High bypass
  ratios move a lot of air gently, which is why modern airliner engines are
  so large in diameter.

**Overall pressure ratio (OPR)**
: Total compression from inlet to combustor. Higher generally means better
  thermal efficiency, limited by materials and stage count.

**Specific thrust**
: Thrust per unit of air mass flow. Low specific thrust usually accompanies
  high bypass and good fuel consumption.

**Thrust-specific fuel consumption (TSFC)**
: Fuel burned per unit thrust per unit time. Lower is better.

**Turbine inlet temperature (TIT)**
: The hottest gas the turbine sees. One of the strongest levers on
  efficiency, and limited by what the blades survive.

## Performance and operations

**Balanced field length**
: The runway needed for a take-off to stay safe whether an engine fails
  just before or just after the decision speed. Usually the governing
  runway requirement.

**Block time**
: Total time for a flight, gate to gate.

**Matching chart**
: Every sizing requirement drawn on the wing-loading against
  thrust-to-weight plane. The region satisfying all of them is the design
  space available to you.

**V₁, V_R, V₂**
: Decision speed, rotation speed, and take-off safety speed. Past V₁ the
  take-off must be continued; at V_R the nose comes up; V₂ is the
  engine-out climb speed to be reached by screen height.

**Wing loading (W/S)**
: Weight per unit wing area. It sets approach speed, runway length and
  ride quality in turbulence.

## Software and method

**Design vector**
: The set of numbers defining one candidate aircraft. ALAS uses
  sixteen.

**Differential evolution**
: The optimisation algorithm behind the search. It maintains a population
  of candidates and combines them, needing no derivatives, which suits an
  objective built from solvers rather than formulas.

**Fidelity**
: How detailed a calculation is. ALAS runs a cheap model inside the
  search and an accurate one on the result.

**MSES**
: A coupled viscous and compressible section solver. The only part of
  ALAS that resolves a shock rather than correlating it.

**Objective function**
: The single number the optimizer minimises: efficiency, less a penalty for
  every constraint the candidate violates.

**SUAVE**
: The mission simulation package ALAS uses to fly a design through a
  route, running in its own isolated environment.
