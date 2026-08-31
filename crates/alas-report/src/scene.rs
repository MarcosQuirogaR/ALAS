// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Backend-neutral 2D/3D scene description, coordinate transforms, and cameras.

// SceneElement's public fields are documented by their variant contracts; this keeps the graph under the repository's source-size limit.
#![allow(missing_docs)]

include!("scene_parts/part_01.rs");
include!("scene_parts/part_02.rs");
