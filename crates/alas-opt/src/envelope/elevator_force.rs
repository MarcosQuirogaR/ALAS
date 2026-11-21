// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The pilot-force limit on a reversible (unpowered) elevator: how far the
//! pilot can hold it trailing edge up at a dynamic pressure, which bounds the
//! forward CG at rotation and at the landing flare.
//!
//! A cable-driven elevator moves only as far as the column force holds it
//! against the aerodynamic hinge moment. With the hinge moment `H = q_h S_e
//! c_e C_h` and the gearing `G = d delta_e / d x_column` (rad/m), the column
//! force is `F = G H` (Sadraey, *Aircraft Design: A Systems Engineering
//! Approach*, Wiley 2013, sec. 12.7, eqs. 12.147, 12.151, 12.152a; Roskam,
//! *Airplane Design* Part VII, ch. 2, eq. 2.4). With
//! `C_h = C_h,alpha alpha_h + C_h,delta delta_e + C_h,delta_t delta_t`
//! (Roskam, *Airplane Design* Part VI, 1987, sec. 10.4.2, eq. 10.140, the
//! symmetric section's `C_h0 = 0`, eq. 10.141) and the tab at
//! `delta_t = delta_t,trim + k delta_e` (`k` a geared balance tab), the pull
//! `F` holds
//!
//! ```text
//! delta_e = (F / (G q_h S_e c_e) - C_h,alpha alpha_h - C_h,delta_t delta_t,trim)
//!           / (C_h,delta + k C_h,delta_t)
//! ```
//!
//! bounded by the elevator's up travel. Sign convention: hinge moment and
//! `delta` positive trailing edge down, `alpha_h` positive nose up, a pull
//! `F > 0`. The pull is the CS 25.143(d) maximum (14 CFR 25.143(d) table:
//! pitch, short-term, 75 lbf with two hands, 50 lbf with one), the trim tab
//! at its declared nose-up travel.
//!
//! The derivatives follow Roskam Part VI sec. 10.4 (DATCOM methods): the
//! two-dimensional `c_h,alpha` (Figs. 10.63a/b with Fig. 10.64a, eq. 10.129)
//! and `c_h,delta` (Fig. 10.69a/b, eq. 10.134) of a round-nose, sealed-gap
//! plain flap on a NACA four-digit symmetric section, the trailing-edge
//! condition (eq. 10.128) taken as met so `c'' = c'` (eqs. 10.131, 10.136),
//! the balance factors of Figs. 10.65a and 10.71 (round nose), the
//! Prandtl-Glauert factor (eqs. 10.133, 10.138), then the three-dimensional
//! `C_h,alpha` (eqs. 10.142-10.144, Fig. 10.77) and `C_h,delta` (eqs.
//! 10.145-10.147, Fig. 10.78). The tab derivative is eq. 10.139 (Figs.
//! 10.72-10.74) carried to three dimensions by eq. 10.145 with the tab's
//! parameters (sec. 10.4.2.3) and scaled by the tab's share of the elevator
//! span. The charts were digitised by eye from the printed figures (about
//! +-0.02 on each ratio).

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

use super::chart::interpolate;
use super::rotation::{
    elevator_effectiveness, elevator_up_deflection_deg, large_deflection_effectiveness_factor,
};
use super::tail_stall::{
    strip_area_fraction, symmetric_thickness, SEA_LEVEL_KINEMATIC_VISCOSITY_M2_S,
    SEA_LEVEL_SPEED_OF_SOUND_M_S,
};

/// Short-term pitch force with two hands on the wheel, N: 75 lbf (CS
/// 25.143(d) / 14 CFR 25.143(d) table). Rotation, after V1 with both hands
/// on the column.
pub(super) const ROTATION_PULL_FORCE_N: f64 = 75.0 * LBF_N;
/// Short-term pitch force with one hand, N: 50 lbf (same table). The flare,
/// one hand on the power levers.
pub(super) const FLARE_PULL_FORCE_N: f64 = 50.0 * LBF_N;
/// The approach is flown trimmed: zero column force with the tab
/// (CS 25.161(c), longitudinal trim).
pub(super) const TRIMMED_FORCE_N: f64 = 0.0;
const LBF_N: f64 = 4.448_221_615_260_5;

