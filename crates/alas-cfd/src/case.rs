// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Generated case metadata and geometry preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedCase {
    /// Case directory.
    pub path: PathBuf,
    /// Exact selected geometry snapshot.
    pub airfoil: AirfoilSnapshot,
    /// Effective speed and Reynolds number recorded for reproducibility.
    pub effective_speed_m_s: f64,
    /// Effective Reynolds number.
    pub effective_reynolds: f64,
    /// Mach-derived solver and numerical controls used for this case.
    pub simulation: EffectiveSimulationSettings,
    /// Number of background cells requested.
    pub background_cells: (u32, u32, u32),
    /// Files written by the builder.
    pub files: Vec<PathBuf>,
}

/// Build a fresh isolated OpenFOAM case with no external process.
pub fn generate_case(config: &CfdStudyConfig, case_dir: &Path) -> Result<GeneratedCase, String> {
    config.validate().map_err(|errors| errors.join(" "))?;
    let simulation = config.effective_simulation();
    let airfoil = resolve_airfoil(&config.airfoil_name)?;
    let mesh::MeshPreflightBundle { manifest, geo } =
        mesh::run_mesh_preflight(config, &airfoil).map_err(|error| error.to_string())?;
    let Some(gmsh) = geo.filter(|_| manifest.is_ready()) else {
        return Err(manifest.blocking_summary());
    };
    if case_dir.exists() {
        let mut entries = fs::read_dir(case_dir)
            .map_err(|error| format!("cannot inspect existing case directory: {error}"))?;
        if entries.next().is_some() {
            return Err(format!(
                "case directory {} is not empty; choose a fresh run directory to prevent stale mesh, fields or result files from being reused",
                case_dir.display()
            ));
        }
    }
    fs::create_dir_all(case_dir)
        .map_err(|error| format!("cannot create case directory: {error}"))?;
    let mut files = Vec::new();
    let directories = [
        case_dir.join("0"),
        case_dir.join("constant"),
        case_dir.join("constant/triSurface"),
        case_dir.join("system"),
    ];
    for directory in directories {
        fs::create_dir_all(&directory)
            .map_err(|error| format!("cannot create {}: {error}", directory.display()))?;
    }
    write_case_file(
        case_dir,
        "constant/airfoil.csv",
        csv_coordinates(&airfoil.coordinates),
        &mut files,
    )?;
    write_case_file(case_dir, "system/airfoil.geo", gmsh.source, &mut files)?;
    let mesh_report = serde_json::to_string_pretty(&gmsh.report)
        .map_err(|error| format!("cannot encode mesh report: {error}"))?;
    write_case_file(case_dir, "system/mesh-report.json", mesh_report, &mut files)?;
    let manifest_json = manifest
        .to_json_pretty()
        .map_err(|error| error.to_string())?;
    write_case_file(
        case_dir,
        mesh::MESH_MANIFEST_FILE,
        manifest_json,
        &mut files,
    )?;
    write_case_file(
        case_dir,
        "system/fvSchemes",
        fv_schemes(config, true),
        &mut files,
    )?;
    write_case_file(
        case_dir,
        "system/fvSolution",
        fv_solution(config),
        &mut files,
    )?;
    write_case_file(
        case_dir,
        "system/controlDict",
        control_dict(config),
        &mut files,
    )?;
    write_case_file(case_dir, "system/sampleDict", sample_dict(), &mut files)?;
    write_case_file(
        case_dir,
        "system/meshQualityDict",
        mesh_quality_dict(&mesh::MeshQualityThresholds::template_defaults()),
        &mut files,
    )?;
    if simulation.compressible {
        write_case_file(
            case_dir,
            "constant/thermophysicalProperties",
            thermophysical_properties(config),
            &mut files,
        )?;
    } else {
        write_case_file(
            case_dir,
            "constant/transportProperties",
            transport_properties(config),
            &mut files,
        )?;
    }
    write_case_file(
        case_dir,
        "constant/turbulenceProperties",
        turbulence_properties(),
        &mut files,
    )?;
    write_case_file(case_dir, "0/U", initial_u(config), &mut files)?;
    write_case_file(case_dir, "0/p", initial_p(config), &mut files)?;
    write_case_file(case_dir, "0/k", initial_k(config), &mut files)?;
    write_case_file(case_dir, "0/omega", initial_omega(config), &mut files)?;
    write_case_file(case_dir, "0/nut", initial_nut(config), &mut files)?;
    if simulation.compressible {
        write_case_file(case_dir, "0/T", initial_t(config), &mut files)?;
        write_case_file(case_dir, "0/alphat", initial_alphat(), &mut files)?;
    }
    // Provenance records the config as RESOLVED, not as typed.  A control whose
    // default depends on the mesh preset would otherwise be written as `null`
    // here while the dictionaries carry a concrete number, and re-running from
    // this file on a different preset would silently emit a different case.
    let resolved = config.with_effective_simulation();
    let snapshot = StudyProvenance {
        template_version: TEMPLATE_VERSION.to_owned(),
        config: resolved,
        airfoil: airfoil.clone(),
        effective_speed_m_s: config.effective_speed_m_s(),
        effective_reynolds: config.effective_reynolds(),
        frame: FrameConvention::default(),
        reference: Some(ReferenceConventions::from_config(config)),
        backend: None,
        openfoam_version: None,
        file_hashes: BTreeMap::new(),
    };
    let provenance = serde_json::to_string_pretty(&snapshot)
        .map_err(|error| format!("cannot encode case provenance: {error}"))?;
    write_case_file(case_dir, "study.json", provenance, &mut files)?;
    let readme = case_readme(config, &airfoil);
    write_case_file(case_dir, "README.md", readme, &mut files)?;
    Ok(GeneratedCase {
        path: case_dir.to_path_buf(),
        airfoil,
        effective_speed_m_s: config.effective_speed_m_s(),
        effective_reynolds: config.effective_reynolds(),
        simulation,
        background_cells: config.mesh.preset.base_cells(),
        files,
    })
}

