// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Certified operating speed limits a preset declares from its type
//! certificate data sheet, and how they reach the mission profile.
//!
//! VMO and MMO are stated against pressure altitude. A limit that holds at
//! every altitude is one row; a chart or a "straight line variation between
//! points" is several, read as [`crate::mission::LimitTable`] reads them. One
//! rule copies them into the profile for every preset that declares them, so
//! no preset carries its own hook.

use crate::MissionProfileConfig;

/// Metres per foot.
const FT_M: f64 = 0.3048;
/// Metres per second per knot.
const KT_M_S: f64 = 1_852.0 / 3_600.0;

/// VMO and MMO from the type certificate data sheet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OperatingSpeedLimits {
    /// VMO rows, `(pressure altitude m, calibrated airspeed m/s)`, ascending;
    /// empty when the source states none. Indicated airspeeds are taken as
    /// calibrated (no position-error correction is published with them).
    pub vmo_cas_m_s: &'static [(f64, f64)],
    /// MMO rows, `(pressure altitude m, Mach)`, ascending; empty when the
    /// source states none.
    pub mmo: &'static [(f64, f64)],
    /// VFE of the landing flap setting, m/s calibrated; 0 when not stated.
    pub vfe_landing_cas_m_s: f64,
    /// Exact source, revision and location.
    pub source: &'static str,
}

impl OperatingSpeedLimits {
    /// Copy the limits into `profile`: a single row sets the single value, more
    /// rows set the altitude schedule (and clear the single value, which the
    /// schedule replaces). A limit with no rows leaves the profile's as it is.
    pub fn apply(&self, profile: &mut MissionProfileConfig) {
        apply_rows(
            self.vmo_cas_m_s,
            &mut profile.max_operating_cas_m_s,
            &mut profile.vmo_schedule,
        );
        apply_rows(
            self.mmo,
            &mut profile.max_operating_mach,
            &mut profile.mmo_schedule,
        );
        if self.vfe_landing_cas_m_s > 0.0 {
            profile.vfe_landing_cas_m_s = self.vfe_landing_cas_m_s;
        }
    }
}

/// Ratio of the minimum design wing-flap speed VF of the landing flap position to the stall
/// speed in the landing configuration, both at maximum landing weight: CS-25 Amendment 27,
/// CS 25.335(e)(3)(iii), "VF may not be less than ... 1.8 VS0 with the wing-flaps in landing
/// position at maximum landing weight". The established VFE does not exceed VF (CS 25.1511).
pub const LANDING_FLAP_DESIGN_SPEED_PER_VS0: f64 = 1.8;

/// ISA sea-level density, kg/m^3: a sea-level stall speed is an equivalent and a calibrated one.
const ISA_SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;

/// Where a preset's landing-flap VFE comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandingVfeSource {
    /// Stated by the type certificate data sheet ([`OperatingSpeedLimits::source`]).
    TypeCertificate,
    /// Not stated by any source on file: the declared default
    /// [`default_landing_vfe_cas_m_s`], the design flap speed floor of the landing flap.
    DesignFlapSpeedDefault,
}

/// Declared default landing-flap VFE, m/s calibrated, for an aeroplane whose type certificate
/// states none: the least design flap speed CS 25.335(e)(3)(iii) allows for the landing flap,
/// [`LANDING_FLAP_DESIGN_SPEED_PER_VS0`] VS0, with VS0 the sea-level 1-g stall speed of the
/// landing configuration (the reference stall speed of CS 25.103) at `landing_mass_kg`, from
/// the landing maximum lift coefficient `cl_max_land` on `wing_area_m2`. It is the VFE of an
/// aeroplane whose landing flap is designed to that floor and placarded at its design speed; a
/// type placarded below its design speed has a lower VFE. `None` for an input that is not finite
/// and positive.
pub fn default_landing_vfe_cas_m_s(
    landing_mass_kg: f64,
    wing_area_m2: f64,
    cl_max_land: f64,
) -> Option<f64> {
    let inputs = [landing_mass_kg, wing_area_m2, cl_max_land];
    if inputs
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return None;
    }
    let weight_n = landing_mass_kg * alas_atmo::STANDARD_GRAVITY;
    let vs0_m_s =
        (2.0 * weight_n / (ISA_SEA_LEVEL_DENSITY_KG_M3 * wing_area_m2 * cl_max_land)).sqrt();
    Some(LANDING_FLAP_DESIGN_SPEED_PER_VS0 * vs0_m_s)
}

