// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Dimension-checked evaluation of parsed expressions.
//!
//! Rules: `+`, `-`, comparisons, `min`, `max`, `clamp`, `if` branches and
//! `step`/`table` values need equal dimensions; `*` and `/` combine them; an
//! exponent must be a dimensionless number and must leave integral stored
//! exponents; transcendental functions need dimensionless arguments.
//! Numerical edge cases (division by zero, `sqrt` of a negative number)
//! follow IEEE and are reported by the caller when a *final* value is not
//! finite, so an untaken `if` branch cannot fail an evaluation; dimension
//! errors are raised in every branch, taken or not.

use std::collections::BTreeMap;

use super::parser::{BinOp, Expr};
use super::quantity::{Dim, Quantity, Series};
use super::units;
use super::ScoreError;

/// A value an expression can handle.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Scalar.
    Q(Quantity),
    /// Time series.
    S(Series),
}

/// Variable bindings.
pub type Env = BTreeMap<String, Value>;

fn dim_err<T>(message: String) -> Result<T, ScoreError> {
    Err(ScoreError::Dimension(message))
}

fn scalar(v: Value, what: &str) -> Result<Quantity, ScoreError> {
    match v {
        Value::Q(q) => Ok(q),
        Value::S(_) => dim_err(format!("{what}: a time series cannot be used here")),
    }
}

fn same_dim(a: Quantity, b: Quantity, op: &str) -> Result<(), ScoreError> {
    if a.dim == b.dim {
        Ok(())
    } else {
        dim_err(format!("{op}: [{}] and [{}] differ", a.dim, b.dim))
    }
}

fn dimless(q: Quantity, what: &str) -> Result<f64, ScoreError> {
    if q.dim.is_one() {
        Ok(q.value)
    } else {
        dim_err(format!(
            "{what} needs a dimensionless argument, got [{}]",
            q.dim
        ))
    }
}

fn flag(b: bool) -> Quantity {
    Quantity::number(if b { 1.0 } else { 0.0 })
}

/// Evaluate `expr` in `env`.
pub fn eval(expr: &Expr, env: &Env) -> Result<Value, ScoreError> {
    match expr {
        Expr::Num(v) => Ok(Value::Q(Quantity::number(*v))),
        Expr::Var(name) => match env.get(name) {
            Some(v) => Ok(v.clone()),
            None => units::lookup(name)
                .map(Value::Q)
                .ok_or_else(|| ScoreError::UnknownName(name.clone())),
        },
        Expr::Neg(e) => {
            let q = scalar(eval(e, env)?, "unary -")?;
            Ok(Value::Q(Quantity::new(-q.value, q.dim)))
        }
        Expr::Not(e) => {
            let q = scalar(eval(e, env)?, "!")?;
            Ok(Value::Q(flag(dimless(q, "!")? == 0.0)))
        }
        Expr::Bin(op, a, b) => {
            let x = scalar(eval(a, env)?, "operand")?;
            let y = scalar(eval(b, env)?, "operand")?;
            binary(*op, x, y).map(Value::Q)
        }
        Expr::Call(name, args) => {
            let vals = args
                .iter()
                .map(|a| eval(a, env))
                .collect::<Result<Vec<_>, _>>()?;
            call(name, vals).map(|q| match q {
                Out::Q(q) => Value::Q(q),
                Out::S(s) => Value::S(s),
            })
        }
    }
}

/// Evaluate to a finite scalar.
pub fn eval_scalar(expr: &Expr, env: &Env, what: &str) -> Result<Quantity, ScoreError> {
    let q = scalar(eval(expr, env)?, what)?;
    if q.value.is_finite() {
        Ok(q)
    } else {
        Err(ScoreError::NonFinite(what.to_string()))
    }
}

