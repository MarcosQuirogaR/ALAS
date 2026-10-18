// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The score engine: validates a preset once (parse, names, dimensions) and
//! then evaluates rounds, rule gates and the flight aggregation.
//!
//! Inputs are SI numbers by name. Names resolve to, in order: round inputs,
//! preset parameter defaults, preset reference-field defaults. A name not
//! declared by the mission outputs, the extra inputs (`outputs`), the signals
//! or the preset is rejected, so a typo cannot silently read a default.

use std::collections::BTreeMap;

use crate::spec::mission::OUTPUTS;
use crate::spec::score::{Aggregation, Preset};

use super::eval::{eval, eval_scalar, Env, Value};
use super::outputs::{EXTRA_INPUTS, SIGNALS};
use super::parser::{parse, Expr};
use super::quantity::{Dim, Quantity, Series};
use super::{units, ScoreError};

/// Inputs of one scored round, SI.
#[derive(Debug, Clone, Default)]
pub struct RoundInputs {
    /// Scalars by name: mission outputs, decisions, parameter or reference
    /// overrides (in the unit declared for the name).
    pub values: BTreeMap<String, f64>,
    /// Signals by name (see `outputs::SIGNALS`).
    pub series: BTreeMap<String, Series>,
}

/// One evaluated rule gate.
#[derive(Debug, Clone, PartialEq)]
pub struct GateResult {
    /// Gate id.
    pub id: String,
    /// Description.
    pub description: String,
    /// Hard rule or soft margin.
    pub hard: bool,
    /// Normalised margin; >= 0 when satisfied, negative is the violation.
    pub margin: f64,
    /// `margin >= 0`.
    pub satisfied: bool,
    /// Rulebook citation.
    pub citation: String,
}

/// Result of one round.
#[derive(Debug, Clone, PartialEq)]
pub struct RoundResult {
    /// Terms in evaluation order.
    pub terms: Vec<(String, Quantity)>,
    /// Round score (points).
    pub score: f64,
    /// Rule gates.
    pub gates: Vec<GateResult>,
    /// Gates skipped because an input they read was not supplied.
    pub unevaluated_gates: Vec<String>,
    /// Every binding the round used (for the aggregation bonus).
    pub bindings: Env,
}

impl RoundResult {
    /// All evaluated hard gates satisfied. Gates without inputs are not
    /// counted: check [`Self::complete`] before trusting a `true`.
    pub fn feasible(&self) -> bool {
        self.gates.iter().all(|g| !g.hard || g.satisfied)
    }

    /// Every gate of the preset was evaluated.
    pub fn complete(&self) -> bool {
        self.unevaluated_gates.is_empty()
    }

    /// Value of a named term.
    pub fn term(&self, name: &str) -> Option<f64> {
        self.terms.iter().find(|t| t.0 == name).map(|t| t.1.value)
    }
}

struct Compiled {
    name: String,
    expr: Expr,
}

/// A validated preset ready to evaluate.
pub struct ScoreEngine {
    preset: Preset,
    declared: BTreeMap<String, (f64, Dim)>,
    signals: BTreeMap<String, Dim>,
    terms: Vec<Compiled>,
    round: Expr,
    gates: Vec<Compiled>,
    bonus: Option<Expr>,
}

fn unit_of(text: &str) -> Result<Quantity, ScoreError> {
    units::parse_unit(text).map_err(ScoreError::Preset)
}

