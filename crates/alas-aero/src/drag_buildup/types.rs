// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Analyses/Aerodynamics/Fidelity_Zero.py and the
// mission analysis model/Methods/Aerodynamics/Common/Fidelity_Zero/Drag/ family.
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! The inputs the drag buildup reads and the breakdown it reports.

/// The settings `mission analysis model.Analyses.Aerodynamics.Fidelity_Zero()` carries that
/// the drag chain reads.
///
/// The numeric fields' defaults are that analysis's own, because
/// `mission_builder.py:85-87` (the only place in the reference that builds
/// one) attaches it to a vehicle and overrides nothing. The
/// `area_weighted_compressibility` field is an explicit product/reference
/// policy seam: product default is force-conserving, while
/// `reference_compatibility()` replays the frozen direct sum. Carrying both
/// the numeric inputs and that policy as fields makes the choice checkable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragSettings {
    /// `C` in the wing form factor. Upstream default 1.1.
    pub wing_parasite_drag_form_factor: f64,
    /// The fuselage form factor's multiplier on the peak velocity increment.
    /// Upstream default 2.3.
    pub fuselage_parasite_drag_form_factor: f64,
    /// `K` in the viscous, lift-dependent induced drag. Upstream default 0.38.
    pub viscous_lift_dependent_drag_factor: f64,
    /// The factor the whole untrimmed buildup is multiplied by. Upstream
    /// default 1.02.
    pub trim_drag_correction_factor: f64,
    /// A flat addition to the total. Upstream default 0.
    pub drag_coefficient_increment: f64,
    /// The spoiler contribution. Upstream default 0.
    pub spoiler_drag_increment: f64,
    /// A fractional lift-to-drag adjustment the total is divided through by.
    /// Upstream default 0.
    pub lift_to_drag_adjustment: f64,
    /// Whether per-wing compressibility coefficients are converted through
    /// their own reference areas before aircraft normalization. Product
    /// evaluations use `true`; frozen translation fixtures may set `false`
    /// to replay the historical direct coefficient sum explicitly.
    pub area_weighted_compressibility: bool,
}

impl Default for DragSettings {
    fn default() -> Self {
        Self {
            wing_parasite_drag_form_factor: 1.1,
            fuselage_parasite_drag_form_factor: 2.3,
            viscous_lift_dependent_drag_factor: 0.38,
            trim_drag_correction_factor: 1.02,
            drag_coefficient_increment: 0.0,
            spoiler_drag_increment: 0.0,
            lift_to_drag_adjustment: 0.0,
            area_weighted_compressibility: true,
        }
    }
}

impl DragSettings {
    /// Return the frozen translation settings before compressibility area
    /// normalization was corrected.
    ///
    /// This is for historical fixtures only. Product callers should use
    /// [`Default::default`], whose aggregation conserves drag force across
    /// wings with different reference areas.
    pub fn reference_compatibility() -> Self {
        Self {
            area_weighted_compressibility: false,
            ..Self::default()
        }
    }
}

/// The freestream state the correlations read.
///
/// Taken as data rather than computed from an altitude. Upstream reads these
/// three off `state.conditions.freestream`, which a mission segment fills from
/// `mission analysis model.Analyses.Atmospheric.US_Standard_1976`: `alas-atmo::us1976`'s own
/// green row here. Re-deriving them inside this module would make every drag
/// number a function of an atmosphere model as well as of a drag correlation,
/// and a disagreement in the first would be reported as a disagreement in the
/// second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Freestream {
    /// Mach number.
    pub mach: f64,
    /// Static temperature, in Kelvin.
    pub temperature_k: f64,
    /// Reynolds number *per metre*: every consumer multiplies it by its own
    /// reference length. Upstream computes it as `rho * V / mu` and names it
    /// `reynolds_number` regardless.
    pub reynolds_number_per_m: f64,
}

/// Everything the buildup reads off one wing.
///
/// The last two fields are the vortex lattice's, not this module's. With
/// `span_efficiency` at its `None` default the inviscid induced drag *is*
/// `drag_breakdown.induced.inviscid_wings[tag]`, which
/// `mission analysis model.Analyses.Aerodynamics.Vortex_Lattice` writes, so this row takes
/// the lift solution as input, the same way `alas-mass::transport_weight`
/// takes `sealevel_static_thrust` from `turbofan_sizing`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingParams {
    /// Mean aerodynamic chord, in metres.
    pub mean_aerodynamic_chord_m: f64,
    /// Quarter-chord sweep, in radians.
    pub quarter_chord_sweep_rad: f64,
    /// Thickness-to-chord ratio.
    pub thickness_to_chord: f64,
    /// Reference area, in square metres.
    pub reference_area_m2: f64,
    /// Wetted area, in square metres.
    pub wetted_area_m2: f64,
    /// Upper-surface transition location, as a fraction of chord.
    pub transition_x_upper: f64,
    /// Lower-surface transition location, as a fraction of chord.
    pub transition_x_lower: f64,
    /// Aspect ratio.
    pub aspect_ratio: f64,
    /// This wing's lift coefficient, from the vortex-lattice solution.
    pub inviscid_lift_coefficient: f64,
    /// This wing's inviscid induced drag coefficient, from the same solution.
    pub inviscid_induced_drag_coefficient: f64,
}

