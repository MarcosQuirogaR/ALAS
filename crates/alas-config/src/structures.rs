// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/structures_config.py
// Reference: alas @ rust-port-baseline.

//! The wingbox the structural analysis sizes, and how it is solved.
//!
//! The split here follows [`crate::mses`]: everything the user chooses in
//! advance: how many spars and where, which materials, the rib pattern, the
//! safety margin: lives in this struct, and everything that follows from
//! those choices (cap dimensions, rib spacing, the skin thickness bump)
//! is computed rather than stored. A derived quantity kept as a field is a
//! second source of truth that drifts from the first.
//!
//! Running a real finite-element solve is opt-in, because it needs a licensed
//! install and takes minutes. Everything else: the sizing, the mesh files,
//! and the analytical deflection, stress and frequency estimates, is
//! computed whenever the analysis is enabled, so the absence of a solver
//! degrades the results rather than removing them.
//!
//! Several fields declare a label and a unit but no explanation upstream;
//! those explanations are this port's, as CONTRIBUTING.md requires, and they
//! change no value.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Wingbox structural analysis settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct StructuresConfig {
    /// Whether a normal run sizes and analyses a wingbox.
    #[config(
        label = "Enabled",
        help = "Size a generic wingbox (skin/spars/ribs) for the optimized design's main wing, write NASTRAN .bdf files, and compute theoretical (no-NASTRAN) deformations/stresses/frequencies as part of a normal Run, populating the Structural Analysis Results tab. This switch controls the downstream structural solve; the configured spars, materials, and gauges still define the main-wing mass centroid used by weight and balance, without replacing the Torenbeek total wing mass."
    )]
    pub enabled: bool,

    /// Where each spar sits along the chord.
    #[config(
        label = "Spar chord positions",
        unit = "x/c",
        help = "Chordwise position of each spar, as a fraction of local chord (0=leading edge, 1=trailing edge). One entry per spar, any order (sorted automatically). E.g. (0.25, 0.70) is a classic front/rear 2-spar box; add a third entry for a mid-spar."
    )]
    pub spar_chord_fractions: Vec<f64>,

    /// Which ribs carry a trailing-edge panel.
    #[config(
        options = TeRibMode,
        label = "Trailing-edge rib panel mode",
        help = "Which ribs get trailing-edge panels (rear-spar to TE), preventing TE buckling: 'all', 'none', 'alternate', 'inboard' (only inboard of the wing break), 'outboard', or 'step_N' (every Nth rib)."
    )]
    pub te_rib_mode: String,

    /// Whether a partial-span third spar runs from the root to the kink.
    #[config(
        label = "Add center spar (root-to-kink)",
        help = "Adds an optional third spar running only from the root to the wing break/kink station, at center_spar_chord_fraction of local chord: the partial-span reinforcement spar common on widebody wings (extra bending/shear capacity where root load is highest, without the mass of running it all the way to the tip). Off by default (classic 2-spar box). This spar carries no load and contributes no mass outboard of the kink; it simply doesn't exist there."
    )]
    pub center_spar_enabled: bool,

    /// Where that partial-span spar sits along the chord.
    #[config(
        label = "Center spar chord position",
        unit = "x/c",
        help = "Chordwise position of the optional center spar (see center_spar_enabled), as a fraction of local chord. Only used when center_spar_enabled is True."
    )]
    pub center_spar_chord_fraction: f64,

    /// What the skin panels are made of.
    #[config(
        options = Material,
        label = "Skin material",
        help = "Material name from the built-in structural material database, used for the wing skin panels."
    )]
    pub skin_material: String,

    /// What the spar shear webs are made of.
    #[config(
        options = Material,
        label = "Spar web material",
        help = "Material for the spar shear webs."
    )]
    pub spar_web_material: String,

    /// What the spar caps are made of.
    #[config(
        options = Material,
        label = "Spar cap material",
        help = "Material for the spar caps (the primary bending-load-carrying members)."
    )]
    pub spar_cap_material: String,

    /// What the rib webs are made of.
    #[config(
        options = Material,
        label = "Rib material",
        help = "Material for the rib webs."
    )]
    pub rib_material: String,

    /// Margin applied on top of the certified ultimate loads.
    #[config(
        label = "Additional safety factor",
        unit = "-",
        help = "Extra margin multiplied onto the design loads on top of the CS-25 ultimate load factors already used (DesignRequirements.ultimate_load_factor / limit_load_factor_neg). 1.0 = no extra margin beyond CS-25 ultimate."
    )]
    pub additional_safety_factor: f64,

    /// Skin thickness.
    #[config(
        label = "Skin gauge",
        unit = "m",
        help = "Wing skin thickness; this is the value actually used (no shear-flow/buckling upsizing is modeled, see structural_sizing.py), so treat it as a practical starting assumption for this class of aircraft, not a bare absolute-minimum gauge. 6mm matches the reference sizing scripts' own baseline for a large long-range wing; a much thinner value (e.g. 2mm) understates real skin panel buckling resistance and inflates the auto-derived rib count substantially, since rib spacing scales with sqrt(t_skin)."
    )]
    pub t_skin_min_m: f64,

    /// Thinnest a spar web may be sized to.
    #[config(
        label = "Minimum spar web gauge",
        unit = "m",
        help = "Floor on spar web thickness. The web is sized for shear, which for a large wing lands below what is practical to manufacture and handle, so the floor rather than the shear calculation is usually what sets it."
    )]
    pub t_web_min_m: f64,

    /// Rib web thickness.
    #[config(
        label = "Rib web thickness",
        unit = "m",
        help = "Thickness of every rib web. Ribs carry no bending, so this is a fixed practical gauge rather than a sized quantity."
    )]
    pub t_rib_m: f64,

    /// Trailing-edge strip thickness.
    #[config(
        label = "Trailing-edge strip thickness",
        unit = "m",
        help = "Thickness of the panel running from the rear spar to the trailing edge on those ribs that carry one."
    )]
    pub t_te_strip_m: f64,

    /// How far out the spar caps keep their full root section.
    #[config(
        label = "Cap taper lock station",
        unit = "0-1 of semispan",
        help = "Spanwise fraction below which spar caps keep their full root section (bending moment is highest inboard, so locking the section here preserves most of the tip-deflection stiffness). Above this station, caps taper linearly down to cap_taper_tip_fraction at the tip."
    )]
    pub cap_taper_eta_lock: f64,

    /// How much cap section is left at the tip.
    #[config(
        label = "Cap taper tip fraction",
        unit = "-",
        help = "Fraction of the locked-section cap flange width/thickness remaining at the wingtip."
    )]
    pub cap_taper_tip_fraction: f64,

    /// Panel-buckling coefficient the rib spacing is derived from.
    #[config(
        label = "Rib spacing buckling coefficient",
        unit = "-",
        help = "Empirical panel-buckling coefficient (c) in the Euler skin-panel critical stress formula used to auto-derive rib spacing/count: higher allows wider rib spacing for the same skin thickness."
    )]
    pub rib_buckling_coeff: f64,

    /// Effective radius of gyration of a stiffened skin panel.
    #[config(
        label = "Stiffened-panel radius of gyration",
        unit = "m",
        help = "Effective radius of gyration of a skin panel stiffened by a stringer, used by the same rib-spacing buckling formula."
    )]
    pub rib_radius_of_gyration_m: f64,

    /// An exact rib count, instead of the derived one.
    #[config(
        label = "Rib count override",
        help = "Force an exact number of ribs instead of the auto-derived panel-buckling spacing. Leave blank to let the app determine rib count/spacing."
    )]
    pub num_ribs_override: Option<i64>,

    /// How finely the spanwise integrals are sampled.
    #[config(
        label = "Spanwise integration stations",
        help = "Number of spanwise points used for load/moment/deflection integration, structural wing-mass centroid integration, and the analytical deformation/stress solver. Higher = smoother curves, slower."
    )]
    pub spanwise_stations: i64,

    /// How finely each rib section is sampled for the mesh.
    #[config(
        label = "Mesh chordwise points per rib",
        help = "Number of chordwise points sampled per rib cross-section in the FEM mesh."
    )]
    pub mesh_chordwise_points: i64,

    /// Where the finite-element solver lives.
    #[config(
        hidden,
        label = "NASTRAN executable path",
        help = "Path (repo-root-relative or absolute) to nastran.exe. Leave blank to only generate .bdf files and use the theoretical (analytical) deformation/stress/frequency estimates, no NASTRAN install is required for that path. Set on Setup > External Tools."
    )]
    pub nastran_exe_path: String,

    /// Optional solver binary passed through MSC's launcher.
    ///
    /// Some MSC Student Edition installations ship a working `nastran.exe`
    /// launcher separately from the `analysis.exe` it must start. Keeping the
    /// two paths distinct avoids copying files into the vendor installation.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[config(
        hidden,
        label = "MSC NASTRAN solver override",
        help = "Optional absolute path to MSC analysis.exe. When set, the NASTRAN launcher receives it as the a.solver keyword. Leave blank for complete installations whose launcher finds its own solver."
    )]
    pub nastran_solver_path: String,

    /// Directory containing the locally built NASA NASTRAN-95 executable and
    /// its `rf/` files.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[config(
        hidden,
        label = "Local NASTRAN-95 directory",
        help = "Directory containing build/bin/nastran.exe and rf/NASINFO for the local NASA NASTRAN-95 adapter. This is independent of MSC NASTRAN; set it under Setup > External Tools to retain a local SOL 101/SOL 103 comparison across desktop launches."
    )]
    pub nastran95_dir_path: String,

    /// Optional directory containing the GNU Fortran runtime used by the local
    /// NASTRAN-95 executable.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[config(
        hidden,
        label = "NASTRAN-95 runtime directory",
        help = "Optional directory containing libgfortran and companion runtime DLLs for local NASTRAN-95. Leave blank when the runtime is already on PATH."
    )]
    pub nastran95_runtime_path: String,

    /// Optional short absolute staging directory for legacy NASTRAN-95 rigid
    /// format files.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[config(
        hidden,
        label = "NASTRAN-95 RF staging directory",
        help = "Optional short absolute directory used to stage local NASTRAN-95 rf files. The 1970s RFOPEN loader accepts at most 37 bytes here; C:/nas-rf is suitable on Windows."
    )]
    pub nastran95_rf_stage_path: String,

    /// Optional open-core allocation for the local NASTRAN-95 solver.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[config(
        hidden,
        label = "NASTRAN-95 open-core words",
        help = "Optional NASTRAN-95 OCMEM allocation in words. Leave blank to use the full allocation compiled into nastran.exe. Current local builds record that limit beside the executable; rebuild with a larger COMMON /ZZZZZZ/ allocation to solve a larger mesh."
    )]
    pub nastran95_open_core_words: String,

    /// Whether the solver is actually invoked.
    #[config(
        hidden,
        label = "Run NASTRAN",
        help = "Actually invoke nastran_exe_path as a subprocess on the generated .bdf files. On by default: when the executable isn't configured/found, the app still writes valid .bdf files and shows the theoretical (analytical) results only, so leaving this on is always safe. Set on Setup > External Tools."
    )]
    pub run_nastran: bool,

    /// Whether the static load cases are solved.
    #[config(
        label = "Run static analysis (SOL 101)",
        help = "Pull-up / push-down / 1g-level static load cases -> deformation and stress."
    )]
    pub run_sol_static: bool,

    /// Whether the natural modes are extracted.
    #[config(
        label = "Run normal modes (SOL 103)",
        help = "Natural frequencies and mode shapes."
    )]
    pub run_sol_modes: bool,

    /// Whether the harmonic response is solved.
    #[config(
        label = "Run sine sweep (SOL 111)",
        help = "Modal frequency response to a harmonic engine-mounted excitation force."
    )]
    pub run_sol_vibration_sine: bool,

    /// Whether force-PSD random-response RMS is calculated from SOL 111.
    #[config(
        label = "Calculate random-vibration RMS from SOL 111",
        help = "Integrate the solved unit-force SOL 111 response against the one-sided force PSD below. This produces displacement RMS in metres over the configured frequency sweep; it is not a base-acceleration calculation."
    )]
    pub run_sol_vibration_random: bool,

    /// How long one solution may take.
    #[config(
        label = "NASTRAN timeout per solution",
        unit = "s",
        help = "Max time allowed for one NASTRAN solution (SOL 101/103/111 each run separately) before it's killed."
    )]
    pub timeout_s: f64,

    /// How many modes to extract.
    #[config(
        label = "Number of modes to extract",
        help = "Max structural modes for SOL 103's EIGRL and the analytical Rayleigh-quotient estimate."
    )]
    pub n_modes: i64,

    /// Top of the frequency sweep.
    #[config(
        label = "Frequency sweep upper limit",
        unit = "Hz",
        help = "Highest frequency the sine sweep excites. Above the last mode of interest there is nothing left to find, and every extra step costs a solve."
    )]
    pub freq_sweep_max_hz: f64,

    /// Frequency sweep increment.
    #[config(
        label = "Frequency sweep step",
        unit = "Hz",
        help = "Spacing between sweep frequencies. A step coarser than a mode's half-power bandwidth steps over the resonance without seeing it."
    )]
    pub freq_step_hz: f64,

    /// Structural damping assumed for the vibration response.
    #[config(
        label = "Modal damping ratio",
        unit = "fraction of critical",
        help = "Structural damping assumed for the sine/random vibration response (2% is a common metallic-airframe assumption)."
    )]
    pub modal_damping_ratio: f64,

    /// The native force-PSD input for random-vibration RMS.
    #[config(
        label = "Random excitation force PSD",
        unit = "N^2/Hz",
        help = "One-sided, flat force power spectral density at the engine excitation grid. RMS displacement is integral(|H(f)|^2 S_F df)^(1/2), where H is the solved SOL 111 unit-force receptance. Use the measured or specified PSD for the aircraft; 1 N^2/Hz is the neutral unit-input default."
    )]
    pub random_force_psd_n2_per_hz: f64,

    /// Frozen Python-only acceleration PSD input.
    #[config(
        label = "Legacy random vibration base PSD (not used)",
        unit = "g^2/Hz",
        help = "Frozen-reference acceleration PSD retained for saved-file compatibility. It is not used by the product RMS path, which requires the force PSD above."
    )]
    pub psd_base_g2_per_hz: f64,

    /// Where the post-processor lives.
    #[config(
        hidden,
        label = "Patran executable path",
        help = "Path (repo-root-relative or absolute) to patran.exe. Leave blank to skip, no Patran install is required for anything else in Structural Analysis. Set on Setup > External Tools."
    )]
    pub patran_exe_path: String,

    /// Whether deformation plots are rendered by the post-processor.
    #[config(
        hidden,
        label = "Render Patran deformation plots",
        help = "After a successful NASTRAN SOL 101 static solve, batch-replay a Patran session per load case to export a deformation-plot PNG (same view MSC Patran's own interactive GUI shows). On by default; needs a real licensed Patran install and launches it as a subprocess per load case (a few seconds each), has no effect when patran_exe_path isn't configured. Requires run_nastran and run_sol_static to both be on. Set on Setup > External Tools."
    )]
    pub run_patran_export: bool,
}

