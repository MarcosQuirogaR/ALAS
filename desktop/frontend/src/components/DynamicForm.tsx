// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { HelpHover } from "./InfoTip";

// A field label that IS its own help target: hovering the variable name shows
// a themed popover explaining it (no separate ⓘ icon -- an icon per row read as
// clutter). The label text still ellipsis-truncates so a long name never spills
// into the control column.
function LabelWithHelp({ label, help, heading }: { label: string; help?: string; heading?: string }) {
  return (
    <span className="af-form-label">
      <HelpHover heading={heading ?? label} content={help} className="af-label-text">
        {label}
      </HelpHover>
    </span>
  );
}

// Renders a form from a schema tree produced by alas/sidecar/schema.py
// (GET /schema) -- the renderer for the sidecar's
// "auto-generated form from any dataclass" mechanism. Adding a new config
// field on the Python side still requires zero frontend code.
//
// Styling is class-based (theme.css / App.css) so it matches the PySide6
// app's DataclassForm look: a label column, a control column, nested
// dataclasses in a titled fieldset, and a collapsible "Advanced (N)" section.

export type SchemaField = {
  name: string;
  label: string;
  unit: string;
  help: string;
  advanced: boolean;
  kind: string;
  value?: any;
  min?: number;
  max?: number;
  decimals?: number;
  columns?: string[];
  editable?: boolean;
  options?: string[];
  options_by_aircraft_type?: Record<string, string[]>;
  /** Read-only unless a sibling field currently equals this value (e.g.
   * "Passenger count" is auto-recomputed from the cabin preset and only
   * hand-editable once cabin_preset == "Custom"). */
  readonly_unless?: { field: string; value: string };
  fields?: SchemaField[];
};

type Values = Record<string, any>;

type DynamicFormProps = {
  fields: SchemaField[];
  values: Values;
  onChange: (name: string, value: any) => void;
  /** Leaf field names that currently have a validation error (red border). */
  errorFields?: Set<string>;
  /** Flow fields into responsive columns to fill wide pages (no side preview). */
  columns?: boolean;
  /** Enclosing form levels' values, nearest first. Set automatically when this
   * form recurses into a nested dataclass; lets a field's `readonly_unless`
   * reference a controller declared on a parent dataclass. */
  ancestorValues?: Values[];
};

// Numeric input tuning: decimals/step derived from
// value magnitude so a step of 500 doesn't turn up on a field at 0.03.
function numberStep(value: number, unit: string, decimalsOverride?: number): { decimals: number; step: number } {
  if (decimalsOverride !== undefined) {
    return { decimals: decimalsOverride, step: Math.pow(10, -decimalsOverride) };
  }
  const av = value !== 0 ? Math.abs(value) : 1.0;
  if (unit === "kg" || unit === "Pa") {
    if (av >= 100_000) return { decimals: 0, step: 10_000 };
    if (av >= 10_000) return { decimals: 0, step: 1_000 };
    if (av >= 1_000) return { decimals: 0, step: 100 };
    return { decimals: 1, step: 10 };
  }
  if (av >= 10_000) return { decimals: 0, step: 500 };
  if (av >= 1_000) return { decimals: 0, step: 50 };
  if (av >= 100) return { decimals: 1, step: 5 };
  if (av >= 10) return { decimals: 2, step: 1 };
  if (av >= 1) return { decimals: 3, step: 0.1 };
  if (av >= 0.01) return { decimals: 3, step: 0.01 };
  return { decimals: 5, step: 0.001 };
}

const WEIGHT_MIN = 0.001;
const WEIGHT_MAX = 1000.0;
const WEIGHT_STEPS = 200;

function weightValToPos(v: number): number {
  const clamped = Math.min(WEIGHT_MAX, Math.max(WEIGHT_MIN, v));
  const logMin = Math.log10(WEIGHT_MIN);
  const logMax = Math.log10(WEIGHT_MAX);
  return Math.round(((Math.log10(clamped) - logMin) / (logMax - logMin)) * WEIGHT_STEPS);
}

function weightPosToVal(p: number): number {
  const logMin = Math.log10(WEIGHT_MIN);
  const logMax = Math.log10(WEIGHT_MAX);
  return Math.pow(10, logMin + (p / WEIGHT_STEPS) * (logMax - logMin));
}

