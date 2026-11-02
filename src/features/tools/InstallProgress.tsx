import type { ToolId, ToolInstallProgress } from "../../api";
import { ProgressBar } from "../../components/ProgressBar";
import { AlertCircleIcon } from "../../components/icons";
import { formatBytes, formatPercent } from "../../lib/format";
import type { InstallState } from "../../state/reducer";
import { TOOL_INFO } from "./useToolInstall";
import "./tools.css";

function describe(progress: ToolInstallProgress | null): { text: string; value: number | null } {
  if (!progress) return { text: "Starting the download…", value: null };
  switch (progress.stage) {
    case "download":
      if (progress.total) {
        const value = progress.done / progress.total;
        return {
          text: `Downloading ${formatPercent(value)} · ${formatBytes(progress.done)} of ${formatBytes(progress.total)}`,
          value,
        };
      }
      return { text: `Downloading · ${formatBytes(progress.done)}`, value: null };
    case "verify":
      return { text: "Checking the file…", value: null };
    case "extract":
      return { text: "Unpacking…", value: null };
    case "done":
      return { text: "Finishing…", value: 1 };
  }
}

/** Progress or error of a tool install. Renders nothing when no install ran. */
export function InstallProgress({ tool, install }: { tool: ToolId; install: InstallState | undefined }) {
  if (!install) return null;
  if (install.status === "failed") {
    return (
      <div className="install-error selectable" role="alert">
        <AlertCircleIcon size="sm" />
        <span>
          Could not install {TOOL_INFO[tool].name}: {install.message}
        </span>
      </div>
    );
  }
  const { text, value } = describe(install.progress);
  return (
    <div className="install-progress">
      <div className="install-progress__text num">{text}</div>
      <ProgressBar
        value={value}
        tone="downloading"
        label={`Installing ${TOOL_INFO[tool].name}`}
      />
    </div>
  );
}