impl Default for StructuresConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            spar_chord_fractions: vec![0.25, 0.70],
            te_rib_mode: "all".to_owned(),
            center_spar_enabled: false,
            center_spar_chord_fraction: 0.50,
            skin_material: "Al 7075-T6".to_owned(),
            spar_web_material: "Al 7075-T6".to_owned(),
            spar_cap_material: "CFRP UD".to_owned(),
            rib_material: "Al 7075-T6".to_owned(),
            additional_safety_factor: 1.0,
            t_skin_min_m: 0.006,
            t_web_min_m: 0.002,
            t_rib_m: 0.004,
            t_te_strip_m: 0.002,
            cap_taper_eta_lock: 0.40,
            cap_taper_tip_fraction: 0.20,
            rib_buckling_coeff: 1.5,
            rib_radius_of_gyration_m: 0.030,
            num_ribs_override: None,
            spanwise_stations: 200,
            mesh_chordwise_points: 50,
            nastran_exe_path: String::new(),
            nastran_solver_path: String::new(),
            nastran95_dir_path: String::new(),
            nastran95_runtime_path: String::new(),
            nastran95_rf_stage_path: String::new(),
            nastran95_open_core_words: String::new(),
            run_nastran: true,
            run_sol_static: true,
            run_sol_modes: true,
            // SOL 111 is part of the default structural response set; the
            // NASTRAN adapter still honors explicit user/reference limits.
            run_sol_vibration_sine: true,
            // A unit force PSD makes SOL 111 immediately observable; a user
            // can replace it with the aircraft-specific excitation level.
            run_sol_vibration_random: true,
            timeout_s: 3600.0,
            // Sixteen modes is the smallest full-mesh NASTRAN-95 request
            // validated to retain all four active Rayleigh target matches.
            n_modes: 16,
            freq_sweep_max_hz: 60.0,
            freq_step_hz: 1.0,
            modal_damping_ratio: 0.02,
            random_force_psd_n2_per_hz: 1.0,
            psd_base_g2_per_hz: 0.01,
            patran_exe_path: String::new(),
            run_patran_export: true,
        }
    }
}

