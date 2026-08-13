import { useEffect, useRef, useState } from "react";

import { useT } from "../i18n";
import type { Advanced } from "../types";
import { NO_ADVANCED } from "../types";
import { Dropdown } from "./Dropdown";

interface Props {
  value: Advanced;
  disabled: boolean;
  onChange: (value: Advanced) => void;
}

const ENCODER_PRESETS = [
  "ultrafast",
  "superfast",
  "veryfast",
  "faster",
  "fast",
  "medium",
  "slow",
  "slower",
  "veryslow",
];

/** Every field left empty means "leave the preset alone". */
function isEmpty(advanced: Advanced) {
  return Object.values(advanced).every((setting) => setting == null);
}

function count(advanced: Advanced) {
  return Object.values(advanced).filter((setting) => setting != null).length;
}

/**
 * The escape hatch under the quality presets.
 *
 * Deliberately behind a click: the whole point of Small/Balanced/High is that
 * nobody has to know what a CRF is, and putting bitrate boxes on the main
 * screen would undo that for the people the presets are for. What is in here
 * overrides the preset rather than replacing it, so a single field can be set
 * without having to describe a whole encode.
 */
export function AdvancedPanel({ value, disabled, onChange }: Props) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const active = count(value);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  const set = (patch: Partial<Advanced>) => onChange({ ...value, ...patch });

  /** Blank clears the override rather than sending a zero. */
  const number = (raw: string) => {
    const parsed = Number(raw);
    return raw.trim() === "" || !Number.isFinite(parsed) || parsed <= 0 ? null : parsed;
  };

  return (
    <div className="advanced" ref={root}>
      <button
        type="button"
        className={`linkbutton${active > 0 ? " is-active" : ""}`}
        disabled={disabled}
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        {t("advanced.open")}
        {active > 0 && <span className="badge">{active}</span>}
      </button>

      {open && (
        <div className="advanced-menu">
          <p className="advanced-lead muted">{t("advanced.lead")}</p>

          <label className="advanced-field">
            <span>{t("advanced.videoKbps")}</span>
            <input
              type="number"
              min={1}
              inputMode="numeric"
              placeholder={t("advanced.preset")}
              value={value.videoKbps ?? ""}
              disabled={disabled}
              onChange={(event) => set({ videoKbps: number(event.target.value) })}
            />
          </label>

          <label className="advanced-field">
            <span>{t("advanced.crf")}</span>
            <input
              type="number"
              min={0}
              max={51}
              inputMode="numeric"
              // A bitrate and a constant quality cannot both be in charge, and
              // the backend drops the CRF when both arrive. Saying so here is
              // better than silently ignoring what was typed.
              placeholder={value.videoKbps != null ? t("advanced.crfUnused") : t("advanced.preset")}
              value={value.crf ?? ""}
              disabled={disabled || value.videoKbps != null}
              onChange={(event) => set({ crf: number(event.target.value) })}
            />
          </label>

          <label className="advanced-field">
            <span>{t("advanced.maxHeight")}</span>
            <input
              type="number"
              min={16}
              inputMode="numeric"
              placeholder={t("advanced.preset")}
              value={value.maxHeight ?? ""}
              disabled={disabled}
              onChange={(event) => set({ maxHeight: number(event.target.value) })}
            />
          </label>

          <label className="advanced-field">
            <span>{t("advanced.fps")}</span>
            <input
              type="number"
              min={1}
              inputMode="decimal"
              placeholder={t("advanced.preset")}
              value={value.fps ?? ""}
              disabled={disabled}
              onChange={(event) => set({ fps: number(event.target.value) })}
            />
          </label>

          <div className="advanced-field">
            <span>{t("advanced.encoderPreset")}</span>
            <Dropdown
              label={t("advanced.encoderPreset")}
              value={value.encoderPreset ?? ""}
              disabled={disabled}
              options={[
                { value: "", label: t("advanced.preset") },
                ...ENCODER_PRESETS.map((preset) => ({ value: preset, label: preset })),
              ]}
              onChange={(preset) => set({ encoderPreset: preset === "" ? null : preset })}
            />
          </div>

          <hr className="advanced-rule" />

          <label className="advanced-field">
            <span>{t("advanced.audioKbps")}</span>
            <input
              type="number"
              min={1}
              inputMode="numeric"
              placeholder={t("advanced.preset")}
              value={value.audioKbps ?? ""}
              disabled={disabled}
              onChange={(event) => set({ audioKbps: number(event.target.value) })}
            />
          </label>

          <label className="advanced-field">
            <span>{t("advanced.sampleRate")}</span>
            <input
              type="number"
              min={8000}
              inputMode="numeric"
              placeholder={t("advanced.preset")}
              value={value.sampleRate ?? ""}
              disabled={disabled}
              onChange={(event) => set({ sampleRate: number(event.target.value) })}
            />
          </label>

          <div className="advanced-field">
            <span>{t("advanced.channels")}</span>
            <Dropdown
              label={t("advanced.channels")}
              value={value.channels == null ? "" : String(value.channels)}
              disabled={disabled}
              options={[
                { value: "", label: t("advanced.preset") },
                { value: "1", label: t("advanced.mono") },
                { value: "2", label: t("advanced.stereo") },
              ]}
              onChange={(channels) => set({ channels: channels === "" ? null : Number(channels) })}
            />
          </div>

          <div className="advanced-foot">
            <button
              type="button"
              className="linkbutton"
              disabled={disabled || isEmpty(value)}
              onClick={() => onChange(NO_ADVANCED)}
            >
              {t("advanced.reset")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
