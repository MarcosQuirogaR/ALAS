// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The physics stages a partial run (`alas --only`) can ask for.

/// One physics stage of a partial run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PartialStage {
    /// The built aircraft geometry.
    Geometry,
    /// The mass breakdown and centre of gravity.
    Mass,
    /// The polar, design point and trimmed cruise point.
    Aero,
    /// Neutral point, static margin and CG envelope verdict.
    Stability,
    /// Field performance on an ISA sea-level runway.
    Field,
    /// The native mission on the planned route.
    Mission,
    /// Structural sizing (the configured solvers).
    Structures,
    /// The physical feasibility gate.
    Feasibility,
}

impl PartialStage {
    const ALL: [Self; 8] = [
        Self::Geometry,
        Self::Mass,
        Self::Aero,
        Self::Stability,
        Self::Field,
        Self::Mission,
        Self::Structures,
        Self::Feasibility,
    ];

    /// The stage's command-line name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Geometry => "geometry",
            Self::Mass => "mass",
            Self::Aero => "aero",
            Self::Stability => "stability",
            Self::Field => "field",
            Self::Mission => "mission",
            Self::Structures => "structures",
            Self::Feasibility => "feasibility",
        }
    }

    /// Parse a comma-separated list (`gate` is `feasibility`, `all` every stage).
    ///
    /// # Errors
    ///
    /// An unknown or empty stage name.
    pub fn parse_list(text: &str) -> Result<Vec<Self>, String> {
        let mut stages = Vec::new();
        for word in text.split(',').map(str::trim).filter(|w| !w.is_empty()) {
            let word = word.to_ascii_lowercase();
            if word == "all" {
                stages.extend(Self::ALL);
                continue;
            }
            let stage = match word.as_str() {
                "gate" => Self::Feasibility,
                "structure" | "structural" => Self::Structures,
                other => Self::ALL
                    .into_iter()
                    .find(|stage| stage.name() == other)
                    .ok_or_else(|| {
                        format!(
                            "unknown stage `{other}` for --only; use geometry, mass, aero, stability, field, mission, structures, feasibility (gate) or all"
                        )
                    })?,
            };
            stages.push(stage);
        }
        stages.sort();
        stages.dedup();
        if stages.is_empty() {
            return Err("--only needs at least one stage".to_owned());
        }
        Ok(stages)
    }

    pub(super) fn needs_report(self) -> bool {
        !matches!(self, Self::Geometry | Self::Mass)
    }
}

// A failed unwrap here is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::PartialStage;

    #[test]
    fn stage_lists_parse_aliases_sort_and_deduplicate() {
        assert_eq!(
            PartialStage::parse_list("gate, mission,Mass,mission").unwrap(),
            vec![
                PartialStage::Mass,
                PartialStage::Mission,
                PartialStage::Feasibility
            ]
        );
        assert_eq!(PartialStage::parse_list("all").unwrap().len(), 8);
        assert_eq!(
            PartialStage::parse_list("structural").unwrap(),
            vec![PartialStage::Structures]
        );
        assert!(PartialStage::parse_list("").is_err());
        assert!(PartialStage::parse_list("mission,wings").is_err());
    }

    #[test]
    fn only_geometry_and_mass_skip_the_report() {
        assert!(!PartialStage::Geometry.needs_report());
        assert!(!PartialStage::Mass.needs_report());
        assert!(PartialStage::Mission.needs_report());
        assert!(PartialStage::Feasibility.needs_report());
    }
}