impl StructuresConfig {
    /// The spar list the wingbox is actually built from, and whether each
    /// spar runs the full span.
    ///
    /// The optional centre spar is folded in here rather than at each call
    /// site, so the pipeline, the live preview and the command line compose
    /// the same list the same way instead of each deriving it independently.
    pub fn resolved_spars(&self) -> (Vec<f64>, Vec<bool>) {
        let mut fractions = self.spar_chord_fractions.clone();
        let mut full_span = vec![true; fractions.len()];
        if self.center_spar_enabled {
            fractions.push(self.center_spar_chord_fraction);
            full_span.push(false);
        }
        (fractions, full_span)
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, Kind, OptionSource};

    #[test]
    fn the_default_wingbox_is_a_two_spar_box_running_the_full_span() {
        let (fractions, full_span) = StructuresConfig::default().resolved_spars();
        assert_eq!(fractions, vec![0.25, 0.70]);
        assert_eq!(full_span, vec![true, true]);
    }

    #[test]
    fn the_center_spar_is_appended_and_marked_as_partial_span() {
        // It stops at the kink, so anything building the box has to know not
        // to run it to the tip, which is what the second list carries.
        let config = StructuresConfig {
            center_spar_enabled: true,
            ..Default::default()
        };
        let (fractions, full_span) = config.resolved_spars();
        assert_eq!(fractions, vec![0.25, 0.70, 0.50]);
        assert_eq!(full_span, vec![true, true, false]);
    }