fn apply_rows(rows: &[(f64, f64)], single: &mut f64, schedule: &mut Vec<(f64, f64)>) {
    match rows {
        [] => {}
        [(_, value)] => {
            *single = *value;
            schedule.clear();
        }
        _ => {
            *single = 0.0;
            *schedule = rows.to_vec();
        }
    }
}

/// Learjet 60: EASA TCDS IM.A.212 Issue 04, section 4, airspeed limits chart
/// (indicated airspeed and indicated Mach against altitude), read off the
/// printed chart. VMO 300 kt from sea level to 8,000 ft, 340 kt from 8,000 ft
/// to 20,800 ft, a straight line to 330 kt at 23,300 ft, and 330 kt above,
/// where the MMO line meets it at about 27,300 ft. MMO 0.81 to 37,000 ft, a
/// straight line to 0.78 at 43,000 ft, and 0.78 above. Below 27,000 ft the
/// chart's Mach boundary is the VMO line, so 0.81 is held there and VMO binds.
/// VFE flaps 40 deg (landing) 165 kt, the same TCDS, section 4 (p 11).
pub const LJ60: OperatingSpeedLimits = OperatingSpeedLimits {
    vmo_cas_m_s: &[
        (0.0, 300.0 * KT_M_S),
        (8_000.0 * FT_M, 300.0 * KT_M_S),
        (8_000.0 * FT_M, 340.0 * KT_M_S),
        (20_800.0 * FT_M, 340.0 * KT_M_S),
        (23_300.0 * FT_M, 330.0 * KT_M_S),
    ],
    mmo: &[(37_000.0 * FT_M, 0.81), (43_000.0 * FT_M, 0.78)],
    vfe_landing_cas_m_s: 165.0 * KT_M_S,
    source: "EASA TCDS IM.A.212 Issue 04, 12 November 2024, section 4: airspeed limits chart (p 10), VFE (p 11)",
};

/// A220-300 (BD-500-1A11): EASA TCDS EASA.IM.A.570 Issue 25, 21 August 2026, Section 2,
/// 10. Airspeed Limits (p 22): VMO 300 kt from sea level to 8,000 ft, 330 kt from 10,000 ft to
/// 27,500 ft, MMO 0.82 above 27,500 ft; the TCDS states indicated airspeeds, taken as calibrated
/// (no position error is published). No value is stated between 8,000 and 10,000 ft, so the
/// 300 kt of the lower band is held to 10,000 ft (the stated limits are never exceeded). No VFE
/// is stated (see the AFM), so the approach takes the declared design flap speed default.
pub const A220_300: OperatingSpeedLimits = OperatingSpeedLimits {
    vmo_cas_m_s: &[
        (0.0, 300.0 * KT_M_S),
        (10_000.0 * FT_M, 300.0 * KT_M_S),
        (10_000.0 * FT_M, 330.0 * KT_M_S),
        (27_500.0 * FT_M, 330.0 * KT_M_S),
    ],
    mmo: &[(27_500.0 * FT_M, 0.82)],
    vfe_landing_cas_m_s: 0.0,
    source: "EASA TCDS EASA.IM.A.570 Issue 25, 21 August 2026, Section 2 BD-500-1A11, 10. Airspeed Limits (p 22)",
};

