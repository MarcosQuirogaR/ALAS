// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Explicit frame, reference and coefficient conventions shared by the
//! dictionary writer, the result parsers, the case README and the GUI.
//!
//! * Frame: chord along +x from the leading edge (x = 0) to the trailing
//!   edge (x = c), section normal +y, one-cell extrusion along +z with span
//!   `s = c * EXTRUSION_SPAN_TO_CHORD`.
//! * Velocity: `U = U_inf (cos a, sin a, 0)`; positive angle of attack
//!   rotates the freestream toward +y, equivalent to leading edge up.
//! * Forces: drag positive along `U`; lift positive 90 degrees
//!   counter-clockwise from `U` in the x-y plane, `(-sin a, cos a, 0)`.
//! * Moment: reference at the quarter chord on the extrusion mid-plane
//!   `(c/4, 0, s/2)`; pitch axis `(0, 0, -1)` so that `Cm` is positive
//!   nose-up (leading edge toward +y).  This matches the aerospace
//!   convention used by the rest of ALAS and the native surface
//!   integration, which evaluates `-M_z`.
//! * Normalisation: `q = rho U^2 / 2`, `Aref = c * s`, `lRef = c`.  The
//!   extrusion span cancels in section coefficients because forces are
//!   integrated over the same span.
//!
//! Every quantity here is SI.  Nothing in this module runs a solver.

use super::*;

/// OpenFOAM `forceCoeffs` pitch axis giving nose-up-positive `Cm`.
pub const PITCH_AXIS: [f64; 3] = [0.0, 0.0, -1.0];

/// Chord fraction of the pitching-moment reference point.
pub const MOMENT_REFERENCE_X_OVER_C: f64 = 0.25;

/// Freestream Mach number above which the incompressible template is
/// rejected before launch.
pub const INCOMPRESSIBLE_LIMIT_MACH: f64 = 0.3;

/// Freestream Mach number from which compressibility is flagged as a caution.
pub const COMPRESSIBILITY_CAUTION_MACH: f64 = 0.2;

/// Chord Reynolds number below which a fully turbulent SST solution is
/// transition-sensitive and flagged as a caution.
pub const TRANSITION_SENSITIVE_REYNOLDS: f64 = 5.0e5;

/// Angle of attack magnitude above which steady RANS separation and stall
/// behaviour is flagged as a caution.
pub const STEADY_RANS_ALPHA_CAUTION_DEG: f64 = 10.0;

/// Target y+ band (exclusive) inside the buffer layer, where neither the
/// viscous-sublayer nor the log-law branch of the wall treatment is resolved
/// cleanly.
pub const BUFFER_LAYER_Y_PLUS: (f64, f64) = (5.0, 30.0);

/// Freestream turbulence intensity (fraction) above which the inflow state
/// is flagged as unusually high for external aerodynamics.
pub const HIGH_TURBULENCE_INTENSITY: f64 = 0.05;

/// Reference lengths, areas, directions and moment point for one study.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferenceConventions {
    /// Reference chord `lRef` in metres.
    pub chord_m: f64,
    /// Extrusion span in metres.
    pub span_m: f64,
    /// Reference area `Aref = chord * span` in square metres.
    pub area_m2: f64,
    /// Freestream speed magnitude `magUInf` in m/s.
    pub speed_m_s: f64,
    /// Geometric angle of attack in degrees.
    pub angle_of_attack_deg: f64,
    /// Dynamic pressure `rho U^2 / 2` in Pa.
    pub dynamic_pressure_pa: f64,
    /// Freestream velocity vector in m/s.
    pub freestream_velocity_m_s: [f64; 3],
    /// Unit drag direction (along the freestream).
    pub drag_direction: [f64; 3],
    /// Unit lift direction (normal to the freestream, toward +y at zero angle).
    pub lift_direction: [f64; 3],
    /// Pitch axis passed to `forceCoeffs`; see [`PITCH_AXIS`].
    pub pitch_axis: [f64; 3],
    /// Moment reference point `CofR` in metres.
    pub moment_reference_m: [f64; 3],
}