    #[test]
    fn every_material_field_offers_the_structural_material_list() {
        let schema = StructuresConfig::default().schema();
        for name in [
            "skin_material",
            "spar_web_material",
            "spar_cap_material",
            "rib_material",
        ] {
            match &schema.field(name).unwrap().entry {
                Entry::Leaf(leaf) => {
                    assert_eq!(leaf.options, Some(OptionSource::Material), "{name}");
                }
                Entry::Node(_) => panic!("{name} is not a group"),
            }
        }
    }

    #[test]
    fn the_default_materials_are_all_in_the_database() {
        // A default naming a material the table does not carry would fail at
        // the first sizing pass rather than here, which is much further from
        // the mistake.
        let config = StructuresConfig::default();
        for name in [
            &config.skin_material,
            &config.spar_web_material,
            &config.spar_cap_material,
            &config.rib_material,
        ] {
            assert!(crate::materials::get(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn the_spar_positions_reach_the_form_as_a_list_of_numbers() {
        let schema = StructuresConfig::default().schema();
        match &schema.field("spar_chord_fractions").unwrap().entry {
            Entry::Leaf(leaf) => assert_eq!(leaf.kind, Kind::NumberList),
            Entry::Node(_) => panic!("a spar list is not a group"),
        }
    }

    #[test]
    fn an_unset_rib_count_reports_itself_unset_rather_than_zero() {
        // Zero ribs is a wing; "let the app decide" is not. The two have to
        // stay distinguishable in the form.
        let schema = StructuresConfig::default().schema();
        match &schema.field("num_ribs_override").unwrap().entry {
            Entry::Leaf(leaf) => {
                assert_eq!(leaf.kind, Kind::Optional);
                assert_eq!(leaf.value, serde_json::Value::Null);
            }
            Entry::Node(_) => panic!("a rib count is not a group"),
        }
    }

    #[test]
    fn the_frequency_sweep_takes_more_than_one_step() {
        let config = StructuresConfig::default();
        assert!(config.freq_step_hz > 0.0);
        assert!(config.freq_step_hz < config.freq_sweep_max_hz);
    }

    #[test]
    fn the_optional_msc_solver_override_preserves_old_saved_files() {
        let default_json = serde_json::to_value(StructuresConfig::default()).unwrap();
        assert!(default_json.get("nastran_solver_path").is_none());

        let configured = StructuresConfig {
            nastran_solver_path: "C:/MSC/analysis.exe".to_owned(),
            ..StructuresConfig::default()
        };
        let configured_json = serde_json::to_value(&configured).unwrap();
        assert_eq!(
            configured_json["nastran_solver_path"],
            "C:/MSC/analysis.exe"
        );

        let restored: StructuresConfig = serde_json::from_value(default_json).unwrap();
        assert!(restored.nastran_solver_path.is_empty());
    }

    #[test]
    fn local_nastran95_paths_are_optional_machine_preferences() {
        let default_json = serde_json::to_value(StructuresConfig::default()).unwrap();
        assert!(default_json.get("nastran95_dir_path").is_none());

        let configured = StructuresConfig {
            nastran95_dir_path: "C:/nastran-95".to_owned(),
            nastran95_runtime_path: "C:/msys64/mingw64/bin".to_owned(),
            nastran95_rf_stage_path: "C:/nas-rf".to_owned(),
            nastran95_open_core_words: "32000000".to_owned(),
            ..StructuresConfig::default()
        };
        let configured_json = serde_json::to_value(configured).unwrap();
        assert_eq!(configured_json["nastran95_dir_path"], "C:/nastran-95");
        assert_eq!(configured_json["nastran95_rf_stage_path"], "C:/nas-rf");
        assert_eq!(configured_json["nastran95_open_core_words"], "32000000");
    }
}