fn write_case_file(
    case_dir: &Path,
    relative: &str,
    contents: String,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let path = case_dir.join(relative);
    fs::write(&path, contents)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    files.push(path);
    Ok(())
}

fn csv_coordinates(coordinates: &[(f64, f64)]) -> String {
    let mut out = String::from("x_over_c,y_over_c\n");
    for &(x, y) in coordinates {
        out.push_str(&format!("{x:.16e},{y:.16e}\n"));
    }
    out
}

pub(crate) fn fv_schemes(config: &CfdStudyConfig, startup: bool) -> String {
    if config.effective_simulation().compressible {
        return compressible_fv_schemes(config, startup);
    }
    let scheme = if startup && config.solver.startup_iterations > 0 {
        "bounded Gauss upwind"
    } else {
        match config.solver.convection_scheme {
            ConvectionScheme::BoundedUpwind => "bounded Gauss upwind",
            ConvectionScheme::BoundedLinearUpwind => "bounded Gauss linearUpwind grad(U)",
        }
    };
    // The startup stage is bounded upwind for every transported variable, as
    // in the validated OpenCFD SST tutorial; the final stage uses the selected
    // turbulence convection scheme, whose default remains that tutorial's
    // bounded limited-linear.
    let (k_scheme, omega_scheme) = if startup {
        (
            "bounded Gauss upwind".to_owned(),
            "bounded Gauss upwind".to_owned(),
        )
    } else {
        let selected = config.solver.turbulence_convection_scheme;
        (selected.scheme_for("k"), selected.scheme_for("omega"))
    };
    // `cellLimited Gauss linear 0` is not the unlimited gradient in OpenFOAM's
    // grammar, so a zero coefficient emits the plain scheme instead.
    let gradient_scheme = if config.solver.gradient_limiter > 0.0 {
        format!(
            "cellLimited Gauss linear {:.6}",
            config.solver.gradient_limiter
        )
    } else {
        "Gauss linear".to_owned()
    };
    let non_orthogonal = format!("limited {:.6}", config.solver.non_orthogonal_limiter);
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object fvSchemes;\n}}\n\nddtSchemes {{ default steadyState; }}\ngradSchemes {{ default {gradient_scheme}; grad(U) {gradient_scheme}; }}\ndivSchemes\n{{\n    default none;\n    div(phi,U) {scheme};\n    div(phi,k) {k_scheme};\n    div(phi,omega) {omega_scheme};\n    div((nuEff*dev2(T(grad(U))))) Gauss linear;\n}}\nlaplacianSchemes {{ default Gauss linear {non_orthogonal}; }}\ninterpolationSchemes {{ default linear; }}\nsnGradSchemes {{ default {non_orthogonal}; }}\nwallDist {{ method meshWave; }}\nfluxRequired {{ default no; p; }}\n",
        k_scheme = k_scheme,
        omega_scheme = omega_scheme,
        gradient_scheme = gradient_scheme,
        non_orthogonal = non_orthogonal,
    )
}

/// `rhoSimpleFoam` discretisation based on the OpenCFD v2606
/// `compressible/rhoSimpleFoam/aerofoilNACA0012` tutorial.  The velocity and
/// energy fluxes remain bounded in the shock region; the pressure-flux and
/// turbulence terms use upwind transport during startup and for the final SST
/// equations.  The generated mesh is still subject to the same mesh-quality
/// gate as the incompressible case.
fn compressible_fv_schemes(config: &CfdStudyConfig, startup: bool) -> String {
    let (velocity, energy, gradient) = if startup {
        (
            "bounded Gauss upwind",
            "bounded Gauss upwind",
            "cellLimited Gauss linear 1",
        )
    } else {
        (
            "bounded Gauss linearUpwind limited",
            "bounded Gauss linearUpwind limited",
            "cellLimited Gauss linear 1",
        )
    };
    let _ = config;
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object fvSchemes;\n}}\n\nddtSchemes\n{{ default steadyState; }}\ngradSchemes\n{{\n    default Gauss linear;\n    grad(U) {gradient};\n}}\ndivSchemes\n{{\n    default none;\n    div(phi,U) {velocity};\n    energy {energy};\n    div(phi,e) {energy};\n    div(phi,K) {energy};\n    div(phi,Ekp) {energy};\n    div(phid,p) Gauss upwind;\n    div((phi|interpolate(rho)),p) bounded Gauss upwind;\n    div(phi,k) bounded Gauss upwind;\n    div(phi,omega) bounded Gauss upwind;\n    div(((rho*nuEff)*dev2(T(grad(U))))) Gauss linear;\n}}\nlaplacianSchemes\n{{ default Gauss linear limited corrected 0.5; }}\ninterpolationSchemes\n{{ default linear; }}\nsnGradSchemes\n{{ default limited corrected 0.5; }}\nwallDist\n{{ method meshWave; }}\nfluxRequired {{ default no; p; }}\n"
    )
}