/// C-295: EASA TCDS EASA.A.186 Issue 08, Section 5, 10. Airspeed Limits - C-295:
/// VMO 245 kt at sea level, 254 kt at 7,000 ft, 245 kt at 14,000 ft and 200 kt
/// at 25,000 ft, with a straight line variation between points. No MMO is stated.
/// VFE landing (23 deg) 175 kt, the same section.
pub const C295: OperatingSpeedLimits = OperatingSpeedLimits {
    vmo_cas_m_s: &[
        (0.0, 245.0 * KT_M_S),
        (7_000.0 * FT_M, 254.0 * KT_M_S),
        (14_000.0 * FT_M, 245.0 * KT_M_S),
        (25_000.0 * FT_M, 200.0 * KT_M_S),
    ],
    mmo: &[],
    vfe_landing_cas_m_s: 175.0 * KT_M_S,
    source: "EASA TCDS EASA.A.186 Issue 08, 9 December 2025, Section 5, 10. Airspeed Limits - C-295 (p 76)",
};

/// A320-200 (CFM56-5B, flaps Full 27/35): EASA TCDS EASA.A.064 Issue 62, Section 1 A320
/// series, 10. Airspeed Limits: VMO 350 kt, MMO 0.82, VFE Full (landing) 177 kt.
pub const A320_200: OperatingSpeedLimits = OperatingSpeedLimits {
    vmo_cas_m_s: &[(0.0, 350.0 * KT_M_S)],
    mmo: &[(0.0, 0.82)],
    vfe_landing_cas_m_s: 177.0 * KT_M_S,
    source: "EASA TCDS EASA.A.064 Issue 62, 26 June 2026, Section 1 A320 series, 10. Airspeed Limits (p 45)",
};

/// A340-312 (A340-300 series, CFM56-5C3/F): FAA TCDS A43NM Revision 10, July 28, 2014, Section
/// III Data Pertinent to All Model A340-200 and A340-300 Series Airplanes, Airspeed Limits (IAS
/// unless otherwise stated), p 6: VMO/MMO 330 kt / 0.86 at every altitude; VFE FULL (24/32,
/// landing) 180 kt. The same page states VD/MD 365 kt / 0.93 (a check, never an input). EASA TCDS
/// EASA.A.015 Issue 28 (8. Air Speeds Limits, p 34) refers to the flight manual.
pub const A340_300: OperatingSpeedLimits = OperatingSpeedLimits {
    vmo_cas_m_s: &[(0.0, 330.0 * KT_M_S)],
    mmo: &[(0.0, 0.86)],
    vfe_landing_cas_m_s: 180.0 * KT_M_S,
    source: "FAA TCDS A43NM Revision 10, July 28, 2014, Section III Model A340-200/-300 series, Airspeed Limits (p 6)",
};

/// ERJ 190-400 (E195-E2): FAA TCDS A57NM Revision 27, October 29, 2026, IX Model ERJ 190-400
/// (p 9, "Same as model ERJ 190-300" for airspeeds) and VIII Model ERJ 190-300, Airspeed Limits
/// (I.A.S.), p 7: VMO 300 kt to 8,000 ft, increasing linearly to 320 kt at 10,000 ft, 320 kt to
/// 28,887 ft; MMO 0.82 from 28,887 ft to 41,000 ft. VFE FULL 165 kt, taken as the landing flap
/// (detent 5, the other landing setting, 180 kt: a modelling choice). The 62,500 kg weight
/// variant is EASA TCDS EASA.IM.A.071 Issue 29, Section 5, item 13, footnote (16); that TCDS
/// refers to the flight manual for airspeeds (III.10, p 40). The FAA basis replaces 25.335(b)(1)
/// with SC 25-643-SC (82 FR 14117, 17 March 2017); the EASA basis is CS 25 Amendment 14.
pub const E195_E2: OperatingSpeedLimits = OperatingSpeedLimits {
    vmo_cas_m_s: &[
        (0.0, 300.0 * KT_M_S),
        (8_000.0 * FT_M, 300.0 * KT_M_S),
        (10_000.0 * FT_M, 320.0 * KT_M_S),
        (28_887.0 * FT_M, 320.0 * KT_M_S),
    ],
    mmo: &[(28_887.0 * FT_M, 0.82)],
    vfe_landing_cas_m_s: 165.0 * KT_M_S,
    source: "FAA TCDS A57NM Revision 27, October 29, 2026, IX Model ERJ 190-400 (p 9) and VIII Model ERJ 190-300, Airspeed Limits (I.A.S.) (p 7)",
};