impl ReferenceConventions {
    /// Derive every reference quantity from the effective configuration.
    pub fn from_config(config: &CfdStudyConfig) -> Self {
        let chord_m = config.chord_m;
        let span_m = chord_m * EXTRUSION_SPAN_TO_CHORD;
        let speed_m_s = config.effective_speed_m_s();
        let alpha = config.angle_of_attack_deg.to_radians();
        let (sin, cos) = alpha.sin_cos();
        Self {
            chord_m,
            span_m,
            area_m2: chord_m * span_m,
            speed_m_s,
            angle_of_attack_deg: config.angle_of_attack_deg,
            dynamic_pressure_pa: 0.5 * config.density_kg_m3 * speed_m_s * speed_m_s,
            freestream_velocity_m_s: [speed_m_s * cos, speed_m_s * sin, 0.0],
            drag_direction: [cos, sin, 0.0],
            lift_direction: [-sin, cos, 0.0],
            pitch_axis: PITCH_AXIS,
            moment_reference_m: [MOMENT_REFERENCE_X_OVER_C * chord_m, 0.0, 0.5 * span_m],
        }
    }
}

/// Effective OpenFOAM boundary-condition types for one named patch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchCondition {
    /// Patch name in `constant/polyMesh/boundary`.
    pub patch: String,
    /// Physical role of the patch.
    pub role: String,
    /// `0/U` condition type.
    pub velocity: String,
    /// `0/p` condition type (kinematic pressure).
    pub pressure: String,
    /// `0/k` condition type.
    pub k: String,
    /// `0/omega` condition type.
    pub omega: String,
    /// `0/nut` condition type.
    pub nut: String,
}

fn patch(
    patch: &str,
    role: &str,
    velocity: &str,
    pressure: &str,
    k: &str,
    omega: &str,
    nut: &str,
) -> PatchCondition {
    PatchCondition {
        patch: patch.to_owned(),
        role: role.to_owned(),
        velocity: velocity.to_owned(),
        pressure: pressure.to_owned(),
        k: k.to_owned(),
        omega: omega.to_owned(),
        nut: nut.to_owned(),
    }
}

/// The boundary-condition table written by the versioned template for the
/// selected far-field treatment.  The dictionary writer and this table are
/// cross-checked by tests so the GUI never shows a condition the case does
/// not carry.
pub fn boundary_table(boundaries: &BoundarySettings) -> Vec<PatchCondition> {
    let (inlet, outlet, far_field) = match boundaries.far_field {
        FarFieldCondition::FixedValue => (
            patch(
                &boundaries.inlet_patch,
                "inlet",
                "fixedValue",
                "zeroGradient",
                "fixedValue",
                "fixedValue",
                "calculated",
            ),
            patch(
                &boundaries.outlet_patch,
                "outlet",
                "zeroGradient",
                "fixedValue",
                "zeroGradient",
                "zeroGradient",
                "calculated",
            ),
            patch(
                "farField",
                "far field",
                "fixedValue",
                "zeroGradient",
                "fixedValue",
                "fixedValue",
                "calculated",
            ),
        ),
        FarFieldCondition::Freestream => (
            patch(
                &boundaries.inlet_patch,
                "inlet",
                "freestreamVelocity",
                "freestreamPressure",
                "freestream",
                "freestream",
                "freestream",
            ),
            patch(
                &boundaries.outlet_patch,
                "outlet",
                "freestreamVelocity",
                "freestreamPressure",
                "freestream",
                "freestream",
                "freestream",
            ),
            patch(
                "farField",
                "far field",
                "freestreamVelocity",
                "freestreamPressure",
                "freestream",
                "freestream",
                "freestream",
            ),
        ),
    };
    vec![
        inlet,
        outlet,
        far_field,
        patch(
            &boundaries.airfoil_patch,
            "wall",
            "noSlip",
            "zeroGradient",
            "kqRWallFunction",
            "omegaWallFunction",
            "nutUSpaldingWallFunction",
        ),
        patch(
            "frontAndBack",
            "2-D constraint",
            "empty",
            "empty",
            "empty",
            "empty",
            "empty",
        ),
    ]
}

/// Whether a regime flag stops a launch or only qualifies the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegimeSeverity {
    /// The template must not be launched; `validate` reports the same limit.
    Blocking,
    /// The case may run, but the result needs regime-specific evidence.
    Caution,
}

/// One explicit statement about the physical regime of the study.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegimeFlag {
    /// Stable machine-readable code.
    pub code: String,
    /// Blocking or caution.
    pub severity: RegimeSeverity,
    /// Human-readable statement of the limitation.
    pub message: String,
}

