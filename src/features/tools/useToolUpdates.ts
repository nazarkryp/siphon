import { useCallback } from "react";
import { api, type ToolStatus, type ToolUpdate } from "../../api";
import { errorMessage } from "../../lib/errors";
import { useStore } from "../../state/store";

// React can run an effect twice in the same frame. This flag blocks the second check.
let running = false;

/** The newest tool releases and a check function. The state is shared by every caller. */
export function useToolUpdates() {
  const { state, dispatch } = useStore();

  const check = useCallback(async () => {
    if (running) return;
    running = true;
    dispatch({ type: "updateCheckStarted" });
    try {
      dispatch({ type: "updateCheckFinished", updates: await api.checkToolUpdates() });
    } catch (error) {
      const failed = { latest: null, error: errorMessage(error) };
      dispatch({ type: "updateCheckFinished", updates: { ffmpeg: failed, ytDlp: failed } });
    } finally {
      running = false;
    }
  }, [dispatch]);

  return { toolUpdates: state.toolUpdates, check };
}

/** True when the found tool is a release build at the newest version. */
export function isUpToDate(status: ToolStatus | null, update: ToolUpdate | null): boolean {
  if (!status?.found || status.devBuild || !status.version || !update?.latest) return false;
  return !isNewer(update.latest, status.version);
}

/** Compares versions such as "9.0.2" or "2026.10.26.1" number by number. */
function isNewer(latest: string, current: string): boolean {
  const a = latest.split(".").map(Number);
  const b = current.split(".").map(Number);
  // Siphon cannot compare a version that is not numbers with dots. It offers the update.
  if (b.some(Number.isNaN)) return true;
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const diff = (a[i] ?? 0) - (b[i] ?? 0);
    if (diff !== 0) return diff > 0;
  }
  return false;
}
