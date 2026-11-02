import { useState } from "react";
import { api } from "../../api";
import { Button } from "../../components/Button";
import { ClipboardIcon, DownloadTrayIcon } from "../../components/icons";
import { useAppData, useRunAction, useStore } from "../../state/store";

const CARDS = [
  {
    kicker: "Direct files",
    title: ".mp4, .zip, .pdf, .iso",
    text: "Split into parallel parts. Resumes after a stop or a restart.",
  },
  {
    kicker: "HLS streams",
    title: ".m3u8 playlists",
    text: "Pick a quality. Segments are joined into one MP4 with ffmpeg.",
  },
  {
    kicker: "Video pages",
    title: "Any page with a player",
    text: "Siphon scans the page, or asks yt-dlp for sites it can’t read.",
  },
];

export function EmptyQueue() {
  const { dispatch } = useStore();
  const { settings } = useAppData();
  const runAction = useRunAction();
  const [saving, setSaving] = useState(false);

  async function setClipboardWatch(on: boolean) {
    setSaving(true);
    const result = await runAction(() => api.saveSettings({ ...settings, clipboardWatch: on }));
    if (result.ok) dispatch({ type: "settingsSaved", settings: result.value });
    setSaving(false);
  }

  return (
    <div className="empty-queue">
      <div className="empty-queue__icon">
        <DownloadTrayIcon className="empty-queue__glyph" />
      </div>
      <h1 className="empty-queue__title">Nothing to download yet</h1>
      <p className="empty-queue__text">
        Paste a link above and press Fetch. Siphon finds the file or video behind it and lets you
        pick the quality.
      </p>
      <ul className="empty-queue__cards">
        {CARDS.map((card) => (
          <li key={card.kicker} className="info-card">
            <div className="info-card__kicker">{card.kicker}</div>
            <div className="info-card__title">{card.title}</div>
            <div className="info-card__text">{card.text}</div>
          </li>
        ))}
      </ul>
      <div className="empty-queue__clipboard">
        <ClipboardIcon />
        {settings.clipboardWatch ? (
          <span>
            Clipboard watch is on — copy a link anywhere and Siphon will ask before adding it.
          </span>
        ) : (
          <span>Clipboard watch is off.</span>
        )}
        <Button variant="link" disabled={saving} onClick={() => void setClipboardWatch(!settings.clipboardWatch)}>
          {settings.clipboardWatch ? "Turn off" : "Turn on"}
        </Button>
      </div>
    </div>
  );
}
