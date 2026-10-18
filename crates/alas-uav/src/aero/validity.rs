// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Validity guard for the NeuralFoil-based section models.
//!
//! NeuralFoil normalises `ln Re` and the angle of attack and extrapolates
//! silently outside its training data, so the guard is ours (UAV-PLAN 3.1).
//! Each (Re, alpha, Mach, confidence) query is classified as
//! [`Severity::Ok`], [`Severity::Warn`] (answer returned, flagged) or
//! [`Severity::Refused`] (outside any plausible domain).
//!
//! Thresholds (ESTIMATES; Re limits from plan section 3.1, the alpha limit
//! from the network's post-stall blend centre of +-20 deg in
//! `alas-aero::neuralfoil::corrections`):
//!
//! | quantity | ok | warn | refused |
//! |---|---|---|---|
//! | section Re | 8e4 .. 3e6 | 2e4 .. 8e4 and > 3e6 | < 2e4 |
//! | abs(alpha) | <= 20 deg | 20 .. 30 deg | > 30 deg |
//! | Mach | <= 0.12 | 0.12 .. 0.3 | > 0.3 |
//! | network confidence | >= 0.5 | 0.05 .. 0.5 | < 0.05 |

use std::fmt;

/// How serious a validity finding is.  Ordered: `Ok < Warn < Refused`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Inside the validated domain.
    Ok,
    /// Answer given, outside the validated domain.
    Warn,
    /// Outside any plausible domain; no answer should be trusted.
    Refused,
}

/// What left the domain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Flag {
    /// Section Reynolds number too low (value).
    LowReynolds(f64),
    /// Section Reynolds number above the data range (value).
    HighReynolds(f64),
    /// Effective angle of attack outside the polar range, deg (value).
    Alpha(f64),
    /// Mach number above the incompressible network's domain (value).
    Mach(f64),
    /// The network's own analysis confidence is low (value).
    Confidence(f64),
    /// A tabulated polar was queried outside its table (value is Re or alpha).
    OutsideTable(f64),
    /// The strip iteration did not converge (final residual).
    NotConverged(f64),
}

impl fmt::Display for Flag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LowReynolds(v) => write!(f, "section Re {v:.3e} below the validated range"),
            Self::HighReynolds(v) => write!(f, "section Re {v:.3e} above the data range"),
            Self::Alpha(v) => write!(
                f,
                "effective angle of attack {v:.1} deg outside the polar range"
            ),
            Self::Mach(v) => write!(f, "Mach {v:.3} above the incompressible domain"),
            Self::Confidence(v) => write!(f, "NeuralFoil confidence {v:.2} is low"),
            Self::OutsideTable(v) => write!(f, "query {v:.4e} outside the imported table"),
            Self::NotConverged(v) => write!(f, "strip iteration not converged (residual {v:.2e})"),
        }
    }
}

/// Domain limits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Section Re below which the query is refused.
    pub re_refuse: f64,
    /// Section Re below which the query is flagged.
    pub re_warn: f64,
    /// Section Re above which the query is flagged (UAV class stops near 3e6).
    pub re_high: f64,
    /// `abs(alpha)` above which the query is flagged, deg.
    pub alpha_warn_deg: f64,
    /// `abs(alpha)` above which the query is refused, deg.
    pub alpha_refuse_deg: f64,
    /// Mach above which the query is flagged.
    pub mach_warn: f64,
    /// Mach above which the query is refused.
    pub mach_refuse: f64,
    /// Confidence below which the query is flagged.
    pub confidence_warn: f64,
    /// Confidence below which the query is refused.
    pub confidence_refuse: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            re_refuse: 2.0e4,
            re_warn: 8.0e4,
            re_high: 3.0e6,
            alpha_warn_deg: 20.0,
            alpha_refuse_deg: 30.0,
            mach_warn: 0.12,
            mach_refuse: 0.3,
            confidence_warn: 0.5,
            confidence_refuse: 0.05,
        }
    }
}

/// Findings of one query or of a whole analysis.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Validity {
    /// Flags with their severity, in the order raised.
    pub flags: Vec<(Severity, Flag)>,
}

impl Validity {
    /// Worst severity (`Ok` when there are no flags).
    pub fn severity(&self) -> Severity {
        self.flags
            .iter()
            .map(|(s, _)| *s)
            .max()
            .unwrap_or(Severity::Ok)
    }

    /// Merge another finding set.
    pub fn extend(&mut self, other: &Validity) {
        self.flags.extend(other.flags.iter().copied());
    }

    /// Record one flag.
    pub fn push(&mut self, severity: Severity, flag: Flag) {
        self.flags.push((severity, flag));
    }
}

impl Limits {
    /// Classify a NeuralFoil query.  `confidence` is the network's
    /// `analysis_confidence` (`None` for non-network sources).
    pub fn classify(
        &self,
        re: f64,
        alpha_deg: f64,
        mach: f64,
        confidence: Option<f64>,
    ) -> Validity {
        let mut v = Validity::default();
        if !(re.is_finite() && alpha_deg.is_finite() && mach.is_finite()) {
            v.push(Severity::Refused, Flag::LowReynolds(re));
            return v;
        }
        if re < self.re_refuse {
            v.push(Severity::Refused, Flag::LowReynolds(re));
        } else if re < self.re_warn {
            v.push(Severity::Warn, Flag::LowReynolds(re));
        } else if re > self.re_high {
            v.push(Severity::Warn, Flag::HighReynolds(re));
        }
        let a = alpha_deg.abs();
        if a > self.alpha_refuse_deg {
            v.push(Severity::Refused, Flag::Alpha(alpha_deg));
        } else if a > self.alpha_warn_deg {
            v.push(Severity::Warn, Flag::Alpha(alpha_deg));
        }
        if mach > self.mach_refuse {
            v.push(Severity::Refused, Flag::Mach(mach));
        } else if mach > self.mach_warn {
            v.push(Severity::Warn, Flag::Mach(mach));
        }
        if let Some(c) = confidence {
            if !c.is_finite() || c < self.confidence_refuse {
                v.push(Severity::Refused, Flag::Confidence(c));
            } else if c < self.confidence_warn {
                v.push(Severity::Warn, Flag::Confidence(c));
            }
        }
        v
    }
}
