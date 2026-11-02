import type {
  AppState,
  Download,
  Settings,
  ToolId,
  ToolInstallProgress,
  ToolUpdates,
  ToolsStatus,
} from "../api";

export type Filter = "all" | "active" | "queued" | "paused" | "done" | "failed";

export interface Notice {
  id: number;
  kind: "error" | "info";
  message: string;
}

export type LoadState =
  | { status: "loading" }
  | { status: "ready" }
  | { status: "error"; message: string };

export type InstallState =
  | { status: "running"; progress: ToolInstallProgress | null }
  | { status: "failed"; message: string };

const TOOL_PATH_SETTING: Record<ToolId, "ffmpegPath" | "ytDlpPath"> = {
  ffmpeg: "ffmpegPath",
  ytDlp: "ytDlpPath",
};

export interface StoreState {
  load: LoadState;
  downloads: Download[];
  /** Ids removed by events that came before the first state load. */
  removedWhileLoading: string[];
  settings: Settings | null;
  tools: ToolsStatus | null;
  expanded: Record<string, boolean>;
  /** Ids of the selected queue rows. */
  selected: Record<string, true>;
  filter: Filter;
  clipboardUrl: string | null;
  ytDlpBannerDismissed: boolean;
  notices: Notice[];
  installs: Partial<Record<ToolId, InstallState>>;
  /** Newest tool releases. Settings checks them once per app run and again on "Detect again". */
  toolUpdates: ToolUpdatesState;
}

export type ToolUpdatesState =
  | { status: "idle" }
  | { status: "checking" }
  | { status: "done"; updates: ToolUpdates };

export type Action =
  | { type: "loadStarted" }
  | { type: "loaded"; state: AppState }
  | { type: "loadFailed"; message: string }
  | { type: "downloadUpserted"; download: Download }
  | { type: "downloadRemoved"; id: string }
  | { type: "downloadOptionsChanged"; id: string; referer: string | null; userAgent: string | null }
  | { type: "toolsChanged"; tools: ToolsStatus }
  | { type: "settingsSaved"; settings: Settings }
  | { type: "filterChanged"; filter: Filter }
  | { type: "expandToggled"; id: string }
  | { type: "selectionToggled"; id: string }
  | { type: "selectionSet"; ids: string[] }
  | { type: "selectionCleared" }
  | { type: "clipboardLinkChanged"; url: string | null }
  | { type: "ytDlpBannerDismissed" }
  | { type: "noticeAdded"; notice: Notice }
  | { type: "noticeDismissed"; id: number }
  | { type: "installStarted"; tool: ToolId }
  | { type: "installProgressed"; progress: ToolInstallProgress }
  | { type: "installSucceeded"; tool: ToolId; tools: ToolsStatus }
  | { type: "installFailed"; tool: ToolId; message: string }
  | { type: "updateCheckStarted" }
  | { type: "updateCheckFinished"; updates: ToolUpdates };

export const initialState: StoreState = {
  load: { status: "loading" },
  downloads: [],
  removedWhileLoading: [],
  settings: null,
  tools: null,
  expanded: {},
  selected: {},
  filter: "all",
  clipboardUrl: null,
  ytDlpBannerDismissed: false,
  notices: [],
  installs: {},
  toolUpdates: { status: "idle" },
};

const MAX_NOTICES = 4;

function upsert(list: Download[], download: Download): Download[] {
  const index = list.findIndex((d) => d.id === download.id);
  if (index === -1) return [...list, download];
  const next = list.slice();
  next[index] = download;
  return next;
}

/** Events that came while the first load ran are newer than or equal to the snapshot. */
function mergeSnapshot(state: StoreState, snapshot: AppState): Download[] {
  let merged = snapshot.downloads;
  for (const d of state.downloads) merged = upsert(merged, d);
  const removed = new Set(state.removedWhileLoading);
  return merged.filter((d) => !removed.has(d.id));
}

export function reducer(state: StoreState, action: Action): StoreState {
  switch (action.type) {
    case "loadStarted":
      return { ...state, load: { status: "loading" } };
    case "loaded":
      return {
        ...state,
        load: { status: "ready" },
        downloads: mergeSnapshot(state, action.state),
        removedWhileLoading: [],
        settings: action.state.settings,
        tools: state.tools ?? action.state.tools,
      };
    case "loadFailed":
      return { ...state, load: { status: "error", message: action.message } };
    case "downloadUpserted":
      return { ...state, downloads: upsert(state.downloads, action.download) };
    case "downloadRemoved": {
      const { [action.id]: _removed, ...expanded } = state.expanded;
      const { [action.id]: _unselected, ...selected } = state.selected;
      return {
        ...state,
        downloads: state.downloads.filter((d) => d.id !== action.id),
        expanded,
        selected,
        removedWhileLoading:
          state.load.status === "ready"
            ? state.removedWhileLoading
            : [...state.removedWhileLoading, action.id],
      };
    }
    case "downloadOptionsChanged":
      return {
        ...state,
        downloads: state.downloads.map((d) =>
          d.id === action.id ? { ...d, referer: action.referer, userAgent: action.userAgent } : d,
        ),
      };
    case "toolsChanged":
      return { ...state, tools: action.tools };
    case "settingsSaved":
      return { ...state, settings: action.settings };
    case "filterChanged":
      // A selection covers only the rows of one filter.
      return { ...state, filter: action.filter, selected: {} };
    case "expandToggled":
      return {
        ...state,
        expanded: { ...state.expanded, [action.id]: !state.expanded[action.id] },
      };
    case "selectionToggled": {
      const { [action.id]: wasSelected, ...selected } = state.selected;
      return { ...state, selected: wasSelected ? selected : { ...selected, [action.id]: true } };
    }
    case "selectionSet":
      return { ...state, selected: Object.fromEntries(action.ids.map((id) => [id, true as const])) };
    case "selectionCleared":
      return { ...state, selected: {} };
    case "clipboardLinkChanged":
      return { ...state, clipboardUrl: action.url };
    case "ytDlpBannerDismissed":
      return { ...state, ytDlpBannerDismissed: true };
    case "noticeAdded":
      return { ...state, notices: [...state.notices, action.notice].slice(-MAX_NOTICES) };
    case "noticeDismissed":
      return { ...state, notices: state.notices.filter((n) => n.id !== action.id) };
    case "installStarted":
      return {
        ...state,
        installs: { ...state.installs, [action.tool]: { status: "running", progress: null } },
      };
    case "installProgressed": {
      const { tool } = action.progress;
      // Late events after the install ended must not bring the progress back.
      if (state.installs[tool]?.status !== "running") return state;
      return {
        ...state,
        installs: { ...state.installs, [tool]: { status: "running", progress: action.progress } },
      };
    }
    case "installSucceeded": {
      const { [action.tool]: _finished, ...installs } = state.installs;
      // Rust saves the new path in Settings. Keep our copy the same.
      const path = action.tools[action.tool].path;
      return {
        ...state,
        installs,
        tools: action.tools,
        settings: state.settings && { ...state.settings, [TOOL_PATH_SETTING[action.tool]]: path },
      };
    }
    case "installFailed":
      return {
        ...state,
        installs: { ...state.installs, [action.tool]: { status: "failed", message: action.message } },
      };
    case "updateCheckStarted":
      return { ...state, toolUpdates: { status: "checking" } };
    case "updateCheckFinished":
      return { ...state, toolUpdates: { status: "done", updates: action.updates } };
  }
}