impl ScoreEngine {
    /// Compile `preset` with the given variant (or its default variant).
    pub fn new(preset: &Preset, variant: Option<&str>) -> Result<Self, ScoreError> {
        let mut declared: BTreeMap<String, (f64, Dim)> = BTreeMap::new();
        let mut add = |name: &str, unit: &str| -> Result<(), ScoreError> {
            let u = unit_of(unit)?;
            if units::lookup(name).is_some() {
                return Err(ScoreError::Preset(format!(
                    "name '{name}' collides with a unit symbol"
                )));
            }
            if declared
                .insert(name.to_string(), (u.value, u.dim))
                .is_some()
            {
                return Err(ScoreError::Preset(format!(
                    "name '{name}' is declared twice"
                )));
            }
            Ok(())
        };
        for o in OUTPUTS {
            add(o.name, o.unit_si)?;
        }
        for d in EXTRA_INPUTS {
            add(d.name, d.unit_si)?;
        }
        for p in &preset.parameters {
            add(&p.name, &p.unit_si)?;
        }
        for r in &preset.score.reference_field {
            add(&r.name, &r.unit_si)?;
        }
        let mut signals = BTreeMap::new();
        for s in SIGNALS {
            signals.insert(s.name.to_string(), unit_of(s.unit_si)?.dim);
        }

        // Variant overrides replace term (or `round_expr`) text.
        let chosen = match variant {
            Some(id) => Some(
                preset
                    .variants
                    .iter()
                    .find(|v| v.id == id)
                    .ok_or_else(|| ScoreError::Preset(format!("unknown variant '{id}'")))?,
            ),
            None => preset.variants.iter().find(|v| v.default),
        };
        let overrides: BTreeMap<&str, &str> = chosen
            .map(|v| {
                v.overrides
                    .iter()
                    .map(|(k, e)| (k.as_str(), e.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        for key in overrides.keys() {
            let known = *key == "round_expr" || preset.score.terms.iter().any(|t| t.name == *key);
            if !known {
                return Err(ScoreError::Preset(format!(
                    "variant overrides unknown term '{key}'"
                )));
            }
        }
        let text_of = |name: &str, default: &str| -> String {
            overrides
                .get(name)
                .map_or_else(|| default.to_string(), |s| (*s).to_string())
        };

        let mut engine = ScoreEngine {
            preset: preset.clone(),
            declared,
            signals,
            terms: Vec::new(),
            round: Expr::Num(0.0),
            gates: Vec::new(),
            bonus: None,
        };
        for t in &preset.score.terms {
            let expr = parse(&text_of(&t.name, &t.expr)).map_err(|e| in_term(&t.name, e))?;
            engine.terms.push(Compiled {
                name: t.name.clone(),
                expr,
            });
        }
        engine.round = parse(&text_of("round_expr", &preset.score.round_expr))
            .map_err(|e| in_term("round_expr", e))?;
        for g in &preset.gates {
            let expr = parse(&g.margin_expr).map_err(|e| in_term(&g.id, e))?;
            engine.gates.push(Compiled {
                name: g.id.clone(),
                expr,
            });
        }
        if let Aggregation::MeanBestKPlusBonus { bonus_expr, .. } = &preset.score.aggregation {
            engine.bonus = Some(parse(bonus_expr).map_err(|e| in_term("aggregation bonus", e))?);
        }
        engine.check_dimensions()?;
        Ok(engine)
    }

    /// The compiled preset.
    pub fn preset(&self) -> &Preset {
        &self.preset
    }

    /// Dimension of a declared scalar name.
    pub fn dim_of(&self, name: &str) -> Option<Dim> {
        self.declared.get(name).map(|d| d.1)
    }

    /// Evaluate every expression on unit probes: any dimension error,
    /// unknown name or arity error surfaces at load time.
    fn check_dimensions(&self) -> Result<(), ScoreError> {
        let mut env = Env::new();
        for (name, (_, dim)) in &self.declared {
            env.insert(name.clone(), Value::Q(Quantity::new(1.0, *dim)));
        }
        for (name, dim) in &self.signals {
            let s = Series {
                t: vec![0.0, 1.0],
                y: vec![1.0, 1.0],
                dim: *dim,
            };
            env.insert(name.clone(), Value::S(s));
        }
        for t in &self.terms {
            let v = eval(&t.expr, &env).map_err(|e| in_term(&t.name, e))?;
            env.insert(t.name.clone(), v);
        }
        let r = eval(&self.round, &env).map_err(|e| in_term("round_expr", e))?;
        require_dimensionless(&r, "round_expr")?;
        for g in &self.gates {
            let v = eval(&g.expr, &env).map_err(|e| in_term(&g.name, e))?;
            require_dimensionless(&v, &g.name)?;
        }
        if let Some(b) = &self.bonus {
            env.insert("round_score".into(), Value::Q(Quantity::number(1.0)));
            let v = eval(b, &env).map_err(|e| in_term("aggregation bonus", e))?;
            require_dimensionless(&v, "aggregation bonus")?;
        }
        Ok(())
    }

    fn build_env(&self, inputs: &RoundInputs) -> Result<Env, ScoreError> {
        let mut env = Env::new();
        for p in &self.preset.parameters {
            let (factor, dim) = self.declared[&p.name];
            env.insert(
                p.name.clone(),
                Value::Q(Quantity::new(p.default * factor, dim)),
            );
        }
        for r in &self.preset.score.reference_field {
            let (factor, dim) = self.declared[&r.name];
            env.insert(
                r.name.clone(),
                Value::Q(Quantity::new(r.default * factor, dim)),
            );
        }
        for (name, v) in &inputs.values {
            let Some(&(factor, dim)) = self.declared.get(name) else {
                return Err(ScoreError::Input(format!(
                    "'{name}' is not a declared input"
                )));
            };
            if !v.is_finite() {
                return Err(ScoreError::Input(format!("'{name}' is not finite")));
            }
            if let Some(p) = self.preset.parameters.iter().find(|p| p.name == *name) {
                if *v < p.min || *v > p.max {
                    return Err(ScoreError::Input(format!(
                        "parameter '{name}' = {v} outside [{}, {}]",
                        p.min, p.max
                    )));
                }
            }
            env.insert(name.clone(), Value::Q(Quantity::new(v * factor, dim)));
        }
        for (name, s) in &inputs.series {
            let Some(&dim) = self.signals.get(name) else {
                return Err(ScoreError::Input(format!(
                    "'{name}' is not a declared signal"
                )));
            };
            if s.t.len() != s.y.len() || s.t.is_empty() || s.t.windows(2).any(|w| w[1] < w[0]) {
                return Err(ScoreError::Input(format!("signal '{name}' is malformed")));
            }
            env.insert(name.clone(), Value::S(Series { dim, ..s.clone() }));
        }
        Ok(env)
    }

    /// Evaluate terms, round score and gates for one round.
    pub fn evaluate_round(&self, inputs: &RoundInputs) -> Result<RoundResult, ScoreError> {
        let mut env = self.build_env(inputs)?;
        let mut terms = Vec::new();
        for t in &self.terms {
            let q = eval_scalar(&t.expr, &env, &t.name)?;
            env.insert(t.name.clone(), Value::Q(q));
            terms.push((t.name.clone(), q));
        }
        let score = eval_scalar(&self.round, &env, "round_expr")?.value;
        let mut gates = Vec::new();
        let mut unevaluated_gates = Vec::new();
        for (g, c) in self.preset.gates.iter().zip(&self.gates) {
            let margin = match eval_scalar(&c.expr, &env, &c.name) {
                Ok(q) => q.value,
                Err(ScoreError::UnknownName(_)) => {
                    unevaluated_gates.push(g.id.clone());
                    continue;
                }
                Err(e) => return Err(e),
            };
            gates.push(GateResult {
                id: g.id.clone(),
                description: g.description.clone(),
                hard: g.hard,
                margin,
                satisfied: margin >= 0.0,
                citation: g.citation.clone(),
            });
        }
        Ok(RoundResult {
            terms,
            score,
            gates,
            unevaluated_gates,
            bindings: env,
        })
    }

    /// Combine round results into the flight score with the preset's
    /// aggregation rule.
    pub fn flight_score(&self, rounds: &[RoundResult]) -> Result<f64, ScoreError> {
        let mut s: Vec<f64> = rounds.iter().map(|r| r.score).collect();
        if s.is_empty() {
            return Err(ScoreError::Input("no rounds to aggregate".into()));
        }
        s.sort_by(f64::total_cmp);
        let n = s.len();
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let best = |k: usize| s[n - k.min(n)..].to_vec();
        Ok(match &self.preset.score.aggregation {
            Aggregation::MeanBestK { k, min_rounds } => {
                if n >= *min_rounds as usize {
                    mean(&best(*k as usize))
                } else {
                    s[n - 1]
                }
            }
            Aggregation::MeanExceptWorst => {
                if n >= 2 {
                    mean(&s[1..])
                } else {
                    s[0]
                }
            }
            Aggregation::MeanDropWorstAbove { rounds: above } => {
                if n > *above as usize && n >= 2 {
                    mean(&s[1..])
                } else {
                    mean(&s)
                }
            }
            Aggregation::RankWeighted { weights } => {
                if n > weights.len() {
                    let fixed: f64 = weights.iter().sum();
                    let rest = (1.0 - fixed) / (n - weights.len()) as f64;
                    s.iter()
                        .enumerate()
                        .map(|(i, v)| v * weights.get(i).copied().unwrap_or(rest))
                        .sum()
                } else {
                    mean(&s)
                }
            }
            Aggregation::SumBestK { k } => best(*k as usize).iter().sum(),
            Aggregation::Single => s[n - 1],
            Aggregation::MeanBestKPlusBonus { k, .. } => {
                let mut ranked: Vec<&RoundResult> = rounds.iter().collect();
                ranked.sort_by(|a, b| b.score.total_cmp(&a.score));
                ranked.truncate((*k as usize).min(n));
                let mut bonus = 0.0_f64;
                if let Some(expr) = &self.bonus {
                    for r in ranked {
                        let mut env = r.bindings.clone();
                        env.insert("round_score".into(), Value::Q(Quantity::number(r.score)));
                        bonus = bonus.max(eval_scalar(expr, &env, "aggregation bonus")?.value);
                    }
                }
                mean(&best(*k as usize)) + bonus
            }
        })
    }
}

fn in_term(name: &str, e: ScoreError) -> ScoreError {
    match e {
        ScoreError::Parse { pos, message } => ScoreError::Parse {
            pos,
            message: format!("{name}: {message}"),
        },
        ScoreError::Dimension(m) => ScoreError::Dimension(format!("{name}: {m}")),
        ScoreError::UnknownName(m) => ScoreError::UnknownName(format!("{m} (in {name})")),
        ScoreError::Arity(m) => ScoreError::Arity(format!("{name}: {m}")),
        other => other,
    }
}

fn require_dimensionless(v: &Value, what: &str) -> Result<(), ScoreError> {
    match v {
        Value::Q(q) if q.dim.is_one() => Ok(()),
        Value::Q(q) => Err(ScoreError::Dimension(format!(
            "{what} must be dimensionless, got [{}]",
            q.dim
        ))),
        Value::S(_) => Err(ScoreError::Dimension(format!("{what} is a time series"))),
    }
}
