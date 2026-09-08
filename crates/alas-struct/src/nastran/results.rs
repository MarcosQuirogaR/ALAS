// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/integration/nastran_runner.py (_read_static, _read_modes,
// _read_vibration and the four result dataclasses).
// Reference: alas @ rust-port-baseline.

//! What a solve produced, read back off the result file.
//!
//! Three readers, one per solution, each reducing an OP2 table to the handful
//! of numbers this program reports: the tip deflection and peak root stress of
//! each static load case, the elastic mode frequencies and their front-spar
//! shapes, and the frequency response at the four monitor grids.
//!
//! Almost all of the content here is the reader's behaviour when something is
//! *absent*. That is not defensiveness -- it is the normal case. A solve can
//! write some subcases and not others, a `.op2` can carry displacements and no
//! stress table, a monitor grid can be missing from the result, and every mode
//! a modal solve finds can be a rigid-body mode below the reporting threshold.
//! None of those is an error: each one means a smaller answer, and upstream
//! returns exactly that. The parity fixture is built scenario by scenario
//! around these branches for the same reason.
//!
//! Two shapes differ from upstream's, both because Python's dictionaries carry
//! ordering that a `BTreeMap` would silently reorder:
//!
//! * The label-keyed results are [`LabelledValues`], an insertion-ordered list
//!   of pairs. Upstream's `tip_deflection_m` comes out in load-case order and
//!   its `miles_rms_m` in monitor order -- root, kink, engine, tip -- and both
//!   are read back in that order by anything that reports them. Sorted by name
//!   they would read `engine, kink, root, tip`, which is nothing.
//! * The mode-shape reader returns `None` rather than an empty vector for
//!   `mode_shape_y_m`, because upstream distinguishes them: `None` means it
//!   never got as far as building a shape.

include!("results_parts/part_01.rs");
include!("results_parts/part_02.rs");
