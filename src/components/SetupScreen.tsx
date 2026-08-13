import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

import { useT } from "../i18n";
import { formatBytes } from "../lib/format";
import type { EngineProgress, Settings, SetupState } from "../types";
import { IconCheck } from "./Icons";

const LIBREOFFICE_DOWNLOAD = "https://www.libreoffice.org/download/download-libreoffice/";

interface Props {
  state: SetupState;
  progress: EngineProgress | null;
  busy: boolean;
  error: string | null;
  onApply: (settings: Settings) => Promise<boolean>;
  onRecheck: () => void;
  onClose: () => void;
}

/**
 * The module picker. Shown once after install, and reachable again from the
 * footer — people change their minds, and a 400 MB engine should be as easy to
 * remove as it was to add.
 */
export function SetupScreen({ state, progress, busy, error, onApply, onRecheck, onClose }: Props) {
  const t = useT();
  const [draft, setDraft] = useState<Settings>(state.settings);
  const engine = (id: string) => state.engines.find((candidate) => candidate.id === id);

  const documentBytes =
    (engine("pandoc")?.downloadBytes ?? 0) + (engine("typst")?.downloadBytes ?? 0);
  const blenderBytes = engine("blender")?.downloadBytes ?? 0;
  const speechBytes =
    (engine("whisper")?.downloadBytes ?? 0) + (engine("whisper-model")?.downloadBytes ?? 0);

  const set = (changes: Partial<Settings>) => setDraft((prev) => ({ ...prev, ...changes }));

  const save = async () => {
    if (await onApply(draft)) onClose();
  };

  return (
    <div className="setup">
      <div className="setup-inner">
        <h1>{t("setup.title")}</h1>
        <p className="muted">{t("setup.subtitle")}</p>

        <div className="modules">
          <Module
            title={t("setup.mediaTitle")}
            detail={t("setup.mediaDetail")}
            badge={t("setup.included")}
            checked
            locked
          />

          <Module
            title={t("setup.docsTitle")}
            detail={t("setup.docsDetail")}
            badge={t("setup.download", { size: formatBytes(documentBytes) })}
            checked={draft.documents}
            disabled={busy}
            onChange={(checked) => set({ documents: checked })}
          >
            <div className="submodule">
              {state.libreoffice ? (
                <span className="ok-line">
                  <IconCheck className="ok" /> {t("setup.libreFound")}
                </span>
              ) : (
                <>
                  <span className="muted">{t("setup.libreMissing")}</span>
                  <span className="submodule-actions">
                    <button
                      type="button"
                      className="linklike"
                      onClick={() => void openUrl(LIBREOFFICE_DOWNLOAD)}
                    >
                      {t("setup.getLibre")}
                    </button>
                    <button type="button" className="linklike" onClick={onRecheck}>
                      {t("setup.recheck")}
                    </button>
                  </span>
                </>
              )}
            </div>
          </Module>

          <Module
            title={t("setup.speechTitle")}
            detail={t("setup.speechDetail")}
            badge={t("setup.download", { size: formatBytes(speechBytes) })}
            checked={draft.speech}
            disabled={busy}
            onChange={(checked) => set({ speech: checked })}
          />

          <Module
            title={t("setup.modelsTitle")}
            detail={t("setup.modelsDetail")}
            badge={t("setup.free")}
            checked={draft.models}
            disabled={busy}
            onChange={(checked) => set({ models: checked, blender: checked && draft.blender })}
          >
            <label className={`submodule checkline${draft.models ? "" : " is-off"}`}>
              <input
                type="checkbox"
                checked={draft.blender}
                disabled={busy || !draft.models}
                onChange={(event) => set({ blender: event.target.checked })}
              />
              <span>
                {t("setup.blenderOption")}
                <span className="muted">
                  {" — "}
                  {t("setup.blenderNote", { size: formatBytes(blenderBytes) })}
                </span>
              </span>
            </label>
          </Module>
        </div>

        {progress && (
          <div className="setup-progress">
            <div className="row-meta">
              {progress.phase === "download"
                ? t("setup.downloading", { label: progress.label })
                : t("setup.unpacking", { label: progress.label })}
              {" — "}
              {progress.total
                ? t("setup.progressOf", {
                    received: formatBytes(progress.received),
                    total: formatBytes(progress.total),
                  })
                : formatBytes(progress.received)}
            </div>
            <div className="bar">
              <div
                className="bar-fill"
                style={{
                  width: progress.total
                    ? `${Math.round((progress.received / progress.total) * 100)}%`
                    : "100%",
                }}
              />
            </div>
          </div>
        )}

        {error && <p className="notice">{error}</p>}

        <div className="setup-actions">
          <button type="button" className="ghost" disabled={busy} onClick={onClose}>
            {state.settings.setupDone ? t("action.cancel") : t("setup.skip")}
          </button>
          <button type="button" className="primary" disabled={busy} onClick={() => void save()}>
            {busy ? t("setup.busy") : t("setup.continue")}
          </button>
        </div>
      </div>
    </div>
  );
}

interface ModuleProps {
  title: string;
  detail: string;
  badge: string;
  checked: boolean;
  locked?: boolean;
  disabled?: boolean;
  onChange?: (checked: boolean) => void;
  children?: React.ReactNode;
}

function Module({
  title,
  detail,
  badge,
  checked,
  locked,
  disabled,
  onChange,
  children,
}: ModuleProps) {
  return (
    <div className={`module${checked ? " is-on" : ""}`}>
      <label className="module-head">
        <input
          type="checkbox"
          checked={checked}
          disabled={locked || disabled}
          onChange={(event) => onChange?.(event.target.checked)}
        />
        <span className="module-title">{title}</span>
        <span className="module-badge">{badge}</span>
      </label>
      <p className="module-detail">{detail}</p>
      {checked && children}
    </div>
  );
}
