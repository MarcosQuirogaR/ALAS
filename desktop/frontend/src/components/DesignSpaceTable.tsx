// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useEffect, useState } from "react";

// An editable Variable / Unit /
// Initial Value / Lower / Upper / Description table for the optimizer's
// search space, driven entirely by GET /design-space/specs (a JSON view of
// DESIGN_VARIABLE_SPECS) so a new design DOF needs no frontend code, same
// contract as the Qt table.

export type DesignSpaceSpec = {
  name: string;
  label: string;
  unit: string;
  description: string;
  default: number;
  lower: number;
  upper: number;
  /** Display-only rounding for the Initial/Lower/Upper columns (see
   * DesignVariableSpec.decimals) -- full precision is still kept internally. */
  decimals: number;
};

type Row = { name: string; initial: number; lower: number; upper: number };

type DesignSpaceTableProps = {
  specs: DesignSpaceSpec[];
  /** A DesignVector-shaped dict (e.g. from a just-loaded preset). When this
   * prop changes identity, Initial Value is set to the new vector and
   * Lower/Upper are rescaled around it -- mirrors
   * design_space_table.py::set_design_vector exactly. */
  designVector?: Record<string, number> | null;
  onChange: (initialDesign: Record<string, number>, bounds: [number, number][]) => void;
};

/** Mirrors design_space_table.py::set_design_vector's bound-expansion rule. */
function expandedBounds(name: string, val: number, spec: DesignSpaceSpec): [number, number] {
  if (name.includes("bump")) return [spec.lower, spec.upper];
  if (name.includes("twist")) return [-5.0, -1.0];
  if (name.includes("shift")) return [spec.lower, spec.upper];
  if (name.includes("scale")) return [0.9, 1.1];
  return [val * 0.9, val * 1.1];
}

/** Round to a spec's display decimals -- e.g. a raw `34.000000000000004`
 * (float drift from bound-expansion arithmetic) or an optimizer result with
 * far more precision than the field is meaningfully edited at becomes
 * `34`. Applied at the point a value enters table state (load *and* user
 * edit), not just at render, so the displayed value and the actual value
 * never disagree. */
function roundTo(val: number, decimals: number): number {
  const f = 10 ** decimals;
  return Math.round(val * f) / f;
}

function rowsFromDefaults(specs: DesignSpaceSpec[]): Row[] {
  return specs.map((s) => ({ name: s.name, initial: s.default, lower: s.lower, upper: s.upper }));
}

export function DesignSpaceTable({ specs, designVector, onChange }: DesignSpaceTableProps) {
  const [rows, setRows] = useState<Row[]>(() => rowsFromDefaults(specs));

  useEffect(() => {
    setRows(rowsFromDefaults(specs));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [specs]);

  useEffect(() => {
    if (!designVector) return;
    setRows(
      specs.map((s) => {
        const val = designVector[s.name] ?? s.default;
        const [lower, upper] = expandedBounds(s.name, val, s);
        return {
          name: s.name,
          initial: roundTo(val, s.decimals),
          lower: roundTo(lower, s.decimals),
          upper: roundTo(upper, s.decimals),
        };
      })
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [designVector]);

  useEffect(() => {
    const initialDesign = Object.fromEntries(rows.map((r) => [r.name, r.initial]));
    const bounds: [number, number][] = rows.map((r) => (r.lower <= r.upper ? [r.lower, r.upper] : [r.upper, r.lower]));
    onChange(initialDesign, bounds);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows]);

  function updateCell(name: string, key: "initial" | "lower" | "upper", value: number) {
    setRows((prev) => prev.map((r) => (r.name === name ? { ...r, [key]: value } : r)));
  }

  /** Snap to the field's display decimals once the user leaves it -- live
   * typing stays unrounded (onChange above) so a rounded re-render never
   * fights an in-progress keystroke; committing on blur is what keeps a
   * manually-typed value from carrying more precision than the field
   * meaningfully supports, same as a value loaded from a preset. */
  function roundCellOnBlur(name: string, key: "initial" | "lower" | "upper", decimals: number) {
    setRows((prev) => prev.map((r) => (r.name === name ? { ...r, [key]: roundTo(r[key], decimals) } : r)));
  }

  function resetToDefaults() {
    setRows(rowsFromDefaults(specs));
  }

  return (
    <div className="af-stack">
      <table className="af-table">
        <thead>
          <tr>
            {["Variable", "Unit", "Initial Value", "Lower", "Upper", "Description"].map((h) => (
              <th key={h}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const spec = specs.find((s) => s.name === row.name)!;
            return (
              <tr key={row.name}>
                <td>{spec.label}</td>
                <td className="desc">{spec.unit}</td>
                {(["initial", "lower", "upper"] as const).map((k) => (
                  <td key={k} style={{ width: 110 }}>
                    <input
                      type="number"
                      step={10 ** -spec.decimals}
                      value={row[k]}
                      onChange={(e) => updateCell(row.name, k, parseFloat(e.target.value || "0"))}
                      onBlur={() => roundCellOnBlur(row.name, k, spec.decimals)}
                    />
                  </td>
                ))}
                <td className="desc">{spec.description}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <div className="af-row">
        <span className="af-spacer" />
        <button onClick={resetToDefaults}>Reset to defaults</button>
      </div>
    </div>
  );
}

// Shared with App so a Run always uses the loaded preset's design vector +
// its recentred bounds, even if the Design Space page was never opened (the
// table component, and hence its onChange, only mounts on that page). Mirrors
// the same rows the table builds from a design vector.
export function deriveInitialBounds(
  specs: DesignSpaceSpec[],
  designVector: Record<string, number> | null | undefined
): { initial: Record<string, number>; bounds: [number, number][] } {
  const initial: Record<string, number> = {};
  const bounds: [number, number][] = [];
  for (const s of specs) {
    const val = designVector?.[s.name] ?? s.default;
    const [lo, hi] = designVector ? expandedBounds(s.name, val, s) : [s.lower, s.upper];
    initial[s.name] = val;
    bounds.push(lo <= hi ? [lo, hi] : [hi, lo]);
  }
  return { initial, bounds };
}