/// Non-orthogonal correctors for the potential-flow initial guess.
///
/// `potentialFoam` solves one Laplace problem for the initial flux field.  On
/// the generated meshes the non-orthogonal correction is converged after a
/// handful of passes and every further pass is a full PCG solve to `1e-8`.
/// This value is an initialization cost, not a solution tolerance.
const POTENTIAL_FLOW_NON_ORTHOGONAL_CORRECTORS: u32 = 5;

/// Fraction of the acceptance `residual_tolerance` written into OpenFOAM's own
/// `residualControl`.
///
/// If the solver's stopping rule equals the acceptance gate, then every run the
/// solver ends by itself satisfies the gate *by construction* and the gate
/// carries no independent information.  That was measurable: in
/// `V1-inletoutlet-coarse` the pressure residual was `1.00840831099e-5` at outer
/// iteration 806 and `9.98679938612e-6` at 807, and the run stopped at 807
/// precisely because it had just crossed `1e-5` — a 0.13 % margin that is
/// necessary rather than earned.  Stopping at half the gate gives a self-stopped
/// run real headroom, and the gate still binds independently on a run that
/// exhausts its iteration budget.  This tightens the solver; it does not relax
/// the acceptance criterion, which is unchanged at `residual_tolerance`.
const RESIDUAL_CONTROL_MARGIN: f64 = 0.5;

fn fv_solution(config: &CfdStudyConfig) -> String {
    if config.effective_simulation().compressible {
        return compressible_fv_solution(config);
    }
    let transport = config.solver.momentum_linear_solver.fv_solution_entry();
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object fvSolution;\n}}\n\nsolvers\n{{\n    // potentialFoam writes a flux field before simpleFoam starts.\n    Phi\n    {{ solver PCG; preconditioner DIC; tolerance {inner_tolerance:.3e}; relTol 0; }}\n    p\n    {{ solver GAMG; tolerance {inner_tolerance:.3e}; relTol {pressure_rel_tol}; smoother GaussSeidel; }}\n    U\n    {{ {transport} tolerance {inner_tolerance:.3e}; relTol 0.05; }}\n    k\n    {{ {transport} tolerance {inner_tolerance:.3e}; relTol 0.05; }}\n    omega\n    {{ {transport} tolerance {inner_tolerance:.3e}; relTol 0.05; }}\n}}\n\ncache\n{{\n    grad(U);\n}}\n\nSIMPLE\n{{\n    nNonOrthogonalCorrectors {non_orthogonal_correctors};\n    consistent yes;\n    residualControl\n    {{\n        p {residual:.3e};\n        U {residual:.3e};\n        k {residual:.3e};\n        omega {residual:.3e};\n    }}\n}}\n\npotentialFlow\n{{\n    nNonOrthogonalCorrectors {potential_correctors};\n}}\n\n// Relaxation for the SIMPLEC (consistent yes) loop.  These factors set the\n// iteration path only; the converged fields satisfy the same discrete\n// equations at any stable pair, so they are a speed/robustness control and\n// never a physics or tolerance change.\nrelaxationFactors\n{{\n    fields {{ p {pressure_relaxation}; }}\n    equations {{ U {equation_relaxation}; k {turbulence_relaxation}; omega {turbulence_relaxation}; }}\n}}\n",
        // One source of truth with the convergence gate.  The classifier's
        // skipped-equation guard reasons about residuals relative to this
        // number; when it was a literal here and a constant there, the two
        // could drift silently and the guard would be comparing against a
        // tolerance the case no longer emitted.
        inner_tolerance = LINEAR_SOLVER_RESIDUAL_FLOOR,
        residual = config.solver.residual_tolerance * RESIDUAL_CONTROL_MARGIN,
        potential_correctors = POTENTIAL_FLOW_NON_ORTHOGONAL_CORRECTORS,
        pressure_relaxation = config.solver.pressure_relaxation,
        equation_relaxation = config.solver.equation_relaxation,
        turbulence_relaxation = config
            .solver
            .turbulence_relaxation
            .unwrap_or(config.solver.equation_relaxation),
        non_orthogonal_correctors = config.solver.non_orthogonal_correctors,
        pressure_rel_tol = config
            .solver
            .effective_pressure_relative_tolerance(config.mesh.preset),
        transport = transport,
    )
}

