// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! User add-ons for ALAS: small calculators that read the design, produce
//! numbers, and may write them back to the design under the user's control.
//!
//! An add-on is a folder holding `addon.toml` ([`AddonManifest`]) and an entry
//! file. Two kinds exist:
//!
//! - `script`: a Rhai script evaluated in-process inside a sandbox
//!   ([`script`]): no file, network or process access, bounded time and size.
//! - `process`: an external program speaking JSON over stdin/stdout
//!   ([`process`]). Not sandboxed; it must be enabled per add-on.
//!
//! This crate is the GUI-free core. Outputs that name a target are applied
//! through [`apply::plan`] and [`apply::apply`], and recorded as
//! "set by add-on <name> <version>".

// Unit tests assert that their constructed fixture inputs are valid.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod apply;
pub mod discovery;
mod error;
pub mod manifest;
pub mod process;
pub mod script;
pub mod settings;

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::Value;

pub use discovery::{discover, install_examples, Discovery, InstalledAddon};
pub use error::AddonError;
pub use manifest::{AddonKind, AddonManifest};
pub use settings::AddonSettings;

/// Largest entry script accepted, bytes.
const MAX_SCRIPT_BYTES: u64 = 256 * 1024;

/// What one run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct RunOutcome {
    /// Every declared output, in manifest order of declaration.
    pub outputs: BTreeMap<String, f64>,
    /// Messages returned by the add-on.
    pub messages: Vec<String>,
    /// The add-on log: script `print` output or process stderr.
    pub log: String,
}

/// Fill defaults, reject unknown or non-finite inputs.
pub fn resolve_inputs(
    manifest: &AddonManifest,
    given: &BTreeMap<String, f64>,
) -> Result<BTreeMap<String, f64>, AddonError> {
    if let Some(unknown) = given
        .keys()
        .find(|name| !manifest.inputs.iter().any(|spec| &spec.name == *name))
    {
        return Err(AddonError::Inputs(format!("unknown input `{unknown}`")));
    }
    let mut resolved = BTreeMap::new();
    for spec in &manifest.inputs {
        let value = given.get(&spec.name).copied().unwrap_or(spec.default);
        if !value.is_finite() {
            return Err(AddonError::Inputs(format!(
                "input `{}` must be a finite number",
                spec.name
            )));
        }
        resolved.insert(spec.name.clone(), value);
    }
    Ok(resolved)
}

/// Keep exactly the declared outputs; each must be present and finite.
fn check_outputs(
    manifest: &AddonManifest,
    mut raw: BTreeMap<String, f64>,
    messages: &mut Vec<String>,
) -> Result<BTreeMap<String, f64>, AddonError> {
    let mut outputs = BTreeMap::new();
    for spec in &manifest.outputs {
        let value = raw
            .remove(&spec.name)
            .ok_or_else(|| AddonError::Outputs(format!("missing output `{}`", spec.name)))?;
        if !value.is_finite() {
            return Err(AddonError::Outputs(format!(
                "output `{}` is not finite",
                spec.name
            )));
        }
        outputs.insert(spec.name.clone(), value);
    }
    if !raw.is_empty() {
        let names: Vec<_> = raw.keys().cloned().collect();
        messages.push(format!("ignored undeclared outputs: {}", names.join(", ")));
    }
    Ok(outputs)
}

/// Run an installed add-on.
///
/// `design` is the read-only design summary handed to the add-on. A disabled
/// add-on is refused here, not only in the GUI.
pub fn run_addon(
    addon: &InstalledAddon,
    inputs: &BTreeMap<String, f64>,
    design: &Value,
    settings: &AddonSettings,
) -> Result<RunOutcome, AddonError> {
    let manifest = &addon.manifest;
    if !settings.is_enabled(&addon.id, &manifest.version, manifest.kind) {
        return Err(AddonError::Process(format!(
            "add-on `{}` is not enabled",
            manifest.name
        )));
    }
    let inputs = resolve_inputs(manifest, inputs)?;
    let timeout = Duration::from_secs_f64(manifest.timeout_seconds());
    let mut messages = Vec::new();
    let (raw, log) = match manifest.kind {
        AddonKind::Script => {
            let path = addon.dir.join(&manifest.entry);
            let size = std::fs::metadata(&path)
                .map_err(|error| AddonError::Io(format!("{}: {error}", path.display())))?
                .len();
            if size > MAX_SCRIPT_BYTES {
                return Err(AddonError::Script(format!(
                    "script is {size} bytes; the limit is {MAX_SCRIPT_BYTES}"
                )));
            }
            let source = std::fs::read_to_string(&path)
                .map_err(|error| AddonError::Io(format!("{}: {error}", path.display())))?;
            script::run_script(
                &source,
                &inputs,
                design,
                script::ScriptLimits::with_timeout(timeout),
            )?
        }
        AddonKind::Process => {
            let launch = process::ProcessSettings {
                python: settings.python_path(),
            };
            let result = process::run_process(
                &addon.dir,
                &manifest.entry,
                &inputs,
                design,
                &launch,
                timeout,
            )?;
            messages = result.messages;
            (result.outputs, result.stderr)
        }
    };
    let outputs = check_outputs(manifest, raw, &mut messages)?;
    Ok(RunOutcome {
        outputs,
        messages,
        log,
    })
}
