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
    /// Number of background cells requested.
    pub background_cells: (u32, u32, u32),
    /// Files written by the builder.
    pub files: Vec<PathBuf>,
}

/// Build a fresh isolated OpenFOAM case with no external process.
pub fn generate_case(config: &CfdStudyConfig, case_dir: &Path) -> Result<GeneratedCase, String> {
    config.validate().map_err(|errors| errors.join(" "))?;
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
        "constant/transportProperties",
        transport_properties(config),
        &mut files,
    )?;
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
    let snapshot = StudyProvenance {
        template_version: TEMPLATE_VERSION.to_owned(),
        config: config.clone(),
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
    let scheme = if startup && config.solver.startup_iterations > 0 {
        "bounded Gauss upwind"
    } else {
        match config.solver.convection_scheme {
            ConvectionScheme::BoundedUpwind => "bounded Gauss upwind",
            ConvectionScheme::BoundedLinearUpwind => "bounded Gauss linearUpwind grad(U)",
        }
    };
    // Match the validated OpenCFD SST tutorial: the startup stage uses
    // bounded upwind for every transported variable, while the final stage
    // uses the bounded limited-linear scalar scheme for k and omega.
    let turbulence_scheme = if startup {
        "bounded Gauss upwind"
    } else {
        "bounded Gauss limitedLinear 1"
    };
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object fvSchemes;\n}}\n\nddtSchemes {{ default steadyState; }}\ngradSchemes {{ default cellLimited Gauss linear 1; grad(U) cellLimited Gauss linear 1; }}\ndivSchemes\n{{\n    default none;\n    div(phi,U) {scheme};\n    div(phi,k) {turbulence_scheme};\n    div(phi,omega) {turbulence_scheme};\n    div((nuEff*dev2(T(grad(U))))) Gauss linear;\n}}\nlaplacianSchemes {{ default Gauss linear limited 0.5; }}\ninterpolationSchemes {{ default linear; }}\nsnGradSchemes {{ default limited 0.5; }}\nwallDist {{ method meshWave; }}\nfluxRequired {{ default no; p; }}\n",
        turbulence_scheme = turbulence_scheme,
    )
}

fn fv_solution(config: &CfdStudyConfig) -> String {
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object fvSolution;\n}}\n\nsolvers\n{{\n    // potentialFoam writes a flux field before simpleFoam starts.\n    Phi\n    {{ solver PCG; preconditioner DIC; tolerance 1e-8; relTol 0; }}\n    p\n    {{ solver GAMG; tolerance 1e-8; relTol 0.05; smoother GaussSeidel; cache {{ grad(U); }} }}\n    U\n    {{ solver smoothSolver; smoother symGaussSeidel; tolerance 1e-8; relTol 0.05; }}\n    k\n    {{ solver smoothSolver; smoother symGaussSeidel; tolerance 1e-8; relTol 0.05; }}\n    omega\n    {{ solver smoothSolver; smoother symGaussSeidel; tolerance 1e-8; relTol 0.05; }}\n}}\n\nSIMPLE\n{{\n    nNonOrthogonalCorrectors 1;\n    consistent yes;\n    residualControl\n    {{\n        p {residual:.3e};\n        U {residual:.3e};\n        k {residual:.3e};\n        omega {residual:.3e};\n    }}\n}}\n\npotentialFlow\n{{\n    nNonOrthogonalCorrectors 20;\n}}\n\nrelaxationFactors\n{{\n    fields {{ p 0.3; }}\n    equations {{ U 0.7; k 0.7; omega 0.7; }}\n}}\n",
        residual = config.solver.residual_tolerance,
    )
}

fn control_dict(config: &CfdStudyConfig) -> String {
    let reference = ReferenceConventions::from_config(config);
    let mut out = format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object controlDict;\n}}\n\napplication simpleFoam;\nstartFrom startTime;\nstartTime 0;\nstopAt endTime;\nendTime {end};\ndeltaT 1;\nwriteControl timeStep;\nwriteInterval {write};\npurgeWrite 0;\nwriteFormat ascii;\nwritePrecision 12;\nwriteCompression off;\ntimeFormat general;\ntimePrecision 8;\nrunTimeModifiable true;\n\nfunctions\n{{\n    forceCoeffs\n    {{\n        type forceCoeffs;\n        libs (forces);\n        patches (airfoil);\n        rho rhoInf;\n        rhoInf {rho:.16e};\n        CofR ({quarter:.16e} 0 {midspan:.16e});\n        liftDir ({lx:.16e} {ly:.16e} 0);\n        dragDir ({dx:.16e} {dy:.16e} 0);\n        pitchAxis ({px} {py} {pz});\n        magUInf {speed:.16e};\n        lRef {chord:.16e};\n        Aref {area:.16e};\n        writeControl timeStep;\n        writeInterval 1;\n    }}\n    forces\n    {{\n        type forces;\n        libs (forces);\n        patches (airfoil);\n        rho rhoInf;\n        rhoInf {rho:.16e};\n        CofR ({quarter:.16e} 0 {midspan:.16e});\n        writeControl timeStep;\n        writeInterval 1;\n        log off;\n    }}\n}}\n",
        end = config.solver.max_iterations,
        write = config.solver.write_interval.max(1),
        rho = config.density_kg_m3,
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
    // These solver-side function objects write wallShearStress and yPlus
    // with the active turbulence model. Running generic postProcess without
    // that model can silently produce zero y+ files, so keep the diagnostics
    // attached to simpleFoam's controlDict.
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

