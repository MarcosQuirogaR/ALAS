// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preset sizing with the same declared fuel and mounted masses as product sizing.

use alas_config::{materials, presets, AlasConfig};
use alas_geom::{builder::AircraftBuilder, wing_structure::WingStructureGeometry};
use alas_struct::{
    analytical::LoadCaseResult,
    scope::{wing_mounted_relief, WingFuelDesignCase},
    sizing::{self, WingFuelRelief, WingboxSizing},
    tanks,
};
use serde_json::{json, Value};
use std::io;

pub(super) struct Model {
    pub name: String,
    pub sizing: WingboxSizing,
    pub case: LoadCaseResult,
    pub ei: Vec<f64>,
    pub sections: Vec<Section>,
    pub point_forces: Vec<(f64, f64)>,
    pub e: f64,
    pub nu: f64,
    pub converged: bool,
    pub fuel_description: String,
    pub skin: &'static materials::MaterialSpec,
    pub web: &'static materials::MaterialSpec,
    pub cap: &'static materials::MaterialSpec,
}

#[derive(serde::Serialize)]
pub(super) struct Section {
    pub area: f64,
    pub i1: f64,
    pub i2: f64,
    pub j: f64,
    pub recovery: f64,
}

pub(super) fn build(name: &str) -> io::Result<Model> {
    let preset = presets::get(name).map_err(io::Error::other)?;
    let config = AlasConfig::from_value(&json!({"preset": name})).map_err(io::Error::other)?;
    let design = &preset.design_vector;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(design), false)
        .map_err(io::Error::other)?;
    let wing = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .ok_or_else(|| io::Error::other("main wing absent"))?;
    let root = wing
        .xsecs
        .first()
        .ok_or_else(|| io::Error::other("root airfoil absent"))?;
    let tip = wing
        .xsecs
        .last()
        .ok_or_else(|| io::Error::other("tip airfoil absent"))?;
    let cfg = &config.structures;
    let (fractions, full_span) = cfg.resolved_spars();
    let geometry = WingStructureGeometry::new(
        design,
        &config.geometry.wing,
        &root.airfoil,
        &tip.airfoil,
        &fractions,
        Some(&full_span),
    )
    .map_err(io::Error::other)?;
    let mut req = config.requirements.clone();
    req.mtow_kg = alas_mass::wing_reconciliation::structural_design_mass_kg(&config);
    let stations = sizing::sizing_stations(&geometry, cfg);
    let (front, rear) = sizing::box_chord_band(&geometry);
    let declared = alas_mass::wing_reconciliation::declared_wing_fuel_case(
        &config, design, &req, &geometry, &stations, front, rear,
    );
    let fuel = declared
        .as_ref()
        .map(|case| case.running_mass_kg_m.clone())
        .unwrap_or_else(|| {
            tanks::integral_fuel_running_mass_kg_m(&geometry, &stations, front, rear)
        });
    let fuel_scope = declared
        .as_ref()
        .map_or(WingFuelRelief::EnclosedBoxVolume, |case| {
            WingFuelRelief::Declared {
                running_mass_kg_m: &fuel,
                design_case: WingFuelDesignCase::declared(
                    case.capacity_kg,
                    case.design_gross_mass_kg,
                    case.max_zero_fuel_mass_kg,
                ),
            }
        });
    let mounted = wing_mounted_relief(&config.geometry.engine, &config.mass_model, &req);
    let skin = materials::get(&cfg.skin_material).map_err(io::Error::other)?;
    let web = materials::get(&cfg.spar_web_material).map_err(io::Error::other)?;
    let cap = materials::get(&cfg.spar_cap_material).map_err(io::Error::other)?;
    let rib = materials::get(&cfg.rib_material).map_err(io::Error::other)?;
    let scoped = sizing::size_wingbox_with_scope(
        &geometry,
        cfg,
        &req,
        skin,
        web,
        cap,
        rib,
        &fuel_scope,
        &mounted,
    );
    if !scoped.scope.relief_convergence.is_settled() {
        return Err(io::Error::other("inertial relief did not converge"));
    }
    let sized = sizing::size_for_linear_model(
        &geometry,
        scoped.sizing,
        cfg,
        &req,
        &config.geometry.engine,
        &config.mass_model,
        skin,
        web,
        cap,
        &fuel,
        &mounted.point_masses_kg,
        alas_struct::feasibility::LinearModelLimits {
            max_curvature_relative_error: cfg.max_linear_curvature_relative_error,
        },
    );
    let case = sized
        .response
        .load_cases
        .iter()
        .find(|case| case.name == sized.sizing.sizing_load_case)
        .ok_or_else(|| io::Error::other("sizing case absent"))?
        .clone();
    let e = cap.e_pa;
    let sections = (0..stations.len())
        .map(|station| {
            section(
                &sized.sizing,
                station,
                sized.response.ei_nm2[station] / e,
                skin,
                web,
                cap,
            )
        })
        .collect();
    Ok(Model {
        name: name.into(),
        sections,
        e,
        nu: cap.nu,
        converged: sized.converged,
        skin,
        web,
        cap,
        fuel_description: format!("{:?}", scoped.scope.wing_fuel),
        point_forces: mounted
            .point_masses_kg
            .iter()
            .map(|&(y, mass)| (y, -case.load_factor * req.gravity_m_s2 * mass))
            .collect(),
        sizing: sized.sizing,
        ei: sized.response.ei_nm2,
        case,
    })
}

