import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AddLinksResult,
  AddRequest,
  AppState,
  Download,
  ResolveResult,
  Settings,
  ToolId,
  ToolInstallProgress,
  ToolUpdates,
  ToolsStatus,
} from "./types";

export * from "./types";

// Commands. Rust names are in snake_case. Tauri maps the camelCase argument keys.
export const api = {
  getState: () => invoke<AppState>("get_state"),
  resolveLink: (url: string, referer: string | null, userAgent: string | null) =>
    invoke<ResolveResult>("resolve_link", { url, referer, userAgent }),
  addDownload: (request: AddRequest) => invoke<Download>("add_download", { request }),
  addLinks: (urls: string[]) => invoke<AddLinksResult>("add_links", { urls }),
  pause: (id: string) => invoke<void>("pause_download", { id }),
  resume: (id: string) => invoke<void>("resume_download", { id }),
  startNow: (id: string) => invoke<void>("start_now", { id }),
  retry: (id: string) => invoke<void>("retry_download", { id }),
  cancel: (id: string) => invoke<void>("cancel_download", { id }),
  remove: (id: string, deleteFile: boolean) => invoke<void>("remove_download", { id, deleteFile }),
  updateOptions: (id: string, referer: string | null, userAgent: string | null) =>
    invoke<void>("update_options", { id, referer, userAgent }),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  detectTools: () => invoke<ToolsStatus>("detect_tools"),
  /** Reads the newest release of each tool from GitHub. */
  checkToolUpdates: () => invoke<ToolUpdates>("check_tool_updates"),
  /** Downloads the latest build, checks its SHA-256, saves its path in Settings. */
  installTool: (tool: ToolId) => invoke<ToolsStatus>("install_tool", { tool }),
  pickFolder: (start: string | null) => invoke<string | null>("pick_folder", { start }),
  freeSpace: (folder: string) => invoke<number | null>("free_space", { folder }),
  openFile: (id: string) => invoke<void>("open_file", { id }),
  showInFolder: (id: string) => invoke<void>("show_in_folder", { id }),
};

// Events from Rust.
export const events = {
  onDownloadUpdated: (cb: (d: Download) => void): Promise<UnlistenFn> =>
    listen<Download>("download-updated", (e) => cb(e.payload)),
  onDownloadRemoved: (cb: (id: string) => void): Promise<UnlistenFn> =>
    listen<{ id: string }>("download-removed", (e) => cb(e.payload.id)),
  onToolsChanged: (cb: (t: ToolsStatus) => void): Promise<UnlistenFn> =>
    listen<ToolsStatus>("tools-changed", (e) => cb(e.payload)),
  onClipboardLink: (cb: (url: string) => void): Promise<UnlistenFn> =>
    listen<{ url: string }>("clipboard-link", (e) => cb(e.payload.url)),
  onToolInstallProgress: (cb: (p: ToolInstallProgress) => void): Promise<UnlistenFn> =>
    listen<ToolInstallProgress>("tool-install-progress", (e) => cb(e.payload)),
};
