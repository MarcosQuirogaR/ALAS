// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { createContext, useContext } from "react";

// App-wide "Learn-more help" state. When `verbose` is off (View > Learn-more
// help), the InfoTip affordances and the collapsible "How this works"
// deep-dive panels render nothing -- a one-click way to A/B the denser-help
// presentation against a leaner one without rebuilding. Persisted in
// localStorage by App.tsx (same pattern as theme/zoom); default on.

export type HelpState = { verbose: boolean };

export const HelpContext = createContext<HelpState>({ verbose: true });

export function useHelp(): HelpState {
  return useContext(HelpContext);
}
