import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useNotify } from "../state/store";
import { errorMessage } from "../lib/errors";
import { CloseIcon, Logo, MaximizeIcon, MinimizeIcon, RestoreIcon } from "./icons";

export function WindowBar() {
  const notify = useNotify();
  const [maximized, setMaximized] = useState(false);

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
    const unlisten = appWindow.onResized(() => void sync()).catch(() => null);
    return () => {
      active = false;
      void unlisten.then((stop) => stop?.());
    };
  }, [notify]);

  function run(action: () => Promise<void>) {
    Promise.resolve()
      .then(action)
      .catch((error: unknown) => notify("error", errorMessage(error)));
  }

  return (
    <div className="window-bar" data-tauri-drag-region>
      <div className="window-bar__brand" data-tauri-drag-region>
        <Logo />
        <span className="window-bar__name">Siphon</span>
      </div>
      <button
        type="button"
        className="window-bar__button"
        aria-label="Minimize"
        title="Minimize"
        onClick={() => run(() => getCurrentWindow().minimize())}
      >
        <MinimizeIcon />
      </button>
      <button
        type="button"
        className="window-bar__button"
        aria-label={maximized ? "Restore" : "Maximize"}
        title={maximized ? "Restore" : "Maximize"}
        onClick={() => run(() => getCurrentWindow().toggleMaximize())}
      >
        {maximized ? <RestoreIcon /> : <MaximizeIcon />}
      </button>
      <button
        type="button"
        className="window-bar__button window-bar__button--close"
        aria-label="Close"
        title="Close"
        onClick={() => run(() => getCurrentWindow().close())}
      >
        <CloseIcon />
      </button>
    </div>
  );
}
