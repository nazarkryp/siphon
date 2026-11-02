import type { ReactNode } from "react";
import type { ToolId } from "../../api";
import { Banner } from "../../components/Banner";
import { Button } from "../../components/Button";
import { useDialogs } from "../../state/dialogs";
import { useAppData, useStore } from "../../state/store";
import { InstallProgress } from "../tools/InstallProgress";
import { TOOL_INFO, useToolInstall } from "../tools/useToolInstall";

function ToolBanner({
  tool,
  tone,
  title,
  onDismiss,
  children,
}: {
  tool: ToolId;
  tone: "warning" | "neutral";
  title?: string;
  onDismiss?: () => void;
  children: ReactNode;
}) {
  const dialogs = useDialogs();
  const { install, start } = useToolInstall(tool);
  const installing = install?.status === "running";
  const { name, size } = TOOL_INFO[tool];

  return (
    <Banner
      tone={tone}
      title={title}
      onDismiss={onDismiss}
      live={!installing}
      actions={
        <>
          <Button disabled={installing} onClick={() => dialogs.open({ type: "settings", focus: tool })}>
            Set {name} path
          </Button>
          <Button variant="primary" loading={installing} onClick={() => void start()}>
            {install?.status === "failed" ? "Try again" : `Install ${name} (${size})`}
          </Button>
        </>
      }
    >
      {install ? <InstallProgress tool={tool} install={install} /> : children}
    </Banner>
  );
}

export function ToolBanners() {
  const { state, dispatch } = useStore();
  const { tools } = useAppData();
  if (!tools) return null;
  const showFfmpeg = !tools.ffmpeg.found;
  const showYtDlp = !tools.ytDlp.found && !state.ytDlpBannerDismissed;
  if (!showFfmpeg && !showYtDlp) return null;

  return (
    <div className="tool-banners">
      {showFfmpeg && (
        <ToolBanner tool="ffmpeg" tone="warning" title="ffmpeg not found">
          HLS downloads and video + audio downloads can’t start. Direct files still work.
        </ToolBanner>
      )}
      {showYtDlp && (
        <ToolBanner tool="ytDlp" tone="neutral" onDismiss={() => dispatch({ type: "ytDlpBannerDismissed" })}>
          <strong>yt-dlp not found.</strong> Siphon works, but sites that need yt-dlp (for example
          YouTube) will fail.
        </ToolBanner>
      )}
    </div>
  );
}
