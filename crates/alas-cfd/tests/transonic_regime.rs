//! Regression coverage for the Mach-derived compressible airfoil path.

// This file is itself a test binary, so an expect that fails is the
// assertion failing, and the default-then-mutate config setup below is the
// normal way tests build a config that overrides a single field.
#![cfg_attr(test, allow(clippy::expect_used, clippy::field_reassign_with_default))]

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use alas_cfd::{
    generate_case, CfdSolverKind, CfdStudyConfig, ConvectionScheme, FlowRegime, MeshPreset,
    OperatingInput,
};

fn scratch_dir(label: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("alas-cfd-transonic-{label}-{stamp}"))
}

#[test]
fn mach_policy_switches_solver_and_shock_safe_controls() {
    let mut low = CfdStudyConfig::default();
    low.speed_m_s = 51.0;
    let low_effective = low.effective_simulation();
    assert_eq!(low.flow_regime(), FlowRegime::LowSubsonic);
    assert_eq!(low_effective.solver, CfdSolverKind::SimpleFoam);
    assert!(!low_effective.compressible);

    let mut transonic = CfdStudyConfig::default();
    transonic.speed_m_s = 300.0;
    let mach = transonic.mach_number();
    assert!((0.8..1.2).contains(&mach), "Mach = {mach}");
    let effective = transonic.effective_simulation();
    assert_eq!(effective.regime, FlowRegime::Transonic);
    assert_eq!(effective.solver, CfdSolverKind::RhoSimpleFoam);
    assert!(effective.compressible);
    assert_eq!(effective.max_iterations, 3_000);
    assert_eq!(effective.startup_iterations, 150);
    assert_eq!(
        effective.convection_scheme,
        ConvectionScheme::BoundedLinearUpwind
    );
    assert_eq!(effective.gradient_limiter, 1.0);
    assert_eq!(effective.pressure_relaxation, 0.3);
    assert_eq!(effective.equation_relaxation, 0.3);

    // Reynolds mode must feed exactly the same effective speed/Mach policy.
    let mut from_reynolds = CfdStudyConfig::default();
    from_reynolds.operating_input = OperatingInput::Reynolds;
    from_reynolds.reynolds = from_reynolds.density_kg_m3 * 300.0 * from_reynolds.chord_m
        / from_reynolds.dynamic_viscosity_pa_s;
    assert!((from_reynolds.effective_speed_m_s() - 300.0).abs() < 1.0e-12);
    assert_eq!(from_reynolds.solver_kind(), CfdSolverKind::RhoSimpleFoam);
    assert_eq!(from_reynolds.flow_regime(), FlowRegime::Transonic);
}

#[test]
fn generated_transonic_case_contains_a_real_perfect_gas_contract() {
    let mut config = CfdStudyConfig::default();
    config.speed_m_s = 300.0;
    config.mesh.preset = MeshPreset::Coarse;
    let case_dir = scratch_dir("case");
    let generated = generate_case(&config, &case_dir).expect("transonic case generation");
    assert_eq!(generated.simulation.solver, CfdSolverKind::RhoSimpleFoam);
    assert!(generated.simulation.compressible);

    let control = fs::read_to_string(case_dir.join("system/controlDict")).expect("controlDict");
    assert!(control.contains("application rhoSimpleFoam;"), "{control}");
    assert!(control.contains("endTime 3000;"), "{control}");

    let thermo = fs::read_to_string(case_dir.join("constant/thermophysicalProperties"))
        .expect("thermophysicalProperties");
    assert!(thermo.contains("type hePsiThermo;"), "{thermo}");
    assert!(thermo.contains("equationOfState perfectGas;"), "{thermo}");
    assert!(
        thermo.contains("energy sensibleInternalEnergy;"),
        "{thermo}"
    );
    assert!(!case_dir.join("constant/transportProperties").exists());

    let schemes = fs::read_to_string(case_dir.join("system/fvSchemes")).expect("fvSchemes");
    // Case generation emits the bounded-upwind startup dictionary.  The
    // runner rewrites this same file to bounded linear-upwind after the
    // automatic 150-iteration warm-up, so the initial state is shock-safe as
    // well as the final policy.
    assert!(schemes.contains("bounded Gauss upwind"), "{schemes}");
    assert!(schemes.contains("div(phi,e)"), "{schemes}");
    assert!(schemes.contains("div(phid,p)"), "{schemes}");

    let solution = fs::read_to_string(case_dir.join("system/fvSolution")).expect("fvSolution");
    assert!(solution.contains("\"rho.*\""), "{solution}");
    assert!(solution.contains("\"(U|e).*\""), "{solution}");
    assert!(solution.contains("e 0.300;"), "{solution}");

    for field in ["U", "p", "T", "alphat", "k", "omega", "nut"] {
        assert!(
            case_dir.join(format!("0/{field}")).exists(),
            "missing {field}"
        );
    }
    let pressure = fs::read_to_string(case_dir.join("0/p")).expect("p");
    assert!(
        pressure.contains("dimensions [1 -1 -2 0 0 0 0];"),
        "{pressure}"
    );
    let study = fs::read_to_string(case_dir.join("study.json")).expect("study provenance");
    assert!(study.contains("\"max_iterations\": 3000"), "{study}");
    assert!(
        study.contains("\"convection_scheme\": \"bounded_linear_upwind\""),
        "{study}"
    );
    assert!(
        study.contains("\"momentum_linear_solver\": \"pbicgstab\""),
        "{study}"
    );

    let _ = fs::remove_dir_all(case_dir);
}
