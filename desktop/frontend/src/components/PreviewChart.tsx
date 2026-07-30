// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useMemo } from "react";
import { sidecarPostText } from "../lib/sidecarClient";
import { FigureCard } from "./FigureCard";
import type { ThemeName } from "./MenuBar";

// A live preview chart: POST /preview/{name} with the current config + design
// vector, rebuilt (debounced) whenever they change -- no pipeline run needed.
// Rendered as inline SVG so it scales crisply to fill its panel.
export function PreviewChart({
  name,
  title,
  config,
  design,
  theme,
  fill = false,
}: {
  name: string;
  title: string;
  config: Record<string, any>;
  design: Record<string, number> | null;
  theme: ThemeName;
  fill?: boolean;
}) {
  const key = useMemo(() => JSON.stringify({ config, design }), [config, design]);
  return (
    <FigureCard
      title={title}
      deps={[name, key, theme]}
      debounceMs={350}
      fill={fill}
      sizeAware
      loader={(size) =>
        sidecarPostText(`/preview/${name}?theme=${theme}`, {
          config, design,
          width_px: size?.width, height_px: size?.height,
        })
      }
    />
  );
}