/// ATR 72-212A (ATR 72-600): FAA TCDS A53EU Revision 38, August 27, 2026, VII ATR72-212A,
/// Airspeed Limits, p 25 (the same values as the "(IAS)" block of VI, p 23, taken as
/// calibrated): VMO 250 kt and MMO 0.55 at every altitude, so MMO governs above about 20,300 ft
/// (maximum operating altitude 25,000 ft, p 27); VFE 30 deg (landing) 150 kt. EASA TCDS
/// EASA.A.084 Issue 14 refers to the flight manual; the governing 25.335 text is JAR 25 Change
/// 11 at Amendment 88/1, the same (b)(1)/(b)(2) structure.
pub const ATR72_212A: OperatingSpeedLimits = OperatingSpeedLimits {
    vmo_cas_m_s: &[(0.0, 250.0 * KT_M_S)],
    mmo: &[(0.0, 0.55)],
    vfe_landing_cas_m_s: 150.0 * KT_M_S,
    source: "FAA TCDS A53EU Revision 38, August 27, 2026, VII ATR72-212A, Airspeed Limits (p 25 of 34)",
};

/// DC-10-30 / DC-10-30F: FAA TCDS A22WE Revision 13, 30 April 2018, Section III Model DC-10-30,
/// Airspeed Limits (CAS), page 9 of 50 (page revision 10; Section IV DC-10-30F, page 13 of 50,
/// the same): VMO 350 kt at sea level, 376 kt at 10,000 ft, 376 kt (M 0.88) at 24,820 ft; MMO
/// 0.88 above (258 kt at 42,000 ft is M 0.88). The TCDS lists points; a straight line between
/// them is taken. VFE 50 deg (landing) 178 kt, both weight columns. No VD/MD is stated. Read
/// from the copy in NTSB docket DCA26MA024, Exhibit 7B, Attachment 1 (16 April 2026); its
/// currency was not confirmed on the FAA DRS.
pub const DC10_30: OperatingSpeedLimits = OperatingSpeedLimits {
    vmo_cas_m_s: &[
        (0.0, 350.0 * KT_M_S),
        (10_000.0 * FT_M, 376.0 * KT_M_S),
        (24_820.0 * FT_M, 376.0 * KT_M_S),
    ],
    mmo: &[(24_820.0 * FT_M, 0.88)],
    vfe_landing_cas_m_s: 178.0 * KT_M_S,
    source: "FAA TCDS A22WE Revision 13, 30 April 2018, Section III Model DC-10-30, Airspeed Limits (p 9 of 50); Section IV DC-10-30F (p 13 of 50) the same",
};

/// A350-900, SECONDARY SOURCE (manufacturer-derived): the EASA (EASA.A.151 Issue 33, p 10,
/// III.1.5), UK CAA (UK.TC.A.00157 Issue 1, p 11) and FAA (T00063IB Rev 29, p 1) data sheets all
/// refer the airspeed limits to the AFM. Airbus, Safety First #21 (January 2016), "Control your
/// speed... in cruise", p 8/17, tabulates A350 VMO 340 kt, MMO 0.89 (VD 375 kt, MD 0.96 as check
/// values; p 9/17: "VD is a Calibrated Air Speed (CAS)"); VMO is printed in kt without a speed
/// type and is taken as CAS. No altitude band is published. To be replaced by the AFM limitations.
pub const A350_900: OperatingSpeedLimits = OperatingSpeedLimits {
    vmo_cas_m_s: &[(0.0, 340.0 * KT_M_S)],
    mmo: &[(0.0, 0.89)],
    vfe_landing_cas_m_s: 0.0,
    source: "SECONDARY SOURCE (manufacturer-derived; TCDS EASA.A.151 Iss 33 p 10, UK.TC.A.00157 Iss 1 p 11 and FAA T00063IB Rev 29 p 1 refer to the AFM): Airbus Safety First #21, January 2016, 'Control your speed... in cruise', p 8/17 table, A350 VMO 340 kt, MMO 0.89; https://mms-safetyfirst.s3.eu-west-3.amazonaws.com/pdf/safety+first/control-your-speed-in-cruise.pdf, accessed 2026-11-19",
};

