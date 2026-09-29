// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// The `mplot` timeout for the polar summary dump, native aerodynamic model's `timeout_mplot`
/// default (`mses_analysis.py` overrides only the `mset` and `mses` timeouts).
pub(super) const TIMEOUT_MPLOT_S: f64 = 10.0;

/// Maximum allowable incidence delta (degrees) for MSES continuation.
///
/// A continuation step reuses the existing mesh and flowfield (`mdat.case`) as
/// the initial guess for a different angle of attack, rather than generating a
/// fresh isentropic mesh at the target angle. MSES continuation guidance calls
/// for increments no larger than 0.5 deg in general, tightened to 0.25-0.1 deg
/// near critical Mach, where Newton under-relaxation and shock-induced
/// separation make larger jumps unreliable. This driver has no per-Mach-regime
/// branch, so 0.5 deg is used uniformly as the conservative ceiling for every
/// case; steps larger than this always fall back to a clean target-angle mesh.
pub(super) const MAX_CONTINUATION_STEP_DEG: f64 = 0.5;

/// Maximum number of bounded intermediate continuation hops one bridge
/// attempt may take between a converged state and a requested angle that is
/// farther away than [`MAX_CONTINUATION_STEP_DEG`].
///
/// This is a resource bound, not a physical one: a gap that would need more
/// hops than this never narrows the requested sweep, it only means that
/// particular gap falls back to the existing clean target-angle mesh instead
/// of a multi-hop bridge.
pub(super) const MAX_BRIDGE_HOPS: usize = 8;

/// Outcome of [`Mses::bridge_to_target`].
#[derive(Debug, Clone, Copy)]
pub(super) enum BridgeOutcome {
    /// `mdat.case` now holds a genuinely converged state within
    /// [`MAX_CONTINUATION_STEP_DEG`] of the target angle (the value carried
    /// here), so the caller may treat it as a continuation anchor.
    Reached(f64),
    /// The bridge could not get within bound of the target inside its hop
    /// and per-hop retry budget. `mdat.case` has been restored to the
    /// starting converged state where possible, so the caller must not trust
    /// it and must fall back to a clean target-angle mesh.
    Exhausted,
    /// Cancellation landed during a bridge step; no partial bridge progress
    /// is trusted, the caller should stop the sweep and keep what it has.
    Cancelled,
}

/// Outcome of [`Mses::pressure_from_checkpoint`].
pub(super) enum CheckpointAttempt {
    /// The checkpoint's flowfield was restored, bridged, and re-solved at
    /// the exact requested angle, and that solve genuinely converged.
    Solved(WorkDir, f64),
    /// Cancellation landed while trying the checkpoint anchor.
    Cancelled,
    /// The checkpoint could not be turned into a converged exact-target
    /// result (restore failure, bridge exhaustion, or a non-converged
    /// exact-target solve); the caller should fall back to the existing
    /// cold-start search rather than discard the checkpoint's evidence;
    /// there was none to discard, since nothing here was ever presented as
    /// this call's result.
    NotUsable,
}

/// Copy the working solver state aside once a point has genuinely converged.
///
/// Free functions rather than closures over `solve_sweep`'s local `dir`
/// because both `solve_sweep` and [`Mses::bridge_to_target`] need the same
/// save/restore behavior.
pub(super) fn save_converged_state(dir: &Path) -> bool {
    let src = dir.join("mdat.case");
    let dst = dir.join("mdat.case.last_converged");
    if src.is_file() {
        std::fs::copy(&src, &dst).is_ok()
    } else {
        false
    }
}

/// Restore the last genuinely converged state, undoing whatever a failed
/// solve left in `mdat.case`. Returns `false` when there is nothing to
/// restore (no point has converged yet, or the backup copy itself failed).
pub(super) fn restore_converged_state(dir: &Path) -> bool {
    let src = dir.join("mdat.case.last_converged");
    let dst = dir.join("mdat.case");
    if src.is_file() {
        std::fs::copy(&src, &dst).is_ok()
    } else {
        false
    }
}

#[derive(Debug)]
pub(super) struct MsesFailure {
    pub(super) status: MsesStatus,
    pub(super) message: String,
}

