import { useState, type ReactNode } from "react";
import { api, type Download } from "../../api";
import { IconButton } from "../../components/IconButton";
import { Menu, type MenuItem } from "../../components/Menu";
import {
  CopyIcon,
  FolderIcon,
  OpenIcon,
  OptionsIcon,
  PauseIcon,
  PlayIcon,
  RetryIcon,
  StopIcon,
  TrashIcon,
} from "../../components/icons";
import { errorMessage } from "../../lib/errors";
import { useDialogs } from "../../state/dialogs";
import { useNotify, useRunAction } from "../../state/store";

export function RowActions({ download }: { download: Download }) {
  const { id, status, fileName } = download;
  const dialogs = useDialogs();
  const notify = useNotify();
  const runAction = useRunAction();
  const [pending, setPending] = useState(false);

  async function run(call: () => Promise<void>) {
    setPending(true);
    await runAction(call);
    setPending(false);
  }

  function action(label: string, icon: ReactNode, call: () => Promise<void>) {
    return (
      <IconButton
        label={`${label}: ${fileName}`}
        title={label}
        icon={icon}
        disabled={pending}
        onClick={() => void run(call)}
      />
    );
  }

  const items: MenuItem[] = [];
  if (status !== "done") {
    items.push({
      label: "Options",
      icon: <OptionsIcon />,
      onSelect: () => dialogs.open({ type: "options", id }),
    });
  }
  items.push({
    label: "Copy link",
    icon: <CopyIcon />,
    onSelect: () =>
      navigator.clipboard
        .writeText(download.sourceUrl)
        .then(() => notify("info", "Link copied."))
        .catch((error: unknown) => notify("error", `Could not copy the link: ${errorMessage(error)}`)),
  });
  if (status === "done") {
    items.push({
      label: "Show in folder",
      icon: <FolderIcon />,
      onSelect: () => void run(() => api.showInFolder(id)),
    });
  } else {
    items.push({
      label: "Cancel download",
      icon: <StopIcon />,
      onSelect: () => void run(() => api.cancel(id)),
    });
  }
  items.push({
    label: "Remove",
    icon: <TrashIcon />,
    danger: true,
    separated: true,
    onSelect: () => dialogs.open({ type: "remove", ids: [id] }),
  });

  return (
    <div className="row-actions">
      {status === "downloading" && action("Pause", <PauseIcon />, () => api.pause(id))}
      {status === "paused" && action("Resume", <PlayIcon />, () => api.resume(id))}
      {status === "queued" && action("Start now", <PlayIcon />, () => api.startNow(id))}
      {status === "failed" && action("Retry", <RetryIcon />, () => api.retry(id))}
      {status === "done" && action("Open file", <OpenIcon />, () => api.openFile(id))}
      {status === "done" && action("Show in folder", <FolderIcon />, () => api.showInFolder(id))}
      <Menu label={`More actions: ${fileName}`} items={items} />
    </div>
  );
}
