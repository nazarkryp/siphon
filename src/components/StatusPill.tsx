import type { ReactNode } from "react";
import type { DownloadStatus } from "../api";
import {
  AlertCircleIcon,
  CheckIcon,
  ClockIcon,
  DownloadIcon,
  LayersIcon,
  LockIcon,
  PauseIcon,
} from "./icons";

export type PillTone =
  | "queued"
  | "downloading"
  | "paused"
  | "merging"
  | "done"
  | "failed"
  | "needs-ffmpeg";

interface PillProps {
  tone: PillTone;
  icon: ReactNode;
  children: ReactNode;
}

export function Pill({ tone, icon, children }: PillProps) {
  return (
    <span className={`pill pill--${tone}`}>
      {icon}
      {children}
    </span>
  );
}

const STATUS_PILLS: Record<DownloadStatus, { tone: PillTone; label: string; icon: ReactNode }> = {
  queued: { tone: "queued", label: "Queued", icon: <ClockIcon size="sm" /> },
  downloading: { tone: "downloading", label: "Downloading", icon: <DownloadIcon size="sm" /> },
  paused: { tone: "paused", label: "Paused", icon: <PauseIcon size="sm" /> },
  merging: { tone: "merging", label: "Merging", icon: <LayersIcon size="sm" /> },
  done: { tone: "done", label: "Done", icon: <CheckIcon size="sm" /> },
  failed: { tone: "failed", label: "Failed", icon: <AlertCircleIcon size="sm" /> },
  needsFfmpeg: { tone: "needs-ffmpeg", label: "Needs ffmpeg", icon: <LockIcon size="sm" /> },
};

export function StatusPill({ status }: { status: DownloadStatus }) {
  const pill = STATUS_PILLS[status];
  return (
    <Pill tone={pill.tone} icon={pill.icon}>
      {pill.label}
    </Pill>
  );
}
