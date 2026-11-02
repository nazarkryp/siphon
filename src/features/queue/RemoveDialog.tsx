import { useId, useState } from "react";
import { api, type Download } from "../../api";
import { Button } from "../../components/Button";
import { Dialog } from "../../components/Dialog";
import { AlertCircleIcon } from "../../components/icons";
import { useStore } from "../../state/store";
import { failureText, runEach } from "./bulk";

export function RemoveDialog({ downloads, onClose }: { downloads: Download[]; onClose: () => void }) {
  const { dispatch } = useStore();
  const checkboxId = useId();
  const [deleteFile, setDeleteFile] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const single = downloads.length === 1;
  const allDone = downloads.every((d) => d.status === "done");
  const files = single ? "the file" : "the files";

  async function remove() {
    setRemoving(true);
    setError(null);
    const failures = await runEach(downloads, async (d) => {
      await api.remove(d.id, deleteFile);
      dispatch({ type: "downloadRemoved", id: d.id });
    });
    if (failures.length === 0) {
      onClose();
      return;
    }
    // The dialog stays open with the downloads that are left, so Remove can try them again.
    setError(failureText("remove", failures, downloads.length));
    setRemoving(false);
  }

  return (
    <Dialog
      title={single ? "Remove download" : "Remove downloads"}
      size="sm"
      onClose={onClose}
      footer={
        <>
          {error && (
            <div className="dialog__footer-error selectable" role="alert">
              <AlertCircleIcon size="sm" />
              {error}
            </div>
          )}
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" loading={removing} onClick={() => void remove()}>
            Remove
          </Button>
        </>
      }
    >
      <div className="form-stack">
        <p className="dialog-lead">
          Remove{" "}
          {single ? (
            <strong className="selectable">{downloads[0].fileName}</strong>
          ) : (
            <strong>{downloads.length} downloads</strong>
          )}{" "}
          from the list?
        </p>
        <div className="checkbox-row">
          <input
            id={checkboxId}
            type="checkbox"
            checked={deleteFile}
            onChange={(event) => setDeleteFile(event.target.checked)}
          />
          <label htmlFor={checkboxId}>
            {allDone ? `Also delete ${files}` : `Also delete ${files} and the downloaded parts`}
          </label>
        </div>
      </div>
    </Dialog>
  );
}