#[derive(Debug, Default)]
pub(super) struct SweepOutcome {
    pub(super) accumulated: HashMap<String, Vec<f64>>,
    pub(super) point_diagnostics: Vec<MsesPolarPointDiagnostic>,
    pub(super) solver_attempts: Vec<MsesSolverAttempt>,
    /// A reusable snapshot of each genuinely converged, successfully
    /// MPlot-extracted point, for a later warm-start pressure solve.
    pub(super) checkpoints: Vec<MsesConvergedCheckpoint>,
    /// Whether the caller stopped the sweep before all requested points ran.
    pub(super) cancelled: bool,
}

/// The process-local OSMAP resource selected for one MSES driver.
#[derive(Debug, Clone)]
pub(super) struct OsmapSelection {
    pub(super) required: bool,
    pub(super) status: MsesOsmapStatus,
    pub(super) path: Option<PathBuf>,
    pub(super) diagnostic: Option<String>,
}

/// Inspect the leading Fortran records used by the installed MSES map reader.
///
/// The first records are stable across the MIT map distributions: both files
/// begin with a 12-byte record, while the next numeric record is 224 bytes for
/// the double-precision map and 112 bytes for the single-precision map.  This
/// lightweight check deliberately rejects unknown files instead of guessing a
/// precision from file size.
pub(super) fn inspect_osmap(path: &Path) -> Result<(), String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut header = [0_u8; 24];
    file.read_exact(&mut header)
        .map_err(|error| format!("could not read OSMAP header: {error}"))?;
    let record = |offset: usize| {
        let bytes = [
            header[offset],
            header[offset + 1],
            header[offset + 2],
            header[offset + 3],
        ];
        i32::from_le_bytes(bytes)
    };
    let first = record(0);
    let first_trailing = record(16);
    let table_record = record(20);
    if first != 12 || first_trailing != 12 {
        return Err(format!(
            "unsupported Fortran record header (first={first}, trailing={first_trailing})"
        ));
    }
    if table_record != 224 {
        if table_record == 112 {
            return Err(
                "single-precision osmap.dat detected; MSES requires osmapDP.dat".to_owned(),
            );
        }
        return Err(format!(
            "unsupported OSMAP precision record length {table_record} (expected 224 for double precision)"
        ));
    }
    Ok(())
}

pub(super) fn osmap_candidate(path: PathBuf, source: &str) -> Result<PathBuf, String> {
    // MSES runs in a per-solve temporary directory.  A relative configured
    // path or `MSES_OSMAP` value would therefore resolve against that
    // temporary cwd inside the child, even though this preflight checked it
    // against ALAS's cwd.  Normalize it once before both validation and the
    // child environment so a resource cannot pass preflight and then appear
    // missing to the solver.
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    };
    if !path.is_file() {
        return Err(format!(
            "{source} OSMAP file does not exist: {}",
            path.display()
        ));
    }
    inspect_osmap(&path)
        .map(|()| path.clone())
        .map_err(|error| {
            format!(
                "{source} OSMAP file {} is incompatible: {error}",
                path.display()
            )
        })
}

/// Locate the OSMAP resource shipped with an ALAS release.
///
/// MSES itself remains a separately installed process, so its executable
/// directory is not a reliable place for a resource supplied by ALAS.  The
/// release keeps the map below `assets/mses`; search only bounded application
/// roots rather than the working directory so launching ALAS from another
/// folder cannot change which resource is selected.
fn bundled_osmap_candidates(mses_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut add_root = |root: PathBuf| {
        if !roots.iter().any(|candidate| candidate == &root) {
            roots.push(root);
        }
    };

    if let Some(root) = std::env::var_os("ALAS_APP_DIR").map(PathBuf::from) {
        add_root(root);
    }

    if let Some(root) = mses_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
    {
        // This is the release layout: <package>/external tools/MSES.
        add_root(root);
    }

    if let Ok(executable) = std::env::current_exe() {
        if let Some(root) = executable.parent() {
            add_root(root.to_path_buf());
        }
    }

    roots
        .into_iter()
        .map(|root| root.join("assets").join("mses").join("osmapDP.dat"))
        .collect()
}