/// Aft column travel from neutral of a transport wheel control, m: 8 in
/// (20 cm) (Torenbeek, *Synthesis of Subsonic Airplane Design*, 1982, Fig.
/// 3-23, p. 90, after proposed FAR 25.772, ISO 1558 and MS 33576; Sadraey
/// 2013, Fig. 7.10, p. 361, the same 20 cm). With the elevator's declared up
/// travel over it, the linear gearing `G = |delta_up| / x_aft` [E: linear
/// gearing assumed].
pub(super) const AFT_COLUMN_TRAVEL_M: f64 = 0.20;
/// Aerodynamic balance of a reversible elevator, overhang over the flap
/// chord aft of the hinge, `c_b / c_f` [E]: the hinge no farther aft than
/// about 20 % of the control-surface chord (Raymer, *Aircraft Design: A
/// Conceptual Approach*, 6th ed., ch. 6, Fig. 6.5), i.e. `0.2 / 0.8`. The
/// most balance the rule admits, so the lightest forces.
pub(super) const BALANCE_OVERHANG_RATIO: f64 = 0.25;
/// Trim-tab chord over the elevator chord: the middle of "usually about
/// 0.2-0.4" (Sadraey 2013, sec. 12.7.1.5, p. 719) [E: class value].
pub(super) const TAB_CHORD_RATIO: f64 = 0.30;
/// Trim-tab span over the elevator span [M: measured on a photograph]. No
/// handbook in the collection states a class value (Sadraey sec. 12.7.1.5
/// gives the chord ratio only; Roskam Part VI sec. 10.4.2.3 takes the tab
/// stations as inputs) and neither TCDS prints tab spans. Measured along
/// the trailing edge of the left ATR 72-212A elevator in UK AAIB Bulletin
/// 2/2017 (G-COBO, EW/C2016/03/02), Figure 2, p. 17: the trim tab runs from
/// the elevator root to 0.61 of the elevator trailing-edge length; the
/// photograph is taken from below the root, so the perspective lengthens the
/// near (tab) end and the true ratio is at most that, about +/-0.1. 0.6.
pub(super) const TAB_SPAN_RATIO: f64 = 0.60;

/// The hinge-moment derivatives (per rad, on `S_e c_e`) and the control
/// terms of a reversible elevator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReversibleElevator {
    /// `C_h,alpha`, per rad of tail angle of attack.
    pub ch_alpha: f64,
    /// `C_h,delta`, per rad of elevator.
    pub ch_delta: f64,
    /// `C_h,delta_t`, per rad of tab (whole elevator's hinge moment).
    pub ch_tab: f64,
    /// Geared balance tab, tab rad per elevator rad.
    pub balance_tab_gearing: f64,
    /// Trim tab at its nose-up stop, rad (trailing edge down, positive).
    pub trim_tab_rad: f64,
    /// Column gearing, rad of elevator per m of column.
    pub gearing_rad_per_m: f64,
    /// Elevator area times its mean chord, m^3 (both sides).
    pub area_chord_m3: f64,
    /// Up travel, rad (negative).
    pub up_travel_rad: f64,
    /// Elevator chord over the local tail chord.
    pub chord_fraction: f64,
    /// Elevator span over the tail semi-span.
    pub span_fraction: f64,
}

impl ReversibleElevator {
    /// The elevator deflection the pull `force_n` holds at the tail dynamic
    /// pressure `q_h_pa` and tail angle of attack `alpha_h_rad`, rad
    /// (negative up), never beyond the up travel. An overbalanced or
    /// non-finite set holds the travel.
    #[must_use]
    pub fn held_deflection_rad(&self, q_h_pa: f64, alpha_h_rad: f64, force_n: f64) -> f64 {
        let effective = self.ch_delta + self.balance_tab_gearing * self.ch_tab;
        let scale = self.gearing_rad_per_m * q_h_pa * self.area_chord_m3;
        if !(effective < 0.0 && scale > 0.0 && alpha_h_rad.is_finite()) {
            return self.up_travel_rad;
        }
        let delta =
            (force_n / scale - self.ch_alpha * alpha_h_rad - self.ch_tab * self.trim_tab_rad)
                / effective;
        if delta.is_finite() {
            delta.max(self.up_travel_rad)
        } else {
            self.up_travel_rad
        }
    }