// Searchable dropdown for editable option fields (airfoils: 1,666 entries).
// The old <datalist> approach renders as a bare text box in WebView2 with
// suggestions only appearing mid-keystroke -- users reasonably read that as
// "not a dropdown". This is a real combo box: click/focus opens a filtered,
// scrollable option list; typing filters it; free text is still accepted
// (AeroSandbox resolves e.g. arbitrary NACA codes not in the library).
const COMBO_MAX_SHOWN = 200;

function ComboSelect({
  value,
  options,
  onChange,
  className,
}: {
  value: string;
  options: string[];
  onChange: (v: string) => void;
  className: string;
}) {
  const [open, setOpen] = useState(false);
  // null = not filtering (show the full list); a string = live filter text.
  const [query, setQuery] = useState<string | null>(null);

  const filtered = useMemo(() => {
    const q = (query ?? "").trim().toLowerCase();
    if (!q) return options;
    return options.filter((o) => o.toLowerCase().includes(q));
  }, [options, query]);
  const shown = filtered.slice(0, COMBO_MAX_SHOWN);

  return (
    <div className="af-combo">
      <input
        className={className}
        type="text"
        value={query ?? value ?? ""}
        placeholder="Type to search…"
        onFocus={() => {
          setOpen(true);
          setQuery(null);
        }}
        onBlur={() => {
          setOpen(false);
          setQuery(null);
        }}
        onChange={(e) => {
          setQuery(e.target.value);
          setOpen(true);
          onChange(e.target.value); // free text commits as typed (datalist parity)
        }}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            setOpen(false);
            (e.currentTarget as HTMLInputElement).blur();
          } else if (e.key === "Enter" && shown.length > 0 && query !== null) {
            onChange(shown[0]);
            setQuery(null);
            setOpen(false);
            (e.currentTarget as HTMLInputElement).blur();
          }
        }}
      />
      <span
        className="af-combo-arrow"
        // mousedown (not click): fires before the input's blur closes the panel
        onMouseDown={(e) => {
          e.preventDefault();
          setOpen((o) => !o);
        }}
      >
        ▾
      </span>
      {open && (
        <div className="af-combo-panel">
          {shown.map((o) => (
            <div
              key={o}
              className={"af-combo-opt" + (o === value ? " selected" : "")}
              onMouseDown={(e) => {
                e.preventDefault();
                onChange(o);
                setQuery(null);
                setOpen(false);
              }}
            >
              {o}
            </div>
          ))}
          {filtered.length > COMBO_MAX_SHOWN && (
            <div className="af-combo-note">…{filtered.length - COMBO_MAX_SHOWN} more — keep typing to narrow</div>
          )}
          {shown.length === 0 && <div className="af-combo-note">No matches (free text is allowed)</div>}
        </div>
      )}
    </div>
  );
}

// A numeric <input> that keeps a local text buffer so mid-edit states like
// "1." or "-0.00" survive keystrokes. It re-syncs from the incoming value only
// while unfocused, so external changes (preset load, DOE, reset) still update.
function NumberInput({
  value,
  format,
  step,
  min,
  max,
  className,
  disabled,
  title,
  onCommit,
}: {
  value: number;
  format: (v: number) => string;
  step: number;
  min?: number;
  max?: number;
  className: string;
  disabled?: boolean;
  title?: string;
  onCommit: (v: number) => void;
}) {
  const [text, setText] = useState(() => format(value));
  const focused = useRef(false);

  useEffect(() => {
    if (!focused.current) setText(format(value));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value]);

  return (
    <input
      className={className}
      type="number"
      step={step}
      min={min}
      max={max}
      value={text}
      disabled={disabled}
      title={title}
      onFocus={() => (focused.current = true)}
      onBlur={() => {
        focused.current = false;
        setText(format(value));
      }}
      onChange={(e) => {
        setText(e.target.value);
        const n = parseFloat(e.target.value);
        if (!Number.isNaN(n)) onCommit(n);
      }}
    />
  );
}