pub(super) fn resolve_osmap(config: &MsesConfig, mses_dir: &Path) -> OsmapSelection {
    let required = config.xtr_upper >= 1.0 || config.xtr_lower >= 1.0;
    let explicit = config
        .osmap_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                mses_dir.join(path)
            }
        });

    // An explicit saved setting is authoritative. This prevents a typo from
    // being hidden by a different map elsewhere on the machine.
    if let Some(path) = explicit {
        return match osmap_candidate(path.clone(), "configured") {
            Ok(path) => OsmapSelection {
                required,
                status: MsesOsmapStatus::Available,
                path: Some(path),
                diagnostic: Some(
                    "configured double-precision OSMAP passed the local header check".to_owned(),
                ),
            },
            Err(error) => OsmapSelection {
                required,
                status: if path.is_file() {
                    MsesOsmapStatus::Incompatible
                } else {
                    MsesOsmapStatus::Missing
                },
                path: None,
                diagnostic: Some(error),
            },
        };
    }

    // MSES names its environment hook MSES_OSMAP. It is intentionally read
    // here and passed back only to child processes; ALAS never mutates the
    // parent process environment.
    if let Ok(value) = std::env::var("MSES_OSMAP") {
        let value = value.trim();
        if !value.is_empty() {
            let path = PathBuf::from(value);
            match osmap_candidate(path.clone(), "MSES_OSMAP") {
                Ok(path) => {
                    return OsmapSelection {
                        required,
                        status: MsesOsmapStatus::Available,
                        path: Some(path),
                        diagnostic: Some("process-local MSES_OSMAP double-precision resource passed the local header check".to_owned()),
                    };
                }
                Err(error) if path.is_file() => {
                    return OsmapSelection {
                        required,
                        status: MsesOsmapStatus::Incompatible,
                        path: None,
                        diagnostic: Some(error),
                    };
                }
                Err(_) => {}
            }
        }
    }

    let adjacent = mses_dir.join("osmapDP.dat");
    match osmap_candidate(adjacent.clone(), "adjacent") {
        Ok(path) => OsmapSelection {
            required,
            status: MsesOsmapStatus::Available,
            path: Some(path),
            diagnostic: Some(
                "adjacent double-precision osmapDP.dat passed the local header check".to_owned(),
            ),
        },
        Err(adjacent_error) => {
            for candidate in bundled_osmap_candidates(mses_dir) {
                if let Ok(path) = osmap_candidate(candidate, "bundled ALAS") {
                    return OsmapSelection {
                        required,
                        status: MsesOsmapStatus::Available,
                        path: Some(path),
                        diagnostic: Some(
                            "bundled ALAS double-precision osmapDP.dat passed the local header check".to_owned(),
                        ),
                    };
                }
            }

            OsmapSelection {
                required,
                status: if adjacent.is_file() {
                    MsesOsmapStatus::Incompatible
                } else if required {
                    MsesOsmapStatus::Missing
                } else {
                    MsesOsmapStatus::NotRequired
                },
                path: None,
                diagnostic: required.then_some(adjacent_error),
            }
        }
    }
}

impl MsesFailure {
    pub(super) fn solver(message: impl Into<String>) -> Self {
        Self {
            status: MsesStatus::Error,
            message: message.into(),
        }
    }

    pub(super) fn parse(message: impl Into<String>) -> Self {
        Self {
            status: MsesStatus::ParseFailure,
            message: message.into(),
        }
    }

    pub(super) fn from_run(error: RunError) -> Self {
        let message = error.to_string();
        let status = match &error {
            RunError::Spawn { .. } => MsesStatus::LaunchFailure,
            RunError::Timeout { .. } => MsesStatus::Timeout,
            RunError::Io { .. } | RunError::Reader { .. } | RunError::InvalidTimeout { .. } => {
                MsesStatus::Error
            }
            RunError::Cancelled { .. } => MsesStatus::Error,
        };
        Self { status, message }
    }
}

pub(super) fn cancelled(error: &MsesFailure) -> bool {
    error.message.contains("was cancelled and terminated")
}
