// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CPACS 3.5 `toolspecific` extension for electric UAV operating data.
//!
//! CPACS 3.5 provides the `toolspecific/tool` extension point for data that
//! the base schema does not standardize.  The native UAV layer uses that
//! extension point rather than relabeling battery current or a propeller map
//! as a turbofan field.  The caller supplies a schema location that is shipped
//! alongside its CPACS document, then this module injects a namespace-bound
//! payload into an existing CPACS 3.5 document.

include!("cpacs35_parts/part_01.rs");
include!("cpacs35_parts/part_02.rs");
