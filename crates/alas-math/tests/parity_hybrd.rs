// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares MINPACK's `hybrd` against `scipy.optimize.fsolve`, by replaying
//! the reference's entire sequence of residual evaluations.
//!
//! **The root is the weak half of this check and the call log is the strong
//! half.** Most root finders land on the root of a well-posed system; what
//! distinguishes this translation from a different solver of similar quality
//! is *where it went on the way*, because `hybrd` stops on its trust region
//! rather than on its residual and therefore returns wherever the iteration
//! was standing. So the fixture records every point the reference evaluated
//! at, in call order, and this test asserts that the port evaluated at the
//! same points in the same order: 390 of them across eight systems. A
//! divergence in the finite-difference step, in the dogleg blend, in the
//! trust-region update or in the rank-one Broyden correction shows up here as
//! the index at which the two paths part, which is most of the diagnosis.
//!
//! Two tiers, the split this ledger's payload and drag rows already carry.
//! Discrete at `exact`: the number of evaluations, which is MINPACK's `nfev`,
//! and the termination status, which is its `info`. An implementation that
//! agreed on every number while spending a different number of evaluations
//! would have taken a different path to the same place, and that is a
//! disagreement however close the root is. Everything continuous is compared
//! at `linalg`, the tier the ledger names for this row.
//!
//! The residual systems are reimplemented here rather than read from the
//! fixture, and they have to be: the point of replaying a call log is that
//! the port drives its *own* iteration, and a residual looked up by index
//! would make the log agree with itself. They are transcribed from
//! `gen_math_hybrd.py`, which chose them to be closed-form algebra for this
//! reason, no library call whose last ulp could differ between the two
//! languages sits between the unknowns and the residual.
//!
//! That transcription has to preserve *associativity*, not just the formula.
//! The Jacobian here is a forward difference divided by `h = sqrt(eps)`, about
//! `1.5e-8`, so a one-ulp difference in a residual at a probe point is
//! amplified by eight orders into a Jacobian entry and straight into the next
//! trial point. Writing `sqrt(10) * (d*d)` as `(sqrt(10) * d) * d` is enough
//! to part the two paths at the first step, which is what happened while
//! this test was being written, and why each system below is transcribed
//! bracket for bracket rather than merely term for term. The same sensitivity
//! is what makes the call log worth recording: it detects at the first step
//! what a comparison of final roots would have absorbed.
//!
//! # Why the residual is compared by norm along the path and not component by
//! # component
//!
//! A residual is what a system is trying to drive to zero, so its components
//! pass through zero, and several of them do so by subtracting nearly equal
//! numbers: Rosenbrock's second row is literally `1 - x[0]` as `x[0]`
//! approaches one. There, a difference in the last ulp of an `x` that the
//! tier already permits is amplified into a large *relative* difference in
//! `f`, while the absolute difference stays around `5e-12`. That is
//! cancellation in the test's own arithmetic, not a disagreement between two
//! solvers, and framing it as one would be reading the tier backwards.
//!
//! `hybrd` itself never reads a residual component in isolation: every
//! decision it makes: accept the step, grow or shrink the trust region,
//! declare convergence: goes through `enorm(fvec)`, and the components
//! enter the Jacobian only as differences that are then factored. So the norm
//! is compared along the whole path, because that is the quantity the
//! iteration branches on, and the components are compared at the one
//! evaluation where both implementations stand at exactly the same point by
//! construction (the starting point) which is where a mistranscribed
//! system would show up. Anywhere later, a mistranscribed system moves the
//! path, and the `x` comparison catches it first and more clearly.

