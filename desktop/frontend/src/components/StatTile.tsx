// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

// A single labeled metric, the shared building block behind the Results
// Summary tab's headline numbers and (see FieldPerformancePanel) the Field
// Performance tab's V-speeds/distances -- both are "a label and a number,"
// and looked inconsistent when the Summary tab used this look inline while
// Field Performance rendered its numbers as plain tables instead.
export function StatTile({
  label,
  value,
  sub,
  minWidth = 150,
}: {
  label: string;
  value: string;
  /** Optional muted second line below the value (e.g. the same speed in a second unit). */
  sub?: string;
  minWidth?: number;
}) {
  return (
    <div className="af-stat-tile" style={{ minWidth }}>
      <div className="af-help">{label}</div>
      <div className="af-stat-value">{value}</div>
      {sub && <div className="af-help" style={{ marginTop: 1 }}>{sub}</div>}
    </div>
  );
}
