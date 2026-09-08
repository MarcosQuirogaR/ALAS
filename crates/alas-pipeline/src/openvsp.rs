// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! OpenVSP geometry interchange through its supported AngelScript API.
//!
//! A `.vsp3` file is OpenVSP's private, versioned XML serialization. Writing
//! that XML without OpenVSP would couple this application to implementation
//! details that have changed between releases. The supported script API is a
//! stable OpenVSP input format: this exporter writes a `.vspscript` that builds
//! the computed aircraft and calls `WriteVSPFile`, producing the adjacent
//! `.vsp3` with the installed OpenVSP version's own serializer.

include!("openvsp_parts/part_01.rs");
include!("openvsp_parts/part_02.rs");
