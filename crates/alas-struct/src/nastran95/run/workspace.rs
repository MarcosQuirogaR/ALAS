// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Staging the solver work directory and the rigid-format library it reads.

use super::*;

/// Reject legacy `DOSNAM` paths before NASTRAN silently truncates a filename
/// and writes its log under an unrelated one-character name.
pub(super) fn validate_dosnam_paths(work_dir: &Path) -> Result<(), String> {
    let names = [
        "scr", "run.log", "run.optp", "run.nptp", "run.dic", "run.pch",
    ];
    for name in names {
        let path = work_dir.join(name);
        let displayed = short(&path);
        let bytes = displayed.len();
        if bytes > MAX_DOSNAM_PATH_BYTES {
            return Err(format!(
                "NASTRAN-95 work path is {bytes} bytes, but DOSNAM permits at most \
                 {MAX_DOSNAM_PATH_BYTES}: {displayed}. Configure a short RF staging directory \
                 so ALAS can place the transient run beside it."
            ));
        }
    }
    Ok(())
}

/// Validate the allocation before staging a run. The Fortran executable exits
/// with code zero after printing its own cap message, which otherwise looks
/// like a missing-result failure to the caller.
pub(super) fn open_core_words(configured: Option<&str>, maximum: u64) -> Result<String, String> {
    let owned_default;
    let raw = match configured {
        Some(value) => value,
        None => {
            owned_default = maximum.to_string();
            &owned_default
        }
    };
    let words = raw.parse::<u64>().map_err(|error| {
        format!("NASTRAN-95 OCMEM must be a positive integer number of words, got {raw:?}: {error}")
    })?;
    if words == 0 || words > maximum {
        return Err(format!(
            "NASTRAN-95 OCMEM must be between 1 and {maximum} words for this local build, got {words}. Rebuild nastran.exe with a larger COMMON /ZZZZZZ/ allocation to run a larger mesh."
        ));
    }
    Ok(words.to_string())
}

/// Read the limit emitted beside a current executable. A missing or malformed
/// marker means this is an older build whose 14M-word COMMON allocation is the
/// only trustworthy contract.
pub(super) fn compiled_open_core_limit(executable: &Path) -> u64 {
    executable
        .parent()
        .and_then(|dir| std::fs::read_to_string(dir.join(OPEN_CORE_LIMIT_FILE)).ok())
        .and_then(|text| text.trim().parse::<u64>().ok())
        .filter(|words| *words > 0)
        .unwrap_or(LEGACY_MAX_OPEN_CORE_WORDS)
}

/// Pick the directory holding copied rigid formats for one run.
///
/// Keeping the default beneath `work_dir` makes a short standalone run wholly
/// self-contained. A caller that stores result artifacts deep under a user
/// output tree can instead set `ALAS_NASTRAN95_RF_STAGE` to an ALAS-owned short
/// absolute directory; only copied rigid formats and the patched `NASINFO` land
/// there.
pub(super) fn rigid_format_stage_dir(
    work_dir: &Path,
    configured: Option<&Path>,
) -> Result<PathBuf, String> {
    match configured {
        Some(path) if path.is_absolute() => Ok(path.to_path_buf()),
        Some(path) => Err(format!(
            "NASTRAN-95 RF staging directory must be absolute, got {}",
            path.display()
        )),
        None => Ok(work_dir.join("rf")),
    }
}

/// Resolve before `Command::current_dir` changes how a relative environment path
/// would be interpreted by the Fortran executable.
pub(super) fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|error| {
            format!(
                "cannot resolve NASTRAN-95 work directory {}: {error}",
                path.display()
            )
        })
}

/// Reject a rigid-format path that would truncate `AERO10` inside `RFOPEN`.
pub(super) fn validate_rigid_format_stage(rf: &Path) -> Result<(), String> {
    let displayed = short(rf);
    let bytes = displayed.len();
    if bytes <= MAX_RIGID_FORMAT_DIR_BYTES {
        return Ok(());
    }
    Err(format!(
        "NASTRAN-95 rigid-format directory is {bytes} bytes, but RFOPEN permits at most \
         {MAX_RIGID_FORMAT_DIR_BYTES}; use a shorter work directory or set \
         ALAS_NASTRAN95_RF_STAGE to a dedicated short directory (got {displayed})"
    ))
}

/// Empty `work_dir`, create its scratch, and stage `rf/` with the timing
/// constants switched on.
pub(super) fn stage(solver: &Nastran95Solver, work_dir: &Path, rf: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_dir_all(work_dir);
    std::fs::create_dir_all(work_dir.join("scr"))?;
    std::fs::create_dir_all(rf)?;
    for entry in std::fs::read_dir(&solver.rf_source)? {
        let entry = entry?;
        let target = rf.join(entry.file_name());
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    // Set TIM = 16 in section two so the run uses the sixteen GINO timing
    // constants NASINFO already ships and skips the TMTSIO benchmark.
    let nasinfo = solver.rf_source.join("NASINFO");
    let text = std::fs::read_to_string(&nasinfo)?;
    let patched = text.replace("TIM =    -99 ", "TIM =     16 ");
    std::fs::write(rf.join("NASINFO"), patched)?;
    Ok(())
}

/// A path as the string the solver's `GETENV` reads, with the extended-length
/// prefix stripped: the classic scanner does not accept it.
pub(super) fn short(path: &Path) -> String {
    let text = path.display().to_string();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}