/// A380-800, SECONDARY SOURCE (manufacturer-derived): the EASA (EASA.A.110 Issue 17, pp 11, 13)
/// and UK CAA (UK.TC.A.00158 Issue 1, pp 11, 13) data sheets refer the airspeed limits to the
/// AFM. Airbus, Safety First #10 (August 2010), "A380 - Flutter tests", p 2/4: "VMO = 340 kt and
/// VD = 375 kt", "MMO = 0,89 and MD = 0,96" (VD/MD as check values); MMO 0.89 also in UAE GCAA
/// AAIS Final Report AIFN/0015/2023 (14 April 2026), p 3. VMO is printed in kt without a speed
/// type and is taken as CAS. No altitude band is published. To be replaced by the AFM limitations.
pub const A380_800: OperatingSpeedLimits = OperatingSpeedLimits {
    vmo_cas_m_s: &[(0.0, 340.0 * KT_M_S)],
    mmo: &[(0.0, 0.89)],
    vfe_landing_cas_m_s: 0.0,
    source: "SECONDARY SOURCE (manufacturer-derived; TCDS EASA.A.110 Iss 17 pp 11, 13 and UK.TC.A.00158 Iss 1 refer to the AFM): Airbus Safety First #10, August 2010, 'A380 - Flutter tests', p 2/4, VMO 340 kt, MMO 0.89; http://web.archive.org/web/20250419171746/https://safetyfirst.airbus.com/app/themes/mh_newsdesk/documents/archives/a380-flutter-tests.pdf (Airbus original now 404), accessed 2026-11-19; MMO 0.89 also UAE GCAA AAIS AIFN/0015/2023 p 3",
};

/// A400M-180: EASA TCDS EASA.A.169 Issue 07, Section 1, 10. Airspeed Limits (p 11): VMO 300 kt
/// IAS, MMO 0.72 ("VMO = M 0.72"); ISA crossover 25,224 ft. These are the civil type design
/// limits (WV001, 137,500 kg MTOW; TCDS Note 1: no eligible MSN); the military standard's limits
/// are not public. The certified VD basis is SC C-02 "Design dive speed Vd" (p 5, text not
/// public), so the CS 25.335(b) VD the registry derives from them is a declared method, not the
/// certified VD. No VFE is stated (see the AFM): the declared design flap speed default stands.
pub const A400M_180: OperatingSpeedLimits = OperatingSpeedLimits {
    vmo_cas_m_s: &[(0.0, 300.0 * KT_M_S)],
    mmo: &[(0.0, 0.72)],
    vfe_landing_cas_m_s: 0.0,
    source: "EASA TCDS EASA.A.169 Issue 07, 28 November 2025, Section 1 A400M-180, 10. Airspeed Limits (p 11); civil type design WV001 (137,500 kg MTOW), TCDS Note 1: no eligible MSN; preset is the military standard 141 t",
};

