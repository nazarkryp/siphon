import { createContext, useContext } from "react";

export type SettingsFocus = "folder" | "ffmpeg" | "ytDlp";

export type DialogRequest =
  | { type: "addLink"; url: string; onAdded?: () => void }
  | { type: "settings"; focus?: SettingsFocus }
  | { type: "options"; id: string }
  | { type: "remove"; ids: string[] };

export interface Dialogs {
  open: (request: DialogRequest) => void;
}

export const DialogsContext = createContext<Dialogs | null>(null);

export function useDialogs(): Dialogs {
  const value = useContext(DialogsContext);
  if (!value) throw new Error("useDialogs needs a DialogsContext provider above it.");
  return value;
}
