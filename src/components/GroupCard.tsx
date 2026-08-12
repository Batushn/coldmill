import { useI18n } from "../i18n";
import type { MediaKind } from "../types";
import { KindIcon } from "./Icons";

interface Props {
  kind: MediaKind;
  count: number;
  target: string;
  options: string[];
  disabled: boolean;
  onChange: (target: string) => void;
}

/** One media type: how many files, and the single format they all become. */
export function GroupCard({ kind, count, target, options, disabled, onChange }: Props) {
  const { t } = useI18n();
  const label = t.plural(`kind.${kind}`, count);

  return (
    <div className="group">
      <KindIcon kind={kind} className="group-icon" />
      <span className="group-label">{label}</span>
      <span className="group-arrow" aria-hidden>
        →
      </span>
      <select
        className="select"
        value={target}
        disabled={disabled}
        aria-label={t("kind.targetFor", { kind: label })}
        onChange={(event) => onChange(event.target.value)}
      >
        {options.map((option) => (
          <option key={option} value={option}>
            {option.toUpperCase()}
          </option>
        ))}
      </select>
    </div>
  );
}