    /// The tail-lift elevator term `tau K' (span) delta_e` at the deflection
    /// [`Self::held_deflection_rad`] holds, rad: the same effectiveness as
    /// the travel-limited balances (`super::rotation::rotation_tail_lift`).
    #[must_use]
    pub fn elevator_term_rad(&self, q_h_pa: f64, alpha_h_rad: f64, force_n: f64) -> f64 {
        let delta = self.held_deflection_rad(q_h_pa, alpha_h_rad, force_n);
        elevator_effectiveness(self.chord_fraction)
            * large_deflection_effectiveness_factor(delta.to_degrees(), self.chord_fraction)
            * self.span_fraction
            * delta
    }
}

/// The reversible elevator of `plane`, its derivatives at the sea-level
/// speed `speed_m_s` (Reynolds and Mach numbers); `None` unless the
/// configuration declares one (`landing_gear.elevator_trim_tab_nose_up_deg`)
/// or the tail is missing or degenerate.
#[must_use]
pub fn reversible_elevator(
    plane: &Airplane,
    config: &AlasConfig,
    speed_m_s: f64,
) -> Option<ReversibleElevator> {
    let trim_tab_deg = config
        .landing_gear
        .elevator_trim_tab_nose_up_deg
        .filter(|deg| deg.is_finite())?;
    let tail = plane.wings.get(1)?;
    let reference = tail.reference_planform();
    let controls = &config.control_surfaces;
    let cf = controls.elevator_chord_fraction;
    let (eta_i, eta_o) = (
        controls.elevator_span_start_frac.clamp(0.0, 1.0),
        controls.elevator_span_end_frac.clamp(0.0, 1.0),
    );
    let thickness = symmetric_thickness(&config.geometry.empennage.tail_airfoil).unwrap_or(0.12);
    let tail_area = reference.area();
    let tail_mac = reference.mean_aerodynamic_chord();
    let aspect_ratio = reference.aspect_ratio();
    if !(cf > 0.0 && cf < 1.0 && eta_o > eta_i && tail_area > 0.0 && tail_mac > 0.0) {
        return None;
    }
    let reynolds = speed_m_s * tail_mac / SEA_LEVEL_KINEMATIC_VISCOSITY_M2_S;
    let mach = (speed_m_s / SEA_LEVEL_SPEED_OF_SOUND_M_S).clamp(0.0, 0.7);
    let geometry = HingeGeometry {
        chord_fraction: cf,
        thickness,
        reynolds,
        mach,
        aspect_ratio,
        quarter_chord_sweep_rad: tail.mean_sweep_angle(0.25).to_radians(),
        hinge_sweep_rad: tail.mean_sweep_angle(1.0 - cf).to_radians(),
        eta_inboard: eta_i,
        eta_outboard: eta_o,
    };
    let (ch_alpha, ch_delta, ch_tab) = geometry.derivatives();
    let up_travel_rad = elevator_up_deflection_deg(config).to_radians();
    let elevator_area = strip_area_fraction(tail.taper_ratio(), eta_i, eta_o) * tail_area * cf;
    Some(ReversibleElevator {
        ch_alpha,
        ch_delta,
        ch_tab,
        balance_tab_gearing: config
            .landing_gear
            .elevator_balance_tab_gearing
            .filter(|k| k.is_finite())
            .unwrap_or(0.0),
        trim_tab_rad: trim_tab_deg.to_radians(),
        gearing_rad_per_m: up_travel_rad.abs() / AFT_COLUMN_TRAVEL_M,
        area_chord_m3: elevator_area * cf * tail_mac,
        up_travel_rad,
        chord_fraction: cf,
        span_fraction: (eta_o - eta_i).clamp(0.0, 1.0),
    })
}

/// The inputs of Roskam Part VI sec. 10.4 for a plain elevator.
#[derive(Debug, Clone, Copy)]
struct HingeGeometry {
    chord_fraction: f64,
    thickness: f64,
    reynolds: f64,
    mach: f64,
    aspect_ratio: f64,
    quarter_chord_sweep_rad: f64,
    hinge_sweep_rad: f64,
    eta_inboard: f64,
    eta_outboard: f64,
}

