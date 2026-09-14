// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from native aerodynamic model/aerodynamics/aero_2D/mses.py (the input decks and
// keystroke templates MSES.run writes), scoped to what
// alas/physics/mses_analysis.py drives.
// Upstream: native aerodynamic model 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! The text `mset`/`mses`/`mplot` read: the `mses.case` operating-point deck
//! and the menu keystroke scripts.
//!
//! Every string here has to be byte-identical to what native aerodynamic model's `MSES`
//! wrapper writes, because the binaries parse fixed columns and a menu driven
//! by exact keystrokes -- a deck that differs by one space or one digit meshes
//! or solves a different problem, and the `exact`-tier parity test would catch
//! it as forty wrong numbers rather than one wrong line. The one non-obvious
//! part is [`py_float`]: the deck embeds its floats through Python's f-string,
//! which is `repr(float)`, so a whole number reads `9.0`, not `9`.
//!
//! Four constructor arguments native aerodynamic model exposes are never overridden by
//! `mses_analysis.py`, so they are baked in here at native aerodynamic model's own defaults:
//! the `mset` inlet/outlet grid index [`MSET_IO`] and leading-edge clustering
//! [`MSET_X`], and the `mses` critical Mach [`MSES_MCRIT`] and artificial-
//! dissipation constant. `mset_alpha` (the angle the initial
//! mesh is generated at) is passed per call, as native aerodynamic model passes it.

/// MSET inlet/outlet nodes, native aerodynamic model's `mset_io` default.
///
/// These are streamwise far-field boundary nodes, distinct from `N` surface
/// nodes. MSET derives the final domain and may adjust point counts to avoid
/// sheared cells; the raw MPlot field is the authoritative domain record.
const MSET_INLET_OUTLET_POINTS: i64 = 37;
/// The `mset` leading-edge grid clustering parameter (`mset_x`), the default.
const MSET_X: f64 = 0.850;
/// The `mses` critical Mach number (`mses_mcrit`), native aerodynamic model's default.
const MSES_MCRIT: f64 = 0.99;
/// The legacy deck's artificial-dissipation constant.
///
/// The frozen Python parity fixture was generated with a negative value,
/// which disables MSES second-order dissipation. Product runs pass the
/// configured value through [`mses_case_with_mucon`] and default to `1.0`,
/// the normal setting in the MSES user guide. Only the parity fixture reads
/// this form, so it is compiled with the tests.
#[cfg(test)]
const LEGACY_MSES_MUCON: f64 = -1.0;

/// The keystrokes that dump the polar summary to `mplot`'s stdout (menu path
/// `1` -> `12` -> `0` -> `0`), which [`super::parse`] then reads.
pub const MPLOT_POLAR_KEYSTROKES: &str = "1\n12\n0\n0\n";

/// A float formatted as Python's `repr` renders it -- the form its f-strings
/// embed in a deck.
///
/// Rust's `{}` already produces the shortest decimal that round-trips, which is
/// what `repr` produces too, but drops the trailing `.0` on a whole number
/// where `repr` keeps it (`9` versus `9.0`). Appending it when no fractional
/// part, exponent or non-finite marker is present recovers `repr`. Scoped to
/// the normal-magnitude values a deck carries: `repr` switches to exponent
/// notation for `|x| >= 1e16` or `0 < |x| < 1e-4`, which no Mach, angle,
/// Reynolds number or transition station reaches, and Rust's `{}` never emits
/// exponent notation, so the two would diverge only outside that range.
pub fn py_float(x: f64) -> String {
    let s = format!("{x}");
    if s.contains(['.', 'e', 'E']) || !x.is_finite() {
        s
    } else {
        format!("{s}.0")
    }
}

/// The `mset` mesh-generation keystrokes, meshing at `mset_alpha`.
pub fn mset_keystrokes(mset_n: i64, mset_e: f64, mset_alpha: f64) -> String {
    format!(
        "15\ncase\n7\nn {mset_n}\ne {mset_e}\ni {io}\no {io}\nx {x}\n\n1\n{alpha}\n2\n\n3\n4\n0\n",
        mset_e = py_float(mset_e),
        io = MSET_INLET_OUTLET_POINTS,
        x = py_float(MSET_X),
        alpha = py_float(mset_alpha),
    )
}

/// Build the named `blade.<case>` geometry file consumed by MSET.
///
/// A Selig `.dat` file contains only a name and coordinates, while MSET's
/// named blade format has a second line for the far-field boundaries. The
/// distinction matters for arbitrary optimized sections: passing a Selig file
/// as the case name makes MSET look for `blade.<case>` and can leave the solver
/// without a valid mesh. Keep the boundary values explicit and stable, as in
/// the reference MSES wrapper.
pub fn mset_blade(airfoil_name: &str, coordinates: &[(f64, f64)]) -> String {
    let name = airfoil_name.chars().take(32).collect::<String>();
    let mut text = format!("{name:<32}\n  -2.0    3.0    -2.5    3.5\n");
    for &(x, y) in coordinates {
        text.push_str(&format!("{x:14.6} {y:14.6}\n"));
    }
    text
}

/// The `mses` solve keystrokes: run up to `max_iter` Newton iterations, then
/// exit the menu.
pub fn mses_keystrokes(max_iter: i64) -> String {
    format!("{max_iter}\n0\n")
}

/// The keystrokes that dump `mplot`'s menu `option` (12 = boundary layers,
/// 11 = flowfield) to `filename` and exit.
pub fn mplot_dump_keystrokes(option: i64, filename: &str) -> String {
    format!("{option}\n{filename}\n0\n0\n")
}

