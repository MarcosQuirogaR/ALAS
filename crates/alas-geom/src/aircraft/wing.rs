// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from reference geometry/geometry/wing.py
// Upstream: reference geometry 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! reference geometry's `Wing` and `WingXSec`, scoped to the surface
//! `alas-geom::builder`, `alas-mass::torenbeek` and `alas-stab::trim` (all
//! later modules) actually call: `.translate(...)`, `.subdivide_sections(...)`,
//! `.area()`, `.span()`, `.mean_aerodynamic_chord()`, `.aerodynamic_center()`
//! (indexed `[0]` afterward), `.aspect_ratio()`, `.taper_ratio()`,
//! `.mean_sweep_angle(x_nondim)`, `.control_surface_area()`, the
//! `xsecs`/`symmetric`/`name` fields, and `WingXSec.twist` mutated in place by
//! the later trim phase. `docs/PORTING.md` records the scoping decision and a
//! prior grep across all of `alas/`, not just `alas/geometry/`, for what
//! reaches these classes.
//!
//! # `subdivide_sections`'s spacing function
//!
//! Upstream's `subdivide_sections` takes a `spacing_function` (default
//! `np.linspace`), hardcoded here until `VortexLatticeMethod.run()` reached it
//! with `spanwise_spacing_function` (default `np.cosspace`). So the method now
//! takes a [`SpacingFunction`]: `alas-aero::vlm` passes
//! [`SpacingFunction::Cosspace`], `alas-geom::builder`'s three prior sites pass
//! [`SpacingFunction::Linspace`], reproducing its behavior bit for bit.
//!
//! Left untranslated, because nothing in this program's inputs reaches them:
//! `is_entirely_symmetric`, `mean_geometric_chord`, `mean_twist_angle`,
//! `mean_dihedral_angle`, `volume`, every other
//! control-surface method (`get_control_surface_names`,
//! `set_control_surface_deflections`), `mesh_body`, `draw*`,
//! `_compute_frame_of_section` (meshing only), `xsec_area`, and every
//! non-default argument of the methods that are translated (`type=`,
//! `_sectional=` as a public parameter, `include_centerline_distance=`,
//! `control_surface_area`'s `by_name=`). `mesh_thin_surface` and `mesh_line`,
//! also meshing, but reached through reference geometry's own
//! `VortexLatticeMethod.run()` rather than through this program's own
//! source, are translated in [`super::mesh`], not here.
//!
//! [`Wing::control_surface_area`] always returns `0.0`: `WingXSec` here has no
//! `control_surfaces` field at all (see above), so upstream's summing loop is
//! always empty: the same result its formula gives a wing with none defined.
//!
//! # Airfoil identity vs. structural equality
//!
//! [`Wing::subdivide_sections`] branches on whether two adjacent `WingXSec`s
//! share the same airfoil, which upstream tests with Python's default
//! `__eq__`: object identity, true exactly when the same `Airfoil` object
//! was passed to both constructors. This crate's `Airfoil` values are owned,
//! not shared references, so there is no identity to compare; this port uses
//! [`Airfoil`]'s `#[derive(PartialEq)]` (structural equality: same name, same
//! coordinates) instead. That is a faithful translation of the *reachable*
//! behavior, not a `deviation-candidate`: every call site either passes the
//! literal same `Airfoil` value to both `WingXSec`s (the main wing's
//! root/break interval, the hstab/vstab ends) or two that are never
//! coordinate-identical (the main wing's break/tip), so structural and
//! identity equality agree on every input this program constructs.
//!
//! # `aerodynamic_center`'s un-rotated chordwise offset
//!
//! [`Wing::aerodynamic_center`] adds `chord_fraction * section_MAC_length`
//! straight onto the X axis without rotating it by the section's twist:
//! upstream's own `# TODO`. Reproduced exactly; `docs/PORTING.md` records it as
//! a `deviation-candidate`.
//!
//! # `theoretical_reference_mac`
//!
//! Not part of the upstream port: [`Wing::theoretical_reference_mac`]
//! (`wing_parts/part_04.rs`) is a native addition computing the
//! manufacturer/TCDS "theoretical wing" MAC convention, distinct from the
//! ported [`Wing::mean_aerodynamic_chord`]'s physical planform integral. See
//! that method's doc comment and an internal MAC-parity derivation study
//! (2026-09-09) for the evidence it is grounded in.

include!("wing_parts/part_01.rs");
include!("wing_parts/part_02.rs");
include!("wing_parts/part_03.rs");
include!("wing_parts/part_04.rs");