function Field({
  field,
  value,
  siblingValues,
  ancestorValues,
  onChange,
  hasError,
}: {
  field: SchemaField;
  value: any;
  siblingValues: Values;
  /** Values of enclosing form levels, nearest first -- used to resolve a
   * `readonly_unless` controller that lives on a PARENT dataclass. */
  ancestorValues?: Values[];
  onChange: (v: any) => void;
  hasError: boolean;
}) {
  const rowLabel = field.unit ? `${field.label} [${field.unit}]` : field.label;
  const errCls = hasError ? " af-error" : "";
  const ru = field.readonly_unless;
  // Resolve the controlling field against siblings first, then each enclosing
  // level. Sibling-only lookup broke controllers declared one level up (e.g.
  // each seat class's `count` is gated by `class_mix_mode` on the parent cabin
  // config): the name simply wasn't found, so the field read as permanently
  // read-only and its fallback mode became unusable.
  const ruValue = (() => {
    if (!ru) return undefined;
    if (ru.field in siblingValues) return siblingValues[ru.field];
    for (const level of ancestorValues ?? []) {
      if (level && ru.field in level) return level[ru.field];
    }
    return undefined;
  })();
  const readOnly = !!ru && ruValue !== ru.value;
  const readOnlyTitle = readOnly
    ? `${field.help}\n\n(Read-only: set "${ru!.field.replace(/_/g, " ")}" to "${ru!.value}" to edit directly.)`
    : field.help;

  // Native `title` ONLY for the read-only explanation, and only when the field
  // actually is read-only. Setting it to field.help as well meant the OS
  // tooltip and the HelpHover popover both fired on the same hover, drawing two
  // overlapping copies of the same text.
  const Row = (control: ReactNode) => (
    <div className="af-form-row" title={readOnly ? readOnlyTitle : undefined}>
      <LabelWithHelp label={rowLabel} help={field.help} heading={field.label} />
      {control}
    </div>
  );

  switch (field.kind) {
    case "bool":
      return (
        <div className="af-form-row">
          <LabelWithHelp label={rowLabel} help={field.help} heading={field.label} />
          <label className="af-check">
            <input type="checkbox" checked={!!value} onChange={(e) => onChange(e.target.checked)} />
          </label>
        </div>
      );

    case "int":
      return Row(
        <NumberInput
          className={"af-input" + errCls}
          value={Number(value ?? 0)}
          format={(v) => String(Math.round(v))}
          step={1}
          min={field.min}
          max={field.max}
          disabled={readOnly}
          title={readOnly ? readOnlyTitle : undefined}
          onCommit={(v) => onChange(Math.round(v))}
        />
      );

    case "float": {
      const { decimals, step } = numberStep(Number(value ?? 0), field.unit, field.decimals);
      return Row(
        <NumberInput
          className={"af-input" + errCls}
          value={Number(value ?? 0)}
          format={(v) => v.toFixed(decimals)}
          step={step}
          min={field.min}
          max={field.max}
          disabled={readOnly}
          title={readOnly ? readOnlyTitle : undefined}
          onCommit={(v) => onChange(v)}
        />
      );
    }

    case "weight_slider": {
      const v = Number(value ?? 0.001);
      return Row(
        <span className="af-row" style={{ maxWidth: 420 }}>
          <input
            type="range"
            min={0}
            max={WEIGHT_STEPS}
            value={weightValToPos(v)}
            onChange={(e) => onChange(weightPosToVal(Number(e.target.value)))}
            style={{ flex: 1 }}
          />
          <span style={{ width: 56, textAlign: "right", fontVariantNumeric: "tabular-nums" }}>
            {v.toPrecision(3)}
          </span>
        </span>
      );
    }

    case "str": {
      // cabin_preset's option list depends on the sibling aircraft_type field's
      // *current* value, not the schema's snapshot at fetch time.
      const options =
        field.options_by_aircraft_type && siblingValues.aircraft_type
          ? field.options_by_aircraft_type[siblingValues.aircraft_type] ?? field.options
          : field.options;

      if (options && !field.editable) {
        // Defensive: a config loaded from an older save can carry a value
        // outside today's option list -- keep it selectable rather than
        // letting the <select> silently show the wrong entry.
        const opts = value && !options.includes(value) ? [value, ...options] : options;
        return Row(
          <select className={"af-input" + errCls} value={value ?? ""} onChange={(e) => onChange(e.target.value)}>
            {opts.map((o) => (
              <option key={o} value={o}>
                {o}
              </option>
            ))}
          </select>
        );
      }
      if (options && field.editable) {
        return Row(
          <ComboSelect
            value={value ?? ""}
            options={options}
            onChange={onChange}
            className={"af-input" + errCls}
          />
        );
      }
      return Row(
        <input
          className={"af-input" + errCls}
          type="text"
          value={value ?? ""}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    }

    case "optional":
      return Row(
        <input
          className={"af-input" + errCls}
          type="text"
          placeholder="none"
          value={value ?? ""}
          onChange={(e) => onChange(e.target.value === "" ? null : e.target.value)}
        />
      );

    case "number_list": {
      const arr: number[] = Array.isArray(value) ? value : [];
      return Row(
        <span className="af-row">
          {arr.map((v, i) => (
            <input
              key={i}
              className="af-input"
              type="number"
              value={v}
              onChange={(e) => {
                const next = [...arr];
                next[i] = parseFloat(e.target.value || "0");
                onChange(next);
              }}
              style={{ width: 84 }}
            />
          ))}
        </span>
      );
    }

    case "tuple_list": {
      const rows: number[][] = Array.isArray(value) ? value : [];
      return (
        <div className="af-form-row" style={{ alignItems: "flex-start" }}>
          <LabelWithHelp label={rowLabel} help={field.help} heading={field.label} />
          <div>
            {field.columns && (
              <div className="af-row af-help" style={{ marginBottom: 2 }}>
                {field.columns.map((c) => (
                  <span key={c} style={{ width: 84 }}>
                    {c}
                  </span>
                ))}
              </div>
            )}
            {rows.map((row, r) => (
              <div key={r} className="af-row" style={{ marginBottom: 3 }}>
                {row.map((v, c) => (
                  <input
                    key={c}
                    className="af-input"
                    type="number"
                    value={v}
                    onChange={(e) => {
                      const next = rows.map((rr) => [...rr]);
                      next[r][c] = parseFloat(e.target.value || "0");
                      onChange(next);
                    }}
                    style={{ width: 84 }}
                  />
                ))}
              </div>
            ))}
          </div>
        </div>
      );
    }

    default:
      return Row(<input className="af-input" type="text" value={JSON.stringify(value)} disabled />);
  }
}

export function DynamicForm({ fields, values, onChange, errorFields, columns, ancestorValues }: DynamicFormProps) {
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const primary = fields.filter((f) => !f.advanced);
  const advanced = fields.filter((f) => f.advanced);

  function renderField(field: SchemaField) {
    if (field.kind === "dataclass") {
      const childValues: Values = values[field.name] ?? {};
      return (
        <fieldset key={field.name} className="af-fieldset">
          <legend>
            <HelpHover heading={field.label} content={field.help}>
              {field.label}
            </HelpHover>
          </legend>
          <DynamicForm
            fields={field.fields ?? []}
            values={childValues}
            errorFields={errorFields}
            // This level's values become the nested form's nearest ancestor, so
            // a child field can be gated by a control declared out here.
            ancestorValues={[values, ...(ancestorValues ?? [])]}
            onChange={(childName, childValue) =>
              onChange(field.name, { ...childValues, [childName]: childValue })
            }
          />
        </fieldset>
      );
    }
    return (
      <Field
        key={field.name}
        field={field}
        value={values[field.name]}
        siblingValues={values}
        ancestorValues={ancestorValues}
        hasError={!!errorFields?.has(field.name)}
        onChange={(v) => onChange(field.name, v)}
      />
    );
  }

  const colCls = columns ? "af-form-cols" : undefined;
  return (
    <div>
      <div className={colCls}>{primary.map(renderField)}</div>
      {advanced.length > 0 && (
        <div>
          <button type="button" className="af-advanced-toggle" onClick={() => setAdvancedOpen((o) => !o)}>
            {advancedOpen ? "▾" : "▸"} Advanced ({advanced.length})
          </button>
          {advancedOpen && <div className={colCls}>{advanced.map(renderField)}</div>}
        </div>
      )}
    </div>
  );
}