/// Regime flags for the effective configuration.  Cautions are advisory
/// engineering thresholds recorded as constants in this module; they are not
/// validation evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RegimeAssessment {
    /// Flags in evaluation order.
    pub flags: Vec<RegimeFlag>,
}

impl RegimeAssessment {
    /// Whether any blocking flag is present.
    pub fn is_blocked(&self) -> bool {
        self.flags
            .iter()
            .any(|flag| flag.severity == RegimeSeverity::Blocking)
    }

    /// Cautions only, in evaluation order.
    pub fn cautions(&self) -> impl Iterator<Item = &RegimeFlag> {
        self.flags
            .iter()
            .filter(|flag| flag.severity == RegimeSeverity::Caution)
    }

    fn push(&mut self, code: &str, severity: RegimeSeverity, message: String) {
        self.flags.push(RegimeFlag {
            code: code.to_owned(),
            severity,
            message,
        });
    }
}

/// Evaluate blocking limits and advisory cautions for the configuration.
pub fn assess_regime(config: &CfdStudyConfig) -> RegimeAssessment {
    let mut regime = RegimeAssessment::default();
    let reynolds = config.effective_reynolds();
    let mach = config.mach_number();
    let alpha = config.angle_of_attack_deg;
    if config.turbulence_model != "kOmegaSST" {
        regime.push(
            "model",
            RegimeSeverity::Blocking,
            format!(
                "Turbulence model {} is not part of the validated incompressible steady k-omega SST template.",
                config.turbulence_model
            ),
        );
    }
    if !mach.is_finite() || mach > INCOMPRESSIBLE_LIMIT_MACH {
        regime.push(
            "mach",
            RegimeSeverity::Blocking,
            format!(
                "Mach {mach:.3} exceeds the incompressible limit of {INCOMPRESSIBLE_LIMIT_MACH:.1}; a validated compressible study is required."
            ),
        );
    } else if mach >= COMPRESSIBILITY_CAUTION_MACH {
        regime.push(
            "mach",
            RegimeSeverity::Caution,
            format!(
                "Mach {mach:.3} is between {COMPRESSIBILITY_CAUTION_MACH:.1} and {INCOMPRESSIBLE_LIMIT_MACH:.1}; compressibility effects on pressure coefficients are not modelled."
            ),
        );
    }
    if !(1.0e3..=1.0e9).contains(&reynolds) {
        regime.push(
            "reynolds",
            RegimeSeverity::Blocking,
            format!("Reynolds number {reynolds:.3e} is outside the accepted range 1e3 to 1e9."),
        );
    } else if reynolds < TRANSITION_SENSITIVE_REYNOLDS {
        regime.push(
            "reynolds",
            RegimeSeverity::Caution,
            format!(
                "Reynolds number {reynolds:.3e} is below {TRANSITION_SENSITIVE_REYNOLDS:.0e}; the fully turbulent SST template does not model laminar separation or transition."
            ),
        );
    }
    if !alpha.is_finite() || alpha.abs() > 30.0 {
        regime.push(
            "alpha",
            RegimeSeverity::Blocking,
            format!("Angle of attack {alpha:.2} deg is outside the accepted -30 to +30 deg range."),
        );
    } else if alpha.abs() > STEADY_RANS_ALPHA_CAUTION_DEG {
        regime.push(
            "alpha",
            RegimeSeverity::Caution,
            format!(
                "Angle of attack {alpha:.2} deg exceeds {STEADY_RANS_ALPHA_CAUTION_DEG:.0} deg; steady RANS is not validated for stall or large separated regions."
            ),
        );
    }
    let y_plus = config.mesh.target_y_plus;
    if y_plus > BUFFER_LAYER_Y_PLUS.0 && y_plus < BUFFER_LAYER_Y_PLUS.1 {
        regime.push(
            "y_plus",
            RegimeSeverity::Caution,
            format!(
                "Target y+ {y_plus:.1} lies in the buffer layer ({:.0} to {:.0}); the Spalding wall treatment bridges it but near-wall resolution evidence is required.",
                BUFFER_LAYER_Y_PLUS.0, BUFFER_LAYER_Y_PLUS.1
            ),
        );
    }
    if config.turbulence_intensity > HIGH_TURBULENCE_INTENSITY {
        regime.push(
            "turbulence_intensity",
            RegimeSeverity::Caution,
            format!(
                "Freestream turbulence intensity {:.1}% exceeds {:.0}%; external-flow validation data use much lower inflow turbulence.",
                config.turbulence_intensity * 100.0,
                HIGH_TURBULENCE_INTENSITY * 100.0
            ),
        );
    }
    regime
}

