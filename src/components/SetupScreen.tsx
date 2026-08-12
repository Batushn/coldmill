import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

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
  const [draft, setDraft] = useState<Settings>(state.settings);
  const engine = (id: string) => state.engines.find((candidate) => candidate.id === id);

  const documentBytes =
    (engine("pandoc")?.downloadBytes ?? 0) + (engine("typst")?.downloadBytes ?? 0);
  const blenderBytes = engine("blender")?.downloadBytes ?? 0;

  const set = (changes: Partial<Settings>) => setDraft((prev) => ({ ...prev, ...changes }));

  const save = async () => {
    if (await onApply(draft)) onClose();
  };

  return (
    <div className="setup">
      <div className="setup-inner">
        <h1>What do you convert?</h1>
        <p className="muted">
          Pick what you need. Everything else stays off, and you can change this later.
        </p>

        <div className="modules">
          <Module
            title="Photos, audio & video"
            detail="jpg, png, webp, mp3, wav, mp4, mkv and the rest"
            badge="Included"
            checked
            locked
          />

          <Module
            title="Documents"
            detail="docx, odt, markdown, html, epub — and anything to PDF"
            badge={`${formatBytes(documentBytes)} download`}
            checked={draft.documents}
            disabled={busy}
            onChange={(checked) => set({ documents: checked })}
          >
            <div className="submodule">
              {state.libreoffice ? (
                <span className="ok-line">
                  <IconCheck className="ok" /> LibreOffice found — PDF input and .doc / .xls /
                  .ppt are covered
                </span>
              ) : (
                <>
                  <span className="muted">
                    PDF input and legacy Office files (.doc, .xls, .ppt) need LibreOffice, which
                    is a separate system install.
                  </span>
                  <span className="submodule-actions">
                    <button
                      type="button"
                      className="linklike"
                      onClick={() => void openUrl(LIBREOFFICE_DOWNLOAD)}
                    >
                      Get LibreOffice
                    </button>
                    <button type="button" className="linklike" onClick={onRecheck}>
                      re-check
                    </button>
                  </span>
                </>
              )}
            </div>
          </Module>

          <Module
            title="3D models"
            detail="stl, obj, glb and gltf — converted in the app, nothing to download"
            badge="Free"
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
                Add <strong>.blend</strong>, FBX, DAE and PLY support
                <span className="muted"> — downloads Blender, {formatBytes(blenderBytes)}</span>
              </span>
            </label>
          </Module>
        </div>

        {progress && (
          <div className="setup-progress">
            <div className="row-meta">
              {progress.phase === "download" ? "Downloading" : "Unpacking"} {progress.label}
              {progress.total
                ? ` — ${formatBytes(progress.received)} of ${formatBytes(progress.total)}`
                : ` — ${formatBytes(progress.received)}`}
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
            {state.settings.setupDone ? "Cancel" : "Skip for now"}
          </button>
          <button type="button" className="primary" disabled={busy} onClick={() => void save()}>
            {busy ? "Setting up…" : "Continue"}
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
