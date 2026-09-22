// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Aerodynamic analysis: the in-process and mission vortex-lattice methods,
//! the airfoil and lift surrogates, and the parasite/wave-drag buildup.
//!
//! [`operating_point`] carries the instantaneous flight state: true
//! airspeed, angle of attack, sideslip, the three body-axis rotation rates,
//! and the axis-conversion machinery every VLM solve needs to turn that state
//! into a freestream velocity and an induced-rotation field at a set of mesh
//! points, and to move a force between the geometry, body and wind frames a
//! solve and a caller each want it in.
//!
//! [`singularities`] is the potential-flow element [`vlm`] panels every
//! wing into: the induced velocity of a single horseshoe vortex, closed-form
//! and stateless.
//!
//! [`vlm`] is the in-process vortex-lattice solver: it meshes an
//! `alas-geom::aircraft::airplane::Airplane`'s wings into quad panels, assembles
//! the dense influence (AIC) matrix from [`singularities`], solves for each
//! panel's circulation strength, and integrates the near-field force and
//! moment. `alas-aero::vorlax` is the separately queued mission vortex-lattice
//! kernel, and the lift surrogate and drag buildup are not part of this row.
//!
//! [`kulfan`] is the airfoil surrogate's front half: the Kulfan (CST) shape
//! parameterization and the least-squares fit that reduces a few hundred
//! airfoil vertices to it, which is the only input form NeuralFoil accepts.
//!
//! [`neuralfoil`] is the surrogate itself: a trained network shipped as
//! embedded parameters, wrapped in the post-stall blending and the transonic
//! schedule that keep it sensible outside the incompressible, attached
//! conditions it was trained on. It is what makes the airfoil screening sweep
//! possible at all: a few hundred thousand two-dimensional solves is not a
//! panel method's problem.
//!
//! [`analysis`] is the hybrid engine this program's
//! `alas/physics/aerodynamics.py` owns: it drives [`vlm`] for lift,
//! induced drag and pitching moment, and supplies the two things an inviscid
//! solve cannot see: the Raymer parasite-drag buildup and the Korn
//! transonic rise. It is the only module here that reads configuration, which
//! is why this crate depends on `alas-config`. Not to be confused with
//! `alas-aero::drag_buildup`, the mission-only buildup, which is P6 and
//! reachable only from inside the mission network.
//!
//! [`vorlax`] is the *other* vortex lattice, reached only from inside the
//! mission network. It panels
//! differently, imposes its boundary condition differently, and runs its
//! influence kernel in `f32` where [`vlm`] runs in `f64`. The two are
//! deliberately not unified, comparing their answers is a result the
//! program is entitled to report: the same relationship
//! `alas-prop::mission_turbofan` has to `alas-prop::cycle`.
//!
//! [`lift_surrogate`] is what the mission actually flies on. Nothing in a
//! mission segment calls [`vorlax`]: the mission lift analysis
//! runs it once on a fixed ten-by-eight grid of angles of attack and Mach
//! numbers, fits a bicubic spline through the result, and every later lift
//! and induced-drag number is an evaluation of that spline. It is also what
//! closes [`drag_buildup`]'s open input, which takes the per-wing lift
//! solution as data because there is no closed form for it on this path.
//!
//! [`mses`] is the one external-solver row here (P9, not P5): it drives Mark
//! Drela's compiled `mset`/`mses`/`mplot` binaries to run a real coupled
//! viscous/inviscid 2-D section solve, for the model-comparison view. It spawns
//! through `alas-exec` and computes nothing itself: the numbers come out of
//! the binary, which is why it is checked at `exact` and stands apart from the
//! arithmetic modules above.
//!
//! [`vspaero`] defines the independent three-dimensional VSPAERO protocol:
//! SI reference quantities, solver and frame metadata, native setup rendering,
//! and strict native `.polar` parsing. Process execution remains in
//! `alas-exec`; this crate does not substitute an in-process model when the
//! installed solver is unavailable.
//!
//! [`fourier_lifting_line`] is the independent non-VLM aircraft-level check:
//! odd-harmonic Prandtl collocation on the actual projected chord, twist, and
//! airfoil camber of every symmetric lifting surface. It predicts lift and
//! Trefftz-plane induced drag without reading either vortex solver or the
//! fitted aircraft polar.

pub mod analysis;
pub mod vlm;
/// Compatibility alias for unpublished parity fixtures and older callers.
#[doc(hidden)]
pub use vlm as asb_vlm;
pub mod avl;
pub mod drag_buildup;
pub mod flowunsteady;
pub mod fourier_lifting_line;
pub mod kulfan;
pub mod lift_surrogate;
pub mod lifting_line;
pub mod mses;
pub mod neuralfoil;
pub mod operating_point;
pub mod singularities;
mod vector3;
pub mod vorlax;
pub mod vspaero;
pub mod wing_analysis;