fn compressible_fv_solution(config: &CfdStudyConfig) -> String {
    let simulation = config.effective_simulation();
    let residual = config.solver.residual_tolerance.min(1.0e-4) * RESIDUAL_CONTROL_MARGIN;
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object fvSolution;\n}}\n\nsolvers\n{{\n    \"rho.*\"\n    {{ solver diagonal; }}\n    \"p.*\"\n    {{ solver GAMG; smoother GaussSeidel; tolerance 1.0e-8; relTol 0.01; }}\n    \"(U|e).*\"\n    {{ solver PBiCGStab; preconditioner DILU; tolerance 1.0e-8; relTol 0.05; }}\n    \"(k|omega).*\"\n    {{ solver PBiCGStab; preconditioner DILU; tolerance 1.0e-8; relTol 0.05; }}\n}}\n\nSIMPLE\n{{\n    residualControl\n    {{\n        p {residual:.3e};\n        U {residual:.3e};\n        e {residual:.3e};\n        k {residual:.3e};\n        omega {residual:.3e};\n    }}\n    nNonOrthogonalCorrectors {non_orthogonal_correctors};\n    pMinFactor 0.1;\n    pMaxFactor 2;\n}}\n\nrelaxationFactors\n{{\n    fields\n    {{\n        p {pressure_relaxation:.3};\n        rho 0.01;\n    }}\n    equations\n    {{\n        U {equation_relaxation:.3};\n        e {equation_relaxation:.3};\n        k {turbulence_relaxation:.3};\n        omega {turbulence_relaxation:.3};\n    }}\n}}\n",
        residual = residual,
        non_orthogonal_correctors = simulation.non_orthogonal_correctors,
        pressure_relaxation = simulation.pressure_relaxation,
        equation_relaxation = simulation.equation_relaxation,
        turbulence_relaxation = simulation.turbulence_relaxation,
    )
}

fn control_dict(config: &CfdStudyConfig) -> String {
    let reference = ReferenceConventions::from_config(config);
    let simulation = config.effective_simulation();
    let application = simulation.solver.executable();
    // forceCoeffs still needs the free-stream density used to normalise the
    // dimensional force integral, even though the local rho field is solved
    // by rhoSimpleFoam on the compressible path.
    let force_rho = config.density_kg_m3;
    let mut out = format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object controlDict;\n}}\n\napplication {application};\nstartFrom startTime;\nstartTime 0;\nstopAt endTime;\nendTime {end};\ndeltaT 1;\nwriteControl timeStep;\nwriteInterval {write};\npurgeWrite 0;\nwriteFormat ascii;\nwritePrecision 12;\nwriteCompression off;\ntimeFormat general;\ntimePrecision 8;\nrunTimeModifiable true;\n\nfunctions\n{{\n    forceCoeffs\n    {{\n        type forceCoeffs;\n        libs (forces);\n        patches (airfoil);\n        rho rhoInf;\n        rhoInf {rho:.16e};\n        CofR ({quarter:.16e} 0 {midspan:.16e});\n        liftDir ({lx:.16e} {ly:.16e} 0);\n        dragDir ({dx:.16e} {dy:.16e} 0);\n        pitchAxis ({px} {py} {pz});\n        magUInf {speed:.16e};\n        lRef {chord:.16e};\n        Aref {area:.16e};\n        writeControl timeStep;\n        writeInterval 1;\n    }}\n    forces\n    {{\n        type forces;\n        libs (forces);\n        patches (airfoil);\n        rho rhoInf;\n        rhoInf {rho:.16e};\n        CofR ({quarter:.16e} 0 {midspan:.16e});\n        writeControl timeStep;\n        writeInterval 1;\n        log off;\n    }}\n",
        application = application,
        end = simulation.max_iterations,
        write = simulation.write_interval,
        rho = force_rho,
        quarter = reference.moment_reference_m[0],
        midspan = reference.moment_reference_m[2],
        lx = reference.lift_direction[0],
        ly = reference.lift_direction[1],
        dx = reference.drag_direction[0],
        dy = reference.drag_direction[1],
        px = reference.pitch_axis[0],
        py = reference.pitch_axis[1],
        pz = reference.pitch_axis[2],
        speed = reference.speed_m_s,
        chord = reference.chord_m,
        area = reference.area_m2,
    );
    // Close the top-level `functions` dictionary after the force object.  The
    // diagnostics below are inserted immediately before this brace.
    out.push_str("}\n");
    // These solver-side function objects write wallShearStress and yPlus
    // with the active turbulence model. Running generic postProcess without
    // that model can silently produce zero y+ files, so keep the diagnostics
    // attached to the selected solver's controlDict.
    let diagnostics = r#"
    wallShearStress
    {
        type wallShearStress;
        libs (fieldFunctionObjects);
        patches (airfoil);
        writeFields yes;
        executeControl writeTime;
        writeControl writeTime;
    }
    yPlus
    {
        type yPlus;
        libs (fieldFunctionObjects);
        patches (airfoil);
        writeFields yes;
        executeControl writeTime;
        writeControl writeTime;
    }
    #include "sampleDict"
"#;
    if let Some(index) = out.rfind("\n}\n") {
        out.insert_str(index, diagnostics);
    }
    out
}

