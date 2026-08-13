import { useCallback, useEffect, useState } from "react";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

type Phase = "idle" | "available" | "downloading" | "ready" | "failed";

/**
 * Checks for a new release once at startup and, if the user asks for it,
 * downloads and installs it.
 *
 * Nothing is installed without a click: an app that restarts itself while you
 * are three files into a batch is worse than an out-of-date one. A failed or
 * offline check is silent — it is not the user's problem.
 */
export function useUpdater() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [percent, setPercent] = useState(0);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    check()
      .then((found) => {
        if (cancelled || !found) return;
        setUpdate(found);
        setPhase("available");
      })
      .catch(() => {
        /* no network, no manifest, dev build — none of it is worth a banner */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const install = useCallback(async () => {
    if (!update) return;
    setPhase("downloading");
    setPercent(0);

    try {
      let downloaded = 0;
      let total = 0;
      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            total = event.data.contentLength ?? 0;
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            if (total > 0) setPercent(Math.round((downloaded / total) * 100));
            break;
          case "Finished":
            setPercent(100);
            break;
        }
      });
      setPhase("ready");
      await relaunch();
    } catch {
      setPhase("failed");
    }
  }, [update]);

  return {
    version: update?.version ?? null,
    visible: !dismissed && phase !== "idle",
    phase,
    percent,
    install,
    dismiss: () => setDismissed(true),
  };
}
