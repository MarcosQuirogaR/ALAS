// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fixture loading and tolerance assertions for the parity tests.
//!
//! Every translated module is checked against numbers the Python
//! implementation produced, stored under `golden/`. This crate is what those
//! checks are written against.
//!
//! Two decisions here shape how the parity tests read.
//!
//! The first is that a comparison collects every disagreement before it fails.
//! An assertion that stops at the first bad value tells you a module is wrong;
//! a report of all forty says whether one coefficient is off or the whole
//! array is shifted, and that difference is most of the diagnosis.
//!
//! The second is that tolerances are not chosen per test. They come from
//! [`Tier`], and a tier is picked for what the code does: closed-form
//! arithmetic, a factorization, an `f32` kernel, not for what it takes to
//! make today's numbers pass. Loosening a tier to get green is how a real
//! disagreement gets absorbed into a rounding allowance, so the tiers are
//! defined once, here, with the reason each exists.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use serde::de::DeserializeOwned;

/// How closely a translated result has to match the reference.
///
/// The bounds are relative, with an absolute floor that matters only near
/// zero, where a relative comparison is meaningless.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tier {
    /// Bit-for-bit. Statuses, integers, unit factors, generated input decks:
    /// things that are copied rather than computed, where any difference at
    /// all is a mistake.
    Exact,
    /// Closed-form `f64` arithmetic. The two implementations evaluate the same
    /// expressions in the same order, so they may differ only by the last
    /// ulp or so accumulated through a handful of operations.
    Closed,
    /// Anything through a factorization, a spline fit or a least squares.
    /// LAPACK and faer do not pivot identically, and the residual difference
    /// is real but bounded well below anything that matters physically.
    Linalg,
    /// Kernels the reference evaluates in `f32`. Single precision carries
    /// about seven decimal digits, and summation order over a few hundred
    /// panels consumes some of them.
    F32,
    /// Iteratively solved results, where the tolerance belongs to the quantity
    /// rather than to the arithmetic: two root finders that both converged to
    /// the same physical answer can differ by more than the solver tolerance.
    Iter {
        /// Relative bound for this quantity.
        relative: f64,
    },
}

/// Whether this build runs on the C runtime the tiers were established on.
///
/// The `Closed` and `Linalg` bounds were set by reproducing the reference on
/// Windows with the MSVC runtime. Other C runtimes (glibc, for the Linux
/// package) round the transcendental functions (`sin`, `atan2`, `exp`, `pow`)
/// to different last bits; the port calls the same functions in the same
/// order, so the difference is still rounding, but it is amplified through
/// fits and eigen-solves beyond the reference-runtime bounds. Measured on
/// ubuntu-22.04 on 2026-09-23: at most 1.8e-11 relative for closed-form
/// results and 6.8e-8 for least-squares fits. Off the reference runtime the
/// two tiers therefore widen once, here, to bounds still far below anything
/// that matters physically; the parity claim itself rests on the reference
/// runtime, where the gate keeps the original bounds.
pub const REFERENCE_RUNTIME: bool = cfg!(all(windows, target_env = "msvc"));

impl Tier {
    /// The relative and absolute bounds this tier allows.
    pub fn bounds(self) -> (f64, f64) {
        match self {
            Self::Exact => (0.0, 0.0),
            Self::Closed if REFERENCE_RUNTIME => (1e-12, 1e-15),
            Self::Closed => (1e-10, 1e-15),
            Self::Linalg if REFERENCE_RUNTIME => (1e-9, 1e-12),
            Self::Linalg => (1e-7, 1e-12),
            Self::F32 => (1e-5, 1e-7),
            Self::Iter { relative } => (relative, 0.0),
        }
    }

    fn name(self) -> String {
        match self {
            Self::Exact => "exact".to_owned(),
            Self::Closed => "closed".to_owned(),
            Self::Linalg => "linalg".to_owned(),
            Self::F32 => "f32".to_owned(),
            Self::Iter { relative } => format!("iter({relative:e})"),
        }
    }
}

