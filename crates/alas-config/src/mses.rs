// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/mses_config.py
// Reference: alas @ rust-port-baseline.

//! Settings for the MSES two-dimensional section analysis.
//!
//! MSES (Mark Drela, MIT) solves the coupled viscous/inviscid Euler and
//! integral boundary-layer equations for a two-dimensional section. That is a
//! different model from either of the other two this program runs: the
//! vortex-lattice method is three-dimensional and inviscid with correlated
//! parasite and wave drag, and the mission analysis is trajectory-integrated.
//! Running MSES on the optimized design's root section is what lets the model
//! comparison show what a real coupled viscous-compressible solve sees that
//! the other two cannot: transition location, separation, shock-induced
//! drag.
//!
//! MSES is licensed separately by MIT and its executables are not distributed
//! with this program. ALAS does ship the compatible GPL XFOIL
//! Orr-Sommerfeld map used by free-transition cases; without the MSES
//! executables the comparison omits the MSES column rather than failing.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// MSES two-dimensional section-analysis settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct MsesConfig {
    /// Whether a normal run includes an MSES polar sweep.
    #[config(
        hidden,
        label = "Enabled",
        help = "Run an MSES 2-D polar sweep on the optimized design's root airfoil section as part of a normal Run, populating the Model Comparison tab. On by default. Set on Setup > External Tools."
    )]
    pub enabled: bool,

    /// Where the MSES executables live.
    #[config(
        hidden,
        label = "MSES executables directory",
        help = "Path (repo-root-relative or absolute) to the folder containing mset.exe/mses.exe/mplot.exe. MSES is licensed separately by MIT and its executables are not distributed with ALAS: obtain them yourself and point this at your own install. The compatible GPL osmapDP.dat transition map is bundled separately and selected automatically. Without the executables the Model Comparison tab simply omits the MSES column. Set on Setup > External Tools."
    )]
    pub mses_dir: String,

    /// Optional double-precision Orr-Sommerfeld database used by MSES when
    /// transition is left free.  The path is intentionally hidden from the
    /// ordinary setup form: it is an advanced, installation-specific resource
    /// and is resolved relative to `mses_dir` when it is not absolute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        label = "MSES Orr-Sommerfeld database",
        help = "Optional path to the double-precision osmapDP.dat resource used by MSES free-transition calculations. If empty, ALAS checks beside the MSES executables and then the release-bundled assets/mses resource. The selected path and compatibility check are recorded in the run manifest; a single-precision osmap.dat is rejected."
    )]
    pub osmap_path: Option<String>,

    /// How long one mesh-generation call may take.
    #[config(
        label = "MSET timeout",
        unit = "s",
        help = "Max time allowed for one mesh-generation (mset) call before it's killed."
    )]
    pub timeout_mset_s: f64,

    /// How long one flow solve may take.
    #[config(
        label = "MSES timeout",
        unit = "s",
        help = "Max time allowed for one flow-solve (mses) call before it's killed."
    )]
    pub timeout_mses_s: f64,

    /// Newton iteration cap per angle of attack.
    #[config(
        label = "Max solver iterations",
        help = "Newton iteration cap per angle of attack: MSES reports non-convergence rather than looping forever, but a hard cap keeps a single stubborn point from stalling the whole sweep."
    )]
    pub max_iterations: i64,

    /// Critical amplification factor for e^N transition prediction.
    #[config(
        label = "Transition N-crit",
        help = "e^N transition-prediction critical amplification factor. 9.0 is the standard sea-level-cruise default (Drela); lower values (e.g. 4-5) predict earlier transition, appropriate for a high-turbulence/rough-surface environment."
    )]
    pub n_crit: f64,

    /// Forced upper-surface transition station.
    #[config(
        label = "Forced transition x/c, upper surface",
        help = "Force transition at this upper-surface x/c instead of letting MSES predict it. 1.0 = free (natural) transition, the realistic default for a clean cruise wing."
    )]
    pub xtr_upper: f64,

    /// Forced lower-surface transition station.
    #[config(
        label = "Forced transition x/c, lower surface",
        help = "Force transition at this lower-surface x/c. 1.0 = free (natural) transition."
    )]
    pub xtr_lower: f64,

    /// How far either side of the trimmed point the polar sweeps.
    #[config(
        label = "Alpha sweep half-width around the trimmed design point",
        unit = "deg",
        help = "The MSES polar sweeps [trim_alpha - this, trim_alpha + this] so the comparison brackets the actual cruise operating point, not an arbitrary fixed range."
    )]
    pub alpha_sweep_halfwidth_deg: f64,

    /// How many points the polar sweep has.
    #[config(
        label = "Alpha sweep point count",
        help = "Number of alpha points in the MSES polar sweep. Kept small relative to the native VLM sweep (analysis.sweep_n_points) since each MSES point is a real viscous-compressible solve (~1-2s) rather than a linear-algebra VLM solve."
    )]
    pub alpha_sweep_n_points: i64,

    /// Surface panel nodes in the generated mesh.
    #[config(
        label = "MSET panel count (n)",
        help = "Number of surface panel nodes MSET generates for the airfoil mesh."
    )]
    pub mset_n: i64,

    /// Streamwise grid stretching.
    #[config(
        label = "MSET grid density exponent (e)",
        help = "Streamwise grid stretching parameter for the MSET mesh: larger values cluster more points near the airfoil."
    )]
    pub mset_e: f64,

    /// Artificial-dissipation coefficient used by MSES.
    ///
    /// MSES's user guide describes `1.0` as the normal second-order
    /// dissipation setting. A negative value disables second-order
    /// dissipation and is retained only when replaying a frozen legacy
    /// parity fixture; it is not a general product default.
    #[serde(default = "default_mucon", skip_serializing_if = "is_default_mucon")]
    #[config(
        hidden,
        help = "MSES artificial-dissipation coefficient. 1.0 is its normal second-order setting; a negative value disables second-order dissipation and is reserved for legacy parity replay."
    )]
    pub mucon: f64,
}

