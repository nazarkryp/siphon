import type { DownloadKind } from "../api";

export const KIND_LABELS: Record<DownloadKind, string> = {
  file: "File",
  hls: "HLS",
  ytdlp: "yt-dlp",
};

export function KindBadge({ kind }: { kind: DownloadKind }) {
  return <span className="kind-badge">{KIND_LABELS[kind]}</span>;
}
