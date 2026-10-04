// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Publisher links and configuration targets for user-supplied tools.

/// Configuration card shown in the detached External Tools manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalToolConfig {
    All,
    Mses,
    MscNastran,
    MscPatran,
    Nastran95,
    OpenVsp,
    OpenFoam,
    Gmsh,
    ParaView,
    FlowUnsteady,
}

pub(crate) struct UserSuppliedTool {
    pub name: &'static str,
    pub source: &'static str,
    pub licence: &'static str,
    pub url: &'static str,
    pub action: &'static str,
    pub config: ExternalToolConfig,
}

/// Sources follow the project's third-party notices and the linked official
/// publisher pages. ALAS offers publisher links, never an automatic installer
/// for these tools.
pub(crate) const USER_SUPPLIED_TOOLS: &[UserSuppliedTool] = &[
    UserSuppliedTool {
        name: "MSES (mset, mses, mplot)",
        source: "MIT Technology Licensing Office (tlo.mit.edu)",
        licence: "MIT academic, government, or commercial licence; terms vary",
        url: "https://tlo.mit.edu/industry-entrepreneurs/available-technologies/mses-software-high-lift-multielement-airfoil",
        action: "Licensing options",
        config: ExternalToolConfig::Mses,
    },
    UserSuppliedTool {
        name: "MSC Nastran",
        source: "MSC Software official product page",
        licence: "Proprietary",
        url: "https://nexus.hexagon.com/home/product/msc-nastran/",
        action: "License / download",
        config: ExternalToolConfig::MscNastran,
    },
    UserSuppliedTool {
        name: "MSC Patran",
        source: "MSC Software official product page",
        licence: "Proprietary",
        url: "https://nexus.hexagon.com/home/product/patran/",
        action: "License / download",
        config: ExternalToolConfig::MscPatran,
    },
    UserSuppliedTool {
        name: "NASTRAN-95",
        source: "NASA source repository; ALAS requires a compatible local build",
        licence: "NASA Open Source Agreement 1.3",
        url: "https://github.com/nasa/NASTRAN-95",
        action: "Get source",
        config: ExternalToolConfig::Nastran95,
    },
    UserSuppliedTool {
        name: "OpenVSP / VSPAERO (main install)",
        source: "official OpenVSP project (openvsp.org)",
        licence: "NASA Open Source Agreement, as supplied by the selected release",
        url: "https://openvsp.org/download.php",
        action: "Official download",
        config: ExternalToolConfig::OpenVsp,
    },
    UserSuppliedTool {
        name: "OpenFOAM",
        source: "official OpenCFD OpenFOAM distribution (openfoam.com)",
        licence: "GPL; check the terms of the selected release",
        url: "https://www.openfoam.com/download/",
        action: "Official download",
        config: ExternalToolConfig::OpenFoam,
    },
    UserSuppliedTool {
        name: "Gmsh",
        source: "official Gmsh project (gmsh.info)",
        licence: "GPL-2.0-or-later with exception; commercial licensing available",
        url: "https://gmsh.info/",
        action: "Official download",
        config: ExternalToolConfig::Gmsh,
    },
    UserSuppliedTool {
        name: "ParaView",
        source: "official ParaView project (paraview.org)",
        licence: "BSD-3-Clause",
        url: "https://www.paraview.org/download/",
        action: "Official download",
        config: ExternalToolConfig::ParaView,
    },
    UserSuppliedTool {
        name: "FLOWUnsteady / Julia adapter",
        source: "official FLOWUnsteady project (github.com/byuflowlab/FLOWUnsteady)",
        licence: "user-supplied; follows your selected release, not assumed to be the upstream MIT notice",
        url: "https://github.com/byuflowlab/FLOWUnsteady",
        action: "View project",
        config: ExternalToolConfig::FlowUnsteady,
    },
];