const DEFAULT_MUCON: f64 = 1.0;

fn default_mucon() -> f64 {
    DEFAULT_MUCON
}

fn is_default_mucon(value: &f64) -> bool {
    *value == DEFAULT_MUCON
}

impl Default for MsesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mses_dir: "external tools/MSES".to_owned(),
            osmap_path: None,
            timeout_mset_s: 30.0,
            timeout_mses_s: 60.0,
            max_iterations: 100,
            n_crit: 9.0,
            xtr_upper: 1.0,
            xtr_lower: 1.0,
            alpha_sweep_halfwidth_deg: 3.0,
            alpha_sweep_n_points: 7,
            mset_n: 141,
            mset_e: 0.4,
            mucon: DEFAULT_MUCON,
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hidden_field_is_configuration_but_not_part_of_the_form() {
        // The install directory is set on the external-tools page rather than
        // in the settings form, so it must still serialize and still not
        // appear in the schema.
        let config = MsesConfig::default();
        let schema = config.schema();
        assert!(schema.field("mses_dir").is_none());
        assert!(schema.field("enabled").is_none());

        let saved = serde_json::to_value(&config).unwrap();
        assert_eq!(saved["mses_dir"], "external tools/MSES");
        assert_eq!(saved["enabled"], true);
    }

    #[test]
    fn a_shown_field_reaches_the_form_with_its_unit() {
        let schema = MsesConfig::default().schema();
        assert_eq!(schema.field("timeout_mset_s").unwrap().unit, "s");
    }

    #[test]
    fn normal_dissipation_is_the_default_and_old_saved_configs_load_it() {
        let config = MsesConfig::default();
        assert_eq!(config.mucon, DEFAULT_MUCON);

        let mut saved = serde_json::to_value(&config).unwrap();
        assert!(saved.get("mucon").is_none());
        saved.as_object_mut().unwrap().remove("mucon");
        let restored: MsesConfig = serde_json::from_value(saved).unwrap();
        assert_eq!(restored.mucon, DEFAULT_MUCON);
    }
}
