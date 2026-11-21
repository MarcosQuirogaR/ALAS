// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a built-from-source tool *is*: its source revision plus everything that
//! shapes the binary (the embedded patch, the toolchain archives, the build
//! commands). A change to any of them must read as "not installed" rather than
//! leaving an older build standing under the same revision.

use crate::sha256::Sha256;

use super::build::patch_sha256;
use super::manifest::{Artifact, InstallKind, ToolEntry};

/// Hex SHA-256 over the recipe of `tool` for `platform`.
fn recipe_hash(tool: &ToolEntry, platform: &str) -> Option<String> {
    let build = tool.build.as_ref()?;
    let mut hasher = Sha256::new();
    hasher.update_field(patch_sha256(build).unwrap_or_default().as_bytes());
    for part in build.toolchain.get(platform).into_iter().flatten() {
        hasher.update_field(part.url.as_bytes());
        hasher.update_field(part.sha256.as_deref().unwrap_or("").as_bytes());
    }
    for field in [
        serde_json::to_string(&build.steps.get(platform)),
        serde_json::to_string(&build.path_dirs.get(platform)),
        serde_json::to_string(&build.keep),
        serde_json::to_string(&build.check),
    ] {
        hasher.update_field(field.unwrap_or_default().as_bytes());
    }
    Some(hasher.finalize_hex())
}

/// The version string recorded for an install and compared when planning: the
/// publisher's version, plus a short recipe hash for a tool built from source.
pub(super) fn installed_version(tool: &ToolEntry, artifact: &Artifact, platform: &str) -> String {
    if tool.kind != InstallKind::BuildFromSource {
        return artifact.version.clone();
    }
    recipe_hash(tool, platform).map_or_else(
        || artifact.version.clone(),
        |hash| format!("{}+{}", artifact.version, &hash[..12]),
    )
}

/// After an uninstall: where unpacked build toolchains stay (reused by the next
/// build), when there are any. The caller shows it so the space can be reclaimed.
pub fn toolchain_leftover_note(data_root: &std::path::Path) -> Option<String> {
    let dir = data_root.join("toolchains");
    let has_content = std::fs::read_dir(&dir).ok()?.next().is_some();
    has_content.then(|| {
        format!(
            "Unpacked build toolchains stay in {} for the next build (they can be large); delete that folder to reclaim the space.",
            dir.display()
        )
    })
}
