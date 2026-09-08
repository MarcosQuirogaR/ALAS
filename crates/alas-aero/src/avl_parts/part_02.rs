// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn required_value(text: &str, label: &'static str) -> Result<f64, AvlError> {
    optional_value(text, label)?.ok_or(AvlError::MissingValue(label))
}

fn optional_value(text: &str, label: &'static str) -> Result<Option<f64>, AvlError> {
    for line in text.lines() {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        for window in tokens.windows(3) {
            if window[0] == label && window[1] == "=" {
                let normalized = window[2].replace(['D', 'd'], "E");
                let value = normalized
                    .parse::<f64>()
                    .map_err(|_| AvlError::InvalidNumber {
                        label,
                        token: window[2].to_owned(),
                    })?;
                if !value.is_finite() && label != "e" {
                    return Err(AvlError::InvalidNumber {
                        label,
                        token: window[2].to_owned(),
                    });
                }
                return Ok(Some(value));
            }
        }
    }

    // AVL's `MRF` mode retains the same labels in a full-precision,
    // machine-readable layout: values precede a `|` and the corresponding
    // comma-separated labels follow it.  Accepting both forms keeps the
    // parser useful for retained legacy FT files while allowing the product
    // runner to compare SI references without precision loss.
    for line in text.lines() {
        let Some((value_text, label_text)) = line.split_once('|') else {
            continue;
        };
        let labels = label_text.split(',').map(str::trim).collect::<Vec<_>>();
        let Some(index) = labels.iter().position(|candidate| {
            candidate
                .rsplit_once(':')
                .map_or(*candidate, |(_, suffix)| suffix.trim())
                == label
        }) else {
            continue;
        };
        let values = value_text.split_whitespace().collect::<Vec<_>>();
        let Some(token) = values.get(index) else {
            return Err(AvlError::MissingValue(label));
        };
        let normalized = token.replace(['D', 'd'], "E");
        let value = normalized
            .parse::<f64>()
            .map_err(|_| AvlError::InvalidNumber {
                label,
                token: (*token).to_owned(),
            })?;
        if !value.is_finite() && label != "e" {
            return Err(AvlError::InvalidNumber {
                label,
                token: (*token).to_owned(),
            });
        }
        return Ok(Some(value));
    }
    Ok(None)
}

