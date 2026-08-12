interface Props {
  hovering: boolean;
  scanning: number;
  onPick: () => void;
}

/** The empty state: the whole window is the drop target. */
export function DropZone({ hovering, scanning, onPick }: Props) {
  return (
    <div className={`dropzone${hovering ? " is-hovering" : ""}`}>
      <div className="dropzone-inner">
        <div className="dropzone-mark" aria-hidden />
        <h1>Drop your files here</h1>
        <p>
          Audio, video and images. Mixed batches are fine —{" "}
          <button type="button" className="linklike" onClick={onPick}>
            or browse
          </button>
        </p>
        {scanning > 0 && <p className="muted">Reading {scanning} file(s)…</p>}
      </div>
    </div>
  );
}
