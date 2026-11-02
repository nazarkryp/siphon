import { useMemo, useState } from "react";
import { Button } from "./components/Button";
import { Notices } from "./components/Notices";
import { WindowBar } from "./components/WindowBar";
import { AlertCircleIcon, SpinnerIcon } from "./components/icons";
import { AddLinkDialog } from "./features/add-link/AddLinkDialog";
import { ClipboardPrompt } from "./features/clipboard/ClipboardPrompt";
import { OptionsDialog } from "./features/queue/OptionsDialog";
import { QueueScreen } from "./features/queue/QueueScreen";
import { RemoveDialog } from "./features/queue/RemoveDialog";
import { SettingsDialog } from "./features/settings/SettingsDialog";
import { DialogsContext, type DialogRequest } from "./state/dialogs";
import { StoreProvider, useStore } from "./state/store";
import "./app.css";

function ActiveDialog({ request, onClose }: { request: DialogRequest; onClose: () => void }) {
  const { state } = useStore();
  switch (request.type) {
    case "addLink":
      return <AddLinkDialog url={request.url} onAdded={request.onAdded} onClose={onClose} />;
    case "settings":
      return <SettingsDialog focus={request.focus} onClose={onClose} />;
    case "options": {
      const download = state.downloads.find((d) => d.id === request.id);
      // The download can disappear while the dialog is open. Then there is nothing to edit.
      if (!download) return null;
      return <OptionsDialog download={download} onClose={onClose} />;
    }
    case "remove": {
      // Downloads can disappear while the dialog is open. Remove only the ones that are left.
      const downloads = state.downloads.filter((d) => request.ids.includes(d.id));
      if (downloads.length === 0) return null;
      return <RemoveDialog downloads={downloads} onClose={onClose} />;
    }
  }
}

function LoadGate() {
  const { state, reload } = useStore();
  const [dialog, setDialog] = useState<DialogRequest | null>(null);
  const dialogs = useMemo(() => ({ open: setDialog }), []);

  if (state.load.status === "loading" && !state.settings) {
    return (
      <div className="app-message" role="status">
        <SpinnerIcon spin />
        Loading downloads…
      </div>
    );
  }
  if (state.load.status === "error") {
    return (
      <div className="app-message" role="alert">
        <AlertCircleIcon size="lg" className="app-message__icon" />
        <h1 className="app-message__title">Siphon could not load its state</h1>
        <p className="app-message__text selectable">{state.load.message}</p>
        <Button variant="primary" onClick={reload}>
          Try again
        </Button>
      </div>
    );
  }

  return (
    <DialogsContext.Provider value={dialogs}>
      <QueueScreen />
      <ClipboardPrompt />
      {dialog && <ActiveDialog request={dialog} onClose={() => setDialog(null)} />}
    </DialogsContext.Provider>
  );
}

export default function App() {
  return (
    <StoreProvider>
      <div className="app">
        <WindowBar />
        <LoadGate />
        <Notices />
      </div>
    </StoreProvider>
  );
}
