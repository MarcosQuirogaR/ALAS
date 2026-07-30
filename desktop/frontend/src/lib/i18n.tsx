// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { ES } from "./translations.es";

// UI language. Mirrors alas/i18n.py on the backend: English is the
// canonical text written inline in the components, and a catalog keyed by the
// English string supplies the translation. A missing entry falls back to
// English, so partial catalogs are always safe and a newly-added string shows
// up untranslated rather than blank.
//
// The chosen language is persisted (same pattern as theme/zoom) and appended to
// sidecar requests as ?lang= so server-rendered text -- form labels/help and
// figure titles -- comes back in the same language.

export type Lang = "en" | "es";

const LANG_KEY = "alas.lang";

const CATALOGS: Record<Lang, Record<string, string>> = { en: {}, es: ES };

// Module-level mirror so non-React code (sidecarClient) can read the current
// language without a hook.
let currentLang: Lang = "en";

export function getLang(): Lang {
  return currentLang;
}

export function loadLang(): Lang {
  try {
    const v = localStorage.getItem(LANG_KEY);
    if (v === "es" || v === "en") return v;
  } catch {
    /* webview without storage */
  }
  return "en";
}

export function translate(text: string, lang: Lang = currentLang): string {
  if (lang === "en") return text;
  return CATALOGS[lang][text] ?? text;
}

type LangState = { lang: Lang; setLang: (l: Lang) => void; t: (s: string) => string };

const LangContext = createContext<LangState>({
  lang: "en",
  setLang: () => {},
  t: (s) => s,
});

export function LangProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(loadLang);

  useEffect(() => {
    currentLang = lang;
    try {
      localStorage.setItem(LANG_KEY, lang);
    } catch {
      /* no-op */
    }
    document.documentElement.lang = lang;
  }, [lang]);

  const setLang = useCallback((l: Lang) => setLangState(l), []);
  const t = useCallback((s: string) => translate(s, lang), [lang]);
  const value = useMemo(() => ({ lang, setLang, t }), [lang, setLang, t]);

  return <LangContext.Provider value={value}>{children}</LangContext.Provider>;
}

export function useLang(): LangState {
  return useContext(LangContext);
}

/** Convenience for components that only need the translate function. */
export function useT(): (s: string) => string {
  return useContext(LangContext).t;
}