fn sanitized_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| {
            if character.is_ascii_graphic() || character == ' ' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.trim().is_empty() {
        "ALAS aircraft".to_owned()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn sample_airplane() -> Airplane {
        let foil = Airfoil::from_coordinates("triangle", vec![(1.0, 0.0), (0.0, 0.05), (1.0, 0.0)]);
        Airplane {
            name: "Test".to_owned(),
            xyz_ref: [0.4, 0.0, 0.0],
            wings: vec![Wing::new(
                "Main Wing",
                vec![
                    WingXSec::new([0.0, 0.0, 0.0], 1.0, 2.0, foil.clone()),
                    WingXSec::new([0.2, 2.0, 0.1], 0.5, -1.0, foil),
                ],
                true,
            )],
            fuselages: Vec::new(),
            s_ref: 3.0,
            c_ref: 0.8,
            b_ref: 4.0,
        }
    }

    #[test]
    fn deck_is_self_contained_and_duplicates_symmetric_surfaces() {
        let airplane = sample_airplane();
        let deck = render_geometry(AvlDeckRequest {
            airplane: &airplane,
            mach: 0.2,
            chordwise_vortices: 8,
            spanwise_vortices: 20,
        })
        .unwrap_or_else(|error| panic!("sample geometry is invalid: {error}"));
        assert!(deck.contains("3.000000000000 0.800000000000 4.000000000000"));
        assert!(deck.contains("SURFACE\nMain Wing\n8 1.0 20 -1.0"));
        assert!(deck.contains("YDUPLICATE\n0.0"));
        assert_eq!(deck.matches("SECTION").count(), 2);
        assert_eq!(deck.matches("AIRFOIL").count(), 2);
    }

    #[test]
    fn official_total_force_layout_parses_labels_independent_of_columns() {
        let force = r#"
 Vortex Lattice Output -- Total Forces
 Sref = 0.60508 Cref = 0.20200 Bref = 3.1500
 Xref = 0.090048 Yref = 0.0 Zref = 0.0087358
 Alpha = -1.00454 pb/2V = 0.0 p'b/2V = 0.0
 Beta = 0.00000 qc/2V = 0.0
 Mach = 0.000 rb/2V = 0.0
 CYtot = 0.0 Cmtot = 0.01965
 CLtot = 0.32421
 CDtot = 0.00202
 CDvis = 0.0 CDind = 0.00202
 CYff = 0.0 e = 1.3893
"#;
        let polar = parse_total_forces(&[force], AvlModel::ALAS_LIFTING_SURFACES)
            .unwrap_or_else(|error| panic!("official FT layout is invalid: {error}"));
        assert_eq!(polar.reference.area_m2, 0.60508);
        assert_eq!(polar.points[0].alpha_deg, -1.00454);
        assert_eq!(polar.points[0].lift_coefficient, 0.32421);
        assert_eq!(polar.points[0].pitching_moment_coefficient, 0.01965);
        assert_eq!(polar.points[0].span_efficiency, Some(1.3893));
    }

    #[test]
    fn full_precision_machine_readable_total_force_layout_parses() {
        let force = r#"TOT
VERSION 1.0
Vortex Lattice Output -- Total Forces
  5.290355596969410E+02  9.628258954923000E+00  7.175000000000000E+01      | Sref, Cref, Bref
  3.605872687902900E+01  0.000000000000000E+00  0.000000000000000E+00      | Xref, Yref, Zref
  0.000000000000000E+00 -0.000000000000000E+00 -0.000000000000000E+00      | Alpha, pb/2V, p'b/2V
  0.000000000000000E+00  0.000000000000000E+00      | Beta, qc/2V
  8.400000000000000E-01 -0.000000000000000E+00 -0.000000000000000E+00      | Mach, rb/2V, r'b/2V
 -2.826047969867452E-02 -3.185575063856128E-11 -3.185575063856128E-11      | CXtot, Cltot, Cl'tot
 -8.032832424236166E-10  1.979499918925366E-01      | CYtot, Cmtot
 -7.432419759392327E-01  1.203626666896630E-10  1.203626666896630E-10      | CZtot, Cntot, Cn'tot
  7.432419759392327E-01      | CLtot
  2.826047969867452E-02      | CDtot
  0.000000000000000E+00  2.826047969867452E-02      | CDvis, CDind
  7.346096152745775E-01  2.338343228809003E-02 -4.507555750335080E-10  7.549116708905861E-01      | Trefftz Plane: CLff, CDff, CYff, e
"#;
        let polar = parse_total_forces(&[force], AvlModel::ALAS_LIFTING_SURFACES)
            .unwrap_or_else(|error| panic!("machine-readable FT layout is invalid: {error}"));
        assert_eq!(polar.reference.area_m2, 529.035559696941);
        assert_eq!(polar.reference.chord_m, 9.628258954923);
        assert_eq!(polar.points[0].lift_coefficient, 0.7432419759392327);
        assert_eq!(
            polar.points[0].pitching_moment_coefficient,
            0.1979499918925366
        );
        assert_eq!(polar.points[0].span_efficiency, Some(0.7549116708905861));
    }

    #[test]
    fn polar_rejects_a_reference_change_between_force_files() {
        let common = |area: f64| {
            format!(
                "Sref = {area} Cref = 1 Bref = 4\nXref = 0 Yref = 0 Zref = 0\nAlpha = 0 Beta = 0 Mach = 0\nCLtot = 0 CDtot = 0 CDind = 0 Cmtot = 0 e = 1"
            )
        };
        let first = common(3.0);
        let second = common(4.0);
        assert_eq!(
            parse_total_forces(
                &[first.as_str(), second.as_str()],
                AvlModel::ALAS_LIFTING_SURFACES
            ),
            Err(AvlError::InconsistentReference("Sref"))
        );
    }

    #[test]
    fn trefftz_and_span_loading_records_keep_native_columns_typed() {
        let trefftz =
            parse_trefftz_plane(" 0.72 0.03 -0.01 0.91 | Trefftz Plane: CLff, CDff, CYff, e")
                .unwrap_or_else(|error| panic!("Trefftz record is invalid: {error}"));
        assert_eq!(trefftz.induced_drag_coefficient, 0.03);
        assert_eq!(trefftz.span_efficiency, Some(0.91));

        let cnc = "CNC\nVERSION 1.0\nStrip Loadings: XM, YM, ZM, CNCM, CLM, CHM, DYM, ASM\n 2 | # strips\n1D+0 2 3 4 5 6 7 8\n9 10 11 12 13 14 15 16\n";
        let rows = parse_span_loading(
            cnc,
            AvlReference {
                area_m2: 3.0,
                chord_m: 1.0,
                span_m: 4.0,
                moment_reference_m: [0.0; 3],
            },
        )
        .unwrap_or_else(|error| panic!("CNC record is invalid: {error}"));
        assert_eq!(rows[0].index, 1);
        assert_eq!(rows[1].area_m2, 16.0);
    }

    #[test]
    fn strip_forces_parse_official_surface_and_strip_layout_and_reject_frame() {
        let text = "STRP\nVERSION 1.0\nStandard axis orientation,  X fwd, Z down\n3 1 4 | Sref, Cref, Bref\n0 0 0 | Xref, Yref, Zref\nSurface and Strip Forces by surface\n1 | surfaces\nSURFACE\nWing\n1 1 1 1 | Surface #, # Chordwise, # Spanwise, First strip\n2 1 | Surface area, Ave. chord\n1 2 3 4 5 6 7 8 | CLsurf, Clsurf, CYsurf, Cmsurf, CDsurf, Cnsurf, CDisurf, CDvsurf\n9 10 | CL_srf CD_srf\nStrip Forces referred to Strip Area, Chord\nj, Xle, Yle, Zle, Chord, Area, c_cl, ai, cl_norm, cl, cd, cdv, cm_c/4, cm_LE, C.P.x/c\n1 0 1 2 3 4 5 6 7 8 9 10 11 12 13\n";
        let parsed = parse_strip_forces(text, AvlFrame::StabilityAxes)
            .unwrap_or_else(|error| panic!("STRP record is invalid: {error}"));
        assert_eq!(parsed.surfaces[0].strips[0].index, 1);
        assert_eq!(parsed.surfaces[0].reference_coefficients[6], 7.0);
        assert_eq!(
            parse_strip_forces(text, AvlFrame::BodyAxes),
            Err(AvlError::IncompatibleFrame {
                output: "STRP",
                actual: AvlFrame::StabilityAxes,
                expected: AvlFrame::BodyAxes,
            })
        );
    }

    #[test]
    fn derivatives_parse_native_stability_matrix_and_frame_is_explicit() {
        let text = "DERMATS\nVERSION 1.0\n 3 1 4 | Sref, Cref, Bref\n0 0 0 | Xref, Yref, Zref\nStability-axis derivatives...\nalpha, beta\n1 2 | z' force CL : CLa, CLb\n3 4 | y force CY : CYa, CYb\n5 6 | x' force CD : CDa, CDb\n7 8 | x' mom. Cl' : Cla, Clb\n9 10 | y mom. Cm : Cma, Cmb\n11 12 | z' mom. Cn' : Cna, Cnb\nroll rate p', pitch rate q', yaw rate r'\n1 2 3 | z' force CL : CLp, CLq, CLr\n4 5 6 | y force CY : CYp, CYq, CYr\n7 8 9 | x' force CD : CDp, CDq, CDr\n10 11 12 | x' mom. Cl' : Clp, Clq, Clr\n13 14 15 | y mom. Cm : Cmp, Cmq, Cmr\n16 17 18 | z' mom. Cn' : Cnp, Cnq, Cnr\n0 | # control vars\n0 | # design vars\n-1D30 | Neutral point Xnp\n-1D30 | Clb Cnr / Clr Cnb\n";
        let parsed = parse_derivatives(text, AvlFrame::StabilityAxes)
            .unwrap_or_else(|error| panic!("DERMATS record is invalid: {error}"));
        assert_eq!(parsed.first_order.len(), 6);
        assert_eq!(
            parsed.first_order[2].coefficient,
            AvlDerivativeCoefficient::Drag
        );
        assert_eq!(parsed.rate_columns, ["p'", "q'", "r'"]);
        assert_eq!(parsed.neutral_point_m, None);
        assert_eq!(
            parse_derivatives(
                "DERMATB\naxial   vel. u, sideslip vel. v, normal  vel. w\n",
                AvlFrame::StabilityAxes,
            ),
            Err(AvlError::IncompatibleFrame {
                output: "DERMAT*",
                actual: AvlFrame::GeometryAxes,
                expected: AvlFrame::StabilityAxes,
            })
        );
    }

    #[test]
    fn trim_cases_round_trip_the_native_run_layout_and_commands_are_safe() {
        let text = "Run case  1:  Cruise\n\n alpha -> CL = 0.54\n elevator -> Cm pitchmom = 0\n\n alpha = 1.9 deg\n Mach = 0.7\n";
        let cases = parse_trim_cases(text)
            .unwrap_or_else(|error| panic!("trim record is invalid: {error}"));
        assert_eq!(cases[0].constraints[1].constraint, "Cm pitchmom");
        let rendered = render_trim_case(&cases[0])
            .unwrap_or_else(|error| panic!("trim rendering failed: {error}"));
        assert!(rendered.contains("Run case  1:  Cruise"));
        assert!(rendered.contains("alpha"));
        assert_eq!(
            render_output_command(AvlOutputKind::StabilityDerivatives, "st-mrf.dat")
                .unwrap_or_else(|error| panic!("command rendering failed: {error}")),
            "MRF\nST\nst-mrf.dat\n"
        );
        assert!(render_output_command(AvlOutputKind::TotalForces, "bad\nname").is_err());
    }
}