fn section(
    sizing: &WingboxSizing,
    station: usize,
    i1: f64,
    skin: &materials::MaterialSpec,
    web: &materials::MaterialSpec,
    cap: &materials::MaterialSpec,
) -> Section {
    let active: Vec<_> = sizing
        .spars
        .iter()
        .filter(|spar| spar.h[station] > 0.0)
        .collect();
    let mut area = 0.0;
    let mut first = 0.0;
    let mut second = 0.0;
    for spar in &active {
        let x = spar.chord_fraction * sizing.chord[station];
        let cap_area = 2.0 * spar.a_cap[station];
        let web_area = web.e_pa / cap.e_pa * spar.t_web * spar.h[station];
        area += cap_area + web_area;
        first += (cap_area + web_area) * x;
        second += cap_area * (x * x + spar.w_cap[station].powi(2) / 12.0)
            + web_area * (x * x + spar.t_web.powi(2) / 12.0);
    }
    let mut enclosed = 0.0;
    let mut torsion_compliance = 0.0;
    for pair in active.windows(2) {
        let x1 = pair[0].chord_fraction * sizing.chord[station];
        let x2 = pair[1].chord_fraction * sizing.chord[station];
        let width = x2 - x1;
        let cover_area = 2.0 * skin.e_pa / cap.e_pa * sizing.t_skin * width;
        area += cover_area;
        first += cover_area * (x1 + x2) / 2.0;
        second += cover_area * (x1 * x1 + x1 * x2 + x2 * x2) / 3.0;
        enclosed += width * (pair[0].h[station] + pair[1].h[station]) / 2.0;
        torsion_compliance += 2.0 * width.hypot((pair[1].h[station] - pair[0].h[station]) / 2.0)
            / sizing.t_skin
            * cap.g_pa()
            / skin.g_pa();
    }
    for spar in active.first().into_iter().chain(active.last()) {
        torsion_compliance += spar.h[station] / spar.t_web * cap.g_pa() / web.g_pa();
    }
    Section {
        area,
        i1,
        i2: second - first * first / area,
        j: 4.0 * enclosed * enclosed / torsion_compliance,
        recovery: active
            .iter()
            .map(|spar| spar.h[station] / 2.0)
            .fold(0.0_f64, f64::max),
    }
}

impl Model {
    pub(super) fn evidence(&self) -> Value {
        json!({ "preset": self.name, "sizing_converged": self.converged, "fuel_scope": self.fuel_description, "frame": "y=span from centerline, z=up; straight unswept Euler-Bernoulli beam", "load_case": self.case.name, "load_factor": self.case.load_factor, "span_m": self.case.y, "q_net_n_m": self.case.q_net, "point_forces_n": self.point_forces, "moment_nm": self.case.moment_nm, "shear_n": self.case.shear_n, "ei_nm2": self.ei, "deflection_m": self.case.deflection_m, "sections_mks": self.sections, "cap_stress_pa": self.case.spar_stress.iter().map(|spar| &spar.stress_pa).collect::<Vec<_>>(), "E_pa": self.e, "nu": self.nu, "J_scope": "Bredt outer-cell transformed proxy; torsion is unloaded and unvalidated", "chord_m": self.sizing.chord, "t_skin_m": self.sizing.t_skin, "spars": self.sizing.spars.iter().map(|spar| json!({"chord_fraction": spar.chord_fraction, "h_m": spar.h, "w_cap_m": spar.w_cap, "t_cap_m": spar.t_cap, "t_web_m": spar.t_web})).collect::<Vec<_>>(), "materials": [self.skin, self.web, self.cap] })
    }
}
