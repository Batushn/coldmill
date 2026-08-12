import { openUrl } from "@tauri-apps/plugin-opener";

import { LANGUAGE_NAMES, LOCALES, useI18n, type Locale } from "../i18n";
import { IconHeart } from "./Icons";

const SPONSOR_URL = "https://github.com/sponsors/batushn";

/** Language picker and the sponsor heart — the two things that live in every
 *  footer state, empty queue or not. */
export function FooterLinks() {
  const { locale, setLocale, t } = useI18n();

  return (
    <>
      <select
        className="select langpicker"
        value={locale}
        aria-label={t("action.language")}
        title={t("action.language")}
        onChange={(event) => setLocale(event.target.value as Locale)}
      >
        {(Object.keys(LOCALES) as Locale[]).map((code) => (
          <option key={code} value={code}>
            {LANGUAGE_NAMES[code]}
          </option>
        ))}
      </select>

      <button
        type="button"
        className="iconbutton heart"
        title={t("action.support")}
        aria-label={t("action.support")}
        onClick={() => void openUrl(SPONSOR_URL)}
      >
        <IconHeart />
      </button>
    </>
  );
}
