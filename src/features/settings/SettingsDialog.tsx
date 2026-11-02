import { useEffect, useId, useRef, useState } from "react";
import { api, type Settings } from "../../api";
import { Button } from "../../components/Button";
import { Dialog } from "../../components/Dialog";
import { NumberStepper } from "../../components/NumberStepper";
import { Switch } from "../../components/Switch";
import { TextField } from "../../components/TextField";
import { AlertCircleIcon } from "../../components/icons";
import { errorMessage } from "../../lib/errors";
import type { SettingsFocus } from "../../state/dialogs";
import { useAppData, useStore } from "../../state/store";
import { useToolUpdates } from "../tools/useToolUpdates";
import { ToolSetting } from "./ToolSetting";
import "./settings.css";

interface SettingsDialogProps {
  focus?: SettingsFocus;
  onClose: () => void;
}

export function SettingsDialog({ focus, onClose }: SettingsDialogProps) {
  const { dispatch } = useStore();
  const { settings, tools } = useAppData();
  const { toolUpdates, check } = useToolUpdates();
  const updates = toolUpdates.status === "done" ? toolUpdates.updates : null;
  const ids = { downloads: useId(), connections: useId(), clipboard: useId(), clipboardHint: useId() };
  const folderRef = useRef<HTMLInputElement>(null);
  const ffmpegRef = useRef<HTMLInputElement>(null);
  const ytDlpRef = useRef<HTMLInputElement>(null);
  const focusRefs = { folder: folderRef, ffmpeg: ffmpegRef, ytDlp: ytDlpRef };

  const [draft, setDraft] = useState<Settings>(settings);
  const [saving, setSaving] = useState(false);
  const [detecting, setDetecting] = useState(false);
  const [browsing, setBrowsing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function update(patch: Partial<Settings>) {
    setDraft((current) => ({ ...current, ...patch }));
  }

  // Check once per app run. "Detect again" checks again.
  useEffect(() => {
    if (toolUpdates.status === "idle") void check();
  }, [toolUpdates.status, check]);

  // An install saves the new tool path in Settings. Show it in the field.
  useEffect(() => {
    update({ ffmpegPath: settings.ffmpegPath });
  }, [settings.ffmpegPath]);
  useEffect(() => {
    update({ ytDlpPath: settings.ytDlpPath });
  }, [settings.ytDlpPath]);

  async function browse() {
    setBrowsing(true);
    setError(null);
    try {
      const picked = await api.pickFolder(draft.downloadFolder || null);
      if (picked) update({ downloadFolder: picked });
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBrowsing(false);
    }
  }

  async function detect() {
    setDetecting(true);
    setError(null);
    void check();
    try {
      dispatch({ type: "toolsChanged", tools: await api.detectTools() });
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setDetecting(false);
    }
  }

  async function save() {
    setSaving(true);
    setError(null);
    try {
      const saved = await api.saveSettings({
        ...draft,
        downloadFolder: draft.downloadFolder.trim(),
        ffmpegPath: draft.ffmpegPath?.trim() || null,
        ytDlpPath: draft.ytDlpPath?.trim() || null,
      });
      dispatch({ type: "settingsSaved", settings: saved });
      onClose();
    } catch (err) {
      setError(errorMessage(err));
      setSaving(false);
    }
  }

  const toolPathsChanged =
    (draft.ffmpegPath ?? "") !== (settings.ffmpegPath ?? "") ||
    (draft.ytDlpPath ?? "") !== (settings.ytDlpPath ?? "");

  return (
    <Dialog
      title="Settings"
      size="md"
      onClose={onClose}
      initialFocusRef={focus ? focusRefs[focus] : undefined}
      footer={
        <>
          {error && (
            <div className="dialog__footer-error selectable" role="alert">
              <AlertCircleIcon size="sm" />
              {error}
            </div>
          )}
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            loading={saving}
            disabled={draft.downloadFolder.trim() === ""}
            onClick={() => void save()}
          >
            Save
          </Button>
        </>
      }
    >
      <div className="settings">
        <section className="settings__section">
          <h3 className="settings__heading">Downloads</h3>
          <div className="settings__item settings__item--stacked">
            <TextField
              ref={folderRef}
              label={<span className="settings__label">Download folder</span>}
              value={draft.downloadFolder}
              onChange={(event) => update({ downloadFolder: event.target.value })}
              trailing={
                <Button loading={browsing} onClick={() => void browse()}>
                  Browse
                </Button>
              }
            />
          </div>
          <div className="settings__item">
            <div className="settings__text">
              <div className="settings__label" id={ids.downloads}>
                Downloads at the same time
              </div>
              <div className="field-hint">1–10. Others wait in the queue.</div>
            </div>
            <NumberStepper
              value={draft.maxDownloads}
              min={1}
              max={10}
              onChange={(maxDownloads) => update({ maxDownloads })}
              labelledBy={ids.downloads}
              decreaseLabel="Fewer downloads"
              increaseLabel="More downloads"
            />
          </div>
          <div className="settings__item">
            <div className="settings__text">
              <div className="settings__label" id={ids.connections}>
                Connections per file
              </div>
              <div className="field-hint">1–16. Also used for HLS segments. Some servers limit this.</div>
            </div>
            <NumberStepper
              value={draft.connectionsPerFile}
              min={1}
              max={16}
              onChange={(connectionsPerFile) => update({ connectionsPerFile })}
              labelledBy={ids.connections}
              decreaseLabel="Fewer connections"
              increaseLabel="More connections"
            />
          </div>
        </section>

        <section className="settings__section">
          <div className="settings__heading-row">
            <h3 className="settings__heading">Tools</h3>
            <Button loading={detecting} onClick={() => void detect()}>
              Detect again
            </Button>
          </div>
          <ToolSetting
            tool="ffmpeg"
            status={tools?.ffmpeg ?? null}
            update={updates?.ffmpeg ?? null}
            required
            value={draft.ffmpegPath ?? ""}
            onChange={(ffmpegPath) => update({ ffmpegPath })}
            inputRef={ffmpegRef}
            hint="Joins HLS segments and merges video with audio. Streams are copied, never re-encoded."
          />
          <ToolSetting
            tool="ytDlp"
            status={tools?.ytDlp ?? null}
            update={updates?.ytDlp ?? null}
            required={false}
            value={draft.ytDlpPath ?? ""}
            onChange={(ytDlpPath) => update({ ytDlpPath })}
            inputRef={ytDlpRef}
            hint="Only used for sites Siphon can’t read by itself, for example YouTube."
          />
          {toolPathsChanged && (
            <p className="field-hint settings__note">
              Save first. Then the status shows the tools at the new paths.
            </p>
          )}
          <p className="field-hint settings__note">
            Siphon downloads the official builds (yt-dlp from GitHub, ffmpeg from gyan.dev) and
            checks their SHA-256 checksums.
          </p>
        </section>

        <section className="settings__section">
          <h3 className="settings__heading">Clipboard</h3>
          <div className="settings__item settings__item--last">
            <div className="settings__text">
              <div className="settings__label" id={ids.clipboard}>
                Watch the clipboard
              </div>
              <div className="field-hint" id={ids.clipboardHint}>
                When you copy a link Siphon can download, it asks before adding it.
              </div>
            </div>
            <span className="settings__switch-state" aria-hidden="true">
              {draft.clipboardWatch ? "On" : "Off"}
            </span>
            <Switch
              checked={draft.clipboardWatch}
              onChange={(clipboardWatch) => update({ clipboardWatch })}
              labelledBy={ids.clipboard}
              describedBy={ids.clipboardHint}
            />
          </div>
        </section>
      </div>
    </Dialog>
  );
}