impl HingeGeometry {
    /// `(C_h,alpha, C_h,delta, C_h,delta_t)`, per rad, on `S_e c_e`.
    fn derivatives(&self) -> (f64, f64, f64) {
        let cf = self.chord_fraction;
        let t = self.thickness;
        let lift_ratio = lift_slope_ratio(trailing_edge_tangent(t), self.reynolds);
        // Fig. 10.64b prints the intercept as 6.28; it is chart data, not 2 pi.
        #[allow(clippy::approx_constant)]
        let section_slope = (6.28 + 5.05 * t) * lift_ratio;
        let compressibility = 1.0 / (1.0 - self.mach * self.mach).sqrt();
        let balance = balance_ratio(cf, t);
        // Two-dimensional, eqs. 10.129-10.133 and 10.134-10.138.
        let ch_alpha_2d = theory_ch_alpha(cf, t)
            * ch_alpha_ratio(lift_ratio, cf)
            * ch_alpha_balance(balance)
            * compressibility;
        let ch_delta_2d = theory_ch_delta(cf, t)
            * ch_delta_ratio(lift_ratio, cf)
            * ch_delta_balance(balance, t)
            * compressibility;
        // Three-dimensional, eqs. 10.142-10.147.
        let a = self.aspect_ratio;
        let cos_c4 = self.quarter_chord_sweep_rad.cos();
        let cos_hl = self.hinge_sweep_rad.cos();
        let b2 = balance_chord_factor(cf, BALANCE_OVERHANG_RATIO);
        let k_alpha = span_factor(&K_ALPHA, self.eta_inboard, self.eta_outboard);
        let k_delta = span_factor(&K_DELTA, self.eta_inboard, self.eta_outboard);
        let ch_alpha = a * cos_c4 / (a + 2.0 * cos_c4) * ch_alpha_2d
            + interpolate(&DELTA_CH_ALPHA_A, &DELTA_CH_ALPHA, a)
                * section_slope
                * b2
                * k_alpha
                * cos_c4;
        let tau = elevator_effectiveness(cf);
        let induced = 2.0 * cos_c4 / (a + 2.0 * cos_c4);
        let ch_delta = cos_c4 * cos_hl * (ch_delta_2d - tau * ch_alpha_2d * induced)
            + delta_ch_delta_factor(a, cf) * (section_slope * tau) * b2 * k_delta * cos_c4 * cos_hl;
        // Tab, eq. 10.139 then eq. 10.145 with the tab's chord (sec. 10.4.2.3).
        let tab_chord = TAB_CHORD_RATIO * cf;
        // Fig. 10.74's `(alpha_delta_t)` is the thin-airfoil flap effectiveness of the
        // tab chord, negative at constant lift: `-tau(c_t / c)`.
        let tab_2d = interpolate_rows(&TAB_CF, &TAB_RATIOS, &TAB_CH, cf, TAB_CHORD_RATIO)
            * DEG_PER_RAD
            - lift_hinge_derivative(cf) * section_slope * (-elevator_effectiveness(tab_chord));
        let tab_3d = cos_c4
            * cos_hl
            * (tab_2d * compressibility
                - elevator_effectiveness(tab_chord) * ch_alpha_2d * induced)
            * TAB_SPAN_RATIO;
        (ch_alpha, ch_delta, tab_3d)
    }
}

const DEG_PER_RAD: f64 = 180.0 / std::f64::consts::PI;

/// `tan(phi'_TE / 2)` of a NACA four-digit symmetric section of thickness
/// `t`, from the 90 % and 99 % chord ordinates (Roskam Part VI Fig. 10.64a
/// definition; the section's thickness distribution, Abbott and von
/// Doenhoff, *Theory of Wing Sections*, sec. 6.4).
fn trailing_edge_tangent(t: f64) -> f64 {
    (naca_half_thickness(t, 0.90) - naca_half_thickness(t, 0.99)) / 0.09
}

/// NACA four-digit half thickness at chord station `x`, fraction of chord.
fn naca_half_thickness(t: f64, x: f64) -> f64 {
    5.0 * t
        * (0.2969 * x.sqrt() - 0.1260 * x - 0.3516 * x * x + 0.2843 * x.powi(3)
            - 0.1015 * x.powi(4))
}

