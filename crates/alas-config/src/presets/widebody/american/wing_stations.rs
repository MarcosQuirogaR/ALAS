// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The spanwise wing station tables of the McDonnell Douglas DC-10 preset:
//! thickness and dihedral stations (kept apart from the preset table to bound
//! its size).

/// Thickness stations of the DC-10 wing, multipliers on the DC10-BASIC section
/// t/c (the preset's root and tip airfoil).
pub(super) fn dc_10_thickness() -> Vec<crate::WingThicknessStation> {
    // Thickness profile: the DC-10 final wing, Obert, Aerodynamic Design of Transport Aircraft (2009), Fig. 24.21 (p. 208, from
    // AIAA 69-830), digitised on the page image (on the axis ticks; +-0.04 pt t/c, +-0.5 % semispan): t/c 12.2 % at
    // the root, 11.2 % at 20 % semispan, 9.5 % at the kink (curve 9.46 % at 37.3 %), 9.08 % at the curve's final knuckle (43 %),
    // 9.0 % at 60 % and 8.4 % at the tip. The kink station moves with the kink (was 0.379; 0.03 station spacing rule); without the
    // knuckle station the profile ran 0.13 pt thick at 43 %. The single 11.0 % average of Jenkinson Data A over-thickened the outer
    // wing (Korn wave drag 39 counts at M 0.82, CL 0.49, against 9-12 counts at CL 0.3-0.4 in Obert Fig. 24.24, DC-10-10 flight
    // test). Multipliers act on DC10-BASIC's own maximum t/c on the 101-station sampling, 0.1011 at every station.
    // Root station at the side of body: Obert, Aerodynamic Design of Transport Aircraft (2009), Fig. 24.21. The final curve starts
    // at about 11.6 % semispan on the printed 12.2 % dashed line (the curve reads 12.16 % at 12 %). The station sits on the
    // side-of-body loft section (0.1195, the fuselage half width), within the +-0.5 % semispan read of 12 %: at 0.12 it would lie
    // 0.0005 outboard of that section, outside the same-station tolerance, and mesh a 0.013 m sliver panel. Without it the built
    // side-of-body section (the centre box copies it) took the spline value there, 11.70 %, 0.46 pt under the curve at 12 %.
    // It replaces the former centreline station (also 12.2 %): inboard thickness is unchanged (the first station's scale holds to
    // the root), and the in-body station no longer forces a zero slope at the side of body (+0.13 pt at 15 % with both).
    vec![
        crate::WingThicknessStation {
            span_fraction: 0.1195,
            thickness_scale: 1.2065, // t/c 0.1011 -> 0.1220, side of body
        },
        crate::WingThicknessStation {
            span_fraction: 0.2000,
            thickness_scale: 1.1076, // t/c 0.1011 -> 0.1120
        },
        crate::WingThicknessStation {
            span_fraction: 0.373,
            thickness_scale: 0.9395, // t/c 0.1011 -> 0.0950
        },
        crate::WingThicknessStation {
            span_fraction: 0.4300,
            thickness_scale: 0.8981, // t/c 0.1011 -> 0.0908, final knuckle
        },
        crate::WingThicknessStation {
            span_fraction: 0.6000,
            thickness_scale: 0.8900, // t/c 0.1011 -> 0.0900
        },
        crate::WingThicknessStation {
            span_fraction: 1.0000,
            thickness_scale: 0.8307, // t/c 0.1011 -> 0.0840
        },
    ]
}

/// Dihedral stations of the DC-10 wing: DAC-67803A Rev A Figure 2.2 front
/// view, the fit that sets the preset's root, break and tip heights.
pub(super) fn dc_10_dihedral() -> Vec<crate::WingDihedralStation> {
    // Interior stations of the same front-view fit; were on the straight line z = -1.5 + 0.0893 y.
    vec![
        crate::WingDihedralStation {
            span_fraction: 0.2000,
            z_m: -1.0200,
            flight_rise_m: None,
        },
        crate::WingDihedralStation {
            span_fraction: 0.3000,
            z_m: -0.8150,
            flight_rise_m: None,
        },
        crate::WingDihedralStation {
            span_fraction: 0.5000,
            z_m: -0.4750,
            flight_rise_m: None,
        },
        crate::WingDihedralStation {
            span_fraction: 0.6500,
            z_m: -0.2780,
            flight_rise_m: None,
        },
        crate::WingDihedralStation {
            span_fraction: 0.8000,
            z_m: 0.0100,
            flight_rise_m: None,
        },
    ]
}
