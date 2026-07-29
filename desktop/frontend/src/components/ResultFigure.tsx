// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { sidecarFetchText } from "../lib/sidecarClient";
import { FigureCard } from "./FigureCard";
import type { ThemeName } from "./MenuBar";

// One post-run result figure: GET /pipeline/{runId}/figures/{name} as inline
// SVG. Missing figures (404) render as "Not available for this run".
export function ResultFigure({
  runId,
  name,
  title,
  theme,
}: {
  runId: string;
  name: string;
  title: string;
  theme: ThemeName;
}) {
  return (
    <FigureCard
      title={title}
      deps={[runId, name, theme]}
      loader={() => sidecarFetchText(`/pipeline/${runId}/figures/${name}?theme=${theme}`)}
    />
  );
}