/// `c_l,alpha / (c_l,alpha)_theory`, Fig. 10.64a, linear in `log10 Re`
/// between the 10^6, 10^7 and 10^8 curves.
fn lift_slope_ratio(tangent: f64, reynolds: f64) -> f64 {
    const TAN: [f64; 3] = [0.0, 0.1, 0.2];
    const RE6: [f64; 3] = [0.90, 0.795, 0.69];
    const RE7: [f64; 3] = [0.95, 0.88, 0.78];
    const RE8: [f64; 3] = [0.965, 0.92, 0.82];
    let rows = [
        interpolate(&TAN, &RE6, tangent),
        interpolate(&TAN, &RE7, tangent),
        interpolate(&TAN, &RE8, tangent),
    ];
    let log_re = if reynolds > 0.0 {
        reynolds.log10()
    } else {
        6.0
    };
    interpolate(&[6.0, 7.0, 8.0], &rows, log_re)
}

/// `(c_h,alpha)_theory`, per rad, Fig. 10.63b, linear in `t/c` between the
/// zero-thickness and 16 % curves.
fn theory_ch_alpha(cf: f64, t: f64) -> f64 {
    const CF: [f64; 4] = [0.1, 0.2, 0.3, 0.4];
    const THIN: [f64; 4] = [-0.36, -0.50, -0.63, -0.745];
    const THICK: [f64; 4] = [-0.225, -0.37, -0.515, -0.655];
    let (thin, thick) = (interpolate(&CF, &THIN, cf), interpolate(&CF, &THICK, cf));
    thin + (thick - thin) * (t / 0.16).clamp(0.0, 1.0)
}

/// `c'_h,alpha / (c_h,alpha)_theory`, Fig. 10.63a.
fn ch_alpha_ratio(lift_ratio: f64, cf: f64) -> f64 {
    const R: [f64; 13] = [
        0.70, 0.72, 0.74, 0.76, 0.78, 0.80, 0.82, 0.84, 0.86, 0.88, 0.90, 0.95, 1.0,
    ];
    const AT_01: [f64; 13] = [
        -0.11, 0.0, 0.08, 0.18, 0.27, 0.35, 0.43, 0.50, 0.57, 0.64, 0.70, 0.85, 1.0,
    ];
    const AT_04: [f64; 13] = [
        0.14, 0.22, 0.30, 0.38, 0.46, 0.53, 0.61, 0.67, 0.72, 0.77, 0.80, 0.90, 1.0,
    ];
    interpolate(
        &[0.1, 0.4],
        &[
            interpolate(&R, &AT_01, lift_ratio),
            interpolate(&R, &AT_04, lift_ratio),
        ],
        cf,
    )
}

/// `(c_h,delta)_theory`, per rad, Fig. 10.69b.
fn theory_ch_delta(cf: f64, t: f64) -> f64 {
    const T: [f64; 7] = [0.0, 0.04, 0.06, 0.08, 0.10, 0.12, 0.15];
    const AT_01: [f64; 7] = [-0.895, -0.825, -0.80, -0.775, -0.75, -0.725, -0.665];
    const AT_04: [f64; 7] = [-1.03, -1.0, -0.985, -0.97, -0.955, -0.94, -0.92];
    interpolate(
        &[0.1, 0.4],
        &[interpolate(&T, &AT_01, t), interpolate(&T, &AT_04, t)],
        cf,
    )
}

/// `c'_h,delta / (c_h,delta)_theory`, Fig. 10.69a.
fn ch_delta_ratio(lift_ratio: f64, cf: f64) -> f64 {
    const R: [f64; 9] = [0.60, 0.65, 0.70, 0.75, 0.80, 0.85, 0.90, 0.95, 1.0];
    const ROWS: [[f64; 9]; 3] = [
        [0.645, 0.70, 0.75, 0.80, 0.84, 0.88, 0.92, 0.96, 1.0],
        [0.555, 0.645, 0.715, 0.775, 0.825, 0.87, 0.91, 0.955, 1.0],
        [0.41, 0.54, 0.655, 0.735, 0.80, 0.85, 0.90, 0.95, 1.0],
    ];
    let by_cf: Vec<f64> = ROWS
        .iter()
        .map(|row| interpolate(&R, row, lift_ratio))
        .collect();
    interpolate(&[0.1, 0.25, 0.4], &by_cf, cf)
}

/// Balance ratio `sqrt((c_b/c_f)^2 - (t_c / 2 c_f)^2)` of Figs. 10.65a and
/// 10.71, `t_c` the section thickness at the hinge.
fn balance_ratio(cf: f64, t: f64) -> f64 {
    let half_hinge_thickness = naca_half_thickness(t, 1.0 - cf) / cf;
    (BALANCE_OVERHANG_RATIO.powi(2) - half_hinge_thickness.powi(2))
        .max(0.0)
        .sqrt()
}

