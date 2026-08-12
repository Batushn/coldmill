import { useT } from "../i18n";
import type { Quality } from "../types";

const CHOICES: Quality[] = ["small", "balanced", "high"];

interface Props {
  value: Quality;
  disabled: boolean;
  onChange: (value: Quality) => void;
}

export function QualitySegmented({ value, disabled, onChange }: Props) {
  const t = useT();

  return (
    <div className="segmented" role="radiogroup" aria-label={t("quality.label")}>
      {CHOICES.map((choice) => (
        <button
          key={choice}
          type="button"
          role="radio"
          aria-checked={value === choice}
          title={t(`quality.${choice}Hint`)}
          disabled={disabled}
          className={value === choice ? "is-active" : undefined}
          onClick={() => onChange(choice)}
        >
          {t(`quality.${choice}`)}
        </button>
      ))}
    </div>
  );
}
