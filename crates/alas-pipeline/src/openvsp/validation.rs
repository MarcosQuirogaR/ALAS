// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Structural validation for generated OpenVSP scripts.

pub(super) fn validate_script(script: &str) -> Result<(), &'static str> {
    if !script.contains("int main()")
        || !script.contains("WriteVSPFile(")
        || !script.contains("VSPAEROComputeGeometry")
        || !script.contains("ThinGeomSet")
        || !script.contains("ALAS_OPENVSP_EXPORT_COMPLETE")
    {
        return Err("OpenVSP script is missing its entry point or completion contract");
    }
    let mut depth = 0_i64;
    for character in script.chars() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return Err("OpenVSP script has an unmatched closing brace");
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err("OpenVSP script has unbalanced braces");
    }
    if script.contains("NaN") || script.contains("inf") {
        return Err("OpenVSP script contains a non-finite geometry value");
    }
    Ok(())
}