fn sample_dict() -> String {
    "sample.airfoil\n{\n    type surfaces;\n    libs (sampling);\n    writeControl writeTime;\n    interpolationScheme cellPatchConstrained;\n    surfaceFormat raw;\n    fields (p wallShearStress);\n    surfaces\n    (\n        airfoil\n        {\n            type patch;\n            patches (airfoil);\n            interpolate true;\n        }\n    );\n}\n".to_owned()
}

fn transport_properties(config: &CfdStudyConfig) -> String {
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object transportProperties;\n}}\n\ntransportModel Newtonian;\nnu [0 2 -1 0 0 0 0] {nu:.16e};\n",
        nu = config.dynamic_viscosity_pa_s / config.density_kg_m3,
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
    let (ux, uy) = velocity_components(config);
    let (inlet, outlet, far_field) = match config.boundaries.far_field {
        FarFieldCondition::FixedValue => (
            format!("inlet {{ type fixedValue; value uniform ({ux:.16e} {uy:.16e} 0); }}"),
            "outlet { type zeroGradient; }".to_owned(),
            format!("farField {{ type fixedValue; value uniform ({ux:.16e} {uy:.16e} 0); }}"),
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
    let turbulence = config.effective_turbulence();
    let k = turbulence.k_m2_s2;
    let (inlet, outlet, far_field) = match config.boundaries.far_field {
        FarFieldCondition::FixedValue => (
            format!("inlet {{ type fixedValue; value uniform {k:.16e}; }}"),
            "outlet { type zeroGradient; }".to_owned(),
            format!("farField {{ type fixedValue; value uniform {k:.16e}; }}"),
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
    let turbulence = config.effective_turbulence();
    let omega = turbulence.omega_s_inv;
    let (inlet, outlet, far_field) = match config.boundaries.far_field {
        FarFieldCondition::FixedValue => (
            format!("inlet {{ type fixedValue; value uniform {omega:.16e}; }}"),
            "outlet { type zeroGradient; }".to_owned(),
            format!("farField {{ type fixedValue; value uniform {omega:.16e}; }}"),
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
    format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class volScalarField;\n    object nut;\n}}\n\ndimensions [0 2 -1 0 0 0 0];\ninternalField uniform 0;\nboundaryField\n{{\n    {inlet}\n    {outlet}\n    {far_field}\n    frontAndBack {{ type empty; }}\n    airfoil {{ type nutUSpaldingWallFunction; value uniform 0; }}\n}}\n",
        inlet = inlet,
        outlet = outlet,
        far_field = far_field,
    )
}

fn case_readme(config: &CfdStudyConfig, airfoil: &AirfoilSnapshot) -> String {
    let turbulence = config.effective_turbulence();
    format!(
        "# ALAS OpenFOAM airfoil case\n\nTemplate: `{TEMPLATE_VERSION}`\nAirfoil: `{}`\nCoordinate hash: `{}`\nChord: `{:.8} m`\nAngle of attack: `{:.6} deg`\nSpeed: `{:.8} m/s`\nReynolds number: `{:.8e}`\nDensity: `{:.8} kg/m^3`\nDynamic viscosity: `{:.8e} Pa s`\nStatic temperature: `{:.8} K`\nDry-air speed of sound: `{:.8} m/s`\nFreestream Mach number: `{:.8}`\n\nFreestream turbulence specification: `{}`\nTurbulence intensity: `{:.6e}`\nConfigured length scale: `{:.8e} m`\nConfigured nu_t/nu ratio: `{:.8e}`\nEffective k: `{:.8e} m^2/s^2`\nEffective omega: `{:.8e} 1/s`\nEffective nu_t: `{:.8e} m^2/s`\nEffective nu_t/nu: `{:.8e}`\nEffective length implied by omega: `{:.8e} m`\n\nThe section frame is chord +x, normal +y and extrusion +z. Positive angle rotates the freestream velocity toward +y. Drag is positive along the freestream and lift is positive 90 degrees counter-clockwise from it. The moment reference is x/c = 0.25 on the extrusion mid-plane, Cm is positive nose-up (leading edge toward +y) through the forceCoeffs pitch axis (0 0 -1), lRef is the chord and Aref = chord times the explicit extrusion span. Mach is `U/sqrt(gamma R T)` using dry-air gamma 1.4, R = 287.05287 J/(kg K), and the declared static temperature. The initial template uses incompressible steady k-omega SST RANS and is not validated for low-Reynolds transition, stall, transonic compressibility or unsteady shedding.\n",
        airfoil.name,
        airfoil.coordinate_hash,
        config.chord_m,
        config.angle_of_attack_deg,
        config.effective_speed_m_s(),
        config.effective_reynolds(),
        config.density_kg_m3,
        config.dynamic_viscosity_pa_s,
        config.freestream_temperature_k,
        config.speed_of_sound_m_s(),
        config.mach_number(),
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
