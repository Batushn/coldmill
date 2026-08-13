import { openUrl } from "@tauri-apps/plugin-opener";

import { LANGUAGE_NAMES, LOCALES, useI18n, type Locale } from "../i18n";
import { Dropdown } from "./Dropdown";
import { IconHeart } from "./Icons";

const SPONSOR_URL = "https://github.com/sponsors/batushn";

/** Language picker and the sponsor heart — the two things that live in every
 *  footer state, empty queue or not. */
export function FooterLinks() {
  const { locale, setLocale, t } = useI18n();

  return (
    <>
      <Dropdown
        className="langpicker"
        value={locale}
        options={(Object.keys(LOCALES) as Locale[]).map((code) => ({
          value: code,
          label: LANGUAGE_NAMES[code],
        }))}
        label={t("action.language")}
        align="end"
        onChange={(next) => setLocale(next as Locale)}
      />

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
