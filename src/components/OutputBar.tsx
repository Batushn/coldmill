import { useT } from "../i18n";
import { shortenPath } from "../lib/format";

interface Props {
  outputDir: string | null;
  disabled: boolean;
  onChoose: () => void;
  onReset: () => void;
}

/** Output folder line. Defaults to "next to the source" so most users never
 *  open a dialog at all. */
export function OutputBar({ outputDir, disabled, onChoose, onReset }: Props) {
  const t = useT();

  return (
    <div className="outputbar">
      <span className="muted">{t("output.saveTo")}</span>
      <span className="outputbar-path" title={outputDir ?? undefined}>
        {outputDir ? shortenPath(outputDir) : t("output.sourceFolder")}
      </span>
      <button type="button" className="linklike" disabled={disabled} onClick={onChoose}>
        {t("output.change")}
      </button>
      {outputDir && (
        <button type="button" className="linklike" disabled={disabled} onClick={onReset}>
          {t("output.reset")}
        </button>
      )}
    </div>
  );
}