/// Everything the buildup reads off one fuselage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuselageParams {
    /// Total length, in metres.
    pub length_m: f64,
    /// Effective diameter, in metres.
    pub effective_diameter_m: f64,
    /// Front projected area, in square metres.
    pub front_projected_area_m2: f64,
    /// Wetted area, in square metres.
    pub wetted_area_m2: f64,
}

/// Everything the buildup reads off one nacelle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NacelleParams {
    /// Total length, in metres.
    pub length_m: f64,
    /// Maximum diameter, in metres.
    pub diameter_m: f64,
    /// Wetted area, in square metres.
    pub wetted_area_m2: f64,
    /// How many positions this nacelle is installed at.
    ///
    /// Upstream's excrescence buildup multiplies the wetted area by
    /// `len(nacelle.origin)`, and nothing else reads it.
    /// `vehicle_builder.py:229` gives every nacelle exactly one origin and
    /// appends one component per engine, so this is 1 on every aircraft this
    /// program builds; it is carried rather than assumed because the
    /// alternative is a silently wrong wetted area the day something appends
    /// a two-origin nacelle.
    pub origin_count: usize,
}

/// The aircraft the buildup runs on.
#[derive(Debug, Clone, Copy)]
pub struct DragVehicle<'a> {
    /// The vehicle reference area every component contribution is scaled to,
    /// in square metres.
    pub reference_area_m2: f64,
    /// The wings, in the order upstream's container iterates them.
    pub wings: &'a [WingParams],
    /// The fuselages.
    pub fuselages: &'a [FuselageParams],
    /// The nacelles.
    pub nacelles: &'a [NacelleParams],
    /// How many propulsion networks the vehicle carries.
    ///
    /// Upstream's pylon buildup divides its *reported* skin-friction, form,
    /// compressibility and Reynolds factors by this, having summed them over
    /// nacelles rather than averaged them. The pylon drag itself does not
    /// depend on it.
    pub network_count: usize,
}

/// One component's entry in `drag_breakdown.parasite`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComponentParasiteDrag {
    /// The component's parasite drag coefficient.
    ///
    /// On its own reference area as `parasite_drag_wing` and friends return
    /// it, then rescaled to the vehicle reference area in place by
    /// `parasite_total`, see [`super::parasite::scale_to_vehicle_reference`].
    pub parasite_drag_coefficient: f64,
    /// The skin-friction coefficient, averaged over the two surfaces for a
    /// wing and taken directly for everything else.
    pub skin_friction_coefficient: f64,
    /// The form factor.
    pub form_factor: f64,
    /// The compressibility correction.
    pub compressibility_factor: f64,
    /// The Reynolds-number correction.
    pub reynolds_factor: f64,
}

/// One wing's entry in `drag_breakdown.compressible`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WingCompressibilityDrag {
    /// The compressibility drag coefficient.
    pub compressibility_drag: f64,
    /// The crest-critical Mach number.
    pub crest_critical: f64,
    /// The drag-divergence Mach number.
    ///
    /// Reported by upstream and read by nothing in the buildup; carried so a
    /// port that got the crest-critical fit right and the divergence
    /// relation wrong is still caught.
    pub divergence_mach: f64,
}

/// Everything `Fidelity_Zero`'s drag chain leaves in
/// `state.conditions.aerodynamics.drag_breakdown`.
#[derive(Debug, Clone, PartialEq)]
pub struct DragBreakdown {
    /// Per-wing parasite entries, aligned with [`DragVehicle::wings`].
    pub parasite_wings: Vec<ComponentParasiteDrag>,
    /// Per-fuselage parasite entries, aligned with [`DragVehicle::fuselages`].
    pub parasite_fuselages: Vec<ComponentParasiteDrag>,
    /// Per-nacelle parasite entries, aligned with [`DragVehicle::nacelles`].
    pub parasite_nacelles: Vec<ComponentParasiteDrag>,
    /// The pylon entry, a fixed fraction of the nacelles'.
    pub parasite_pylon: ComponentParasiteDrag,
    /// `drag_breakdown.parasite.total`.
    pub parasite_total: f64,
    /// `drag_breakdown.induced.total`.
    pub induced_total: f64,
    /// `drag_breakdown.induced.viscous`.
    pub induced_viscous: f64,
    /// `drag_breakdown.induced.viscous_wings_drag`, aligned with the wings.
    ///
    /// Recorded before the area scaling the total applies, as upstream does.
    pub induced_viscous_wings: Vec<f64>,
    /// Per-wing compressibility entries, aligned with the wings.
    pub compressible_wings: Vec<WingCompressibilityDrag>,
    /// `drag_breakdown.compressible.total`.
    pub compressible_total: f64,
    /// `drag_breakdown.miscellaneous.total_wetted_area`, in square metres.
    pub miscellaneous_total_wetted_area_m2: f64,
    /// `drag_breakdown.miscellaneous.total`.
    pub miscellaneous_total: f64,
    /// `drag_breakdown.untrimmed`: the four contributions above, summed.
    pub untrimmed: f64,
    /// `drag_breakdown.trim_corrected_drag`.
    pub trim_corrected: f64,
    /// `drag_breakdown.spoiler_drag`.
    pub spoiler: f64,
    /// `drag_breakdown.total`, which is also `conditions.aerodynamics.drag_coefficient`.
    pub total: f64,
}
