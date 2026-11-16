// Shapes shared with the Rust backend (src-tauri/src/model.rs). Keep both files in sync.

export type DownloadKind = "file" | "hls" | "ytdlp";

export type DownloadStatus =
  | "queued"
  | "downloading"
  | "paused"
  | "merging"
  /** ffmpeg changes the video to a compatible format (H.264 + AAC in MP4). */
  | "converting"
  | "done"
  | "failed"
  | "needsFfmpeg";

export interface ByteProgress {
  done: number;
  total: number | null;
}

export type ProgressDetail =
  | { type: "none" }
  | { type: "chunks"; chunks: ByteProgress[] }
  | {
      type: "segments";
      total: number;
      done: number;
      inFlight: number;
      // One character per segment: "0" waiting, "1" in flight, "2" done.
      states: string;
      encrypted: boolean;
    }
  | {
      type: "streams";
      video: ByteProgress;
      audio: ByteProgress;
      videoLabel: string;
      audioLabel: string;
    };

export interface DownloadError {
  message: string;
  hint: string | null;
}

export interface Download {
  id: string;
  sourceUrl: string;
  host: string;
  fileName: string;
  folder: string;
  kind: DownloadKind;
  qualityLabel: string | null;
  status: DownloadStatus;
  totalBytes: number | null;
  totalIsEstimate: boolean;
  doneBytes: number;
  /** Bytes per second. 0 when the download is not running. */
  speed: number;
  etaSeconds: number | null;
  /** 0..1 while ffmpeg merges or remuxes, else null. */
  mergeProgress: number | null;
  detail: ProgressDetail;
  error: DownloadError | null;
  referer: string | null;
  userAgent: string | null;
  /** Unix time in milliseconds. */
  createdAt: number;
  finishedAt: number | null;
}

export type ResolveStepId = "direct" | "hls" | "ytdlp" | "page";
export type ResolveStepState = "found" | "notFound" | "skipped" | "error";

export interface ResolveStep {
  id: ResolveStepId;
  state: ResolveStepState;
  message: string;
}

export interface Quality {
  id: string;
  /** For example "1080p", "Audio only" or "Original file". */
  label: string;
  /** For example "1920×1080". */
  resolution: string | null;
  codecs: string | null;
  /** Bits per second. */
  bitrate: number | null;
  sizeBytes: number | null;
  sizeIsEstimate: boolean;
  /** True when video and audio download separately and ffmpeg merges them. */
  separateAudio: boolean;
  note: string | null;
  /** The saved file opens in the default players everywhere: H.264/AAC in MP4. False for WebM, MKV, VP9, AV1 and Opus, which QuickTime cannot open. */
  playsEverywhere: boolean;
  /** What the default player cannot open: "VP9", "AV1 and Opus", "WebM". Set only on video qualities that Siphon can save in a compatible format; null otherwise. */
  unsupportedFormat: string | null;
}

export interface Stream {
  id: string;
  kind: DownloadKind;
  title: string;
  url: string;
  durationSeconds: number | null;
  encrypted: boolean;
  /** Best quality first. Never empty. */
  qualities: Quality[];
  suggestedFileName: string;
  /** Text subtitles that ffmpeg can add to the video. Empty when the site has none. */
  subtitles: SubtitleTrack[];
}

export interface SubtitleTrack {
  id: string;
  /** The site's language code, for example `en` or `pt-BR`. */
  language: string;
  label: string;
  /** Made by speech recognition, not by a person. */
  auto: boolean;
}

export interface ResolveResult {
  resolveId: string;
  url: string;
  steps: ResolveStep[];
  streams: Stream[];
  /** Free space in the default download folder. */
  freeSpaceBytes: number | null;
}

export interface AddRequest {
  resolveId: string;
  streamId: string;
  qualityId: string;
  fileName: string;
  folder: string;
  referer: string | null;
  userAgent: string | null;
  /** A `SubtitleTrack` id of the stream. ffmpeg adds the track to the video. */
  subtitleId: string | null;
  /** Save in a compatible format. Used only when the quality has an `unsupportedFormat`. */
  compatibleFormat: boolean;
}

export interface AddLinksResult {
  added: Download[];
  failed: { url: string; message: string }[];
}

export interface Settings {
  downloadFolder: string;
  /** 1..10 */
  maxDownloads: number;
  /** 1..16 */
  connectionsPerFile: number;
  /** null: find the tool on PATH. */
  ffmpegPath: string | null;
  ytDlpPath: string | null;
  clipboardWatch: boolean;
  /** Resolve copied video-site links before the user clicks Add. */
  smartPrefetch: boolean;
  /** Use the uploader's English title (YouTube) when the video has one. */
  englishTitles: boolean;
  /** Default of the "Save in a compatible format" checkbox in Add link. */
  compatibleFormat: boolean;
}

export interface ToolStatus {
  found: boolean;
  path: string | null;
  version: string | null;
  /** A development (nightly or git) build, not a release. */
  devBuild: boolean;
  /** Siphon has an official build of this tool for this system, so it can install and update it. */
  installable: boolean;
}

export interface ToolsStatus {
  ffmpeg: ToolStatus;
  ytDlp: ToolStatus;
}

/** The newest release of a tool, or why Siphon could not read it. */
export interface ToolUpdate {
  latest: string | null;
  error: string | null;
}

export interface ToolUpdates {
  ffmpeg: ToolUpdate;
  ytDlp: ToolUpdate;
}

export type ToolId = "ffmpeg" | "ytDlp";

export type InstallStage = "download" | "verify" | "extract" | "done";

export interface ToolInstallProgress {
  tool: ToolId;
  stage: InstallStage;
  done: number;
  total: number | null;
}

export interface AppState {
  downloads: Download[];
  settings: Settings;
  /** Null until the first tool detection ends. */
  tools: ToolsStatus | null;
}
