import { useCallback } from "react";
import { api, type ToolId } from "../../api";
import { errorMessage } from "../../lib/errors";
import { useNotify, useStore } from "../../state/store";

export const TOOL_INFO: Record<ToolId, { name: string; size: string }> = {
  ffmpeg: { name: "ffmpeg", size: "≈ 115 MB" },
  ytDlp: { name: "yt-dlp", size: "≈ 18 MB" },
};

// Two clicks in the same frame see the same store state. This set blocks the second install.
const running = new Set<ToolId>();

/** Install state of one tool and a start function. The state is shared by every caller. */
export function useToolInstall(tool: ToolId) {
  const { state, dispatch } = useStore();
  const notify = useNotify();

  const start = useCallback(async () => {
    if (running.has(tool)) return;
    running.add(tool);
    dispatch({ type: "installStarted", tool });
    try {
      const tools = await api.installTool(tool);
      dispatch({ type: "installSucceeded", tool, tools });
      notify("info", `${TOOL_INFO[tool].name} is installed.`);
    } catch (error) {
      dispatch({ type: "installFailed", tool, message: errorMessage(error) });
    } finally {
      running.delete(tool);
    }
  }, [tool, dispatch, notify]);

  return { install: state.installs[tool], start };
}
