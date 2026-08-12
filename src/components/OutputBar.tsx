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
  return (
    <div className="outputbar">
      <span className="muted">Save to</span>
      <span className="outputbar-path" title={outputDir ?? undefined}>
        {outputDir ? shortenPath(outputDir) : "the source folder"}
      </span>
      <button type="button" className="linklike" disabled={disabled} onClick={onChoose}>
        change
      </button>
      {outputDir && (
        <button type="button" className="linklike" disabled={disabled} onClick={onReset}>
          reset
        </button>
      )}
    </div>
  );
}
