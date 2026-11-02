import type { ToolsStatus } from "../../api";
import { CheckCircleIcon, FolderIcon, MinusCircleIcon, SpinnerIcon, XCircleIcon } from "../../components/icons";
import { useDialogs } from "../../state/dialogs";
import { useAppData } from "../../state/store";

export function StatusLine() {
  const { settings, tools } = useAppData();
  const dialogs = useDialogs();

  return (
    <footer className="status-line">
      {tools ? (
        <ToolItems tools={tools} />
      ) : (
        <span className="status-line__item">
          <SpinnerIcon size="sm" spin />
          Checking tools…
        </span>
      )}
      <span className="status-line__spacer" />
      <button
        type="button"
        className="status-line__item status-line__folder"
        title="Change the download folder"
        onClick={() => dialogs.open({ type: "settings", focus: "folder" })}
      >
        <FolderIcon size="sm" />
        <span className="status-line__path">{settings.downloadFolder}</span>
      </button>
    </footer>
  );
}

function ToolItems({ tools }: { tools: ToolsStatus }) {
  const dialogs = useDialogs();
  const { ffmpeg, ytDlp } = tools;

  return (
    <>
      {ffmpeg.found ? (
        <span className="status-line__item status-line__item--ok">
          <CheckCircleIcon size="sm" />
          ffmpeg{ffmpeg.version ? ` ${ffmpeg.version}` : ""} · ready
        </span>
      ) : (
        <button
          type="button"
          className="status-line__item status-line__item--bad"
          onClick={() => dialogs.open({ type: "settings", focus: "ffmpeg" })}
        >
          <XCircleIcon size="sm" />
          ffmpeg · not found
        </button>
      )}
      {ytDlp.found ? (
        <span className="status-line__item status-line__item--ok">
          <CheckCircleIcon size="sm" />
          yt-dlp{ytDlp.version ? ` ${ytDlp.version}` : ""} · ready
        </span>
      ) : (
        <button
          type="button"
          className="status-line__item"
          onClick={() => dialogs.open({ type: "settings", focus: "ytDlp" })}
        >
          <MinusCircleIcon size="sm" />
          yt-dlp · not found (optional)
        </button>
      )}
    </>
  );
}