/// AVE-v1, a clean-sheet design with no type certificate: its VMO/MMO follow the design rule
/// from its own design cruise requirement (M 0.84), checked by the test
/// `the_ave_limits_are_the_design_rule_at_its_cruise_mach`:
/// - MMO = design cruise Mach + 0.05. Jenkinson, Simpkin and Rhodes, Civil Jet Aircraft Design
///   (1999), sec. 16.3 p 359: the normal operating Mach is about 0.05 below MMO; Obert,
///   Aerodynamic Design of Transport Aircraft (2009), ch. 29 p 377: Mdesign = MMO - 0.03 to
///   0.05. The design cruise Mach is taken as that normal operating Mach.
/// - VMO = the CAS of MMO at 25,839 ft, the median VMO/MMO crossover of Jenkinson Table 16.3
///   (p 359) with its VMO read as CAS, as type certificates state it (the table labels it EAS;
///   read so, the median is 23,517 ft and VMO 390.8 kt). The table holds 1960s-90s regional
///   narrowbodies, so a M 0.84 long-haul design is an extrapolation of it.
///
/// VMO 372.96 kt CAS, constant; MMO 0.89. No VFE: the design flap speed default stands.
pub const AVE_DESIGN_RULE: OperatingSpeedLimits = OperatingSpeedLimits {
    vmo_cas_m_s: &[(0.0, 191.867_38)],
    mmo: &[(0.0, 0.89)],
    vfe_landing_cas_m_s: 0.0,
    source: "ALAS design rule for a clean-sheet design (no type certificate): MMO = design cruise Mach + 0.05, VMO = CAS of MMO at 25,839 ft; Jenkinson, Simpkin and Rhodes, Civil Jet Aircraft Design (1999), sec. 16.3 and Table 16.3, p 359; Obert, Aerodynamic Design of Transport Aircraft (2009), ch. 29, p 377",
};

/// A high speed protection function in a type's certification basis, and the special
/// condition, CRI or certification-basis entry that states it. It moves the design dive speed
/// to CS 25.335(b)(1)(ii) ([`crate::design_speeds::UpsetCriterion::HighSpeedProtection`]). A
/// type is protected only when such a document is on file: never from its numbers. The A400M
/// basis lists SC C-02 "Design dive speed Vd" and SC B-04 "Flight envelope protection" (EASA
/// TCDS EASA.A.169 Issue 07, p 5), but no public text states a high speed protection
/// function, so it is not declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcedHighSpeedProtection {
    /// Document id, issue, date and location.
    pub source: &'static str,
}

/// A320-200 (ceo): the JAA special condition carried by the EASA basis ("an order in view to
/// pull up" above a set speed that "cannot allow to maintain the 7,5 deg slope during 20
/// seconds"), and the original FAA special condition. The speed warning is set at VMO + 4 kt
/// with at most 2 kt tolerance (HC S-24).
pub const A320_HIGH_SPEED_PROTECTION: SourcedHighSpeedProtection = SourcedHighSpeedProtection {
    source: "EASA TCDS EASA.A.064 Issue 62, 26 June 2026: SC A-2.2.3 Design dive speed (p 28); Explanatory Note to TCDS EASA.A.064 Annex I Issue 25: SC A-2.2.3 (p 14), SC B-14 Design dive speed (pp 43-44), HC S-24 overspeed warning setting (p 282); FAA Special Conditions No. 25-ANM-23, 15 Dec 1988",
};

/// A220-300 (BD-500-1A11).
pub const A220_HIGH_SPEED_PROTECTION: SourcedHighSpeedProtection = SourcedHighSpeedProtection {
    source: "EASA TCDS EASA.IM.A.570 Issue 25, 21 August 2026, Section 2 BD-500-1A11: C-06 Design Dive Speed (p 19); FAA Special Conditions No. 25-565-SC, 79 FR 54571, 12 Sep 2014, Automatic Speed Protection for Design Dive Speed",
};

/// A340-300: "equipped with a high speed protection system which limits nose down pilot
/// authority at speeds above Vc/Mc".
pub const A340_HIGH_SPEED_PROTECTION: SourcedHighSpeedProtection = SourcedHighSpeedProtection {
    source: "EASA TCDS EASA.A.015 Issue 28, 15 January 2026, Section 2 A340-300: SC A-4 Design dive speed (p 28); Explanatory Note to TCDS EASA.A.015 Issue 2, SC A-4 Issue 4, 17 Jan 1991 (pp 13-14)",
};

/// A350-900.
pub const A350_HIGH_SPEED_PROTECTION: SourcedHighSpeedProtection = SourcedHighSpeedProtection {
    source: "EASA TCDS EASA.A.151 Issue 33, 1 July 2026: SC C-02 Design dive speed (p 6); FAA Special Conditions No. 25-535-SC, 79 FR 49427, 21 Aug 2014, High-Speed Protection System",
};