/// `meshQualityDict` carrying ONLY the limits ALAS declares.
///
/// `checkMesh -meshQuality` applies `maxInternalSkewness` to internal faces and
/// `maxBoundarySkewness` to boundary faces separately and counts the faces in
/// error, which is this dictionary's contribution: the declared pass/fail.  The
/// boundary *value* comes from elsewhere — `result_io::read_boundary_skewness_max`
/// reads the exact maximum out of the `boundaryField` of the written `skewness`
/// field, and `results.rs` stores it.  Without either route
/// `max_boundary_skewness` stays `NotMeasured`.
///
/// Every criterion ALAS does not declare is set to a value that cannot fail, so
/// a failure here is a failure of a declared limit and of nothing else.  No
/// check is invented and no declared limit is altered.  `maxConcave` is `180`
/// rather than negative because OpenFOAM rejects an out-of-range angle outright
/// (`maxDeg should be [0..180]`), and 180 degrees admits every face.
fn mesh_quality_dict(thresholds: &mesh::MeshQualityThresholds) -> String {
    format!(
        "FoamFile
{{
    version 2.0;
    format ascii;
    class dictionary;
    object meshQualityDict;
}}

// Only the limits declared by MeshQualityThresholds are active here.
maxNonOrtho             {non_ortho:.6};
maxInternalSkewness     {internal:.6};
maxBoundarySkewness     {boundary:.6};

// Not declared by ALAS: set so they cannot fail.
maxConcave              180;
minVol                  -1e30;
minTetQuality           -1e30;
minArea                 -1;
minTwist                -1;
minDeterminant          -1;
minFaceWeight           -1;
minVolRatio             -1;
minTriangleTwist        -1;
nSmoothScale            4;
errorReduction          0.75;
",
        non_ortho = thresholds.max_non_orthogonality_deg,
        internal = thresholds.max_internal_skewness,
        boundary = thresholds.max_boundary_skewness,
    )
}

fn sample_dict() -> String {
    "sample.airfoil\n{\n    type surfaces;\n    libs (sampling);\n    writeControl writeTime;\n    interpolationScheme cellPatchConstrained;\n    surfaceFormat raw;\n    fields (p wallShearStress);\n    surfaces\n    (\n        airfoil\n        {\n            type patch;\n            patches (airfoil);\n            interpolate true;\n        }\n    );\n}\n".to_owned()
}

fn transport_properties(config: &CfdStudyConfig) -> String {
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object transportProperties;\n}}\n\ntransportModel Newtonian;\nnu [0 2 -1 0 0 0 0] {nu:.16e};\n",
        nu = config.dynamic_viscosity_pa_s / config.density_kg_m3,
    )
}

/// Perfect-gas thermodynamic contract for `rhoSimpleFoam`, matching the
/// installed OpenCFD v2606 aerofoil tutorial's `hePsiThermo` family while
/// deriving the molecular properties from ALAS's explicit dry-air constants.
fn thermophysical_properties(config: &CfdStudyConfig) -> String {
    let mol_weight = 1_000.0 * 8.314_462_618_153_24 / DRY_AIR_GAS_CONSTANT_J_KG_K;
    let cp = DRY_AIR_GAMMA * DRY_AIR_GAS_CONSTANT_J_KG_K / (DRY_AIR_GAMMA - 1.0);
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object thermophysicalProperties;\n}}\n\nthermoType\n{{\n    type hePsiThermo;\n    mixture pureMixture;\n    transport const;\n    thermo hConst;\n    equationOfState perfectGas;\n    specie specie;\n    energy sensibleInternalEnergy;\n}}\n\nmixture\n{{\n    specie\n    {{\n        molWeight {mol_weight:.8};\n    }}\n    thermodynamics\n    {{\n        Cp {cp:.8};\n        Hf 0;\n    }}\n    transport\n    {{\n        mu {mu:.8e};\n        Pr 0.71;\n    }}\n}}\n",
        mol_weight = mol_weight,
        cp = cp,
        mu = config.dynamic_viscosity_pa_s,
    )
}

fn turbulence_properties() -> String {
    "FoamFile\n{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object turbulenceProperties;\n}\n\nsimulationType RAS;\n\nRAS\n{\n    RASModel kOmegaSST;\n    turbulence on;\n    printCoeffs on;\n}\n".to_owned()
}

fn velocity_components(config: &CfdStudyConfig) -> (f64, f64) {
    let velocity = ReferenceConventions::from_config(config).freestream_velocity_m_s;
    (velocity[0], velocity[1])
}

fn initial_u(config: &CfdStudyConfig) -> String {
    if config.effective_simulation().compressible {
        return compressible_initial_u(config);
    }
    let (ux, uy) = velocity_components(config);
    let (inlet, outlet, far_field) = match config.boundaries.far_field {
        FarFieldCondition::FixedValue => (
            format!("inlet {{ type fixedValue; value uniform ({ux:.16e} {uy:.16e} 0); }}"),
            "outlet { type zeroGradient; }".to_owned(),
            config.boundaries.far_field_velocity.far_field_entry(ux, uy),
        ),
        FarFieldCondition::Freestream => {
            let value = format!(
                "type freestreamVelocity; freestreamValue uniform ({ux:.16e} {uy:.16e} 0);"
            );
            (
                format!("inlet {{ {value} }}"),
                format!("outlet {{ {value} }}"),
                format!("farField {{ {value} }}"),
            )
        }
    };
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class volVectorField;\n    object U;\n}}\n\ndimensions [0 1 -1 0 0 0 0];\ninternalField uniform ({ux:.16e} {uy:.16e} 0);\nboundaryField\n{{\n    {inlet}\n    {outlet}\n    {far_field}\n    frontAndBack {{ type empty; }}\n    airfoil {{ type noSlip; }}\n}}\n",
        inlet = inlet,
        outlet = outlet,
        far_field = far_field,
    )
}

