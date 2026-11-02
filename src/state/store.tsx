import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  type Dispatch,
  type ReactNode,
} from "react";
import { api, events, type Settings, type ToolsStatus } from "../api";
import { errorMessage } from "../lib/errors";
import { initialState, reducer, type Action, type StoreState } from "./reducer";

interface StoreValue {
  state: StoreState;
  dispatch: Dispatch<Action>;
  reload: () => void;
}

const StoreContext = createContext<StoreValue | null>(null);

let nextNoticeId = 1;

export function StoreProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initialState);

  const load = useCallback(() => {
    let cancelled = false;
    dispatch({ type: "loadStarted" });
    api
      .getState()
      .then((appState) => {
        if (!cancelled) dispatch({ type: "loaded", state: appState });
      })
      .catch((error: unknown) => {
        if (!cancelled) dispatch({ type: "loadFailed", message: errorMessage(error) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    // Subscribe first, so that no update between the snapshot and the subscription is lost.
    const subscriptions = [
      events.onDownloadUpdated((download) => dispatch({ type: "downloadUpserted", download })),
      events.onDownloadRemoved((id) => dispatch({ type: "downloadRemoved", id })),
      events.onToolsChanged((tools) => dispatch({ type: "toolsChanged", tools })),
      events.onClipboardLink((url) => dispatch({ type: "clipboardLinkChanged", url })),
      events.onToolInstallProgress((progress) => dispatch({ type: "installProgressed", progress })),
    ].map((subscription) =>
      subscription.catch((error: unknown) => {
        const message = `Siphon cannot receive download updates: ${errorMessage(error)}`;
        dispatch({ type: "noticeAdded", notice: { id: nextNoticeId++, kind: "error", message } });
        return null;
      }),
    );
    const cancelLoad = load();
    return () => {
      cancelLoad();
      for (const subscription of subscriptions) {
        void subscription.then((unlisten) => unlisten?.());
      }
    };
  }, [load]);

  const value = useMemo(() => ({ state, dispatch, reload: load }), [state, load]);
  return <StoreContext.Provider value={value}>{children}</StoreContext.Provider>;
}

export function useStore(): StoreValue {
  const value = useContext(StoreContext);
  if (!value) throw new Error("useStore needs a StoreProvider above it.");
  return value;
}

/**
 * Settings and tools after the first load. Use only below the "ready" gate.
 * `tools` is null until the first tool detection ends.
 */
export function useAppData(): { settings: Settings; tools: ToolsStatus | null } {
  const { state } = useStore();
  if (!state.settings) throw new Error("App state is not loaded yet.");
  return { settings: state.settings, tools: state.tools };
}

export function useNotify() {
  const { dispatch } = useStore();
  return useCallback(
    (kind: "error" | "info", message: string) => {
      const id = nextNoticeId++;
      dispatch({ type: "noticeAdded", notice: { id, kind, message } });
      return id;
    },
    [dispatch],
  );
}

/** Runs an API call. A rejected call shows its message as an error notice. */
export function useRunAction() {
  const notify = useNotify();
  return useCallback(
    async <T,>(call: () => Promise<T>): Promise<{ ok: true; value: T } | { ok: false }> => {
      try {
        return { ok: true, value: await call() };
      } catch (error) {
        notify("error", errorMessage(error));
        return { ok: false };
      }
    },
    [notify],
  );
}
