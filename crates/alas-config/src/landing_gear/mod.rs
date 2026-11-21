// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/landing_gear_config.py

//! Wheel, tire and strut sizing assumptions. These drive a real sizing pass
//! rather than a fraction of takeoff weight: the wheel count and rated load
//! produce the gear load limits and so the strength boundaries of the CG
//! envelope. Every count accepts zero, meaning "size it"; a non-zero value
//! overrides the sizing and reproduces an existing aircraft's gear.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

mod rotation_basis;
pub use rotation_basis::RotationAngleBasis;

/// Tunable landing-gear sizing assumptions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct LandingGearConfig {
    /// A candidate's solved main-gear group translation, separate from every
    /// published reference anchor (see [`DerivedMainGearStation`]). A replay
    /// writes it back explicitly. It is serialized only when present, so a
    /// saved delivered configuration keeps its placed gear; it is not an
    /// editable setting.
    #[config(skip)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_main_gear: Option<DerivedMainGearStation>,
    /// Margin left in the rated tire load after the static reaction.
    #[config(
        label = "Tire load safety factor",
        help = "Margin applied to the static reaction load when selecting tire count (dimensionless), so rated tire load is never fully used by static load alone.",
        long_help = "Margin applied to the static reaction load when selecting/verifying tire count: real gear is sized so the rated tire load is never fully consumed by static load alone, leaving margin for dynamic (braking, turning, rough-field) loads. Raymer: ~1.07 typical for a preliminary sizing pass."
    )]
    pub tire_safety_factor: f64,

    /// Wheels on the nose gear, or zero to size it.
    #[config(
        label = "Nose-gear wheel count (0 = auto)",
        help = "Wheels on the nose gear strut. 0 = auto: 1 for light aircraft, 2 (the near-universal choice for CS-25/FAR-25 transports) once MTOW exceeds nlg_dual_wheel_mtow_kg."
    )]
    pub n_nlg_wheels: i64,

    /// Where auto-sizing switches to a twin nose wheel.
    #[config(
        label = "MTOW threshold for dual nose wheels",
        unit = "kg",
        help = "Auto-sizing switches from a single to a dual (twin) nose wheel above this MTOW: below it, transport-category aircraft still commonly fly single nose wheels."
    )]
    pub nlg_dual_wheel_mtow_kg: f64,

    /// Main-gear legs, left and right combined, or zero to size them.
    #[config(
        label = "Main-gear strut count (0 = auto)",
        help = "Number of main-gear legs, left and right combined. 0 is auto: 2 below mlg_body_gear_mtow_kg, 4 (adds centreline body gear, A380/747 class) above it.",
        long_help = "Number of main-gear legs (each with its own wheel bogie), left+right combined. 0 = auto: 2 (one per side) below mlg_body_gear_mtow_kg, 4 (adds centreline body gear, e.g. A380/747-class) above it: real widebodies above roughly 300 t add body gear because a two-leg bogie would need an impractically large tire count/track width to carry the load within tire-pressure limits."
    )]
    pub n_mlg_struts: i64,

    /// Where auto-sizing adds centreline body gear.
    #[config(
        label = "MTOW threshold for body (centreline) main gear",
        unit = "kg",
        help = "Auto-sizing adds two centreline body-gear legs (4 main legs total) above this MTOW."
    )]
    pub mlg_body_gear_mtow_kg: f64,

    /// Wheels on each main-gear leg, or zero to size them.
    #[config(
        label = "Wheels per main-gear strut (0 = auto)",
        help = "0 = auto: the smallest of {2, 4, 6} standard bogie sizes whose rated capacity (tire_safety_factor-derated) covers this strut's static reaction load at the aft CG limit."
    )]
    pub wheels_per_mlg_strut: i64,

    /// Main-gear track as a multiple of fuselage diameter.
    #[config(
        label = "Main-gear track / fuselage-diameter factor",
        help = "Main-gear lateral track as a multiple of fuselage diameter (dimensionless).",
        long_help = "Main-gear lateral track width, as a multiple of fuselage diameter. Real transports with wing-root-mounted main gear run track/diameter ~1.75-2.0 (777-300ER 2.03, 787-9 1.90, A340-300 1.91, A380-800 2.00, A320-200 1.92, DC-10-30 1.77): 1.85 is the fleet-average calibration. An earlier default (1.15) understated real track width by roughly a factor of 1.6, which fed directly into the lateral-turnover check (physics.landing_gear) reading artificially safe."
    )]
    pub track_diameter_factor: f64,

    /// Published primary-group wheelbase used as a comparison datum.
    ///
    /// This remains a source value for the parity contract.  When a complete
    /// normalized station anchor set is registered below, the active model
    /// scales those source fractions with its fuselage geometry; this scalar
    /// alone never moves stations or changes reaction loads.
    #[config(
        label = "Reference landing-gear wheelbase",
        unit = "m",
        help = "Published nose-gear to primary main-gear wheelbase, kept for comparison [m].",
        long_help = "Published nose-gear to primary main-gear wheelbase retained for reference comparison. It does not define a station by itself; a complete normalized source anchor set is required to scale source stations with active geometry. Leave unset for a clean-sheet or optimized aircraft."
    )]
    #[serde(default)]
    pub reference_wheelbase_m: Option<f64>,

    /// Source drawing frame used by the normalized longitudinal station
    /// anchors below.  The current Airbus references measure from the
    /// geometric nose-tip extension on an aircraft-characteristics drawing;
    /// this is deliberately not presented as a certified WBM/AFM datum.
    #[serde(default)]
    #[config(skip)]
    pub reference_station_frame: Option<String>,

    /// Fuselage length used to normalize the source drawing stations, m.
    ///
    /// The value is provenance for the fractions, not a frozen absolute
    /// station.  When the active fuselage changes during a clean-sheet or
    /// shrink run, the fractions are re-applied to that active length.
    #[serde(default)]
    #[config(skip)]
    pub reference_station_fuselage_length_m: Option<f64>,

    /// Nose-gear station as a fraction of the source drawing fuselage length.
    #[serde(default)]
    #[config(skip)]
    pub reference_nlg_x_fraction: Option<f64>,

    /// Main-gear stations as fractions of the source drawing fuselage length.
    ///
    /// Order is left wing, right wing, then centreline/body units.  The first
    /// entry is the primary main-gear station used for the scalar
    /// `x_mlg`/wheelbase compatibility fields.
    #[serde(default)]
    #[config(skip)]
    pub reference_mlg_x_fractions: Option<Vec<f64>>,

    /// Published nose-gear to body-main-gear wheelbase, when the source has a
    /// distinct body-gear group (for example the A380 BLG).
    #[config(
        label = "Reference body-gear wheelbase",
        unit = "m",
        help = "Published nose-gear to body-main-gear wheelbase kept for a distinct source comparison [m].",
        long_help = "Published nose-gear to body-main-gear wheelbase retained for a distinct source comparison. It is metadata; the active model uses normalized group stations when those are registered."
    )]
    #[serde(default)]
    pub reference_body_wheelbase_m: Option<f64>,

    /// Published main-gear track, measured according to the source definition.
    ///
    /// Unlike wheelbase, track supplies a source-backed lateral reference when
    /// its definition is known (for the Airbus references below this is the
    /// wing-main-gear centreline spacing). It remains a baseline datum: the
    /// design track is scaled with the active fuselage diameter through
    /// `track_diameter_factor`, so optimization does not freeze a source
    /// aircraft's absolute span.
    #[config(
        label = "Reference main-gear track",
        unit = "m",
        help = "Published main-gear track used as a source baseline when its definition matches the layout [m].",
        long_help = "Published main-gear track used as a source baseline when its definition matches the layout, such as wing-gear centreline spacing. The active design scales it with fuselage diameter and track_diameter_factor; leave unset to retain automatic sizing."
    )]
    #[serde(default)]
    pub reference_track_m: Option<f64>,

    /// Explicit wheel count for each main-gear strut, in layout order.
    ///
    /// The order is left wing, right wing, then centreline/body units. This
    /// permits real heterogeneous arrangements such as the A340's 4 + 4 + 2
    /// wheels while keeping the scalar `wheels_per_mlg_strut` compatibility
    /// control for uniform and automatic designs.
    #[config(
        label = "Main-gear wheels by strut",
        help = "Optional bogie counts per strut in layout order: left wing, right wing, then centreline/body gear. Use even counts (2, 4 or 6), one per main-gear strut.",
        long_help = "Optional per-strut bogie counts in layout order: left wing, right wing, then centreline/body gear. Use standard even counts (2, 4 or 6) and provide one entry per configured main-gear strut. Leave unset for automatic or uniform sizing."
    )]
    #[serde(default)]
    pub mlg_strut_bogie_wheels: Option<Vec<i64>>,

    /// Which reference tire the sizing works from.
    #[config(
        options = TireClass,
        label = "Tire class",
        help = "Which reference tire class to size with: 'auto' picks the smallest class whose rated load, with at most 6 wheels per strut, covers the static gear loads.",
        long_help = "Which reference tire (see physics.landing_gear.TIRE_DATABASE) to size with: 'auto' picks the smallest class whose rated load, combined with a realistic wheel count (<=6/strut), covers the aircraft's static gear loads. Options: auto, light, narrowbody, widebody, heavy."
    )]
    pub tire_class: String,
    /// Certified tire for the main gear; "auto" follows `tire_class`.
    #[config(
        options = CertifiedTire,
        label = "Main-gear tire",
        help = "Certified main-gear tire designation as printed in the aircraft documents (for example H49x19.0-22/32PR). 'auto' sizes from the tire class.",
        long_help = "Certified tire for the main gear: the designation the aircraft's documents print (for example H49x19.0-22/32PR), with its rated load from the manufacturers' tire data books, or a lower bound where no book rates the exact ply rating. \"auto\" sizes the main gear from the tire class instead."
    )]
    #[serde(default = "default_auto", skip_serializing_if = "is_auto")]
    pub mlg_tire: String,
    /// Certified tire for the nose gear; "auto" follows `tire_class`.
    #[config(
        options = CertifiedTire,
        label = "Nose-gear tire",
        help = "Certified nose-gear tire designation as printed in the aircraft documents (for example 49x17/32PR). 'auto' sizes from the tire class.",
        long_help = "Certified tire for the nose gear: the designation the aircraft's documents print (for example 49x17/32PR), with its rated load from the manufacturers' tire data books, or a lower bound where no book rates the exact ply rating. \"auto\" sizes the nose gear from the tire class instead."
    )]
    #[serde(default = "default_auto", skip_serializing_if = "is_auto")]
    pub nlg_tire: String,
    /// What the strut is made of.
    #[config(
        options = StrutMaterial,
        label = "Strut material",
        help = "Landing-gear strut material, shown on the planform diagram and report. 'auto' selects by MTOW class (300M-class steel for larger transports).",
        long_help = "Landing-gear strut/piston material, shown on the planform diagram and in the design report. 'auto' selects by MTOW class (see physics.landing_gear.STRUT_MATERIALS): high-strength steel (300M-class) for larger transports, an aluminium/steel combination for light aircraft. Informational/labelling only; this preliminary-design tool does not run a structural (FEA) stress analysis of the strut itself."
    )]
    pub strut_material: String,

    /// The lateral tip-over criterion.
    #[config(
        label = "Max lateral turnover angle",
        unit = "deg",
        help = "Maximum lateral tip-over angle from vertical at the forward CG limit [deg] (Raymer Ch. 11 / Currey). 63 deg is the transport-category limit.",
        long_help = "Lateral tip-over (overturn) criterion, Raymer Ch.11 / Currey convention: the angle from the vertical whose tangent is CG height over the CG's perpendicular distance to the nose-gear-to-main-gear ground line must not exceed this (evaluated at the forward CG limit, the worst case), or the aircraft risks tipping over in a tight turn. 63 deg is the standard transport-category limit; a higher CG, narrower track, or more forward CG all push the angle up toward it."
    )]
    pub turnover_angle_limit_deg: f64,

    /// Certification braking deceleration, as a fraction of gravity.
    #[serde(default = "default_nlg_dynamic_braking_decel_g")]
    #[config(
        label = "Nose-gear dynamic braking deceleration",
        unit = "fraction of g",
        help = "Deceleration for the nose-gear dynamic braking reaction, in g. 14 CFR 25.733(b)(2): 1.0 g down with 0.31 g forward; load must not exceed 1.5x the static rating.",
        long_help = "Deceleration used for the nose-gear dynamic braking reaction (14 CFR 25.733(b)(2); CS-25 substantively identical; Raymer Sec. 11.2): N_dyn = W (l_m + decel_g * h_cg) / wheelbase. Retrieved rule text (14 CFR 25.733(b)(2)): the braking case combines 1.0g down with 0.31g forward, up to maximum landing weight; the load must not exceed 1.5x the tire's static rating (see tire_dynamic_rating_factor). 0.31 replaces an older '10 ft/s^2' phrasing (which is 0.31 g almost exactly: 3.048/9.80665 = 0.3109) recalled but not confirmed in the current rule text."
    )]
    pub nlg_dynamic_braking_decel_g: f64,

    /// How much of the tire's static rating its dynamic rating gives.
    #[serde(default = "default_tire_dynamic_rating_factor")]
    #[config(
        label = "Tire dynamic-rating factor",
        help = "A tire's dynamic (braking) rated load as a multiple of its static rating (dimensionless). 14 CFR 25.733(b) caps nose-gear braking load at 1.5x.",
        long_help = "A tire's dynamic (braking) rated load, as a multiple of its static rated load: 14 CFR 25.733(b)(2)/(b)(3) cap the nose-gear dynamic-braking load at 1.5x the tire's static rating, and the retrieved Goodyear Aircraft Tire Data Book entry for the A320 main tire (46x17.0R20: 46,000 lb rated, 69,000 lb max braking) is exactly this 1.5x ratio. The nose tire is sized to cover its dynamic braking reaction divided by this factor, in addition to the margined static reaction."
    )]
    pub tire_dynamic_rating_factor: f64,

    /// The longitudinal tip-back criterion.
    #[serde(default = "default_min_tip_back_deg")]
    #[config(
        label = "Minimum tip-back angle floor",
        unit = "deg",
        help = "Optional extra floor on the longitudinal tip-back angle at the most-aft main axle and most-aft CG [deg]. It must always clear the tail-scrape angle.",
        long_help = "Optional extra floor on the longitudinal tip-back (tip-over) angle, the angle from vertical at the most-aft main-gear axle and the most-aft design centre of gravity, atan((x_mlg_aft - x_cg_aft)/h_cg). The tip-back angle must always clear the aircraft's own tail-down (tail-scrape) angle at the main gear (Torenbeek, Synthesis of Subsonic Airplane Design, ch. 10), so the aircraft cannot sit back onto its tail before the tail touches. The default 0 applies that criterion alone; 15 deg reproduces the Raymer/Roskam rule of thumb, which a high-wing turboprop on short sponson gear does not meet."
    )]
    pub min_tip_back_deg: f64,

    /// The rotation attitude the tail-scrape angle must clear.
    #[serde(default = "default_required_rotation_angle_deg")]
    #[config(
        label = "Required rotation angle at lift-off",
        unit = "deg",
        help = "Pitch attitude reachable at rotation (VR) without tail scrape [deg]: the tail-scrape angle must be at least this. 10 deg is typical of transports, an assumption.",
        long_help = "Pitch attitude the aircraft must be able to reach at rotation (VR) without the tail or any other aft lower-fuselage point scraping the runway: the tail-scrape angle computed from the fuselage lower contour must be at least this large. 10 deg is a representative transport-category rotation attitude."
    )]
    pub required_rotation_angle_deg: f64,

    /// What `required_rotation_angle_deg` rests on: its source and gear state.
    #[serde(default, skip_serializing_if = "RotationAngleBasis::is_assumed")]
    #[config(
        options = RotationAngleBasis,
        label = "Rotation angle basis",
        help = "Source and gear state of the required rotation angle: assumed (a miss warns), published static or gear-extended (a miss rejects), or published with the gear state unstated (a miss warns).",
        long_help = "What the required rotation angle rests on. assumed: no published value, a design-practice assumption; a miss is a warning. published_static: a published tail-strike attitude with the main-gear struts compressed, compared with the model's static attitude; a miss rejects the design. published_gear_extended: published with the struts extended (wheels on the runway at lift-off), compared with the model attitude on the ground plane lowered by the lift-off oleo extension (alas-opt envelope::liftoff_oleo_extension_m); a miss rejects the design. published_state_unspecified: published without a gear state, compared with the gear-extended model attitude; a miss is a warning, since the states may not match."
    )]
    pub required_rotation_basis: RotationAngleBasis,

    /// Explicit belly-to-ground clearance, overriding the 0.25 x fuselage
    /// diameter rule of thumb.
    #[serde(default)]
    #[config(
        label = "Fuselage-to-ground clearance",
        unit = "m",
        help = "Static strut length (belly clearance) from the fuselage lower surface to the ground line [m].",
        long_help = "Static strut length (belly clearance) from the fuselage lower surface to the ground line. Leave unset to keep the conceptual-design default of 0.25 x fuselage diameter (alas-mass::stations::gear_vertical_datum); set explicitly once a real strut/tire stack height is known."
    )]
    pub fuselage_ground_clearance_m: Option<f64>,

    /// Optional override of the required pitch angular acceleration at
    /// rotation; `None` takes the class value (5 deg/s^2, the midpoint of
    /// Sadraey's 4-6 deg/s^2 transport range).
    #[serde(default)]
    #[config(
        label = "Rotation pitch angular acceleration",
        unit = "deg/s^2",
        help = "Optional override of the required pitch angular acceleration at rotation in the forward-CG nose-wheel-liftoff balance [deg/s^2].",
        long_help = "Optional override of the required pitch angular acceleration at rotation (V_R) in the forward-CG nose-wheel-liftoff moment balance. Leave unset to use the class value (5 deg/s^2, the midpoint of Sadraey's 4-6 deg/s^2 transport range for a 3-5 s takeoff rotation, Aircraft Design: A Systems Engineering Approach, 2012, sec. 12.3)."
    )]
    pub rotation_pitch_acceleration_deg_s2: Option<f64>,

    /// Optional override of the pitch radius of gyration as a fraction of
    /// MAC; `None` derives it from the mass ledger's takeoff pitch inertia.
    #[serde(default)]
    #[config(
        label = "Pitch radius of gyration (fraction of MAC)",
        help = "Optional override of the pitch radius of gyration r_y, as a fraction of MAC, in the nose-wheel-liftoff balance.",
        long_help = "Optional override of the pitch radius of gyration r_y, as a fraction of MAC, in the nose-wheel-liftoff balance's pitch-inertia term. Leave unset to derive it from the mass ledger's takeoff pitch inertia (Raymer's jet-transport radius when no ledger exists); Torenbeek/Roskam give roughly 0.25-0.35 for transports."
    )]
    pub pitch_radius_of_gyration_frac_mac: Option<f64>,

    /// Whether the horizontal stabiliser is trimmable in flight (a trimmable
    /// horizontal stabiliser, THS). When set, the takeoff setting is derived
    /// from the trim balance at V2 for the CG tested, unless
    /// `takeoff_stabilizer_nose_up_deg` fixes it.
    #[serde(default, skip_serializing_if = "is_false")]
    #[config(
        label = "Trimmable stabiliser",
        help = "Whether the horizontal stabiliser is trimmable (a THS, as on jet transports), set before takeoff for the takeoff CG.",
        long_help = "Whether the horizontal stabiliser is trimmable (a THS, as on jet transports). The crew sets it before takeoff for the takeoff centre of gravity, further nose-up the further forward the CG, so the forward-CG nose-wheel-liftoff balance derives the setting from the trim balance at V2 (elevator neutral, take-off flaps) for each CG it tests. Leave off for a fixed stabiliser (for example a turboprop with elevator trim tabs): the built incidence then holds at rotation."
    )]
    pub trimmable_stabilizer: bool,

    /// Nose-up takeoff setting of the trimmable stabiliser, deg (leading edge
    /// down, against the fuselage reference line), overriding the derived
    /// one; `None` derives it.
    #[serde(default)]
    #[config(
        label = "Takeoff stabiliser nose-up setting",
        unit = "deg",
        help = "Nose-up takeoff setting of a trimmable stabiliser against the fuselage reference line [deg].",
        long_help = "Nose-up (leading-edge-down) takeoff setting of a trimmable horizontal stabiliser, measured against the fuselage reference line. Set it from the aircraft's own take-off trim table to replace the setting derived from the trim balance at V2; the forward-CG nose-wheel-liftoff balance uses it in place of the built tail incidence whenever it gives more tail download. Leave unset to derive it for each CG tested."
    )]
    pub takeoff_stabilizer_nose_up_deg: Option<f64>,

    /// Mechanical nose-up travel of the trimmable stabiliser, deg; the derived
    /// takeoff setting is limited to it. `None` takes the class value (12 deg).
    #[serde(default)]
    #[config(
        label = "Stabiliser nose-up travel",
        unit = "deg",
        help = "Largest nose-up setting of the trimmable stabiliser from the flight-control data (FCOM or AMM) [deg].",
        long_help = "Largest nose-up setting of the trimmable horizontal stabiliser, deg, from the aircraft's flight-control data (FCOM or AMM). The derived takeoff setting is limited to it. Leave unset when the travel is not published: the derived setting is then limited to the class value, 12 deg (Sadraey, Aircraft Design: A Systems Engineering Approach, Wiley, 2013, sec. 6.7.1, p. 306 and Table 6.5, p. 307)."
    )]
    pub stabilizer_nose_up_travel_deg: Option<f64>,

    /// Trailing-edge-up elevator travel at rotation, deg; `None` takes the
    /// class value (25 deg).
    #[serde(default)]
    #[config(
        label = "Elevator up travel at rotation",
        unit = "deg",
        help = "Trailing-edge-up elevator travel at takeoff rotation [deg, positive], used by the forward-CG nose-wheel-liftoff balance.",
        long_help = "Trailing-edge-up elevator travel available at takeoff rotation, deg (positive), used by the forward-CG nose-wheel-liftoff balance with the DATCOM large-deflection correction. Leave unset to use the class value, 25 deg (Sadraey, Aircraft Design: A Systems Engineering Approach, 2012, sec. 12.6, typical transport maximum up-elevator); set it from the aircraft's own flight-control travel when one is published."
    )]
    pub elevator_up_travel_deg: Option<f64>,

    /// Published maximum static nose-gear load, kg: the nose-gear reaction at
    /// the most forward centre of gravity that the airport-planning document
    /// prints with its pavement loads (gross weight less the main-gear load
    /// at the most forward CG). The nose-gear structure is designed to it, so
    /// above the weight where it is lower than the class handling share
    /// `mass_model.pct_load_nlg_max_handling` it bounds the forward ground
    /// limit: `x_mlg - (load / weight) x wheelbase`. `None`: the class share
    /// alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(skip)]
    pub nose_gear_max_static_load_kg: Option<f64>,

    /// Nose-up travel of the elevator trim tab, deg (trailing edge down,
    /// positive), of a reversible (unpowered, cable-driven) elevator. `Some`
    /// declares the elevator reversible: the forward-CG rotation and landing
    /// balances then hold the elevator only as far as the pilot's force
    /// limit (CS 25.143(d)) and this tab allow
    /// (`alas_opt::envelope::elevator_force`). `None`: a powered elevator,
    /// limited by its travel alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(skip)]
    pub elevator_trim_tab_nose_up_deg: Option<f64>,

    /// Geared balance tab of a reversible elevator: tab deflection per unit
    /// elevator deflection with the elevator trailing edge up (dimensionless,
    /// negative: the tab moves trailing edge down). `None`: no balance tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(skip)]
    pub elevator_balance_tab_gearing: Option<f64>,

    /// Computed airborne minimum control speed at sea level, ISA, at the
    /// maximum take-off weight, m/s, which the forward-CG rotation balance
    /// floors its rotation speed with (`V_R >= vr_vmc_factor V_MC`, CS
    /// 25.107(e)(1)(ii)). Set by the feasibility pipeline from its own
    /// one-engine-inoperative yaw balance on the copy the CG gate reads; never
    /// a preset input. `None`: the stall branch alone (the declared
    /// `vmc_vstall_factor` is the CS 25.149 ceiling, not this aircraft's speed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(skip)]
    pub rotation_minimum_control_speed_m_s: Option<f64>,

    /// The type's published sea-level airborne minimum control speed `VMCA`,
    /// calibrated airspeed, m/s, from its flight manual or a near-primary copy
    /// of it. The forward-CG rotation balance applies the computed `VMCA`
    /// floor only where the computed value validates against this one
    /// (within `VMCA_VALIDATION_TOLERANCE` of the feasibility pipeline).
    /// `None`: no published value, so no computed floor. A validation gate
    /// only: it never enters a computed speed, force or limit (pinned by
    /// `the_published_vmca_is_a_validation_gate_and_never_feeds_the_computed_speed`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(skip)]
    pub published_vmca_m_s: Option<f64>,

    /// Wing-body lift coefficient at the ground (pre-rotation) attitude,
    /// as a fraction of `CL_max,TO`, for the same rotation criterion's
    /// wing-lift term.
    #[serde(default = "default_cl_ground_attitude_frac_of_cl_max_to")]
    #[config(
        label = "Ground-attitude CL (fraction of CLmax_TO)",
        help = "Wing-body lift coefficient at the ground attitude at V_R, as a fraction of CLmax_TO [0-1]: before rotation the angle of attack is low and gear-limited.",
        long_help = "Wing-body lift coefficient at the ground (pre-rotation) pitch attitude at V_R, as a fraction of the takeoff maximum lift coefficient (CLmax_TO). The forward-CG nose-wheel-liftoff and takeoff-trim balances compute that lift from the built wing (the takeoff-flap lift at zero fuselage angle of attack) and use this fraction only when the wing gives no finite value. Torenbeek order-of-magnitude estimate; not a measured value."
    )]
    pub cl_ground_attitude_frac_of_cl_max_to: f64,

    /// Tire rolling-friction coefficient on the runway at rotation, for the
    /// same rotation criterion's longitudinal-force term.
    #[serde(default = "default_rotation_rolling_friction_coefficient")]
    #[config(
        label = "Rolling friction coefficient at rotation",
        help = "Tire rolling-friction coefficient on a dry paved runway during the takeoff roll (dimensionless). 0.02 is the usual conceptual value; an estimate.",
        long_help = "Tire rolling-friction coefficient on a dry paved runway during the takeoff roll, used by the forward-CG nose-wheel-liftoff moment balance: the friction force mu*(W - L) acts at the ground and reduces the forward acceleration, so its inertial reaction at the center of gravity is a nose-down moment. 0.02 is the usual conceptual-design value for a dry hard runway; it is an engineering estimate, not a measured value for any specific aircraft."
    )]
    pub rotation_rolling_friction_coefficient: f64,
}

