import { memo, type ReactNode } from "react";
import type { Download } from "../../api";
import { IconButton } from "../../components/IconButton";
import { KindBadge } from "../../components/KindBadge";
import { ProgressBar } from "../../components/ProgressBar";
import { StatusPill } from "../../components/StatusPill";
import { ChevronDownIcon, ChevronUpIcon } from "../../components/icons";
import {
  formatBytes,
  formatBytesOf,
  formatEta,
  formatPercent,
  formatRelative,
  formatSize,
  formatSpeed,
} from "../../lib/format";
import { ProgressDetail } from "../progress/ProgressDetail";
import { RowActions } from "./RowActions";
import { SelectBox } from "./SelectBox";

interface QueueRowProps {
  download: Download;
  position: number | undefined;
  expanded: boolean;
  onToggle: (id: string) => void;
  selected: boolean;
  onSelect: (id: string) => void;
  now: number;
}

function fractionOf(download: Download): number | null {
  if (download.totalBytes == null || download.totalBytes === 0) return null;
  return download.doneBytes / download.totalBytes;
}

function progressText(download: Download): string {
  const { detail, totalBytes, doneBytes, totalIsEstimate } = download;
  const fraction = fractionOf(download);
  if (detail.type === "segments") {
    const percent = formatPercent(detail.total > 0 ? detail.done / detail.total : 0);
    return `${percent} · ${detail.done} of ${detail.total} segments`;
  }
  if (fraction == null || totalBytes == null) return `${formatBytes(doneBytes)} downloaded`;
  const bytes = `${totalIsEstimate ? "≈ " : ""}${formatBytesOf(doneBytes, totalBytes)}`;
  const connections =
    detail.type === "chunks" && download.status === "downloading"
      ? ` · ${detail.chunks.length} connections`
      : "";
  return `${formatPercent(fraction)} · ${bytes}${connections}`;
}

function mergeText(download: Download): string {
  const what = download.detail.type === "streams" ? "Joining video and audio" : "Joining with ffmpeg";
  return download.mergeProgress != null ? `${what} · ${formatPercent(download.mergeProgress)}` : what;
}

/** Speed and time left for narrow windows, where those columns are hidden. */
function narrowExtras(download: Download): string {
  if (download.status !== "downloading") return "";
  const eta = download.etaSeconds != null ? ` · ${formatEta(download.etaSeconds)} left` : "";
  return ` · ${formatSpeed(download.speed)}${eta}`;
}

function ProgressCell({ download, position, now }: Pick<QueueRowProps, "download" | "position" | "now">) {
  const label = `Progress of ${download.fileName}`;
  switch (download.status) {
    case "downloading":
    case "paused":
      return (
        <>
          <ProgressBar value={fractionOf(download)} tone={download.status} label={label} />
          <div className="queue-row__sub num">
            {progressText(download)}
            <span className="queue-row__narrow">{narrowExtras(download)}</span>
          </div>
        </>
      );
    case "merging":
      return (
        <>
          <ProgressBar value={download.mergeProgress ?? null} tone="merging" label={label} striped />
          <div className="queue-row__sub num">{mergeText(download)}</div>
        </>
      );
    case "done":
      return (
        <>
          <ProgressBar value={1} tone="done" label={label} />
          <div className="queue-row__sub">
            {download.finishedAt != null ? `Finished ${formatRelative(download.finishedAt, now)}` : "Finished"}
          </div>
        </>
      );
    case "queued":
      return (
        <>
          {download.doneBytes > 0 && (
            <ProgressBar value={fractionOf(download)} tone="paused" label={label} />
          )}
          <div className="queue-row__sub">
            {position === 1 ? "Next in line" : `Position ${position ?? "—"} in the queue`}
            {download.doneBytes > 0 && ` · ${progressText(download)}`}
          </div>
        </>
      );
    default:
      return null;
  }
}

function WideMessage({ download }: { download: Download }) {
  if (download.status === "failed") {
    return (
      <div className="queue-row__wide selectable">
        <div className="queue-row__error">{download.error?.message ?? "The download failed."}</div>
        {download.error?.hint && <div className="queue-row__hint">{download.error.hint}</div>}
      </div>
    );
  }
  return (
    <div className="queue-row__wide">
      <div className="queue-row__sub queue-row__sub--flush">
        {download.doneBytes > 0
          ? "Waits until ffmpeg is set. The downloaded part is kept."
          : "Waits until ffmpeg is set. Nothing is downloaded yet."}
      </div>
    </div>
  );
}

function Dash() {
  return <span className="queue-row__muted">—</span>;
}

function speedCell(download: Download): ReactNode {
  return download.status === "downloading" ? formatSpeed(download.speed) : <Dash />;
}

function etaCell(download: Download): ReactNode {
  const running = download.status === "downloading" || download.status === "merging";
  if (!running || download.etaSeconds == null) return <Dash />;
  return `${download.status === "merging" ? "≈ " : ""}${formatEta(download.etaSeconds)}`;
}

export const QueueRow = memo(function QueueRow({
  download,
  position,
  expanded,
  onToggle,
  selected,
  onSelect,
  now,
}: QueueRowProps) {
  const hasDetail = download.detail.type !== "none";
  const showDetail = hasDetail && expanded;
  const detailId = `detail-${download.id}`;
  const wide = download.status === "failed" || download.status === "needsFfmpeg";
  const sub = [
    download.host,
    download.qualityLabel,
    download.totalBytes != null ? formatSize(download.totalBytes, download.totalIsEstimate) : null,
  ]
    .filter(Boolean)
    .join(" · ");

  const classes = ["queue-grid", "queue-row"];
  if (showDetail) classes.push("queue-row--expanded");
  if (selected) classes.push("queue-row--selected");
  if (wide) classes.push("queue-row--top");

  return (
    <li className="queue-item">
      <div className={classes.join(" ")}>
        <SelectBox
          label={`Select ${download.fileName}`}
          checked={selected}
          onChange={() => onSelect(download.id)}
        />
        <div className="queue-row__name-cell">
          {hasDetail ? (
            <IconButton
              size="sm"
              className="queue-row__toggle"
              label={showDetail ? `Hide details: ${download.fileName}` : `Show details: ${download.fileName}`}
              title={showDetail ? "Hide details" : "Show details"}
              icon={showDetail ? <ChevronUpIcon /> : <ChevronDownIcon />}
              aria-expanded={showDetail}
              aria-controls={showDetail ? detailId : undefined}
              onClick={() => onToggle(download.id)}
            />
          ) : (
            <span className="queue-row__toggle-space" />
          )}
          <div className="queue-row__name-text">
            <div className="queue-row__name selectable" title={download.fileName}>
              {download.fileName}
            </div>
            <div className="queue-row__sub">{sub}</div>
          </div>
        </div>
        <div>
          <KindBadge kind={download.kind} />
        </div>
        <div>
          <StatusPill status={download.status} />
        </div>
        {wide ? (
          <WideMessage download={download} />
        ) : (
          <>
            <div className="queue-row__progress">
              <ProgressCell download={download} position={position} now={now} />
            </div>
            <div className="queue-row__cell queue-col-speed num">{speedCell(download)}</div>
            <div className="queue-row__cell queue-col-eta num">{etaCell(download)}</div>
          </>
        )}
        <RowActions download={download} />
      </div>
      {showDetail && <ProgressDetail download={download} id={detailId} />}
    </li>
  );
});