fn binary(op: BinOp, x: Quantity, y: Quantity) -> Result<Quantity, ScoreError> {
    let q = |v: f64, d: Dim| Ok(Quantity::new(v, d));
    match op {
        BinOp::Add => {
            same_dim(x, y, "+")?;
            q(x.value + y.value, x.dim)
        }
        BinOp::Sub => {
            same_dim(x, y, "-")?;
            q(x.value - y.value, x.dim)
        }
        BinOp::Mul => q(x.value * y.value, x.dim.times(y.dim)),
        BinOp::Div => q(x.value / y.value, x.dim.over(y.dim)),
        BinOp::Pow => {
            let p = dimless(y, "exponent")?;
            let d = if x.dim.is_one() {
                Dim::ONE
            } else {
                x.dim.powf(p).ok_or_else(|| {
                    ScoreError::Dimension(format!(
                        "[{}]^{p} has a non-representable dimension",
                        x.dim
                    ))
                })?
            };
            q(x.value.powf(p), d)
        }
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Ne => {
            same_dim(x, y, "comparison")?;
            let r = match op {
                BinOp::Lt => x.value < y.value,
                BinOp::Le => x.value <= y.value,
                BinOp::Gt => x.value > y.value,
                BinOp::Ge => x.value >= y.value,
                BinOp::Eq => x.value == y.value,
                _ => x.value != y.value,
            };
            Ok(flag(r))
        }
        BinOp::And | BinOp::Or => {
            let (a, b) = (
                dimless(x, "logical operator")? != 0.0,
                dimless(y, "logical operator")? != 0.0,
            );
            Ok(flag(if op == BinOp::And { a && b } else { a || b }))
        }
    }
}

enum Out {
    Q(Quantity),
    S(Series),
}

fn arity(name: &str, got: usize, ok: bool, expect: &str) -> Result<(), ScoreError> {
    if ok {
        Ok(())
    } else {
        Err(ScoreError::Arity(format!(
            "{name} takes {expect}, got {got}"
        )))
    }
}

fn call(name: &str, vals: Vec<Value>) -> Result<Out, ScoreError> {
    let n = vals.len();
    // Series functions first: their first argument is not a scalar.
    match name {
        "integral_above" | "integral" | "series_max" | "series_mean" | "block_mean" => {
            return series_call(name, vals);
        }
        _ => {}
    }
    let args = vals
        .into_iter()
        .map(|v| scalar(v, name))
        .collect::<Result<Vec<_>, _>>()?;
    let q = |v: f64, d: Dim| Ok(Out::Q(Quantity::new(v, d)));
    match name {
        "min" | "max" => {
            arity(name, n, n >= 1, "at least one argument")?;
            let mut best = args[0];
            for a in &args[1..] {
                same_dim(best, *a, name)?;
                let take = if name == "min" {
                    a.value < best.value
                } else {
                    a.value > best.value
                };
                if take || best.value.is_nan() {
                    best = *a;
                }
            }
            Ok(Out::Q(best))
        }
        "clamp" => {
            arity(name, n, n == 3, "x, lo, hi")?;
            same_dim(args[0], args[1], name)?;
            same_dim(args[0], args[2], name)?;
            q(
                args[0].value.max(args[1].value).min(args[2].value),
                args[0].dim,
            )
        }
        "abs" => {
            arity(name, n, n == 1, "one argument")?;
            q(args[0].value.abs(), args[0].dim)
        }
        "sqrt" => {
            arity(name, n, n == 1, "one argument")?;
            let d = args[0].dim.sqrt().ok_or_else(|| {
                ScoreError::Dimension(format!("sqrt of [{}] is not representable", args[0].dim))
            })?;
            q(args[0].value.sqrt(), d)
        }
        "pow" => {
            arity(name, n, n == 2, "x, p")?;
            binary(BinOp::Pow, args[0], args[1]).map(Out::Q)
        }
        "exp" | "ln" | "sin" | "cos" | "tan" | "atan" | "floor" | "ceil" | "round" => {
            arity(name, n, n == 1, "one argument")?;
            let x = dimless(args[0], name)?;
            let v = match name {
                "exp" => x.exp(),
                "ln" => x.ln(),
                "sin" => x.sin(),
                "cos" => x.cos(),
                "tan" => x.tan(),
                "atan" => x.atan(),
                "floor" => x.floor(),
                "ceil" => x.ceil(),
                _ => x.round(),
            };
            q(v, Dim::ONE)
        }
        "if" => {
            arity(name, n, n == 3, "cond, then, else")?;
            let c = dimless(args[0], "if condition")?;
            same_dim(args[1], args[2], "if branches")?;
            Ok(Out::Q(if c != 0.0 { args[1] } else { args[2] }))
        }
        "rel" => {
            arity(name, n, n == 2, "x, reference")?;
            same_dim(args[0], args[1], "rel")?;
            q(args[0].value / args[1].value, Dim::ONE)
        }
        "step" => step(&args).map(Out::Q),
        "table" => table(&args).map(Out::Q),
        _ => Err(ScoreError::UnknownName(format!("function {name}"))),
    }
}

