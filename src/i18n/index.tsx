import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";

import ar from "./locales/ar.json";
import de from "./locales/de.json";
import en from "./locales/en.json";
import es from "./locales/es.json";
import fr from "./locales/fr.json";
import hi from "./locales/hi.json";
import id from "./locales/id.json";
import it from "./locales/it.json";
import ja from "./locales/ja.json";
import ko from "./locales/ko.json";
import pl from "./locales/pl.json";
import pt from "./locales/pt.json";
import ru from "./locales/ru.json";
import tr from "./locales/tr.json";
import vi from "./locales/vi.json";
import zh from "./locales/zh.json";

/** The sixteen most widely spoken languages, plus the ones this crowd uses. */
export const LOCALES = { en, zh, es, hi, ar, pt, ru, ja, de, fr, ko, it, tr, vi, id, pl } as const;

export type Locale = keyof typeof LOCALES;

/** Endonyms: a language picker nobody can read is not a language picker. */
export const LANGUAGE_NAMES: Record<Locale, string> = {
  en: "English",
  zh: "中文",
  es: "Español",
  hi: "हिन्दी",
  ar: "العربية",
  pt: "Português",
  ru: "Русский",
  ja: "日本語",
  de: "Deutsch",
  fr: "Français",
  ko: "한국어",
  it: "Italiano",
  tr: "Türkçe",
  vi: "Tiếng Việt",
  id: "Bahasa Indonesia",
  pl: "Polski",
};

const RTL: Locale[] = ["ar"];
const STORAGE_KEY = "coldmill.locale";

type Dictionary = Record<string, string>;
type Values = Record<string, string | number>;

/** `navigator.language` is a tag like `pt-BR`; we key on the primary subtag. */
function detect(): Locale {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved && saved in LOCALES) return saved as Locale;

  for (const tag of navigator.languages ?? [navigator.language]) {
    const primary = tag.toLowerCase().split("-")[0];
    if (primary in LOCALES) return primary as Locale;
  }
  return "en";
}

/**
 * Looks up a key, falling back to English so a missing translation shows the
 * original string rather than the key.
 *
 * Plural keys carry a CLDR category suffix (`files_one`, `files_many`) and are
 * selected with `Intl.PluralRules`, which already knows that Russian has three
 * forms and Arabic six. A locale only has to supply the forms it uses.
 */
function translate(dict: Dictionary, key: string, values?: Values, plural?: string): string {
  const template = (plural && dict[`${key}_${plural}`]) ?? dict[`${key}_other`] ?? dict[key];
  const fallback =
    (plural && en[`${key}_${plural}` as keyof typeof en]) ??
    en[`${key}_other` as keyof typeof en] ??
    en[key as keyof typeof en];

  const text = (template ?? fallback ?? key) as string;
  if (!values) return text;
  return text.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in values ? String(values[name]) : match,
  );
}

export interface Translator {
  (key: string, values?: Values): string;
  /** Plural-aware variant: `plural("kind.video", 3)`. */
  plural: (key: string, count: number, values?: Values) => string;
}

interface I18n {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: Translator;
}

const I18nContext = createContext<I18n | null>(null);

export function I18nProvider({ children }: { children: React.ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(detect);

  useEffect(() => {
    document.documentElement.lang = locale;
    document.documentElement.dir = RTL.includes(locale) ? "rtl" : "ltr";
  }, [locale]);

  const setLocale = useCallback((next: Locale) => {
    localStorage.setItem(STORAGE_KEY, next);
    setLocaleState(next);
  }, []);

  const value = useMemo<I18n>(() => {
    const dict = LOCALES[locale] as Dictionary;
    const rules = new Intl.PluralRules(locale);

    const t = ((key: string, values?: Values) =>
      translate(dict, key, values)) as Translator;
    t.plural = (key: string, count: number, values?: Values) =>
      translate(dict, key, { count, ...values }, rules.select(count));

    return { locale, setLocale, t };
  }, [locale, setLocale]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18n {
  const context = useContext(I18nContext);
  if (!context) throw new Error("useI18n must be used inside <I18nProvider>");
  return context;
}

export const useT = () => useI18n().t;
