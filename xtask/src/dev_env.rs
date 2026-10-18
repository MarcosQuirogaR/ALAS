// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `cargo xtask dev-env`: the development-shell environment that shortens the
//! edit-build-test loop (rust-lld linker, an optional per-lane target
//! directory, optional sccache), printed for the shell to evaluate or applied
//! to one command.
//!
//! None of it changes numerical results; it is not set in the repository so
//! that release packaging keeps the reviewed linker. A child process cannot
//! change its parent's environment, hence the print form:
//!
//! ```text
//! cargo xtask dev-env | Invoke-Expression                     # PowerShell
//! eval "$(cargo xtask dev-env --shell sh)"                     # POSIX shell
//! cargo xtask dev-env --run -- cargo build --profile harness   # one command
//! ```

use std::process::Command;

const USAGE: &str =
    "cargo xtask dev-env [--shell powershell|sh] [--target-dir DIR] [--sccache] [--run -- CMD...]";

/// The variables to set, in order, as `(name, value)`.
pub(crate) fn variables(
    windows: bool,
    target_dir: Option<&str>,
    sccache: bool,
) -> Vec<(String, String)> {
    let mut vars = Vec::new();
    if windows {
        // rust-lld ships with the toolchain and links test binaries about
        // twice as fast as link.exe (touch-config rebuild 204 s -> 107 s).
        vars.push((
            "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER".to_owned(),
            "rust-lld".to_owned(),
        ));
    }
    if let Some(dir) = target_dir {
        vars.push(("CARGO_TARGET_DIR".to_owned(), dir.to_owned()));
    }
    if sccache {
        // Incremental compilation and sccache exclude each other.
        vars.push(("RUSTC_WRAPPER".to_owned(), "sccache".to_owned()));
        vars.push(("CARGO_INCREMENTAL".to_owned(), "0".to_owned()));
    }
    vars
}

fn render(vars: &[(String, String)], powershell: bool) -> String {
    let mut out = String::new();
    for (name, value) in vars {
        if powershell {
            out.push_str(&format!("$env:{name} = '{}'\n", value.replace('\'', "''")));
        } else {
            out.push_str(&format!(
                "export {name}='{}'\n",
                value.replace('\'', "'\\''")
            ));
        }
    }
    out
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut powershell = cfg!(windows);
    let mut target_dir = None;
    let mut sccache = false;
    let mut command: Option<Vec<String>> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--shell" => match it.next().map(String::as_str) {
                Some("powershell") => powershell = true,
                Some("sh") => powershell = false,
                _ => return Err(format!("--shell requires powershell or sh\n{USAGE}")),
            },
            "--target-dir" => {
                target_dir = Some(it.next().ok_or_else(|| USAGE.to_owned())?.clone());
            }
            "--sccache" => sccache = true,
            "--run" => {
                let rest: Vec<String> = it
                    .by_ref()
                    .skip_while(|a| a.as_str() == "--")
                    .cloned()
                    .collect();
                if rest.is_empty() {
                    return Err(format!("--run requires a command\n{USAGE}"));
                }
                command = Some(rest);
            }
            other => return Err(format!("unrecognized argument '{other}'\n{USAGE}")),
        }
    }
    let vars = variables(cfg!(windows), target_dir.as_deref(), sccache);
    let Some(command) = command else {
        print!("{}", render(&vars, powershell));
        return Ok(());
    };
    let status = Command::new(&command[0])
        .args(&command[1..])
        .envs(vars.iter().map(|(k, v)| (k, v)))
        .status()
        .map_err(|e| format!("cannot run {}: {e}", command[0]))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} exited with {status}", command[0]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_selects_lld_and_other_hosts_set_nothing() {
        let win = variables(true, None, false);
        assert_eq!(win.len(), 1);
        assert_eq!(win[0].1, "rust-lld");
        assert!(variables(false, None, false).is_empty());
    }

    #[test]
    fn sccache_disables_incremental_and_dirs_are_quoted() {
        let vars = variables(false, Some("C:/it's"), true);
        let names: Vec<&str> = vars.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            ["CARGO_TARGET_DIR", "RUSTC_WRAPPER", "CARGO_INCREMENTAL"]
        );
        assert!(render(&vars, true).contains("'C:/it''s'"));
        assert!(render(&vars, false).contains("export CARGO_INCREMENTAL='0'"));
    }
}
