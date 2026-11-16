import { useEffect, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useNotify } from "../state/store";
import { errorMessage } from "../lib/errors";
import { isMac } from "../lib/platform";
import { Logo } from "./icons";

/**
 * The title bar: three columns. `children` sit centred in the window, `end` at the right end.
 * Empty space in every column moves the window.
 * macOS draws the traffic lights itself (titleBarStyle "Overlay"), so the bar only leaves room for them.
 * Windows and Linux get a logo on the left and caption buttons on the right.
 */
export function WindowBar({ children, end }: { children?: ReactNode; end?: ReactNode }) {
  const notify = useNotify();
  const [maximized, setMaximized] = useState(false);
  const [focused, setFocused] = useState(() => document.hasFocus());

  useEffect(() => {
    let appWindow: ReturnType<typeof getCurrentWindow>;
    try {
      appWindow = getCurrentWindow();
    } catch (error) {
      notify("error", `The window controls do not work: ${errorMessage(error)}`);
      return;
    }
    let active = true;
    // The state only picks the maximize or restore icon. A failed read keeps the last icon.
    const sync = () =>
      appWindow
        .isMaximized()
        .then((value) => active && setMaximized(value))
        .catch(() => undefined);
    void sync();
    const unlistenResize = appWindow.onResized(() => void sync()).catch(() => null);
    const unlistenFocus = appWindow
      .onFocusChanged(({ payload }) => active && setFocused(payload))
      .catch(() => null);
    return () => {
      active = false;
      void unlistenResize.then((stop) => stop?.());
      void unlistenFocus.then((stop) => stop?.());
    };
  }, [notify]);

  function run(action: () => Promise<void>) {
    Promise.resolve()
      .then(action)
      .catch((error: unknown) => notify("error", errorMessage(error)));
  }

  return (
    <div
      className={`window-bar ${isMac ? "window-bar--mac" : "window-bar--caption"}`}
      data-focused={focused}
      data-tauri-drag-region
    >
      <div className="window-bar__start" data-tauri-drag-region>
        {!isMac && <Logo />}
      </div>
      <div className="window-bar__center" data-tauri-drag-region>
        {children}
      </div>
      <div className="window-bar__end" data-tauri-drag-region>
        {end}
        {!isMac && (
          <div className="window-bar__captions">
            <button
              type="button"
              className="window-bar__caption"
              aria-label="Minimize"
              title="Minimize"
              onClick={() => run(() => getCurrentWindow().minimize())}
            >
              <svg viewBox="0 0 10 10" aria-hidden="true" focusable="false">
                <path d="M0 5.5h10" />
              </svg>
            </button>
            <button
              type="button"
              className="window-bar__caption"
              aria-label={maximized ? "Restore" : "Maximize"}
              title={maximized ? "Restore" : "Maximize"}
              onClick={() => run(() => getCurrentWindow().toggleMaximize())}
            >
              <svg viewBox="0 0 10 10" aria-hidden="true" focusable="false">
                {maximized ? (
                  <>
                    <rect x=".5" y="2.5" width="7" height="7" rx="1" />
                    <path d="M2.5 2.5v-1a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" />
                  </>
                ) : (
                  <rect x=".5" y=".5" width="9" height="9" rx="1" />
                )}
              </svg>
            </button>
            <button
              type="button"
              className="window-bar__caption window-bar__caption--close"
              aria-label="Close"
              title="Close"
              onClick={() => run(() => getCurrentWindow().close())}
            >
              <svg viewBox="0 0 10 10" aria-hidden="true" focusable="false">
                <path d="M.5.5l9 9M9.5.5l-9 9" />
              </svg>
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