/// Whether two values agree within a tier.
///
/// Two NaNs count as agreeing: the reference writes NaN where a quantity is
/// undefined, and reproducing that faithfully is agreement, not failure.
pub fn agrees(actual: f64, expected: f64, tier: Tier) -> bool {
    if actual.is_nan() || expected.is_nan() {
        return actual.is_nan() && expected.is_nan();
    }
    if actual == expected {
        return true;
    }
    // A relative bound on an infinite reference allows an infinite deviation,
    // which would make every comparison against an infinity pass. Infinities
    // agree only with themselves, which the equality above has already tested.
    if !actual.is_finite() || !expected.is_finite() {
        return false;
    }
    if tier == Tier::Exact {
        return false;
    }
    let (relative, absolute) = tier.bounds();
    let allowed = (relative * expected.abs()).max(absolute);
    (actual - expected).abs() <= allowed
}

/// An accumulating comparison against reference values.
///
/// Record everything, then call [`Comparison::finish`], which fails the test
/// with the whole report rather than the first line of it.
pub struct Comparison {
    tier: Tier,
    subject: String,
    checked: usize,
    failures: Vec<String>,
}

impl Comparison {
    /// Start comparing `subject` at `tier`.
    pub fn new(subject: impl Into<String>, tier: Tier) -> Self {
        Self {
            tier,
            subject: subject.into(),
            checked: 0,
            failures: Vec::new(),
        }
    }

    /// Compare one value.
    pub fn scalar(&mut self, name: &str, actual: f64, expected: f64) -> &mut Self {
        self.checked += 1;
        if !agrees(actual, expected, self.tier) {
            self.failures.push(format!(
                "  {name}: got {actual:.17e}, reference {expected:.17e} ({})",
                deviation(actual, expected)
            ));
        }
        self
    }

    /// Compare a sequence elementwise, reporting a length mismatch as its own
    /// finding rather than comparing whatever overlaps.
    pub fn slice(&mut self, name: &str, actual: &[f64], expected: &[f64]) -> &mut Self {
        if actual.len() != expected.len() {
            self.checked += 1;
            self.failures.push(format!(
                "  {name}: got {} values, reference has {}",
                actual.len(),
                expected.len()
            ));
            return self;
        }
        for (index, (&a, &e)) in actual.iter().zip(expected).enumerate() {
            self.scalar(&format!("{name}[{index}]"), a, e);
        }
        self
    }

    /// Compare something compared by equality rather than by tolerance.
    pub fn exact<T: PartialEq + std::fmt::Debug>(
        &mut self,
        name: &str,
        actual: &T,
        expected: &T,
    ) -> &mut Self {
        self.checked += 1;
        if actual != expected {
            self.failures
                .push(format!("  {name}: got {actual:?}, reference {expected:?}"));
        }
        self
    }

    /// Fail the test if anything disagreed.
    ///
    /// # Panics
    ///
    /// When any recorded comparison fell outside the tier, which is how a test
    /// reports failure.
    pub fn finish(&self) {
        if self.failures.is_empty() {
            return;
        }
        let report = self.failures.join("\n");
        panic!(
            "{} disagrees with the reference implementation \
             ({} of {} values outside tier `{}`):\n{report}",
            self.subject,
            self.failures.len(),
            self.checked,
            self.tier.name(),
        );
    }
}

fn deviation(actual: f64, expected: f64) -> String {
    if expected == 0.0 {
        return format!("absolute {:.3e}", (actual - expected).abs());
    }
    format!("relative {:.3e}", ((actual - expected) / expected).abs())
}

/// The `golden/` directory, found from this crate rather than the working
/// directory so a test behaves the same however it was invoked.
pub fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("golden"))
        .unwrap_or_else(|| PathBuf::from("golden"))
}

/// Load a fixture as raw JSON.
///
/// # Panics
///
/// When the fixture is absent or unreadable. A missing fixture means the
/// generator has not been run, which is a broken checkout rather than a
/// failing comparison, and saying so plainly beats a deserialization error.
pub fn load_json(family: &str, name: &str) -> serde_json::Value {
    let path = golden_dir().join(family).join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read the fixture at {}: {error}\n\
             run golden/generators/gen_{family}.py against the reference implementation",
            path.display()
        )
    });
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is not valid JSON: {error}", path.display()))
}