/// `(c_h,alpha)_balance / c''_h,alpha`, Fig. 10.65a, round nose; unity at no
/// balance.
fn ch_alpha_balance(balance: f64) -> f64 {
    interpolate(&[0.0, 0.16, 0.34, 0.50], &[1.0, 0.91, 0.46, 0.05], balance)
}

/// `(c_h,delta)_balance / c''_h,delta`, Fig. 10.71, round nose, linear in
/// thickness between the NACA 0009 and 0015 lines; unity at no balance.
fn ch_delta_balance(balance: f64, t: f64) -> f64 {
    let thin = interpolate(&[0.0, 0.18, 0.38], &[1.0, 0.74, 0.0], balance);
    let thick = interpolate(&[0.0, 0.19, 0.455], &[1.0, 0.77, 0.0], balance);
    thin + (thick - thin) * ((t - 0.09) / 0.06).clamp(0.0, 1.0)
}

/// `B_2`, Fig. 10.77c, by `c_f'/c'` and `c_b'/c_f'`.
fn balance_chord_factor(cf: f64, overhang: f64) -> f64 {
    const CF: [f64; 7] = [0.05, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
    const OVERHANG: [f64; 6] = [0.0, 0.2, 0.3, 0.4, 0.5, 0.6];
    const ROWS: [[f64; 7]; 6] = [
        [0.47, 0.66, 0.93, 1.11, 1.25, 1.35, 1.42],
        [0.44, 0.61, 0.87, 1.05, 1.19, 1.28, 1.34],
        [0.40, 0.55, 0.80, 0.98, 1.10, 1.19, 1.25],
        [0.32, 0.45, 0.67, 0.85, 0.99, 1.08, 1.14],
        [0.22, 0.31, 0.50, 0.66, 0.80, 0.92, 1.01],
        [0.10, 0.15, 0.27, 0.40, 0.53, 0.68, 0.90],
    ];
    let by_overhang: Vec<f64> = ROWS.iter().map(|row| interpolate(&CF, row, cf)).collect();
    interpolate(&OVERHANG, &by_overhang, overhang)
}

const DELTA_CH_ALPHA_A: [f64; 8] = [2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0];
/// `Delta C_h,alpha / (c_l,alpha B_2 K_alpha cos Lambda)`, Fig. 10.77a.
const DELTA_CH_ALPHA: [f64; 8] = [0.018, 0.0135, 0.0105, 0.0085, 0.007, 0.006, 0.005, 0.0035];
const ETA: [f64; 6] = [0.0, 0.2, 0.4, 0.6, 0.8, 1.0];
/// `K_alpha`, Fig. 10.77b.
const K_ALPHA: [f64; 6] = [1.0, 1.25, 1.6, 2.2, 3.0, 4.25];
/// `K_delta`, Fig. 10.78b.
const K_DELTA: [f64; 6] = [1.0, 1.17, 1.52, 2.05, 2.95, 4.35];

/// Eqs. 10.144 / 10.147: the span factor of a control surface from
/// `eta_i` to `eta_o`.
fn span_factor(curve: &[f64; 6], eta_i: f64, eta_o: f64) -> f64 {
    let k = |eta: f64| interpolate(&ETA, curve, eta);
    (k(eta_i) * (1.0 - eta_i) - k(eta_o) * (1.0 - eta_o)) / (eta_o - eta_i)
}

/// `Delta C_h,delta / (c_l,delta B_2 K_delta cos Lambda cos Lambda_hl)`,
/// Fig. 10.78a, by aspect ratio and `c_f'/c'`.
fn delta_ch_delta_factor(aspect_ratio: f64, cf: f64) -> f64 {
    const A: [f64; 6] = [2.0, 3.0, 4.0, 6.0, 8.0, 10.0];
    const ROWS: [[f64; 6]; 3] = [
        [0.037, 0.025, 0.0185, 0.0105, 0.0068, 0.0048],
        [0.035, 0.0235, 0.017, 0.0098, 0.0063, 0.0045],
        [0.030, 0.021, 0.0155, 0.009, 0.006, 0.0043],
    ];
    let by_cf: Vec<f64> = ROWS
        .iter()
        .map(|row| interpolate(&A, row, aspect_ratio))
        .collect();
    interpolate(&[0.2, 0.4, 0.6], &by_cf, cf)
}

const TAB_CF: [f64; 6] = [0.3, 0.4, 0.5, 0.6, 0.8, 1.0];
const TAB_RATIOS: [f64; 3] = [0.1, 0.2, 0.3];
/// `(c_h,delta_t)_{c_l,delta}`, per deg, Fig. 10.72 (NACA 0009, round nose,
/// sealed gaps), rows by tab chord over control chord.
const TAB_CH: [[f64; 6]; 3] = [
    [-0.0086, -0.0099, -0.0100, -0.0098, -0.0092, -0.0084],
    [-0.0116, -0.0122, -0.0122, -0.0120, -0.0110, -0.0102],
    [-0.0138, -0.0143, -0.0142, -0.0136, -0.0122, -0.0106],
];

/// Bilinear read of a chart with rows `ratios` over abscissa `xs`.
fn interpolate_rows(xs: &[f64], ratios: &[f64], rows: &[[f64; 6]; 3], x: f64, ratio: f64) -> f64 {
    let by_ratio: Vec<f64> = rows.iter().map(|row| interpolate(xs, row, x)).collect();
    interpolate(ratios, &by_ratio, ratio)
}

/// `(c_h,c_l)_{delta_t,delta}`, Fig. 10.73: a straight line through
/// `c_f/c = 0.06` (zero) and `0.75` (-0.14).
fn lift_hinge_derivative(cf: f64) -> f64 {
    -0.14 * (cf - 0.06) / (0.75 - 0.06)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atr_like() -> HingeGeometry {
        HingeGeometry {
            chord_fraction: 0.37,
            thickness: 0.12,
            reynolds: 6.0e6,
            mach: 0.17,
            aspect_ratio: 4.7,
            quarter_chord_sweep_rad: 0.0,
            hinge_sweep_rad: 0.0,
            eta_inboard: 0.03,
            eta_outboard: 0.97,
        }
    }

    /// The derivatives are negative (restoring) and of the order Sadraey
    /// states for a balanced surface (2013, sec. 12.7, p. 715: about -0.1
    /// and -0.3 per rad).
    #[test]
    fn a_balanced_elevator_has_negative_hinge_derivatives_of_the_typical_order() {
        let (ch_alpha, ch_delta, ch_tab) = atr_like().derivatives();
        assert!((-0.4..-0.02).contains(&ch_alpha), "{ch_alpha}");
        assert!((-0.6..-0.1).contains(&ch_delta), "{ch_delta}");
        assert!((-0.6..-0.05).contains(&ch_tab), "{ch_tab}");
    }

    fn elevator() -> ReversibleElevator {
        ReversibleElevator {
            ch_alpha: -0.2,
            ch_delta: -0.3,
            ch_tab: -0.2,
            balance_tab_gearing: 0.0,
            trim_tab_rad: 5.0_f64.to_radians(),
            gearing_rad_per_m: 2.0,
            area_chord_m3: 2.2,
            up_travel_rad: -23.0_f64.to_radians(),
            chord_fraction: 0.37,
            span_fraction: 0.94,
        }
    }

    /// More dynamic pressure, less elevator for the same pull: the limit
    /// moves aft with weight.
    #[test]
    fn the_held_deflection_shrinks_as_the_dynamic_pressure_grows() {
        let e = elevator();
        let low = e.held_deflection_rad(1000.0, -0.04, ROTATION_PULL_FORCE_N);
        let high = e.held_deflection_rad(2000.0, -0.04, ROTATION_PULL_FORCE_N);
        assert!(low < high && high < 0.0, "{low} {high}");
        // At a very low pressure the travel governs.
        assert_eq!(
            e.held_deflection_rad(1.0, -0.04, ROTATION_PULL_FORCE_N),
            e.up_travel_rad
        );
    }

    /// A geared balance tab lightens the elevator, so the same pull holds
    /// more of it.
    #[test]
    fn a_geared_balance_tab_lets_the_same_pull_hold_more_elevator() {
        let plain = elevator();
        let geared = ReversibleElevator {
            balance_tab_gearing: -0.308,
            ..plain
        };
        let (q, alpha) = (2000.0, -0.04);
        assert!(
            geared.held_deflection_rad(q, alpha, FLARE_PULL_FORCE_N)
                < plain.held_deflection_rad(q, alpha, FLARE_PULL_FORCE_N)
        );
    }
}
