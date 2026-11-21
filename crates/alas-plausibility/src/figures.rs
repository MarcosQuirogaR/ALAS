// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Consistency rules between a figure and the physics it claims to draw: the
//! span-loading lift against the weight it carries, and the propulsion map's
//! cruise thrust lapse.

use super::{Findings, RuleInfo, Severity};

pub(super) const F_SPAN_LIFT: RuleInfo = RuleInfo {
    id: "FIG-SPAN-LIFT",
    statement: "The span-loading solve at the trimmed cruise condition carries the weight: total lift / weight 0.97-1.03 usual; MEDIUM outside 0.93-1.07; HIGH outside 0.85-1.15",
    source: "Level flight L = n W with n = 1; the trim solves CL = W / (q S) for the loaded takeoff mass, so the lattice lift at the trimmed condition must reproduce the weight (a conservation identity, no handbook band)",
};
pub(super) const P_LAPSE: RuleInfo = RuleInfo {
    id: "PROP-LAPSE",
    statement: "Maximum-climb thrust at the cruise altitude and Mach over sea-level static take-off thrust 0.15-0.30 usual; MEDIUM outside 0.12-0.35; HIGH outside 0.08-0.45",
    source: "High-bypass transport cruise thrust is 20-25 % of take-off thrust (Raymer 6th ed. Ch. 5 p.122); thrust lapse parameter T/(delta T_TO) (Torenbeek 2013 Eq. 3.23 p.66, Eq. 8.26 p.244); a lapse above 0.45 means the mass flow was held at its sea-level value",
};

fn banded(value: f64, usual: (f64, f64), medium: (f64, f64), high: (f64, f64)) -> Option<Severity> {
    if !value.is_finite() || value < high.0 || value > high.1 {
        Some(Severity::High)
    } else if value < medium.0 || value > medium.1 {
        Some(Severity::Medium)
    } else if value < usual.0 || value > usual.1 {
        Some(Severity::Low)
    } else {
        None
    }
}

/// Lift the span-loading figure integrates, and the weight it must carry, N.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpanLiftFacts {
    /// Main-wing lift.
    pub wing_lift_n: f64,
    /// Lift of every lifting surface.
    pub total_lift_n: f64,
    /// Aircraft weight at the loaded takeoff mass the trim solves for.
    pub weight_n: f64,
}

/// Judge the span-loading lift balance.
pub fn check_span_lift(out: &mut Findings, facts: &SpanLiftFacts) {
    let total = facts.total_lift_n / facts.weight_n;
    if let Some(severity) = banded(total, (0.97, 1.03), (0.93, 1.07), (0.85, 1.15)) {
        out.add(
            F_SPAN_LIFT.id,
            severity,
            format!(
                "total lift {:.0} kN is {:.3} of the weight {:.0} kN",
                facts.total_lift_n / 1e3,
                total,
                facts.weight_n / 1e3
            ),
        );
    }
}

/// Judge the cruise thrust lapse of the propulsion deck.
pub fn check_thrust_lapse(out: &mut Findings, lapse: f64) {
    if let Some(severity) = banded(lapse, (0.15, 0.30), (0.12, 0.35), (0.08, 0.45)) {
        out.add(
            P_LAPSE.id,
            severity,
            format!("cruise thrust is {lapse:.3} of sea-level static take-off thrust"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn balance(total_over_weight: f64, wing_over_total: f64) -> Findings {
        let mut out = Findings::new("synthetic");
        check_span_lift(
            &mut out,
            &SpanLiftFacts {
                wing_lift_n: 1.0e6 * total_over_weight * wing_over_total,
                total_lift_n: 1.0e6 * total_over_weight,
                weight_n: 1.0e6,
            },
        );
        out
    }

    #[test]
    fn a_lift_that_carries_the_weight_passes() {
        assert!(balance(0.97, 1.02).items().is_empty());
        assert!(balance(1.00, 1.06).items().is_empty());
    }

    #[test]
    fn the_old_sea_level_condition_is_a_high_finding() {
        // The figure used to integrate 15-41 % of the weight.
        assert_eq!(
            balance(0.30, 1.0).severity_of(F_SPAN_LIFT.id),
            Some(Severity::High)
        );
        assert_eq!(
            balance(1.2, 1.0).severity_of(F_SPAN_LIFT.id),
            Some(Severity::High)
        );
    }

    #[test]
    fn the_constant_mass_flow_lapse_is_a_high_finding() {
        let mut out = Findings::new("synthetic");
        check_thrust_lapse(&mut out, 0.22);
        assert!(out.items().is_empty());
        check_thrust_lapse(&mut out, 0.60);
        assert_eq!(out.severity_of(P_LAPSE.id), Some(Severity::High));
    }
}
