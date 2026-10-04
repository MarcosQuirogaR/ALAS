// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Names and formulae of the mission quantities recorded by the search.

use alas_config::ObjectiveKind;

/// Select a single physical quantity only when every plotted row records it.
pub fn history_objective_kind(
    history: &alas_opt::OptimizationHistory,
    kind: ObjectiveKind,
) -> Option<ObjectiveKind> {
    let valid: Vec<usize> = history
        .valid
        .iter()
        .enumerate()
        .filter_map(|(index, valid)| valid.then_some(index))
        .collect();
    (!valid.is_empty()
        && valid.iter().all(|&index| {
            history
                .objective_value
                .get(index)
                .is_some_and(|value| value.is_finite())
        }))
    .then_some(kind)
}

/// Axis and value label, including the recorded physical unit.
pub const fn objective_label(kind: ObjectiveKind) -> &'static str {
    match kind {
        ObjectiveKind::BlockFuel => "Block fuel [kg]",
        ObjectiveKind::TakeoffMass => "Takeoff mass [kg]",
        ObjectiveKind::OperatingEmptyMass => "Operating empty mass [kg]",
        ObjectiveKind::FuelPerSeatKilometre => "Fuel per seat-kilometre [kg/(seat km)]",
    }
}

/// Hover definition of the physical objective before ranking normalization.
pub const fn objective_help(kind: ObjectiveKind) -> &'static str {
    match kind {
        ObjectiveKind::BlockFuel => "Q = m_taxi + m_trip [kg]. Taxi and trip fuel for the sizing mission; reserves are excluded from block fuel.",
        ObjectiveKind::TakeoffMass => "Q = m_takeoff [kg]. Takeoff mass closed by the sizing mission, including operating empty mass, payload and required takeoff fuel.",
        ObjectiveKind::OperatingEmptyMass => "Q = m_OEW [kg]. Operating empty mass evaluated at the closed takeoff mass.",
        ObjectiveKind::FuelPerSeatKilometre => "Q = m_block / (max(N, 1) * max(R, 1e-9)) [kg/(seat km)]. m_block is taxi plus trip fuel [kg], N is carried passengers [seat], and R is sizing-mission range [km].",
    }
}

/// The scalar ranking cost has no physical unit.
pub const RANKING_LABEL: &str = "Normalized ranking cost [dimensionless]";

/// Definition of the native scalar ranking cost and its reference scales.
pub const RANKING_HELP: &str = "J = Q/Qref + w*P [dimensionless]. P is the sum of normalized preference violations and w the dimensionless preference weight; an infeasible candidate adds 1 + H, where H is the sum of normalized hard-constraint violations. Qref = 0.3*M for block fuel, M for takeoff or empty mass, or 0.001 kg/(seat km) for fuel per seat-kilometre. M [kg] is the MTOW plan reference mass. Feasibility is ranked before J. External evaluators supply their own ranking cost.";

/// Physical objective label with its comparison role for summary tables.
pub const fn comparison_label(kind: ObjectiveKind, role: usize) -> &'static str {
    match (kind, role) {
        (ObjectiveKind::BlockFuel, 0) => "Block fuel [kg], preset",
        (ObjectiveKind::BlockFuel, 1) => "Block fuel [kg], constrained start",
        (ObjectiveKind::BlockFuel, _) => "Block fuel [kg], result",
        (ObjectiveKind::TakeoffMass, 0) => "Takeoff mass [kg], preset",
        (ObjectiveKind::TakeoffMass, 1) => "Takeoff mass [kg], constrained start",
        (ObjectiveKind::TakeoffMass, _) => "Takeoff mass [kg], result",
        (ObjectiveKind::OperatingEmptyMass, 0) => "Operating empty mass [kg], preset",
        (ObjectiveKind::OperatingEmptyMass, 1) => "Operating empty mass [kg], constrained start",
        (ObjectiveKind::OperatingEmptyMass, _) => "Operating empty mass [kg], result",
        (ObjectiveKind::FuelPerSeatKilometre, 0) => {
            "Fuel per seat-kilometre [kg/(seat km)], preset"
        }
        (ObjectiveKind::FuelPerSeatKilometre, 1) => {
            "Fuel per seat-kilometre [kg/(seat km)], constrained start"
        }
        (ObjectiveKind::FuelPerSeatKilometre, _) => {
            "Fuel per seat-kilometre [kg/(seat km)], result"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_history_never_combines_mass_and_dimensionless_values() {
        let mut history = alas_opt::OptimizationHistory {
            valid: vec![true, true],
            objective_value: vec![100.0, f64::NAN],
            ..Default::default()
        };
        assert_eq!(
            history_objective_kind(&history, ObjectiveKind::BlockFuel),
            None
        );
        history.valid[1] = false;
        assert_eq!(
            history_objective_kind(&history, ObjectiveKind::BlockFuel),
            Some(ObjectiveKind::BlockFuel)
        );
        history.valid[0] = false;
        assert_eq!(
            history_objective_kind(&history, ObjectiveKind::BlockFuel),
            None
        );
    }
}
