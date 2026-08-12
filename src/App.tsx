import { useCallback, useEffect, useMemo, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";

import { DropZone } from "./components/DropZone";
import { FileRow } from "./components/FileRow";
import { GroupCard } from "./components/GroupCard";
import { OutputBar } from "./components/OutputBar";
import { QualitySegmented } from "./components/QualitySegmented";
import { SetupScreen } from "./components/SetupScreen";
import { useConversion } from "./hooks/useConversion";
import { useEstimates } from "./hooks/useEstimates";
import { useFileQueue } from "./hooks/useFileQueue";
import { useSetup } from "./hooks/useSetup";
import { formatBytes } from "./lib/format";
import { supportedTargets } from "./lib/ipc";
import type { ConvertibleKind, MediaKind, Quality, Settings, TargetMap } from "./types";

const OUTPUT_DIR_KEY = "coldmill.outputDir";
const GROUP_ORDER: ConvertibleKind[] = ["video", "audio", "image", "document", "model"];

const DEFAULT_TARGETS: TargetMap = {
  video: "mp4",
  audio: "mp3",
  image: "jpg",
  document: "pdf",
  model: "glb",
};

type Options = Partial<Record<MediaKind, string[]>>;

export default function App() {
  const queue = useFileQueue();
  const { files, scanning, addPaths, pickFiles, reprobe, remove, clear, resetFinished } = queue;
  const { start, cancel, cancelEverything } = useConversion({
    patchByJob: queue.patchByJob,
    attachJobs: queue.attachJobs,
  });
  const setup = useSetup();

  const [targets, setTargets] = useState<TargetMap>(DEFAULT_TARGETS);
  const [options, setOptions] = useState<Options>({});
  const [quality, setQuality] = useState<Quality>("balanced");
  const [outputDir, setOutputDir] = useState<string | null>(() =>
    localStorage.getItem(OUTPUT_DIR_KEY),
  );
  const [hovering, setHovering] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [setupOpen, setSetupOpen] = useState(false);

  const estimates = useEstimates(files, targets, quality);

  const loadOptions = useCallback(async () => {
    const fresh = await supportedTargets();
    setOptions(fresh);
    // Installing an engine can retire the selected format; fall back to the
    // first one still on offer.
    setTargets((prev) => {
      const next = { ...prev };
      for (const kind of GROUP_ORDER) {
        const available = fresh[kind] ?? [];
        if (available.length > 0 && !available.includes(next[kind])) {
          next[kind] = available[0];
        }
      }
      return next;
    });
  }, []);

  useEffect(() => {
    void loadOptions();
  }, [loadOptions]);

  // First run: ask what this person actually converts.
  useEffect(() => {
    if (setup.state && !setup.state.settings.setupDone) setSetupOpen(true);
  }, [setup.state]);

  // Tauri delivers real filesystem paths here; HTML5 drag events would not.
  useEffect(() => {
    const pending = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "over") {
        setHovering(true);
      } else if (event.payload.type === "drop") {
        setHovering(false);
        void addPaths(event.payload.paths);
      } else {
        setHovering(false);
      }
    });
    return () => {
      pending.then((unlisten) => unlisten());
    };
  }, [addPaths]);

  const groups = useMemo(
    () =>
      GROUP_ORDER.map((kind) => ({
        kind,
        count: files.filter((file) => file.kind === kind).length,
      })).filter((group) => group.count > 0),
    [files],
  );

  const busy = files.some((file) => file.status === "queued" || file.status === "running");
  const pending = files.filter((file) => file.status === "ready");
  const unsupported = files.filter((file) => file.status === "unsupported").length;

  const pendingEstimate = pending.reduce<number | null>((total, file) => {
    const bytes = estimates[file.path];
    return bytes == null ? total : (total ?? 0) + bytes;
  }, null);

  const changeTarget = useCallback(
    (kind: ConvertibleKind, value: string) => {
      setTargets((prev) => ({ ...prev, [kind]: value }));
      // A different target means finished rows are stale — queue them again.
      resetFinished(kind);
    },
    [resetFinished],
  );

  const changeQuality = useCallback(
    (value: Quality) => {
      setQuality(value);
      resetFinished();
    },
    [resetFinished],
  );

  const applySetup = useCallback(
    async (settings: Settings) => {
      const applied = await setup.apply(settings);
      if (applied) {
        await loadOptions();
        // Files rejected for a missing module may be fine now.
        await reprobe(
          files.filter((file) => file.status === "unsupported").map((file) => file.path),
        );
      }
      return applied;
    },
    [files, loadOptions, reprobe, setup],
  );

  const chooseOutputDir = useCallback(async () => {
    const picked = await open({
      directory: true,
      title: "Choose an output folder",
      defaultPath: outputDir ?? undefined,
    });
    if (typeof picked !== "string") return;
    setOutputDir(picked);
    localStorage.setItem(OUTPUT_DIR_KEY, picked);
  }, [outputDir]);

  const resetOutputDir = useCallback(() => {
    setOutputDir(null);
    localStorage.removeItem(OUTPUT_DIR_KEY);
  }, []);

  const convert = useCallback(async () => {
    setNotice(null);
    try {
      await start(pending, targets, quality, outputDir);
    } catch (error) {
      setNotice(String(error));
    }
  }, [outputDir, pending, quality, start, targets]);

  const setupScreen = setupOpen && setup.state && (
    <SetupScreen
      state={setup.state}
      progress={setup.progress}
      busy={setup.busy}
      error={setup.error}
      onApply={applySetup}
      onRecheck={() => void setup.refresh()}
      onClose={() => setSetupOpen(false)}
    />
  );

  if (files.length === 0) {
    return (
      <div className="app">
        {setupScreen}
        <DropZone hovering={hovering} scanning={scanning} onPick={pickFiles} />
        <footer className="actions">
          <span className="spacer" />
          <button type="button" className="ghost" onClick={() => setSetupOpen(true)}>
            Modules
          </button>
        </footer>
      </div>
    );
  }

  return (
    <div className="app">
      {setupScreen}

      <header className="topbar">
        <div className="groups">
          {groups.map((group) => (
            <GroupCard
              key={group.kind}
              kind={group.kind}
              count={group.count}
              target={targets[group.kind]}
              options={options[group.kind] ?? [targets[group.kind]]}
              disabled={busy}
              onChange={(value) => changeTarget(group.kind, value)}
            />
          ))}
        </div>
        <QualitySegmented value={quality} disabled={busy} onChange={changeQuality} />
      </header>

      <OutputBar
        outputDir={outputDir}
        disabled={busy}
        onChoose={chooseOutputDir}
        onReset={resetOutputDir}
      />

      {notice && <p className="notice">{notice}</p>}

      <ul className="rows">
        {files.map((file) => (
          <FileRow
            key={file.id}
            file={file}
            target={file.kind === "unsupported" ? undefined : targets[file.kind]}
            estimate={estimates[file.path]}
            onRemove={remove}
            onCancel={cancel}
          />
        ))}
      </ul>

      <footer className="actions">
        <span className="muted">
          {scanning > 0
            ? `Reading ${scanning} file(s)…`
            : unsupported > 0
              ? `${unsupported} file(s) skipped`
              : `${files.length} file(s)`}
          {pendingEstimate != null && ` · ~${formatBytes(pendingEstimate)} output`}
        </span>
        <span className="spacer" />
        <button type="button" className="ghost" onClick={() => setSetupOpen(true)}>
          Modules
        </button>
        <button type="button" className="ghost" onClick={pickFiles} disabled={busy}>
          Add files
        </button>
        {busy ? (
          <button type="button" className="ghost" onClick={() => void cancelEverything()}>
            Cancel all
          </button>
        ) : (
          <button type="button" className="ghost" onClick={clear}>
            Clear
          </button>
        )}
        <button
          type="button"
          className="primary"
          disabled={busy || pending.length === 0}
          onClick={() => void convert()}
        >
          Convert {pending.length > 0 ? pending.length : ""}
        </button>
      </footer>

      {hovering && <div className="drop-overlay">Drop to add</div>}
    </div>
  );
}