/// Everything the case will actually use, resolved from the configuration
/// without launching a process.  Shown before running and recorded in the
/// case provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectiveConfiguration {
    /// Versioned template identifier.
    pub template_version: String,
    /// Selected database section.
    pub airfoil_name: String,
    /// Reference lengths, directions and moment point.
    pub reference: ReferenceConventions,
    /// Sign and axis conventions in prose.
    pub frame: FrameConvention,
    /// Chord Reynolds number.
    pub reynolds: f64,
    /// Freestream Mach number from the declared static temperature.
    pub mach: f64,
    /// Dry-air speed of sound in m/s.
    pub speed_of_sound_m_s: f64,
    /// Static temperature in K.
    pub temperature_k: f64,
    /// Density in kg/m^3.
    pub density_kg_m3: f64,
    /// Dynamic viscosity in Pa s.
    pub dynamic_viscosity_pa_s: f64,
    /// Kinematic viscosity `nu` written to `transportProperties`, in m^2/s.
    pub kinematic_viscosity_m2_s: f64,
    /// Physical pressure reference in Pa.
    pub pressure_reference_pa: f64,
    /// Kinematic pressure reference `p_ref / rho` written to `0/p`, in m^2/s^2.
    pub pressure_reference_kinematic_m2_s2: f64,
    /// Turbulence model name.
    pub turbulence_model: String,
    /// Derived freestream turbulence state.
    pub turbulence: TurbulenceState,
    /// Boundary-condition table.
    pub boundaries: Vec<PatchCondition>,
    /// Upstream, downstream and half-height domain extents in chords.
    pub domain_extents_chords: [f64; 3],
    /// Mesh preset.
    pub mesh_preset: MeshPreset,
    /// Maximum SIMPLE iterations.
    pub max_iterations: u32,
    /// Final convection scheme.
    pub convection_scheme: ConvectionScheme,
    /// Regime flags.
    pub regime: RegimeAssessment,
}

impl CfdStudyConfig {
    /// Resolve the effective configuration without validation side effects.
    pub fn effective_configuration(&self) -> EffectiveConfiguration {
        EffectiveConfiguration {
            template_version: TEMPLATE_VERSION.to_owned(),
            airfoil_name: self.airfoil_name.clone(),
            reference: ReferenceConventions::from_config(self),
            frame: FrameConvention::default(),
            reynolds: self.effective_reynolds(),
            mach: self.mach_number(),
            speed_of_sound_m_s: self.speed_of_sound_m_s(),
            temperature_k: self.freestream_temperature_k,
            density_kg_m3: self.density_kg_m3,
            dynamic_viscosity_pa_s: self.dynamic_viscosity_pa_s,
            kinematic_viscosity_m2_s: self.dynamic_viscosity_pa_s / self.density_kg_m3,
            pressure_reference_pa: self.boundaries.pressure_reference_pa,
            pressure_reference_kinematic_m2_s2: self.boundaries.pressure_reference_pa
                / self.density_kg_m3,
            turbulence_model: self.turbulence_model.clone(),
            turbulence: self.effective_turbulence(),
            boundaries: boundary_table(&self.boundaries),
            domain_extents_chords: [
                self.mesh.upstream_chords,
                self.mesh.downstream_chords,
                self.mesh.half_height_chords,
            ],
            mesh_preset: self.mesh.preset,
            max_iterations: self.solver.max_iterations,
            convection_scheme: self.solver.convection_scheme,
            regime: assess_regime(self),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::{SurfaceReference, SurfaceSample};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_case_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("alas-cfd-conventions-{label}-{stamp}"))
    }

    fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    #[test]
    fn reference_directions_are_orthonormal_and_area_uses_the_extrusion_span() {
        let mut config = CfdStudyConfig::default();
        config.chord_m = 0.5;
        config.angle_of_attack_deg = 6.0;
        let reference = ReferenceConventions::from_config(&config);
        assert!((reference.span_m - 0.5 * EXTRUSION_SPAN_TO_CHORD).abs() < 1.0e-15);
        assert!((reference.area_m2 - reference.chord_m * reference.span_m).abs() < 1.0e-15);
        assert!((dot(reference.drag_direction, reference.drag_direction) - 1.0).abs() < 1.0e-12);
        assert!((dot(reference.lift_direction, reference.lift_direction) - 1.0).abs() < 1.0e-12);
        assert!(dot(reference.drag_direction, reference.lift_direction).abs() < 1.0e-12);
        // Positive angle rotates the freestream toward +y and keeps lift
        // toward +y at small angles.
        assert!(reference.freestream_velocity_m_s[1] > 0.0);
        assert!(reference.lift_direction[1] > 0.9);
        assert!(reference.lift_direction[0] < 0.0);
        assert_eq!(reference.pitch_axis, PITCH_AXIS);
        assert!((reference.moment_reference_m[0] - 0.125).abs() < 1.0e-15);
        assert!((reference.moment_reference_m[2] - 0.5 * reference.span_m).abs() < 1.0e-15);
        let expected_q = 0.5 * config.density_kg_m3 * config.speed_m_s * config.speed_m_s;
        assert!((reference.dynamic_pressure_pa - expected_q).abs() < 1.0e-9);
    }

    #[test]
    fn pitch_axis_makes_dictionary_and_native_integration_agree_on_nose_up_positive_cm() {
        // A suction patch on the upper surface aft of the quarter chord
        // produces upward lift behind the reference point: a nose-down
        // moment, which must be reported as a negative Cm by both routes.
        let config = CfdStudyConfig::default();
        let reference = ReferenceConventions::from_config(&config);
        let face_area = 1.0e-3;
        let force_y = -face_area * config.density_kg_m3 * -50.0;
        let sample = SurfaceSample {
            patch_face_index: 0,
            global_face_index: 0,
            center_m: [0.75 * config.chord_m, 0.05, 0.5 * reference.span_m],
            // Wall area vectors point out of the fluid, into the body.
            area_vector_m2: [0.0, -face_area, 0.0],
            face_area_m2: face_area,
            surface_length_m: face_area / reference.span_m,
            arc_length_m: 0.0,
            tangent_plus_chord: [1.0, 0.0, 0.0],
            p_kinematic_m2_s2: -50.0,
            pressure_pa: -50.0 * config.density_kg_m3,
            wall_shear_kinematic_m2_s2: [0.0; 3],
            wall_shear_stress_pa: [0.0; 3],
            wall_shear_magnitude_pa: 0.0,
            cf: 0.0,
            cf_magnitude: 0.0,
            cp: 0.0,
        };
        let surface_reference = SurfaceReference {
            density_kg_m3: config.density_kg_m3,
            speed_m_s: reference.speed_m_s,
            chord_m: reference.chord_m,
            angle_of_attack_deg: 0.0,
            reference_area_m2: reference.area_m2,
            pressure_reference_pa: 0.0,
            moment_reference_m: reference.moment_reference_m,
        };
        let summary =
            crate::surface::integrate(&[sample], &surface_reference).expect("integration");
        assert!(force_y > 0.0);
        assert!(summary.cl > 0.0, "cl={}", summary.cl);
        assert!(summary.cm < 0.0, "native cm={}", summary.cm);
        // OpenFOAM forceCoeffs projects M = r x F onto the pitch axis.
        let arm = [
            0.75 * config.chord_m - reference.moment_reference_m[0],
            0.05,
            0.0,
        ];
        let moment_z = arm[0] * force_y - arm[1] * 0.0;
        let openfoam_cm = dot([0.0, 0.0, moment_z], reference.pitch_axis)
            / (reference.dynamic_pressure_pa * reference.area_m2 * reference.chord_m);
        assert!(openfoam_cm < 0.0, "forceCoeffs cm={openfoam_cm}");
        assert!((openfoam_cm - summary.cm).abs() < 1.0e-9 * openfoam_cm.abs().max(1.0e-30));
    }