fn initial_p(config: &CfdStudyConfig) -> String {
    if config.effective_simulation().compressible {
        return compressible_initial_p(config);
    }
    let p_ref = config.boundaries.pressure_reference_pa / config.density_kg_m3;
    let (inlet, outlet, far_field) = match config.boundaries.far_field {
        FarFieldCondition::FixedValue => (
            "inlet { type zeroGradient; }".to_owned(),
            format!("outlet {{ type fixedValue; value uniform {p_ref:.16e}; }}"),
            "farField { type zeroGradient; }".to_owned(),
        ),
        FarFieldCondition::Freestream => {
            let value = format!("type freestreamPressure; freestreamValue uniform {p_ref:.16e};");
            (
                format!("inlet {{ {value} }}"),
                format!("outlet {{ {value} }}"),
                format!("farField {{ {value} }}"),
            )
        }
    };
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class volScalarField;\n    object p;\n}}\n\ndimensions [0 2 -2 0 0 0 0];\ninternalField uniform {p_ref:.16e};\nboundaryField\n{{\n    {inlet}\n    {outlet}\n    {far_field}\n    frontAndBack {{ type empty; }}\n    airfoil {{ type zeroGradient; }}\n}}\n"
    )
}

fn initial_k(config: &CfdStudyConfig) -> String {
    if config.effective_simulation().compressible {
        return compressible_initial_k(config);
    }
    let turbulence = config.effective_turbulence();
    let k = turbulence.k_m2_s2;
    let (inlet, outlet, far_field) = match config.boundaries.far_field {
        FarFieldCondition::FixedValue => (
            format!("inlet {{ type fixedValue; value uniform {k:.16e}; }}"),
            "outlet { type zeroGradient; }".to_owned(),
            config.boundaries.far_field_turbulence.far_field_entry(k),
        ),
        FarFieldCondition::Freestream => {
            let value = format!("type freestream; freestreamValue uniform {k:.16e};");
            (
                format!("inlet {{ {value} }}"),
                format!("outlet {{ {value} }}"),
                format!("farField {{ {value} }}"),
            )
        }
    };
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class volScalarField;\n    object k;\n}}\n\ndimensions [0 2 -2 0 0 0 0];\ninternalField uniform {k:.16e};\nboundaryField\n{{\n    {inlet}\n    {outlet}\n    {far_field}\n    frontAndBack {{ type empty; }}\n    airfoil {{ type kqRWallFunction; value uniform {k:.16e}; }}\n}}\n",
        inlet = inlet,
        outlet = outlet,
        far_field = far_field,
    )
}

fn initial_omega(config: &CfdStudyConfig) -> String {
    if config.effective_simulation().compressible {
        return compressible_initial_omega(config);
    }
    let turbulence = config.effective_turbulence();
    let omega = turbulence.omega_s_inv;
    let (inlet, outlet, far_field) = match config.boundaries.far_field {
        FarFieldCondition::FixedValue => (
            format!("inlet {{ type fixedValue; value uniform {omega:.16e}; }}"),
            "outlet { type zeroGradient; }".to_owned(),
            config
                .boundaries
                .far_field_turbulence
                .far_field_entry(omega),
        ),
        FarFieldCondition::Freestream => {
            let value = format!("type freestream; freestreamValue uniform {omega:.16e};");
            (
                format!("inlet {{ {value} }}"),
                format!("outlet {{ {value} }}"),
                format!("farField {{ {value} }}"),
            )
        }
    };
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class volScalarField;\n    object omega;\n}}\n\ndimensions [0 0 -1 0 0 0 0];\ninternalField uniform {omega:.16e};\nboundaryField\n{{\n    {inlet}\n    {outlet}\n    {far_field}\n    frontAndBack {{ type empty; }}\n    airfoil {{ type omegaWallFunction; blended true; value uniform {omega:.16e}; }}\n}}\n",
        inlet = inlet,
        outlet = outlet,
        far_field = far_field,
    )
}

