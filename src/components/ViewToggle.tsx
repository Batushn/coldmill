import { useT } from "../i18n";
import type { ViewMode } from "../types";
import { IconGrid, IconList } from "./Icons";

interface Props {
  value: ViewMode;
  onChange: (value: ViewMode) => void;
}

export function ViewToggle({ value, onChange }: Props) {
  const t = useT();

  return (
    <div className="viewtoggle" role="group" aria-label={t("view.label")}>
      {(["list", "grid"] as const).map((mode) => (
        <button
          key={mode}
          type="button"
          className={`iconbutton${value === mode ? " is-active" : ""}`}
          title={t(`view.${mode}`)}
          aria-label={t(`view.${mode}`)}
          aria-pressed={value === mode}
          onClick={() => onChange(mode)}
        >
          {mode === "list" ? <IconList /> : <IconGrid />}
        </button>
      ))}
    </div>
  );
}
