import { useState } from "react";
import { api, type Download } from "../../api";
import { Button } from "../../components/Button";
import { Dialog } from "../../components/Dialog";
import { TextField } from "../../components/TextField";
import { AlertCircleIcon } from "../../components/icons";
import { errorMessage } from "../../lib/errors";
import { useStore } from "../../state/store";

function orNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

export function OptionsDialog({ download, onClose }: { download: Download; onClose: () => void }) {
  const { dispatch } = useStore();
  const [referer, setReferer] = useState(download.referer ?? "");
  const [userAgent, setUserAgent] = useState(download.userAgent ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function save() {
    setSaving(true);
    setError(null);
    try {
      const nextReferer = orNull(referer);
      const nextUserAgent = orNull(userAgent);
      await api.updateOptions(download.id, nextReferer, nextUserAgent);
      dispatch({
        type: "downloadOptionsChanged",
        id: download.id,
        referer: nextReferer,
        userAgent: nextUserAgent,
      });
      onClose();
    } catch (err) {
      setError(errorMessage(err));
      setSaving(false);
    }
  }

  return (
    <Dialog
      title="Link options"
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
          <Button variant="primary" loading={saving} onClick={() => void save()}>
            Save
          </Button>
        </>
      }
    >
      <div className="form-stack">
        <p className="dialog-lead selectable">{download.fileName}</p>
        <TextField
          label="Referer"
          value={referer}
          placeholder="None"
          onChange={(event) => setReferer(event.target.value)}
          hint="The page that plays the video. Many HLS servers need it."
        />
        <TextField
          label="User-Agent"
          value={userAgent}
          placeholder="Siphon default (desktop browser)"
          onChange={(event) => setUserAgent(event.target.value)}
          hint="Change only if the server refuses the request. Retry the download to use the new values."
        />
      </div>
    </Dialog>
  );
}
