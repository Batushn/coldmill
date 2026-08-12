import { useT } from "../i18n";

interface Props {
  hovering: boolean;
  scanning: number;
  onPick: () => void;
}

/** The empty state: the whole window is the drop target. */
export function DropZone({ hovering, scanning, onPick }: Props) {
  const t = useT();

  return (
    <div className={`dropzone${hovering ? " is-hovering" : ""}`}>
      <div className="dropzone-inner">
        <div className="dropzone-mark" aria-hidden />
        <h1>{t("dropzone.title")}</h1>
        <p>
          {t("dropzone.hint")}{" "}
          <button type="button" className="linklike" onClick={onPick}>
            {t("dropzone.browse")}
          </button>
        </p>
        {scanning > 0 && <p className="muted">{t("dropzone.reading", { count: scanning })}</p>}
      </div>
    </div>
  );
}
