// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { DynamicForm, SchemaField } from "./DynamicForm";
import { PreviewChart } from "./PreviewChart";
import { HowItWorks } from "./HowItWorks";
import { useT } from "../lib/i18n";
import type { Page } from "../pages";
import type { ThemeName } from "./MenuBar";

// Generic Advanced Settings page: an explanatory header + a "Reset to
// defaults" button + the auto-generated form for one config dataclass group,
// and (for pages that define one) a large live preview chart beside the form --
// pairs a
// ScrollableForm with a side MplCanvas preview in a QSplitter. The preview is
// inline SVG, so it scales to fill the right column rather than leaving it empty.

type FormPageProps = {
  page: Page;
  fields: SchemaField[];
  values: Record<string, any>;
  onChange: (name: string, value: any) => void;
  onReset: () => void;
  errorFields: Set<string>;
  config: Record<string, any>;
  design: Record<string, number> | null;
  theme: ThemeName;
  /** Analysis-fidelity/solver/performance preset picker (Py6-era, per-page) --
   * empty names when the page has no `presetKind`. */
  auxPresetNames?: string[];
  auxPresetDisplay?: Record<string, string>;
  selectedAuxPreset?: string;
  onAuxPresetChange?: (name: string) => void;
};

export function FormPage({
  page,
  fields,
  values,
  onChange,
  onReset,
  errorFields,
  config,
  design,
  theme,
  auxPresetNames = [],
  auxPresetDisplay = {},
  selectedAuxPreset = "",
  onAuxPresetChange,
}: FormPageProps) {
  const t = useT();
  const hasPreview = !!page.preview;
  const hasAuxPreset = !!page.presetKind && auxPresetNames.length > 0;

  return (
    <div className="af-stack">
      <div className="af-page-header">
        <div>
          <h2 className="af-page-title">{t(page.title)}</h2>
          {page.description && <div className="af-page-desc">{t(page.description)}</div>}
        </div>
        <div className="af-row" style={{ gap: 12 }}>
          {hasAuxPreset && (
            <label className="af-row" style={{ gap: 8 }}>
              <span style={{ whiteSpace: "nowrap" }}>{t("Preset")}:</span>
              <select
                className="af-input"
                value={selectedAuxPreset}
                onChange={(e) => onAuxPresetChange?.(e.target.value)}
              >
                <option value="" disabled>
                  Choose…
                </option>
                {auxPresetNames.map((n) => (
                  <option key={n} value={n}>
                    {auxPresetDisplay[n] ?? n}
                  </option>
                ))}
              </select>
            </label>
          )}
          <button onClick={onReset}>{t("Reset to defaults")}</button>
        </div>
      </div>

      {page.detail && page.detail.length > 0 && (
        <HowItWorks>
          {page.detail.map((p, i) => (
            <p key={i}>{t(p)}</p>
          ))}
        </HowItWorks>
      )}

      <div
        style={{
          display: "flex", gap: 24, alignItems: "flex-start", flexWrap: "wrap",
          // With a preview, the form stays a single readable column and the
          // preview (flex 2, uncapped) fills the rest of the row -- no cap
          // needed here. Without one, the form is the *only* thing in this
          // row (see the `columns` prop below, which flows the form itself
          // into responsive columns) -- leaving it uncapped let a lone
          // 620px-capped column starve that multi-column flow of room to
          // ever form a second column, leaving most of the page blank on
          // anything wider than a laptop (same bug InputsScreen.tsx already
          // had fixed for its own layout, same fix here).
          ...(hasPreview ? null : { maxWidth: "min(1600px, 92%)" }),
        }}
      >
        <div style={hasPreview ? { flex: "1 1 400px", minWidth: 340, maxWidth: 620 } : { flex: "1 1 auto", minWidth: 340 }}>
          {fields.length === 0 ? (
            <div className="af-placeholder">{t("Loading configuration...")}</div>
          ) : (
            <DynamicForm fields={fields} values={values} onChange={onChange} errorFields={errorFields} columns={!hasPreview} />
          )}
        </div>
        {hasPreview && (
          <div
            style={{
              flex: "2 1 620px", minWidth: 420, position: "sticky", top: 0,
              // Bounded height (not just width) so the preview *contains*
              // within the viewport on both axes instead of growing however
              // tall its width-driven aspect ratio happens to make it: on a
              // wide-but-short (ultrawide) window, unbounded growth pushes the
              // preview's own bottom (e.g. its x-axis) below the fold, forcing
              // a scroll to see it, and combined with `sticky` an uncapped
              // height would visually overrun past the viewport into whatever
              // content sits below/beside it.
              height: "clamp(320px, calc(100vh - 170px), 1100px)",
            }}
          >
            <PreviewChart
              name={page.preview!}
              title={page.previewTitle ?? "Live preview"}
              config={config}
              design={design}
              theme={theme}
              fill
            />
          </div>
        )}
      </div>
    </div>
  );
}
