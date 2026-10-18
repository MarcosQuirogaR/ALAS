// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Embedded competition presets (one JSON file per rulebook edition).
//!
//! The files live in `score/presets/` and are the data of record: a new
//! edition is a new file plus a review of the transcription, not a code
//! change. Each file's `citation` names the rule document, its version and
//! the sections used; `status` is `provisional` where the primary document
//! was not available or is known to be incomplete.

use crate::spec::score::Preset;

use super::ScoreError;

const FILES: &[(&str, &str)] = &[
    ("acc-2026", include_str!("presets/acc-2026.json")),
    ("acc-2024", include_str!("presets/acc-2024.json")),
    ("xc-2025-clubs", include_str!("presets/xc-2025-clubs.json")),
    ("xc-2023", include_str!("presets/xc-2023.json")),
    (
        "sae-regular-2026",
        include_str!("presets/sae-regular-2026.json"),
    ),
    (
        "sae-micro-2026",
        include_str!("presets/sae-micro-2026.json"),
    ),
    ("dbf-2026", include_str!("presets/dbf-2026.json")),
];

/// Ids of the embedded presets.
pub fn ids() -> Vec<&'static str> {
    FILES.iter().map(|f| f.0).collect()
}

/// Load one embedded preset by id.
pub fn load(id: &str) -> Result<Preset, ScoreError> {
    let (_, text) = FILES
        .iter()
        .find(|f| f.0 == id)
        .ok_or_else(|| ScoreError::Preset(format!("no embedded preset '{id}'")))?;
    let preset: Preset = serde_json::from_str(text)
        .map_err(|e| ScoreError::Preset(format!("{id}: invalid JSON: {e}")))?;
    if preset.id != id {
        return Err(ScoreError::Preset(format!(
            "{id}: file declares id '{}'",
            preset.id
        )));
    }
    Ok(preset)
}

/// Load every embedded preset.
pub fn load_all() -> Result<Vec<Preset>, ScoreError> {
    FILES.iter().map(|f| load(f.0)).collect()
}
