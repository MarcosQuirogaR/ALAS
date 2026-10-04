// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Design-loading CG evidence, independent of the route's dispatched fuel.

use alas_opt::envelope::assess_model_cg_envelope_with_ledger;

use super::*;

pub(in crate::feasibility) fn design_model_cg_assessment(
    config: &AlasConfig,
    report: &AnalysisReport,
    fuel_loading: &FuelLoadingAssessment,
    mass_balance: Option<&MassBalanceAssessment>,
    flown: Option<&ModelCgEnvelopeAssessment>,
) -> Result<Option<ModelCgEnvelopeAssessment>, String> {
    let Some(loading) = fuel_loading.design_takeoff_loading else {
        return Ok(flown.cloned());
    };
    let balance = mass_balance
        .ok_or_else(|| "design CG assessment requires the item-level mass statement".to_owned())?;
    let basis = ledger_loading_basis(balance, "maximum fuel takeoff").ok_or_else(|| {
        "design CG assessment is missing the maximum-fuel loading state".to_owned()
    })?;
    validate_design_basis(
        basis,
        loading,
        balance.ledger_items.len() + balance.tanks.len(),
    )?;
    let critical_neutral_point = report
        .neutral_point_conditions
        .as_ref()
        .map_or(report.x_neutral_point, |conditions| conditions.critical);
    assess_model_cg_envelope_with_ledger(
        &report.airplane,
        basis,
        report.x_neutral_point,
        critical_neutral_point,
        report.airplane.c_ref,
        config,
    )
    .map(Some)
    .map_err(|error| error.to_string())
}

fn validate_design_basis(
    basis: LedgerLoadingBasis,
    loading: alas_mass::loading::MtowFuelLoading,
    summed_items: usize,
) -> Result<(), String> {
    if ![
        basis.oew_mass_kg,
        basis.oew_cg_x_m,
        basis.oew_cg_z_m,
        basis.zero_fuel_mass_kg,
        basis.zero_fuel_cg_x_m,
        basis.zero_fuel_cg_z_m,
        basis.takeoff_mass_kg,
        basis.takeoff_cg_x_m,
        basis.takeoff_cg_z_m,
        loading.zero_fuel_mass_kg,
        loading.takeoff_mass_kg,
    ]
    .iter()
    .all(|value| value.is_finite())
    {
        return Err("design CG loading basis contains a nonfinite mass or station".to_owned());
    }
    // Each ledger item and tank contributes to the two mass sums. This is
    // their floating-point summation error bound, not a physical margin.
    let scale = loading.takeoff_mass_kg.abs().max(1.0);
    let roundoff = f64::EPSILON * (summed_items + 2) as f64 * scale;
    if (basis.zero_fuel_mass_kg - loading.zero_fuel_mass_kg).abs() > roundoff
        || (basis.takeoff_mass_kg - loading.takeoff_mass_kg).abs() > roundoff
    {
        return Err(format!(
            "design CG ledger loading differs from capped loading: ZFW {:.6}/{:.6} kg, takeoff {:.6}/{:.6} kg",
            basis.zero_fuel_mass_kg, loading.zero_fuel_mass_kg,
            basis.takeoff_mass_kg, loading.takeoff_mass_kg,
        ));
    }
    Ok(())
}

// Tests construct every fixture they assert on, so a failed unwrap or
// expect is the assertion failing rather than a library invariant breaking.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_loading_identity_rejects_a_dispatch_mass() {
        let loading = alas_mass::loading::MtowFuelLoading::resolve(100.0, 60.0, Some(30.0))
            .expect("physical loading");
        let mut basis = LedgerLoadingBasis {
            oew_mass_kg: 50.0,
            oew_cg_x_m: 5.0,
            oew_cg_z_m: 1.0,
            zero_fuel_mass_kg: 60.0,
            zero_fuel_cg_x_m: 6.0,
            zero_fuel_cg_z_m: 1.0,
            takeoff_mass_kg: 90.0,
            takeoff_cg_x_m: 7.0,
            takeoff_cg_z_m: 1.0,
            takeoff_pitch_inertia_kg_m2: f64::NAN,
        };
        assert!(validate_design_basis(basis, loading, 20).is_ok());
        basis.takeoff_mass_kg = 75.0;
        assert!(validate_design_basis(basis, loading, 20).is_err());
        basis.takeoff_mass_kg = f64::NAN;
        assert!(validate_design_basis(basis, loading, 20).is_err());
    }
}