/// `step(x, e1, v1, e2, v2, ..., default)`: `v_k` for the first `e_k >= x`
/// (edges ascending), else `default`.
fn step(args: &[Quantity]) -> Result<Quantity, ScoreError> {
    let n = args.len();
    arity(
        "step",
        n,
        n >= 2 && n % 2 == 0,
        "x, (edge, value) pairs, default",
    )?;
    let x = args[0];
    let default = args[n - 1];
    let pairs = &args[1..n - 1];
    for p in pairs.chunks(2) {
        same_dim(x, p[0], "step edge")?;
        same_dim(default, p[1], "step value")?;
    }
    for p in pairs.chunks(2) {
        if x.value <= p[0].value {
            return Ok(p[1]);
        }
    }
    Ok(default)
}

/// `table(x, x0, y0, x1, y1, ...)`: piecewise-linear, clamped at the ends.
fn table(args: &[Quantity]) -> Result<Quantity, ScoreError> {
    let n = args.len();
    arity(
        "table",
        n,
        n >= 5 && n % 2 == 1,
        "x and at least two (x, y) points",
    )?;
    let x = args[0];
    let pts: Vec<(Quantity, Quantity)> = args[1..].chunks(2).map(|c| (c[0], c[1])).collect();
    for (px, py) in &pts {
        same_dim(x, *px, "table abscissa")?;
        same_dim(pts[0].1, *py, "table ordinate")?;
    }
    let first = pts[0];
    let last = pts[pts.len() - 1];
    if x.value <= first.0.value {
        return Ok(first.1);
    }
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if x.value <= b.0.value {
            let f = (x.value - a.0.value) / (b.0.value - a.0.value);
            return Ok(Quantity::new(
                a.1.value + f * (b.1.value - a.1.value),
                a.1.dim,
            ));
        }
    }
    Ok(last.1)
}

fn series_call(name: &str, vals: Vec<Value>) -> Result<Out, ScoreError> {
    let n = vals.len();
    let mut it = vals.into_iter();
    let Some(Value::S(s)) = it.next() else {
        return dim_err(format!("{name}: the first argument must be a time series"));
    };
    let rest = it.map(|v| scalar(v, name)).collect::<Result<Vec<_>, _>>()?;
    match name {
        "integral_above" => {
            arity(name, n, n == 2, "series, limit")?;
            if rest[0].dim != s.dim {
                return dim_err(format!(
                    "integral_above: limit [{}] differs from the signal [{}]",
                    rest[0].dim, s.dim
                ));
            }
            Ok(Out::Q(Quantity::new(
                s.integral_above(rest[0].value),
                s.dim.times(Dim::TIME),
            )))
        }
        "integral" => {
            arity(name, n, n == 1, "series")?;
            Ok(Out::Q(Quantity::new(s.integral(), s.dim.times(Dim::TIME))))
        }
        "series_max" => {
            arity(name, n, n == 1, "series")?;
            Ok(Out::Q(Quantity::new(s.max(), s.dim)))
        }
        "series_mean" => {
            arity(name, n, n == 1, "series")?;
            Ok(Out::Q(Quantity::new(s.mean(), s.dim)))
        }
        _ => {
            arity(name, n, n == 2, "series, window")?;
            if rest[0].dim != Dim::TIME {
                return dim_err(format!(
                    "block_mean window must be a time, got [{}]",
                    rest[0].dim
                ));
            }
            Ok(Out::S(s.block_mean(rest[0].value)))
        }
    }
}
