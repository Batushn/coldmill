import type { Quality } from "../types";

const CHOICES: { value: Quality; label: string; hint: string }[] = [
  { value: "small", label: "Small", hint: "Smallest files, visible quality loss" },
  { value: "balanced", label: "Balanced", hint: "Good quality at a sane size" },
  { value: "high", label: "High", hint: "Best quality, largest files" },
];

interface Props {
  value: Quality;
  disabled: boolean;
  onChange: (value: Quality) => void;
}

export function QualitySegmented({ value, disabled, onChange }: Props) {
  return (
    <div className="segmented" role="radiogroup" aria-label="Quality">
      {CHOICES.map((choice) => (
        <button
          key={choice.value}
          type="button"
          role="radio"
          aria-checked={value === choice.value}
          title={choice.hint}
          disabled={disabled}
          className={value === choice.value ? "is-active" : undefined}
          onClick={() => onChange(choice.value)}
        >
          {choice.label}
        </button>
      ))}
    </div>
  );
}