/// Load a fixture into a type.
///
/// # Panics
///
/// When the fixture is absent, or does not have the expected shape.
pub fn load<T: DeserializeOwned>(family: &str, name: &str) -> T {
    let value = load_json(family, name);
    serde_json::from_value(value).unwrap_or_else(|error| {
        panic!("the fixture {family}/{name} does not have the expected shape: {error}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_admits_no_difference() {
        assert!(agrees(1.0, 1.0, Tier::Exact));
        assert!(!agrees(1.0, 1.0 + f64::EPSILON, Tier::Exact));
    }

    #[test]
    fn closed_admits_a_few_ulps_but_not_a_real_shift() {
        assert!(agrees(1.0, 1.0 + f64::EPSILON, Tier::Closed));
        assert!(!agrees(1.0, 1.000_001, Tier::Closed));
    }

    #[test]
    fn f32_admits_single_precision_noise() {
        assert!(agrees(1.0, 1.000_000_1, Tier::F32));
        assert!(!agrees(1.0, 1.001, Tier::F32));
    }

    #[test]
    fn the_absolute_floor_applies_near_zero() {
        // A relative bound alone would reject this, since the reference is
        // exactly zero and any difference is infinitely far from it.
        assert!(agrees(1e-16, 0.0, Tier::Closed));
        assert!(!agrees(1e-9, 0.0, Tier::Closed));
    }

    #[test]
    fn undefined_values_agree_with_each_other() {
        assert!(agrees(f64::NAN, f64::NAN, Tier::Closed));
        assert!(!agrees(f64::NAN, 1.0, Tier::Closed));
        assert!(!agrees(1.0, f64::NAN, Tier::Closed));
    }

    #[test]
    fn infinities_agree_with_themselves() {
        assert!(agrees(f64::INFINITY, f64::INFINITY, Tier::Closed));
        assert!(!agrees(f64::INFINITY, f64::NEG_INFINITY, Tier::Closed));
    }

    #[test]
    fn a_clean_comparison_does_not_fail() {
        let mut comparison = Comparison::new("nothing", Tier::Closed);
        comparison
            .scalar("a", 1.0, 1.0)
            .slice("b", &[1.0, 2.0], &[1.0, 2.0]);
        comparison.finish();
    }

    #[test]
    #[should_panic(expected = "2 of 3 values")]
    fn every_disagreement_is_reported_not_only_the_first() {
        let mut comparison = Comparison::new("a module", Tier::Closed);
        comparison.scalar("good", 1.0, 1.0);
        comparison.scalar("bad", 2.0, 3.0);
        comparison.scalar("also_bad", 4.0, 5.0);
        comparison.finish();
    }

    #[test]
    #[should_panic(expected = "got 2 values, reference has 3")]
    fn a_length_mismatch_is_its_own_finding() {
        let mut comparison = Comparison::new("a sequence", Tier::Closed);
        comparison.slice("x", &[1.0, 2.0], &[1.0, 2.0, 3.0]);
        comparison.finish();
    }

    #[test]
    fn the_golden_directory_resolves() {
        assert!(golden_dir().ends_with("golden"));
    }

    #[test]
    fn a_fixture_double_is_recovered_bit_for_bit() {
        // Both loaders parse through serde_json, whose default float parser is
        // fast rather than correctly rounded. That costs up to one ulp, which is
        // invisible at every tier except the one where it matters most: an
        // `exact` comparison against a value the reference wrote is then
        // unwinnable. `float_roundtrip` in the workspace manifest buys the
        // correctly-rounded path, and this is what says so out loud; it fails
        // if that feature is ever dropped.
        //
        // The literal is Python's repr of an f32-derived double, the shape every
        // fixture that records single-precision reference output is full of.
        let text = "1.0019999742507935";
        let widened = f64::from(1.002_f32);
        assert_eq!(serde_json::from_str::<f64>(text).unwrap(), widened);

        // `load` goes through a `Value` before it reaches the target type, so
        // the round trip has to hold across that step too.
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(serde_json::from_value::<f64>(value).unwrap(), widened);
    }
}