fn initial_nut(config: &CfdStudyConfig) -> String {
    if config.effective_simulation().compressible {
        return compressible_initial_nut(config);
    }
    let (inlet, outlet, far_field) = match config.boundaries.far_field {
        FarFieldCondition::FixedValue => (
            "inlet { type calculated; value uniform 0; }".to_owned(),
            "outlet { type calculated; value uniform 0; }".to_owned(),
            "farField { type calculated; value uniform 0; }".to_owned(),
        ),
        FarFieldCondition::Freestream => {
            let value = "type freestream; freestreamValue uniform 0;";
            (
                format!("inlet {{ {value} }}"),
                format!("outlet {{ {value} }}"),
                format!("farField {{ {value} }}"),
            )
        }
    };
    // The Spalding wall function inverts a transcendental relation per wall
    // face by Newton iteration.  OpenFOAM's shipped defaults stop at a relative
    // error of 1e-2 after at most 10 passes, which leaves roughly one per cent
    // of iteration-to-iteration noise in the wall eddy viscosity and therefore
    // in the near-wall k and omega source terms.  Both controls are solve
    // accuracy for the same wall relation; neither is a model coefficient.
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class volScalarField;\n    object nut;\n}}\n\ndimensions [0 2 -1 0 0 0 0];\ninternalField uniform 0;\nboundaryField\n{{\n    {inlet}\n    {outlet}\n    {far_field}\n    frontAndBack {{ type empty; }}\n    airfoil {{ type nutUSpaldingWallFunction; tolerance {wall_tolerance:.3e}; maxIter {wall_max_iterations}; value uniform 0; }}\n}}\n",
        inlet = inlet,
        outlet = outlet,
        far_field = far_field,
        wall_tolerance = config.solver.wall_function_tolerance,
        wall_max_iterations = config.solver.wall_function_max_iterations,
    )
}

fn compressible_initial_u(config: &CfdStudyConfig) -> String {
    let (ux, uy) = velocity_components(config);
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class volVectorField;\n    object U;\n}}\n\ndimensions [0 1 -1 0 0 0 0];\ninternalField uniform ({ux:.16e} {uy:.16e} 0);\nboundaryField\n{{\n    inlet {{ type freestreamVelocity; freestreamValue uniform ({ux:.16e} {uy:.16e} 0); value uniform ({ux:.16e} {uy:.16e} 0); }}\n    outlet {{ type freestreamVelocity; freestreamValue uniform ({ux:.16e} {uy:.16e} 0); value uniform ({ux:.16e} {uy:.16e} 0); }}\n    farField {{ type freestreamVelocity; freestreamValue uniform ({ux:.16e} {uy:.16e} 0); value uniform ({ux:.16e} {uy:.16e} 0); }}\n    frontAndBack {{ type empty; }}\n    airfoil {{ type noSlip; }}\n}}\n"
    )
}

fn compressible_initial_p(config: &CfdStudyConfig) -> String {
    let pressure = config.effective_static_pressure_pa();
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class volScalarField;\n    object p;\n}}\n\ndimensions [1 -1 -2 0 0 0 0];\ninternalField uniform {pressure:.16e};\nboundaryField\n{{\n    inlet {{ type freestreamPressure; freestreamValue uniform {pressure:.16e}; value uniform {pressure:.16e}; }}\n    outlet {{ type freestreamPressure; freestreamValue uniform {pressure:.16e}; value uniform {pressure:.16e}; }}\n    farField {{ type freestreamPressure; freestreamValue uniform {pressure:.16e}; value uniform {pressure:.16e}; }}\n    frontAndBack {{ type empty; }}\n    airfoil {{ type zeroGradient; }}\n}}\n"
    )
}

fn compressible_initial_t(config: &CfdStudyConfig) -> String {
    let temperature = config.freestream_temperature_k;
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class volScalarField;\n    object T;\n}}\n\ndimensions [0 0 0 1 0 0 0];\ninternalField uniform {temperature:.16e};\nboundaryField\n{{\n    inlet {{ type inletOutlet; inletValue uniform {temperature:.16e}; value uniform {temperature:.16e}; }}\n    outlet {{ type inletOutlet; inletValue uniform {temperature:.16e}; value uniform {temperature:.16e}; }}\n    farField {{ type inletOutlet; inletValue uniform {temperature:.16e}; value uniform {temperature:.16e}; }}\n    frontAndBack {{ type empty; }}\n    airfoil {{ type zeroGradient; }}\n}}\n"
    )
}

fn initial_t(config: &CfdStudyConfig) -> String {
    compressible_initial_t(config)
}

fn initial_alphat() -> String {
    "FoamFile\n{\n    version 2.0;\n    format ascii;\n    class volScalarField;\n    object alphat;\n}\n\ndimensions [1 -1 -1 0 0 0 0];\ninternalField uniform 0;\nboundaryField\n{\n    inlet { type calculated; value uniform 0; }\n    outlet { type calculated; value uniform 0; }\n    farField { type calculated; value uniform 0; }\n    frontAndBack { type empty; }\n    airfoil { type compressible::alphatWallFunction; value uniform 0; }\n}\n".to_owned()
}

fn compressible_initial_k(config: &CfdStudyConfig) -> String {
    let k = config.effective_turbulence().k_m2_s2;
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class volScalarField;\n    object k;\n}}\n\ndimensions [0 2 -2 0 0 0 0];\ninternalField uniform {k:.16e};\nboundaryField\n{{\n    inlet {{ type inletOutlet; inletValue uniform {k:.16e}; value uniform {k:.16e}; }}\n    outlet {{ type inletOutlet; inletValue uniform {k:.16e}; value uniform {k:.16e}; }}\n    farField {{ type inletOutlet; inletValue uniform {k:.16e}; value uniform {k:.16e}; }}\n    frontAndBack {{ type empty; }}\n    airfoil {{ type kqRWallFunction; value uniform {k:.16e}; }}\n}}\n"
    )
}