mod defaults;
pub use defaults::CERTIFIED_TIRE_CODES;
use defaults::{
    default_auto, default_cl_ground_attitude_frac_of_cl_max_to, default_min_tip_back_deg,
    default_nlg_dynamic_braking_decel_g, default_required_rotation_angle_deg,
    default_rotation_rolling_friction_coefficient, default_tire_dynamic_rating_factor, is_auto,
};

fn is_false(value: &bool) -> bool {
    !*value
}

mod validation;

mod resolution;

mod derived;
mod station;
pub use derived::DerivedMainGearStation;
pub use station::{
    effective_main_gear_station, rotation_pivot_station, EffectiveGearStationExt,
    EffectiveMainGearStation, GearStationRejection, LandingGearStationPositions,
    MainGearFallbackRefusal, ValidGearStation, WingMountedGearDomain,
};

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, OptionSource};

    #[test]
    fn every_count_defaults_to_being_sized_rather_than_asserted() {
        // A wheel count follows from the load, so the shipped default has to
        // be the one that lets the sizing decide.
        let config = LandingGearConfig::default();
        assert_eq!(config.n_nlg_wheels, 0);
        assert_eq!(config.n_mlg_struts, 0);
        assert_eq!(config.wheels_per_mlg_strut, 0);
        assert_eq!(config.reference_wheelbase_m, None);
        assert_eq!(config.reference_station_frame, None);
        assert_eq!(config.reference_station_fuselage_length_m, None);
        assert_eq!(config.reference_nlg_x_fraction, None);
        assert_eq!(config.reference_mlg_x_fractions, None);
        assert_eq!(config.reference_body_wheelbase_m, None);
        assert_eq!(config.reference_track_m, None);
        assert_eq!(config.mlg_strut_bogie_wheels, None);
        assert_eq!(config.tire_class, "auto");
        assert_eq!(config.strut_material, "auto");
    }

    #[test]
    fn the_dynamic_braking_and_tip_back_defaults_match_the_retrieved_sources() {
        // 14 CFR 25.733(b)(2): 1.0g down combined with 0.31g forward, and
        // the load must not exceed 1.5x the tire's static rating; the
        // Goodyear 46x17.0R20 max-braking/rated-load ratio is exactly 1.5.
        let config = LandingGearConfig::default();
        assert!((config.nlg_dynamic_braking_decel_g - 0.31).abs() < 1e-12);
        assert!((config.tire_dynamic_rating_factor - 1.5).abs() < 1e-12);
        assert_eq!(config.min_tip_back_deg, 0.0);
        assert!((config.required_rotation_angle_deg - 10.0).abs() < 1e-12);
        assert_eq!(config.fuselage_ground_clearance_m, None);
    }

    #[test]
    fn body_gear_is_added_well_above_the_twin_nose_wheel_threshold() {
        let config = LandingGearConfig::default();
        assert!(config.mlg_body_gear_mtow_kg > config.nlg_dual_wheel_mtow_kg);
    }

    #[test]
    fn the_strut_material_has_its_own_option_list_and_not_the_general_one() {
        // Its accepted values are the strut materials, which are a different
        // set from the structural material database, and offering the wrong
        // list would let a value through that nothing downstream can resolve.
        let schema = LandingGearConfig::default().schema();
        match &schema.field("strut_material").unwrap().entry {
            Entry::Leaf(leaf) => {
                assert_eq!(leaf.options, Some(OptionSource::StrutMaterial));
                assert!(!OptionSource::StrutMaterial.editable());
            }
            Entry::Node(_) => panic!("a material name is not a group"),
        }
    }

    #[test]
    fn heterogeneous_bogie_lists_require_matching_standard_counts() {
        let mut config = LandingGearConfig {
            n_mlg_struts: 3,
            mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
            ..Default::default()
        };
        assert!(config.validation_errors().is_empty());

        config.mlg_strut_bogie_wheels = Some(vec![4, 4]);
        assert!(config
            .validation_errors()
            .iter()
            .any(|(path, _)| path == "landing_gear.mlg_strut_bogie_wheels"));

        config.mlg_strut_bogie_wheels = Some(vec![4, 3, 2]);
        assert!(config
            .validation_errors()
            .iter()
            .any(|(path, _)| path.ends_with("[1]")));
    }

    #[test]
    fn reference_dimensions_must_be_positive_and_finite() {
        let mut config = LandingGearConfig {
            reference_wheelbase_m: Some(0.0),
            reference_track_m: Some(f64::NAN),
            ..Default::default()
        };
        assert_eq!(config.validation_errors().len(), 2);
        config.reference_wheelbase_m = Some(12.64);
        config.reference_track_m = Some(7.59);
        assert!(config.validation_errors().is_empty());
    }

    #[test]
    fn the_rotation_friction_coefficient_defaults_to_a_dry_runway_and_rejects_nonphysical_values() {
        let mut config = LandingGearConfig::default();
        assert_eq!(config.rotation_rolling_friction_coefficient, 0.02);
        assert!(config.validation_errors().is_empty());
        for bad in [-0.01, 1.0, f64::NAN, f64::INFINITY] {
            config.rotation_rolling_friction_coefficient = bad;
            assert!(
                config
                    .validation_errors()
                    .iter()
                    .any(|(path, _)| path == "landing_gear.rotation_rolling_friction_coefficient"),
                "{bad} must be rejected"
            );
        }
        let mut value = serde_json::to_value(LandingGearConfig::default()).expect("serializes");
        if let Some(object) = value.as_object_mut() {
            object.remove("rotation_rolling_friction_coefficient");
        }
        let legacy: LandingGearConfig =
            serde_json::from_value(value).expect("a configuration without the field still loads");
        assert_eq!(legacy.rotation_rolling_friction_coefficient, 0.02);
    }

    #[test]
    fn normalized_source_stations_scale_with_active_fuselage_length() {
        let config = LandingGearConfig {
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(72.73),
            reference_nlg_x_fraction: Some(4.97 / 72.73),
            reference_mlg_x_fractions: Some(vec![
                33.58 / 72.73,
                33.58 / 72.73,
                36.85 / 72.73,
                36.85 / 72.73,
            ]),
            n_mlg_struts: 4,
            mlg_strut_bogie_wheels: Some(vec![4, 4, 6, 6]),
            ..Default::default()
        };
        assert!(config.validation_errors().is_empty());
        let resolved = config.resolved_station_positions(1.0, 2.0, 10.0, 72.73);
        assert!(resolved.source_scaled);
        assert!((resolved.x_nlg_m - 14.97).abs() < 1.0e-12);
        assert!((resolved.x_mlg_m - 45.542).abs() < 1.0e-12);
        assert!((resolved.primary_mlg_x_m() - 43.58).abs() < 1.0e-12);
        assert!((resolved.primary_wheelbase_m() - 28.61).abs() < 1.0e-12);
        assert!((resolved.effective_wheelbase_m() - 30.572).abs() < 1.0e-12);
        assert_eq!(resolved.main_gear_x_m.len(), 4);

        let shrunk = config.resolved_station_positions(1.0, 2.0, 10.0, 60.0);
        assert!((shrunk.x_nlg_m - (10.0 + 60.0 * 4.97 / 72.73)).abs() < 1.0e-12);
        assert!(shrunk.x_mlg_m < resolved.x_mlg_m);
    }

    #[test]
    fn incomplete_or_invalid_source_stations_keep_model_fallback() {
        let config = LandingGearConfig {
            reference_station_fuselage_length_m: Some(60.0),
            reference_nlg_x_fraction: Some(0.1),
            reference_mlg_x_fractions: Some(vec![f64::NAN, 0.5]),
            n_mlg_struts: 2,
            ..Default::default()
        };
        let resolved = config.resolved_station_positions(3.0, 8.0, 0.0, 60.0);
        assert!(!resolved.source_scaled);
        assert_eq!(resolved.x_nlg_m, 3.0);
        assert_eq!(resolved.x_mlg_m, 8.0);
        assert_eq!(resolved.main_gear_x_m, vec![8.0]);
    }

    #[test]
    fn effective_main_gear_station_a380_unequal_bogies() {
        // A380 layout: 2 wing struts (4 wheels each at 33.58 m) +
        // 2 body struts (6 wheels each at 36.85 m). Total = 20 wheels.
        let stations = [33.58, 33.58, 36.85, 36.85];
        let bogie_wheels = [4, 4, 6, 6];
        let eff = effective_main_gear_station(&stations, Some(&bogie_wheels));
        // (4*33.58 + 4*33.58 + 6*36.85 + 6*36.85) / 20 = 710.84 / 20 = 35.542 m
        assert!(eff.is_ok());
        let valid = eff.unwrap();
        assert!((valid.station_m() - 35.542).abs() < 1.0e-12);
        assert!(valid.is_weighted());
        assert!(!valid.is_unweighted_mean());
        assert!(!valid.is_uniform());
        match valid {
            ValidGearStation::WeightedCentroid {
                station_m,
                total_wheels,
            } => {
                assert!((station_m - 35.542).abs() < 1.0e-12);
                assert_eq!(total_wheels, 20);
            }
            _ => panic!("expected WeightedCentroid, got {valid:?}"),
        }
    }

    #[test]
    fn effective_main_gear_station_twin_gear_invariant() {
        // Twin-gear aircraft (A220, A320, B787) with identical longitudinal stations
        let stations = [18.633948, 18.633948];
        let eff_none = effective_main_gear_station(&stations, None);
        let eff_some = effective_main_gear_station(&stations, Some(&[2, 2]));
        assert!(eff_none.is_ok());
        assert!(eff_some.is_ok());
        let valid_none = eff_none.unwrap();
        let valid_some = eff_some.unwrap();
        assert!((valid_none.station_m() - 18.633948).abs() < 1.0e-12);
        assert!((valid_some.station_m() - 18.633948).abs() < 1.0e-12);
        assert!(valid_none.is_uniform());
        assert!(valid_some.is_uniform());
        assert!(!valid_none.is_weighted());
        assert!(!valid_some.is_weighted());
    }

    #[test]
    fn effective_main_gear_station_permutation_invariance() {
        // Strut ordering in arrays must not alter the resulting centroid
        let stations_layout = [33.58, 33.58, 36.85, 36.85];
        let wheels_layout = [4, 4, 6, 6];
        let eff_layout = effective_main_gear_station(&stations_layout, Some(&wheels_layout))
            .expect("layout should be valid");

        let stations_perm = [36.85, 33.58, 36.85, 33.58];
        let wheels_perm = [6, 4, 6, 4];
        let eff_perm = effective_main_gear_station(&stations_perm, Some(&wheels_perm))
            .expect("permuted layout should be valid");

        // Permutation invariance of the effective gear station holds to floating-point
        // summation round-off (~6e-15 m), not bit-exactly, because IEEE 754 addition is
        // non-associative under strut reordering. An absolute SI bound of 1.0e-12 m
        // (1 picometer, well below any physical manufacturing or sub-atomic scale)
        // rigorously bounds the numerical precision of the centroid summation.
        assert!(
            (eff_layout.station_m() - eff_perm.station_m()).abs() < 1.0e-12,
            "permutation invariant to summation round-off: layout={}, perm={}, diff={:e}",
            eff_layout.station_m(),
            eff_perm.station_m(),
            (eff_layout.station_m() - eff_perm.station_m()).abs()
        );
        assert!((eff_perm.station_m() - 35.542).abs() < 1.0e-12);
        assert!(eff_layout.is_weighted());
        assert!(eff_perm.is_weighted());
    }

    #[test]
    fn effective_main_gear_station_missing_data_conceptual_fallback() {
        // When per-strut counts are missing (None), the declared conceptual
        // assumption of equal strut loading applies an unweighted arithmetic mean.
        let stations = [33.58, 33.58, 36.85, 36.85];
        let unweighted_mean = (33.58 * 2.0 + 36.85 * 2.0) / 4.0; // 35.215 m
        let eff = effective_main_gear_station(&stations, None);
        assert!(eff.is_ok());
        let valid = eff.unwrap();
        assert!((valid.station_m() - unweighted_mean).abs() < 1.0e-12);
        assert!(valid.is_unweighted_mean());
        assert!(!valid.is_weighted());
        assert!(!valid.is_uniform());
        match valid {
            ValidGearStation::UnweightedMean {
                station_m,
                strut_count,
            } => {
                assert!((station_m - unweighted_mean).abs() < 1.0e-12);
                assert_eq!(strut_count, 4);
            }
            _ => panic!("expected UnweightedMean, got {valid:?}"),
        }
    }

    #[test]
    fn uniform_stations_with_malformed_counts_reject() {
        // F4 table: geometric degeneracy must NOT bypass count validation
        // Row 1: Non-standard count (3 and 999 not in {2, 4, 6})
        let row1 = effective_main_gear_station(&[18.633948, 18.633948], Some(&[3, 999]));
        assert!(
            row1.is_err(),
            "non-standard count on uniform stations must reject"
        );
        let rej1 = row1.unwrap_err();
        assert_eq!(rej1.primary_station_ignoring_rejection(), 18.633948);
        assert_eq!(
            rej1.reason,
            "non-standard bogie wheel count: all counts must be even numbers in {2, 4, 6}"
        );

        // Row 2: Length mismatch (3 counts for 2 struts)
        let row2 = effective_main_gear_station(&[18.6, 18.6], Some(&[4, 4, 6]));
        assert!(
            row2.is_err(),
            "length mismatch on uniform stations must reject"
        );
        let rej2 = row2.unwrap_err();
        assert_eq!(rej2.primary_station_ignoring_rejection(), 18.6);
        assert_eq!(
            rej2.reason,
            "strut count mismatch between stations and bogie_wheels"
        );

        // Row 3: Empty counts
        let row3 = effective_main_gear_station(&[18.6, 18.6], Some(&[]));
        assert!(
            row3.is_err(),
            "empty counts on uniform stations must reject"
        );
        let rej3 = row3.unwrap_err();
        assert_eq!(rej3.primary_station_ignoring_rejection(), 18.6);
        assert_eq!(rej3.reason, "bogie_wheels count list is empty");

        // Row 4: Negative count on single strut
        let row4 = effective_main_gear_station(&[18.6], Some(&[-2]));
        assert!(row4.is_err(), "negative count on single strut must reject");
        let rej4 = row4.unwrap_err();
        assert_eq!(rej4.primary_station_ignoring_rejection(), 18.6);
        assert_eq!(
            rej4.reason,
            "non-standard bogie wheel count: all counts must be even numbers in {2, 4, 6}"
        );
    }

    #[test]
    fn non_finite_stations_reject() {
        // F7 cases: NaN or Infinity must reject and never pass is_ok()
        let nan_none = effective_main_gear_station(&[f64::NAN, 33.58], None);
        assert!(nan_none.is_err());
        let rej_nan_none = nan_none.unwrap_err();
        assert!(rej_nan_none.primary_station_m.is_none());
        assert!(rej_nan_none.primary_station_ignoring_rejection().is_nan());
        assert_eq!(
            rej_nan_none.reason,
            "stations slice contains non-finite values"
        );

        let nan_some = effective_main_gear_station(&[f64::NAN, 33.58], Some(&[4, 4]));
        assert!(nan_some.is_err());
        let rej_nan_some = nan_some.unwrap_err();
        assert!(rej_nan_some.primary_station_m.is_none());
        assert!(rej_nan_some.primary_station_ignoring_rejection().is_nan());

        let inf_res = effective_main_gear_station(&[f64::INFINITY, 33.58], Some(&[4, 4]));
        assert!(inf_res.is_err());
        assert_eq!(
            inf_res.unwrap_err().reason,
            "stations slice contains non-finite values"
        );

        let neg_inf_res = effective_main_gear_station(&[33.58, f64::NEG_INFINITY], None);
        assert!(neg_inf_res.is_err());
        assert_eq!(
            neg_inf_res.unwrap_err().reason,
            "stations slice contains non-finite values"
        );
    }

    #[test]
    fn non_finite_fuselage_start_x_falls_back_to_model() {
        let config = LandingGearConfig {
            reference_station_fuselage_length_m: Some(60.0),
            reference_nlg_x_fraction: Some(0.1),
            reference_mlg_x_fractions: Some(vec![0.5]),
            n_mlg_struts: 2,
            ..Default::default()
        };
        let resolved = config.resolved_station_positions(3.0, 8.0, f64::NAN, 60.0);
        assert!(!resolved.source_scaled);
        assert_eq!(resolved.x_nlg_m, 3.0);
        assert_eq!(resolved.x_mlg_m, 8.0);
    }

    #[test]
    fn effective_main_gear_station_malformed_explicit_weights_rejected() {
        let stations = [33.58, 33.58, 36.85, 36.85];
        let primary_station = 33.58;

        // Empty stations: rejected, carries None / NAN, never 0.0 (F8)
        let empty_res = effective_main_gear_station(&[], Some(&[4, 4]));
        assert!(empty_res.is_err());
        let rej_empty = empty_res.unwrap_err();
        assert!(rej_empty.primary_station_m.is_none());
        assert!(rej_empty.primary_station_ignoring_rejection().is_nan());
        assert_eq!(rej_empty.reason, "stations slice is empty");

        // Length mismatch (too few): rejected with explicit flag and reason
        let too_few = effective_main_gear_station(&stations, Some(&[4, 4]));
        assert!(too_few.is_err());
        let rej_few = too_few.unwrap_err();
        assert_eq!(
            rej_few.primary_station_ignoring_rejection(),
            primary_station
        );
        assert_eq!(
            rej_few.reason,
            "strut count mismatch between stations and bogie_wheels"
        );

        // Length mismatch (too many): rejected -> primary station
        let too_many = effective_main_gear_station(&stations, Some(&[4, 4, 6, 6, 2]));
        assert!(too_many.is_err());
        let rej_many = too_many.unwrap_err();
        assert_eq!(
            rej_many.primary_station_ignoring_rejection(),
            primary_station
        );
        assert_eq!(
            rej_many.reason,
            "strut count mismatch between stations and bogie_wheels"
        );

        // Non-standard count (e.g. 3 or 5): rejected -> primary station
        let non_std_3 = effective_main_gear_station(&stations, Some(&[4, 3, 6, 6]));
        assert!(non_std_3.is_err());
        let rej_3 = non_std_3.unwrap_err();
        assert_eq!(rej_3.primary_station_ignoring_rejection(), primary_station);
        assert_eq!(
            rej_3.reason,
            "non-standard bogie wheel count: all counts must be even numbers in {2, 4, 6}"
        );

        let non_std_5 = effective_main_gear_station(&stations, Some(&[4, 5, 6, 6]));
        assert!(non_std_5.is_err());
        let rej_5 = non_std_5.unwrap_err();
        assert_eq!(rej_5.primary_station_ignoring_rejection(), primary_station);

        // Zero count: rejected -> primary station
        let zero_cnt = effective_main_gear_station(&stations, Some(&[4, 4, 0, 6]));
        assert!(zero_cnt.is_err());
        let rej_0 = zero_cnt.unwrap_err();
        assert_eq!(rej_0.primary_station_ignoring_rejection(), primary_station);

        // Negative count: rejected -> primary station
        let neg_cnt = effective_main_gear_station(&stations, Some(&[4, 4, -2, 6]));
        assert!(neg_cnt.is_err());
        let rej_neg = neg_cnt.unwrap_err();
        assert_eq!(
            rej_neg.primary_station_ignoring_rejection(),
            primary_station
        );
    }

    #[test]
    fn the_checked_resolution_refuses_a_wing_mounted_fallback_above_the_crown() {
        // An ATR-like layout: no source anchor registered, so the only
        // station on offer is the wing-mounted fallback, and the wing root
        // sits above the fuselage crown. The heights are illustrative
        // stand-ins for the gate's two inputs, not ATR data.
        let config = LandingGearConfig::default();
        let refusal = config
            .resolved_station_positions_checked(3.0, 11.0, 0.0, 27.166, || {
                WingMountedGearDomain::from_heights(1.85, 1.385)
            })
            .expect_err("a fallback above the crown must be refused");
        assert_eq!(refusal.wing_root_z_m, 1.85);
        assert_eq!(refusal.fuselage_crown_z_m, 1.385);
        assert!(refusal.to_string().contains("no main-gear longitudinal"));
        assert!(refusal.to_string().contains("no wing-root gear bay exists"));
    }

    #[test]
    fn the_checked_resolution_keeps_an_in_domain_fallback_bit_identical() {
        // The low-wing presets (AVE, B787-9, DC-10) reach the fallback
        // branch; the gate must not move the station they already had.
        let config = LandingGearConfig::default();
        let unchecked = config.resolved_station_positions(3.0, 11.0, 0.0, 27.166);
        let checked = config
            .resolved_station_positions_checked(3.0, 11.0, 0.0, 27.166, || {
                WingMountedGearDomain::from_heights(-0.4, 1.385)
            })
            .expect("an in-domain fallback must resolve");
        assert_eq!(checked, unchecked);
        assert!(!checked.source_scaled);
        assert_eq!(checked.x_mlg_m, 11.0);
    }

    #[test]
    fn a_source_anchor_is_admissible_on_any_layout_and_never_asks_the_domain() {
        // A published station scaled onto the active fuselage carries no
        // wing-mounted assumption, so the high-wing verdict is irrelevant -
        // and the query must not even run, because it is the expensive one.
        let config = LandingGearConfig {
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(27.166),
            reference_nlg_x_fraction: Some(0.1),
            reference_mlg_x_fractions: Some(vec![0.45, 0.45]),
            n_mlg_struts: 2,
            ..Default::default()
        };
        let mut domain_queries = 0_usize;
        let resolved = config
            .resolved_station_positions_checked(3.0, 11.0, 0.0, 27.166, || {
                domain_queries += 1;
                WingMountedGearDomain::from_heights(1.85, 1.385)
            })
            .expect("a source-anchored station must resolve on any layout");
        assert!(resolved.source_scaled);
        assert_eq!(domain_queries, 0);
    }

    #[test]
    fn the_domain_boundary_is_strict_and_carries_no_margin() {
        // A root exactly on the crown is not a high-wing layout, and a pair
        // of heights that are not both finite decides nothing: the caller's
        // own finiteness checks own that failure.
        assert!(WingMountedGearDomain::from_heights(1.385, 1.385).applies());
        assert!(!WingMountedGearDomain::from_heights(1.385 + 1.0e-12, 1.385).applies());
        assert!(WingMountedGearDomain::from_heights(f64::NAN, 1.385).applies());
        assert!(WingMountedGearDomain::from_heights(1.85, f64::NAN).applies());
        assert!(WingMountedGearDomain::from_heights(-0.4, 1.385).applies());
    }

    #[test]
    fn effective_main_gear_station_force_moment_equilibrium_closure() {
        // Airbus A380 AC 7-3-0 reference loading condition:
        // WLG: 106,920 kg per strut (4 wheels -> 26,730 kg/wheel)
        // BLG: 160,380 kg per strut (6 wheels -> 26,730 kg/wheel)
        // Total main gear load = 2 * 106,920 + 2 * 160,380 = 534,600 kg (20 wheels * 26,730 kg)
        let stations = [33.58, 33.58, 36.85, 36.85];
        let wheels = [4, 4, 6, 6];
        let eff = effective_main_gear_station(&stations, Some(&wheels))
            .expect("A380 closure stations should be valid");
        let eff_x = eff.station_m();

        let x_nlg = 4.97;
        let g = 9.80665;
        let load_per_wheel = 26_730.0 * g; // N

        // Discrete sum of moments about NLG
        let discrete_moment: f64 = stations
            .iter()
            .zip(wheels.iter())
            .map(|(&x, &w)| (w as f64 * load_per_wheel) * (x - x_nlg))
            .sum();

        // Effective aggregate reaction moment about NLG
        let total_main_load = 20.0 * load_per_wheel;
        let aggregate_moment = total_main_load * (eff_x - x_nlg);

        let moment_residual = (discrete_moment - aggregate_moment).abs();
        assert!(
            moment_residual < 1.0e-6,
            "Moment equilibrium residual was {moment_residual} N*m"
        );
    }
}
