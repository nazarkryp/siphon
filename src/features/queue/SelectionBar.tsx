import { useState, type ReactNode } from "react";
import { api, type Download } from "../../api";
import { Button } from "../../components/Button";
import { CloseIcon, PauseIcon, PlayIcon, RetryIcon, StopIcon, TrashIcon } from "../../components/icons";
import { useDialogs } from "../../state/dialogs";
import { useNotify, useStore } from "../../state/store";
import { failureText, runEach } from "./bulk";

/** Actions for the selected rows. It takes the place of the column names in the queue header. */
export function SelectionBar({ selected }: { selected: Download[] }) {
  const { dispatch } = useStore();
  const dialogs = useDialogs();
  const notify = useNotify();
  const [pending, setPending] = useState(false);

  async function run(verb: string, targets: Download[], call: (id: string) => Promise<void>) {
    setPending(true);
    const failures = await runEach(targets, (d) => call(d.id));
    setPending(false);
    if (failures.length > 0) notify("error", failureText(verb, failures, targets.length));
  }

  function action(
    label: string,
    verb: string,
    icon: ReactNode,
    targets: Download[],
    call: (id: string) => Promise<void>,
  ) {
    return (
      <Button
        variant="ghost"
        icon={icon}
        title={targets.length > 0 ? `${label} ${targets.length} selected` : `None of the selected can ${verb}`}
        disabled={pending || targets.length === 0}
        onClick={() => void run(verb, targets, call)}
      >
        {label}
      </Button>
    );
  }

  const withStatus = (status: Download["status"]) => selected.filter((d) => d.status === status);

  return (
    <div className="selection-bar">
      <span className="selection-bar__count num" role="status">
        {selected.length} selected
      </span>
      {action("Pause", "pause", <PauseIcon />, withStatus("downloading"), api.pause)}
      {action("Resume", "resume", <PlayIcon />, withStatus("paused"), api.resume)}
      {action("Retry", "retry", <RetryIcon />, withStatus("failed"), api.retry)}
      {action(
        "Cancel",
        "cancel",
        <StopIcon />,
        selected.filter((d) => d.status !== "done"),
        api.cancel,
      )}
      <Button
        variant="ghost"
        icon={<TrashIcon />}
        title={`Remove ${selected.length} selected`}
        disabled={pending}
        onClick={() => dialogs.open({ type: "remove", ids: selected.map((d) => d.id) })}
      >
        Remove
      </Button>
      <Button
        variant="ghost"
        className="selection-bar__clear"
        icon={<CloseIcon />}
        title="Clear the selection"
        onClick={() => dispatch({ type: "selectionCleared" })}
      >
        Clear
      </Button>
    </div>
  );
}
