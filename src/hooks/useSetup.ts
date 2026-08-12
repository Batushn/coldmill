import { useCallback, useEffect, useState } from "react";

import { applySetup, onEngineDone, onEngineProgress, setupState } from "../lib/ipc";
import type { EngineProgress, Settings, SetupState } from "../types";

/** Owns the module picker: current state, engine downloads, and saving. */
export function useSetup() {
  const [state, setState] = useState<SetupState | null>(null);
  const [progress, setProgress] = useState<EngineProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setState(await setupState());
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const subscriptions = [
      onEngineProgress(setProgress),
      // Clear the bar between engines so the next one starts from zero.
      onEngineDone(() => setProgress(null)),
    ];
    return () => {
      subscriptions.forEach((pending) => pending.then((unlisten) => unlisten()));
    };
  }, []);

  const apply = useCallback(async (settings: Settings) => {
    setBusy(true);
    setError(null);
    try {
      setState(await applySetup(settings));
      return true;
    } catch (failure) {
      setError(String(failure));
      return false;
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }, []);

  return { state, progress, busy, error, apply, refresh };
}