use alas_math::hybrd::{self, Settings, Status};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Evaluation {
    x: Vec<f64>,
    f: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    system: String,
    n: usize,
    x0: Vec<f64>,
    xtol: f64,
    maxfev: usize,
    epsfcn: f64,
    factor: f64,
    ier: i64,
    nfev: usize,
    x: Vec<f64>,
    fvec: Vec<f64>,
    evaluations: Vec<Evaluation>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

// ----------------------------------------------------------------------
//  The systems, transcribed from the generator
// ----------------------------------------------------------------------

fn rosenbrock(x: &[f64], out: &mut [f64]) {
    out[0] = 10.0 * (x[1] - x[0] * x[0]);
    out[1] = 1.0 - x[0];
}

fn powell_singular(x: &[f64], out: &mut [f64]) {
    // The last row is `sqrt(10) * (x0 - x3)**2`, and the parenthesis is
    // load-bearing: Python squares first and then scales, so writing it as
    // `(sqrt(10) * d) * d` rounds in a different place. That is a one-ulp
    // difference in a residual, which the Jacobian below then divides by
    // `h ~ 1.5e-8` and turns into a 1e-8 difference in the first trial point.
    // See this file's header on why residual associativity is not a detail
    // here.
    let inner = x[1] - 2.0 * x[2];
    let diagonal = x[0] - x[3];
    out[0] = x[0] + 10.0 * x[1];
    out[1] = 5.0_f64.sqrt() * (x[2] - x[3]);
    out[2] = inner * inner;
    out[3] = 10.0_f64.sqrt() * (diagonal * diagonal);
}

fn helical_valley(x: &[f64], out: &mut [f64]) {
    let theta = if x[0] > 0.0 {
        (x[1] / x[0]).atan() / (2.0 * std::f64::consts::PI)
    } else if x[0] < 0.0 {
        (x[1] / x[0]).atan() / (2.0 * std::f64::consts::PI) + 0.5
    } else if x[1] >= 0.0 {
        0.25
    } else {
        -0.25
    };
    let r = (x[0] * x[0] + x[1] * x[1]).sqrt();
    out[0] = 10.0 * (x[2] - 10.0 * theta);
    out[1] = 10.0 * (r - 1.0);
    out[2] = x[2];
}

fn broyden_tridiagonal(x: &[f64], out: &mut [f64]) {
    let n = x.len();
    for i in 0..n {
        let left = if i > 0 { x[i - 1] } else { 0.0 };
        let right = if i < n - 1 { x[i + 1] } else { 0.0 };
        out[i] = (3.0 - 2.0 * x[i]) * x[i] - left - 2.0 * right + 1.0;
    }
}

fn trigonometric(x: &[f64], out: &mut [f64]) {
    let n = x.len();
    let total: f64 = x.iter().map(|v| v.cos()).sum();
    for i in 0..n {
        out[i] = n as f64 - total + (i as f64 + 1.0) * (1.0 - x[i].cos()) - x[i].sin();
    }
}

fn force_balance(x: &[f64], out: &mut [f64]) {
    let points = x.len() / 2;
    for i in 0..points {
        let throttle = x[i];
        let alpha = x[points + i];
        let weight = 1.0 + 0.05 * i as f64;
        let thrust = 0.6 * throttle + 0.15 * throttle * throttle;
        let lift = 4.5 * alpha + 0.25;
        let drag = 0.02 + 0.05 * lift * lift;
        out[2 * i] = thrust * alpha.cos() - drag;
        out[2 * i + 1] = thrust * alpha.sin() + lift - weight;
    }
}

fn system_by_name(name: &str) -> fn(&[f64], &mut [f64]) {
    match name {
        "rosenbrock" => rosenbrock,
        "powell_singular" => powell_singular,
        "helical_valley" => helical_valley,
        "broyden_tridiagonal" => broyden_tridiagonal,
        "trigonometric" => trigonometric,
        "force_balance" => force_balance,
        other => panic!("fixture names an unknown system: {other}"),
    }
}

/// The Euclidean norm, which is the only thing `hybrd` reads a residual
/// vector through.
fn norm(f: &[f64]) -> f64 {
    f.iter().map(|v| v * v).sum::<f64>().sqrt()
}

/// MINPACK's `info`, which SciPy returns as `ier`.
fn status_from_ier(ier: i64) -> Status {
    match ier {
        1 => Status::Converged,
        2 => Status::MaxEvaluations,
        3 => Status::ToleranceTooSmall,
        4 => Status::NoProgressSinceJacobians,
        5 => Status::NoProgressSinceIterations,
        other => panic!("fixture reports an ier this port has no status for: {other}"),
    }
}

#[test]
fn hybrd_matches_scipy_fsolve() {
    let fixture: Fixture = alas_testkit::load("math", "hybrd");
    assert!(!fixture.cases.is_empty(), "fixture has no cases to check");

    for case in &fixture.cases {
        let subject = format!("hybrd({})", case.name);
        let system = system_by_name(&case.system);

        // Drive the port's own iteration, logging where it goes.
        let mut log: Vec<Evaluation> = Vec::new();
        let settings = Settings {
            xtol: case.xtol,
            max_evaluations: Some(case.maxfev),
            step_size: Some(case.epsfcn),
            factor: case.factor,
        };
        let solution = hybrd::solve(
            |x: &[f64], out: &mut [f64]| {
                system(x, out);
                log.push(Evaluation {
                    x: x.to_vec(),
                    f: out.to_vec(),
                });
            },
            &case.x0,
            &settings,
        )
        .unwrap_or_else(|error| panic!("{subject} was refused: {error:?}"));

        // Off the reference runtime the systems' transcendental functions
        // round differently, and a Powell dogleg trajectory amplifies that
        // until the two paths legitimately part (ubuntu-22.04 parts from
        // SciPy at evaluation 14 of the helical valley). The path, the
        // evaluation count and a non-converged stopping point are properties
        // of that trajectory, so there only the converged roots are compared:
        // two converged solves of the same system must find the same root.
        if !alas_testkit::REFERENCE_RUNTIME {
            if case.ier == 1 {
                let mut root = Comparison::new(
                    format!("{subject} [converged root, off reference runtime]"),
                    Tier::Iter { relative: 1e-6 },
                );
                root.exact("status", &solution.status, &status_from_ier(case.ier));
                for (index, (got, want)) in solution.x.iter().zip(&case.x).enumerate() {
                    // Components at a zero root compare absolutely.
                    let tolerance = 1e-6 * want.abs().max(1e-3);
                    root.exact(
                        &format!("|x[{index}] - reference| <= {tolerance:e}"),
                        &((got - want).abs() <= tolerance),
                        &true,
                    );
                }
                root.finish();
            }
            continue;
        }

        // --- The iteration path, at `linalg` ---
        //
        // Checked before the evaluation count, deliberately: a wrong count is
        // a *consequence* of the paths parting, and reporting the count first
        // says only that they parted. Report the index where they part and
        // the two points there, which says how.
        let divergence = log.iter().zip(&case.evaluations).position(|(got, want)| {
            got.x
                .iter()
                .zip(&want.x)
                .any(|(a, b)| !alas_testkit::agrees(*a, *b, Tier::Linalg))
        });
        if let Some(index) = divergence {
            let mut comparison =
                Comparison::new(format!("{subject} evaluation #{index}"), Tier::Linalg);
            comparison.slice("x", &log[index].x, &case.evaluations[index].x);
            comparison.slice("f", &log[index].f, &case.evaluations[index].f);
            comparison.finish();
            panic!(
                "{subject}: the iteration paths agree for {index} evaluations and part at \
                 #{index}, but that point compared equal: the divergence detector and the \
                 comparison disagree"
            );
        }

        // Every point agreed. Check the residual too: component by component
        // at the starting point, where both sides stand at exactly `x0` and a
        // mistranscribed system is the only thing that could differ, and by
        // norm everywhere after, for the reason this file's header gives.
        let mut path = Comparison::new(format!("{subject} [path]"), Tier::Linalg);
        for (index, (got, want)) in log.iter().zip(&case.evaluations).enumerate() {
            if index == 0 {
                path.slice("f @ x0", &got.f, &want.f);
            }
            path.scalar(&format!("|f| @ #{index}"), norm(&got.f), norm(&want.f));
        }
        path.finish();

        // --- Discrete, at `exact` ---
        let mut discrete = Comparison::new(format!("{subject} [discrete]"), Tier::Exact);
        discrete.exact("status", &solution.status, &status_from_ier(case.ier));
        discrete.exact("nfev", &solution.evaluations, &case.nfev);
        discrete.exact("logged evaluations", &log.len(), &case.evaluations.len());
        discrete.finish();

        // --- The answer, at `linalg` ---
        let mut answer = Comparison::new(format!("{subject} [solution]"), Tier::Linalg);
        answer.slice("x", &solution.x, &case.x);
        answer.scalar("|fvec|", norm(&solution.residual), norm(&case.fvec));
        answer.finish();

        assert_eq!(
            solution.x.len(),
            case.n,
            "{subject}: wrong number of unknowns"
        );
    }
}

#[test]
fn every_termination_status_the_reference_reaches_is_covered() {
    // A fixture in which every case converged would leave the four failure
    // exits unchecked, and this port distinguishes them.
    let fixture: Fixture = alas_testkit::load("math", "hybrd");
    let mut seen: Vec<i64> = fixture.cases.iter().map(|c| c.ier).collect();
    seen.sort_unstable();
    seen.dedup();
    assert!(
        seen.len() >= 3,
        "the fixture reaches only {} termination status(es) ({seen:?}); the exits this \
         port distinguishes would be mostly unchecked",
        seen.len()
    );
    assert!(seen.contains(&1), "no case converged");
}
