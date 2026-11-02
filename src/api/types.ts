// Shapes shared with the Rust backend (src-tauri/src/model.rs). Keep both files in sync.

export type DownloadKind = "file" | "hls" | "ytdlp";

export type DownloadStatus =
  | "queued"
  | "downloading"
  | "paused"
  | "merging"
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
}

export interface ToolStatus {
  found: boolean;
  path: string | null;
  version: string | null;
  /** A development (nightly or git) build, not a release. */
  devBuild: boolean;
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