/// The `mses.case` operating-point deck for one angle of attack.
///
/// The fixed-width lines and their trailing comment columns are reproduced
/// verbatim; only the five leading numbers vary. `CLIFin` is always `0.0`
/// (this program solves at prescribed alpha, never prescribed lift), matching
/// the `ISMOM 3` / `IFFBC 2` momentum and far-field choices on the line below.
///
/// The frozen fixture's form, with the legacy dissipation setting; product
/// decks come from [`mses_case_with_mucon`], so this is compiled with the
/// parity tests only.
#[cfg(test)]
pub fn mses_case(
    mach: f64,
    alpha: f64,
    reynolds: f64,
    n_crit: f64,
    xtr_lower: f64,
    xtr_upper: f64,
) -> String {
    mses_case_with_mucon(
        mach,
        alpha,
        reynolds,
        n_crit,
        xtr_lower,
        xtr_upper,
        LEGACY_MSES_MUCON,
    )
}

/// Build an operating-point deck with an explicit artificial-dissipation
/// coefficient.
///
/// `MUCON=1.0` is the normal MSES setting. Negative values are accepted for
/// compatibility with legacy decks because MSES defines them as disabling
/// second-order dissipation; callers should use that mode only when the
/// provenance of the legacy run requires it.
pub fn mses_case_with_mucon(
    mach: f64,
    alpha: f64,
    reynolds: f64,
    n_crit: f64,
    xtr_lower: f64,
    xtr_upper: f64,
    mucon: f64,
) -> String {
    format!(
        "3  4  5  7\n\
         3  4  5  7\n\
         {mach}   0.0   {alpha} | MACHin  CLIFin  ALFAin\n\
         3  2                             | ISMOM  IFFBC  [ DOUXin DOUYin SRCEin ]\n\
         {re}  {ncrit}          | REYNin ACRIT [ KTRTYP ]\n\
         {xtr_l}    {xtr_u}                   | XTR1 XTR2\n\
         {mcrit}  {mucon}                      | MCRIT  MUCON\n\
         0    0                           | ISMOVE  ISPRES\n\
         0    0                           | NMODN   NPOSN\n\n",
        mach = py_float(mach),
        alpha = py_float(alpha),
        re = py_float(reynolds),
        ncrit = py_float(n_crit),
        xtr_l = py_float(xtr_lower),
        xtr_u = py_float(xtr_upper),
        mcrit = py_float(MSES_MCRIT),
        mucon = py_float(mucon),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn py_float_keeps_the_trailing_zero_python_repr_keeps() {
        assert_eq!(py_float(9.0), "9.0");
        assert_eq!(py_float(-1.0), "-1.0");
        assert_eq!(py_float(5_000_000.0), "5000000.0");
        assert_eq!(py_float(0.3), "0.3");
        assert_eq!(py_float(0.99), "0.99");
        assert_eq!(py_float(0.85), "0.85");
        assert_eq!(py_float(2.5), "2.5");
    }

    #[test]
    fn mses_case_matches_native_aerodynamic_model_byte_for_byte() {
        // The exact deck native aerodynamic model wrote for naca2412 at mach 0.3, alpha 3,
        // Re 5e6, n_crit 9, free transition -- captured in golden/aero/mses.json.
        let expected = "3  4  5  7\n\
             3  4  5  7\n\
             0.3   0.0   3.0 | MACHin  CLIFin  ALFAin\n\
             3  2                             | ISMOM  IFFBC  [ DOUXin DOUYin SRCEin ]\n\
             5000000.0  9.0          | REYNin ACRIT [ KTRTYP ]\n\
             1.0    1.0                   | XTR1 XTR2\n\
             0.99  -1.0                      | MCRIT  MUCON\n\
             0    0                           | ISMOVE  ISPRES\n\
             0    0                           | NMODN   NPOSN\n\n";
        assert_eq!(mses_case(0.3, 3.0, 5.0e6, 9.0, 1.0, 1.0), expected);
    }

    #[test]
    fn product_deck_can_use_normal_dissipation_without_changing_legacy_replay() {
        let normal = mses_case_with_mucon(0.3, 3.0, 5.0e6, 9.0, 1.0, 1.0, 1.0);
        assert!(normal.contains("0.99  1.0                      | MCRIT  MUCON"));
        assert!(mses_case(0.3, 3.0, 5.0e6, 9.0, 1.0, 1.0)
            .contains("0.99  -1.0                      | MCRIT  MUCON"));
    }

    #[test]
    fn mset_keystrokes_carry_the_repr_formatted_grid_settings() {
        let keys = mset_keystrokes(141, 0.4, 1.0);
        assert!(keys.starts_with("15\ncase\n7\nn 141\ne 0.4\ni 37\no 37\nx 0.85\n\n1\n1.0\n2\n"));
        assert!(keys.ends_with("\n3\n4\n0\n"));
    }

    #[test]
    fn mset_blade_has_named_farfield_boundaries_before_coordinates() {
        let blade = mset_blade("optimized", &[(1.0, 0.0), (0.0, 0.1)]);
        let mut lines = blade.lines();
        assert_eq!(lines.next(), Some(format!("{:<32}", "optimized").as_str()));
        assert_eq!(lines.next(), Some("  -2.0    3.0    -2.5    3.5"));
        assert_eq!(lines.next(), Some("      1.000000       0.000000"));
        assert_eq!(lines.next(), Some("      0.000000       0.100000"));
        assert_eq!(lines.next(), None);
    }
}