fn compressible_initial_omega(config: &CfdStudyConfig) -> String {
    let omega = config.effective_turbulence().omega_s_inv;
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class volScalarField;\n    object omega;\n}}\n\ndimensions [0 0 -1 0 0 0 0];\ninternalField uniform {omega:.16e};\nboundaryField\n{{\n    inlet {{ type inletOutlet; inletValue uniform {omega:.16e}; value uniform {omega:.16e}; }}\n    outlet {{ type inletOutlet; inletValue uniform {omega:.16e}; value uniform {omega:.16e}; }}\n    farField {{ type inletOutlet; inletValue uniform {omega:.16e}; value uniform {omega:.16e}; }}\n    frontAndBack {{ type empty; }}\n    airfoil {{ type omegaWallFunction; blended true; value uniform {omega:.16e}; }}\n}}\n"
    )
}

fn compressible_initial_nut(_config: &CfdStudyConfig) -> String {
    "FoamFile\n{\n    version 2.0;\n    format ascii;\n    class volScalarField;\n    object nut;\n}\n\ndimensions [0 2 -1 0 0 0 0];\ninternalField uniform 0;\nboundaryField\n{\n    inlet { type calculated; value uniform 0; }\n    outlet { type calculated; value uniform 0; }\n    farField { type calculated; value uniform 0; }\n    frontAndBack { type empty; }\n    airfoil { type nutkWallFunction; value uniform 0; }\n}\n".to_owned()
}

fn case_readme(config: &CfdStudyConfig, airfoil: &AirfoilSnapshot) -> String {
    let turbulence = config.effective_turbulence();
    let simulation = config.effective_simulation();
    format!(
        "# ALAS OpenFOAM airfoil case\n\nTemplate: `{TEMPLATE_VERSION}`\nAirfoil: `{}`\nCoordinate hash: `{}`\nChord: `{:.8} m`\nAngle of attack: `{:.6} deg`\nSpeed: `{:.8} m/s`\nReynolds number: `{:.8e}`\nDensity: `{:.8} kg/m^3`\nDynamic viscosity: `{:.8e} Pa s`\nStatic temperature: `{:.8} K`\nStatic pressure used by thermodynamics: `{:.8} Pa`\nDry-air speed of sound: `{:.8} m/s`\nFreestream Mach number: `{:.8}`\nFlow regime: `{}`\nOpenFOAM solver: `{}`\nCompressible equation set: `{}`\nAutomatic maximum iterations: `{}`\nAutomatic startup iterations: `{}`\nAutomatic momentum relaxation: `{:.3}`\nAutomatic turbulence relaxation: `{:.3}`\nAutomatic pressure relative tolerance: `{:.3e}`\n\nFreestream turbulence specification: `{}`\nTurbulence intensity: `{:.6e}`\nConfigured length scale: `{:.8e} m`\nConfigured nu_t/nu ratio: `{:.8e}`\nEffective k: `{:.8e} m^2/s^2`\nEffective omega: `{:.8e} 1/s`\nEffective nu_t: `{:.8e} m^2/s`\nEffective nu_t/nu: `{:.8e}`\nEffective length implied by omega: `{:.8e} m`\n\nThe section frame is chord +x, normal +y and extrusion +z. Positive angle rotates the freestream velocity toward +y. Drag is positive along the freestream and lift is positive 90 degrees counter-clockwise from it. The moment reference is x/c = 0.25 on the extrusion mid-plane, Cm is positive nose-up (leading edge toward +y) through the forceCoeffs pitch axis (0 0 -1), lRef is the chord and Aref = chord times the explicit extrusion span. Mach is `U/sqrt(gamma R T)` using dry-air gamma 1.4, R = 287.05287 J/(kg K), and the declared static temperature. For Mach below 0.3 the case uses incompressible steady k-omega SST `simpleFoam`; at and above Mach 0.3 it uses perfect-gas steady k-omega SST `rhoSimpleFoam`, includes density and energy equations, and switches to bounded shock-safe convection and damped relaxation. Numerical convergence is evidence from this case; it is not physical validation against experiment.\n",
        airfoil.name,
        airfoil.coordinate_hash,
        config.chord_m,
        config.angle_of_attack_deg,
        config.effective_speed_m_s(),
        config.effective_reynolds(),
        config.density_kg_m3,
        config.dynamic_viscosity_pa_s,
        config.freestream_temperature_k,
        config.effective_static_pressure_pa(),
        config.speed_of_sound_m_s(),
        config.mach_number(),
        simulation.regime.as_str(),
        simulation.solver.executable(),
        simulation.compressible,
        simulation.max_iterations,
        simulation.startup_iterations,
        simulation.equation_relaxation,
        simulation.turbulence_relaxation,
        simulation.pressure_relative_tolerance,
        config.turbulence_specification.as_str(),
        turbulence.intensity_fraction,
        turbulence.configured_length_m,
        turbulence.configured_viscosity_ratio,
        turbulence.k_m2_s2,
        turbulence.omega_s_inv,
        turbulence.nu_t_m2_s,
        turbulence.nu_t_over_nu,
        turbulence.effective_length_m,
    )
}