    #[test]
    fn control_dict_carries_the_typed_reference_conventions() {
        let path = test_case_dir("control-dict");
        let config = CfdStudyConfig::default();
        let reference = ReferenceConventions::from_config(&config);
        generate_case(&config, &path).expect("case generation should succeed");
        let control = fs::read_to_string(path.join("system/controlDict")).expect("controlDict");
        assert!(control.contains("pitchAxis (0 0 -1);"), "{control}");
        assert!(control.contains(&format!("lRef {:.16e};", reference.chord_m)));
        assert!(control.contains(&format!("Aref {:.16e};", reference.area_m2)));
        assert!(control.contains(&format!("magUInf {:.16e};", reference.speed_m_s)));
        assert!(control.contains(&format!(
            "CofR ({:.16e} 0 {:.16e});",
            reference.moment_reference_m[0], reference.moment_reference_m[2]
        )));
        assert!(control.contains(&format!(
            "liftDir ({:.16e} {:.16e} 0);",
            reference.lift_direction[0], reference.lift_direction[1]
        )));
        let velocity = fs::read_to_string(path.join("0/U")).expect("U");
        assert!(velocity.contains(&format!(
            "internalField uniform ({:.16e} {:.16e} 0);",
            reference.freestream_velocity_m_s[0], reference.freestream_velocity_m_s[1]
        )));
        let study = fs::read_to_string(path.join("study.json")).expect("study");
        assert!(study.contains("\"pitch_axis\""));
        assert!(study.contains(TEMPLATE_VERSION));
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn generated_boundary_files_match_the_effective_boundary_table() {
        for far_field in [FarFieldCondition::FixedValue, FarFieldCondition::Freestream] {
            let path = test_case_dir("boundaries");
            let mut config = CfdStudyConfig::default();
            config.boundaries.far_field = far_field;
            generate_case(&config, &path).expect("case generation should succeed");
            let fields: [(&str, fn(&PatchCondition) -> String); 5] = [
                ("0/U", |patch: &PatchCondition| patch.velocity.clone()),
                ("0/p", |patch: &PatchCondition| patch.pressure.clone()),
                ("0/k", |patch: &PatchCondition| patch.k.clone()),
                ("0/omega", |patch: &PatchCondition| patch.omega.clone()),
                ("0/nut", |patch: &PatchCondition| patch.nut.clone()),
            ];
            let table = boundary_table(&config.boundaries);
            assert_eq!(table.len(), 5);
            for (relative, select) in fields {
                let text = fs::read_to_string(path.join(relative)).expect(relative);
                for entry in &table {
                    let needle = format!("{} {{ type {};", entry.patch, select(entry));
                    assert!(
                        text.contains(&needle),
                        "{relative} ({far_field:?}) lacks {needle:?}"
                    );
                }
            }
            let _ = fs::remove_dir_all(path);
        }
    }

    #[test]
    fn regime_flags_track_validation_and_expose_cautions() {
        let config = CfdStudyConfig::default();
        let regime = assess_regime(&config);
        assert!(regime.flags.is_empty(), "{:?}", regime.flags);
        assert!(!regime.is_blocked());

        let mut stall = CfdStudyConfig::default();
        stall.angle_of_attack_deg = 15.0;
        let regime = assess_regime(&stall);
        assert!(!regime.is_blocked());
        assert!(regime.cautions().any(|flag| flag.code == "alpha"));
        assert!(stall.validate().is_ok());

        let mut compressible = CfdStudyConfig::default();
        compressible.speed_m_s = 120.0;
        let regime = assess_regime(&compressible);
        assert!(regime.is_blocked());
        assert!(regime
            .flags
            .iter()
            .any(|flag| flag.code == "mach" && flag.severity == RegimeSeverity::Blocking));
        assert!(compressible.validate().is_err());

        let mut low_reynolds = CfdStudyConfig::default();
        low_reynolds.speed_m_s = 5.0;
        let regime = assess_regime(&low_reynolds);
        assert!(regime.cautions().any(|flag| flag.code == "reynolds"));
        assert!(!regime.is_blocked());

        let mut buffer = CfdStudyConfig::default();
        buffer.mesh.target_y_plus = 12.0;
        assert!(assess_regime(&buffer)
            .cautions()
            .any(|flag| flag.code == "y_plus"));
    }

    #[test]
    fn effective_configuration_round_trips_through_json() {
        let config = CfdStudyConfig::default();
        let effective = config.effective_configuration();
        assert_eq!(effective.template_version, TEMPLATE_VERSION);
        assert_eq!(effective.boundaries.len(), 5);
        assert!(
            (effective.kinematic_viscosity_m2_s
                - config.dynamic_viscosity_pa_s / config.density_kg_m3)
                .abs()
                < 1.0e-18
        );
        let json = serde_json::to_string(&effective).expect("serialize");
        let parsed: EffectiveConfiguration = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, effective);
    }
}