/// A380-800.
pub const A380_HIGH_SPEED_PROTECTION: SourcedHighSpeedProtection = SourcedHighSpeedProtection {
    source: "EASA TCDS EASA.A.110 Issue 17, 5 August 2026: SC C-15 Design dive speed Vd (p 6); FAA 25-535-SC, 79 FR 49427 (the A380 among the types with the function)",
};

/// B787-9.
pub const B787_HIGH_SPEED_PROTECTION: SourcedHighSpeedProtection = SourcedHighSpeedProtection {
    source: "EASA TCDS EASA.IM.A.115 Issue 30, 17 December 2025, Equivalent Safety Findings: CRI C-03 Dive Speed Definition, with Speed Protection System (787-8 p 11, 787-9 p 31)",
};

/// ERJ 190-400 (E195-E2).
pub const E2_HIGH_SPEED_PROTECTION: SourcedHighSpeedProtection = SourcedHighSpeedProtection {
    source: "FAA Special Conditions No. 25-643-SC, 82 FR 14117, 17 Mar 2017, ERJ 190-300 Dive-Speed Definition with High-Speed-Protection System (inherited by the ERJ 190-400, FAA TCDS A57NM Rev 27 IX); ANAC CE/SC 25-065 for the ERJ 190-300/400 as summarised in CAA NZ TAR 19/21B/25 Rev 2 (p 10); EASA TCDS EASA.IM.A.071 Issue 29, Section 5 basis CS 25 Amendment 14 (p 37), which contains CS 25.335(b)(1)(ii) (ED Decision 2013/010/R)",
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::mission::LimitTable;

    #[test]
    fn the_declared_rows_form_valid_limit_tables() {
        for limits in [LJ60, C295, A320_200, A220_300, A340_300, E195_E2, ATR72_212A, DC10_30, A400M_180, AVE_DESIGN_RULE, A350_900, A380_800] {
            for rows in [limits.vmo_cas_m_s, limits.mmo] {
                assert!(LimitTable::from_rows(rows).is_some(), "{}", limits.source);
            }
        }
    }

    #[test]
    fn the_ave_limits_are_the_design_rule_at_its_cruise_mach() {
        let cruise_mach = crate::presets::get("AVE").unwrap().requirements.cruise_mach;
        let mmo = cruise_mach + 0.05;
        let ambient = alas_atmo::us1976_compute_values(25_839.0 * FT_M, 0.0);
        let tas = alas_atmo::airspeed::true_from_mach(mmo, ambient.temperature_k).unwrap();
        let vmo = alas_atmo::airspeed::calibrated_from_true(
            tas,
            ambient.pressure_pa,
            ambient.temperature_k,
        )
        .unwrap();
        assert_eq!(AVE_DESIGN_RULE.mmo.len(), 1);
        assert!((AVE_DESIGN_RULE.mmo[0].1 - mmo).abs() < 1.0e-12, "{mmo}");
        assert!((AVE_DESIGN_RULE.vmo_cas_m_s[0].1 - vmo).abs() < 1.0e-3, "{vmo}");
    }

    #[test]
    fn the_lj60_vmo_and_mmo_lines_meet_where_the_chart_draws_them() {
        // At 27,300 ft the chart's 330 kt VMO line turns into the MMO line:
        // 330 kt calibrated there is Mach 0.81 within the chart reading.
        let ambient = alas_atmo::us1976_compute_values(27_300.0 * FT_M, 0.0);
        let mach =
            alas_atmo::airspeed::mach_from_calibrated(330.0 * KT_M_S, ambient.pressure_pa).unwrap();
        assert!((mach - 0.81).abs() < 0.01, "{mach}");
    }

    #[test]
    fn a_preset_with_a_declared_vfe_slows_below_it_before_the_landing_configuration() {
        for name in ["LJ60", "C295", "A320-200"] {
            let profile = crate::presets::get(name)
                .unwrap()
                .operational_mission_defaults()
                .profile;
            let vfe = profile.vfe_landing_cas_m_s;
            assert!(vfe > 0.0, "{name}");
            assert_eq!(
                profile.climb_descent_speed_reference,
                crate::mission::SpeedReference::CalibratedAirspeed,
                "{name}"
            );
            assert!(profile.descent_4_air_speed_m_s <= vfe, "{name}");
            assert!(profile.landing_air_speed_m_s <= vfe, "{name}");
        }
    }

    /// Hand case, 66,000 kg on 122.6 m^2 with CLmax 2.6: VS0 = sqrt(2 * 66,000 *
    /// 9.80665 / (1.225 * 122.6 * 2.6)) = 57.58 m/s (111.9 kt); the default 1.8 VS0 = 103.6 m/s.
    #[test]
    fn the_default_vfe_is_the_landing_flap_design_speed_floor() {
        let vfe = default_landing_vfe_cas_m_s(66_000.0, 122.6, 2.6).unwrap();
        let vs0 = (2.0 * 66_000.0 * 9.806_65 / (1.225 * 122.6 * 2.6_f64)).sqrt();
        assert!((vfe - 1.8 * vs0).abs() < 1.0e-9, "{vfe}");
        assert!((vs0 - 57.58).abs() < 0.01, "{vs0}");
        assert_eq!(default_landing_vfe_cas_m_s(0.0, 122.6, 2.6), None);
        assert_eq!(default_landing_vfe_cas_m_s(66_000.0, 122.6, f64::NAN), None);
    }

    /// Every preset flies the approach below a landing-flap VFE: its type certificate's where it
    /// states one, otherwise the declared default, and it says which.
    #[test]
    fn every_preset_holds_its_approach_to_a_vfe_and_states_its_source() {
        for preset in crate::presets::registry() {
            let profile = preset.operational_mission_defaults().profile;
            let vfe = profile.vfe_landing_cas_m_s;
            assert!(vfe > 0.0, "{}", preset.name);
            assert!(profile.descent_4_air_speed_m_s <= vfe, "{}", preset.name);
            assert!(profile.landing_air_speed_m_s <= vfe, "{}", preset.name);
            let stated = preset
                .reference
                .operating_speed_limits
                .is_some_and(|limits| limits.vfe_landing_cas_m_s > 0.0);
            let expected = if stated {
                LandingVfeSource::TypeCertificate
            } else {
                LandingVfeSource::DesignFlapSpeedDefault
            };
            assert_eq!(preset.landing_vfe_source(), expected, "{}", preset.name);
        }
    }

    #[test]
    fn one_row_sets_the_single_value_and_more_rows_the_schedule() {
        let mut profile = MissionProfileConfig::default();
        C295.apply(&mut profile);
        assert_eq!(profile.max_operating_cas_m_s, 0.0);
        assert_eq!(profile.vmo_schedule.len(), 4);
        assert_eq!(profile.max_operating_mach, 0.0);
        assert!(profile.mmo_schedule.is_empty());
        let single = OperatingSpeedLimits {
            vmo_cas_m_s: &[(0.0, 150.0)],
            mmo: &[],
            vfe_landing_cas_m_s: 0.0,
            source: "",
        };
        single.apply(&mut profile);
        assert_eq!(profile.max_operating_cas_m_s, 150.0);
        assert!(profile.vmo_schedule.is_empty());
    }

    #[test]
    fn the_a220_flies_its_type_certificate_vmo_and_mmo_with_the_default_vfe() {
        let preset = crate::presets::get("A220-300").unwrap();
        let profile = preset.operational_mission_defaults().profile;
        let vmo_kt = |ft: f64| profile.vmo_cas_m_s_at_altitude(ft * FT_M) / KT_M_S;
        assert!((vmo_kt(5_000.0) - 300.0).abs() < 1.0e-9);
        assert!((vmo_kt(9_000.0) - 300.0).abs() < 1.0e-9);
        assert!((vmo_kt(20_000.0) - 330.0).abs() < 1.0e-9);
        assert!((profile.mmo_at_altitude(35_000.0 * FT_M) - 0.82).abs() < 1.0e-12);
        assert_eq!(
            preset.landing_vfe_source(),
            LandingVfeSource::DesignFlapSpeedDefault
        );
    }
}
