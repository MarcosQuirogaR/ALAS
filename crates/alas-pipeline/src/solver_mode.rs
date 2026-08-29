// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! User-selectable aerodynamic solver roles.
//!
//! The in-process VLM remains the fast design model.  Native AVL is an
//! independent lifting-surface calculation that can validate a finished
//! design or guide a separate, explicitly requested optimization.  Keeping
//! these choices as data prevents an unavailable external executable from
//! silently changing the meaning of an ordinary VLM run.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// One concrete aerodynamic backend used by an optimization result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SolverKind {
    /// Native ALAS vortex-lattice analysis.
    Vlm,
    /// External Athena Vortex Lattice analysis.
    Avl,
}

impl SolverKind {
    /// Stable spelling for manifests and result selectors.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vlm => "vlm",
            Self::Avl => "avl",
        }
    }
}

/// Which aerodynamic result family a run should expose.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AerodynamicSolverMode {
    /// Use the in-process ALAS VLM result as the displayed aerodynamic model.
    Vlm,
    /// Use the native AVL result when it is available and comparable.
    Avl,
    /// Retain both models and expose their comparison.
    #[default]
    Both,
}

impl AerodynamicSolverMode {
    /// Stable command-line and manifest spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vlm => "vlm",
            Self::Avl => "avl",
            Self::Both => "both",
        }
    }
}

impl FromStr for AerodynamicSolverMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "vlm" => Ok(Self::Vlm),
            "avl" => Ok(Self::Avl),
            "both" | "compare" | "comparison" => Ok(Self::Both),
            other => Err(format!(
                "unknown aerodynamic solver '{other}'; expected vlm, avl, or both"
            )),
        }
    }
}

/// Which solver is allowed to determine the optimized design.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationSolverMode {
    /// Existing fast optimizer driven by the in-process VLM objective.
    #[default]
    Vlm,
    /// AVL-guided optimization, requiring a configured native executable.
    Avl,
    /// Run both optimization strategies and retain both candidate designs.
    Both,
}

impl OptimizationSolverMode {
    /// Stable command-line and manifest spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vlm => "vlm",
            Self::Avl => "avl",
            Self::Both => "both",
        }
    }
}

impl FromStr for OptimizationSolverMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "vlm" => Ok(Self::Vlm),
            "avl" => Ok(Self::Avl),
            "both" | "compare" | "comparison" => Ok(Self::Both),
            other => Err(format!(
                "unknown optimization solver '{other}'; expected vlm, avl, or both"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solver_modes_have_stable_user_spellings() {
        assert_eq!(AerodynamicSolverMode::Both.as_str(), "both");
        assert_eq!(OptimizationSolverMode::Avl.as_str(), "avl");
        assert_eq!("comparison".parse(), Ok(AerodynamicSolverMode::Both));
    }

    #[test]
    fn invalid_solver_mode_names_explain_the_allowed_values() {
        let error = match "hybrid".parse::<OptimizationSolverMode>() {
            Ok(mode) => panic!("unexpected solver mode: {mode:?}"),
            Err(error) => error,
        };
        assert!(error.contains("vlm, avl, or both"));
    }
}
