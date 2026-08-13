import { useT } from "../i18n";
import { IconClose } from "./Icons";

interface Props {
  version: string | null;
  phase: "idle" | "available" | "downloading" | "ready" | "failed";
  percent: number;
  onInstall: () => void;
  onDismiss: () => void;
}

/** A single line above everything else. No modal, no nag on every launch. */
export function UpdateBar({ version, phase, percent, onInstall, onDismiss }: Props) {
  const t = useT();

  const message = () => {
    switch (phase) {
      case "downloading":
        return t("update.downloading", { percent });
      case "ready":
        return t("update.restarting");
      case "failed":
        return t("update.failed");
      default:
        return t("update.available", { version: version ?? "" });
    }
  };

  return (
    <div className={`updatebar${phase === "failed" ? " is-failed" : ""}`}>
      <span className="updatebar-text">{message()}</span>

      {phase === "available" && (
        <button type="button" className="updatebar-action" onClick={onInstall}>
          {t("update.install")}
        </button>
      )}

      {phase !== "downloading" && phase !== "ready" && (
        <button
          type="button"
          className="iconbutton"
          title={t("update.later")}
          aria-label={t("update.later")}
          onClick={onDismiss}
        >
          <IconClose />
        </button>
      )}
    </div>
  );
}
