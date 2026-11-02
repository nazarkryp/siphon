import type { ByteProgress, Download, DownloadStatus, ProgressDetail as Detail } from "../../api";
import { ProgressBar, type BarTone } from "../../components/ProgressBar";
import {
  CheckCircleIcon,
  CheckIcon,
  DownloadIcon,
  LayersIcon,
  LockIcon,
  PauseIcon,
} from "../../components/icons";
import { formatBytes, formatBytesOf, formatPercent } from "../../lib/format";
import { groupSegments } from "./segments";
import "./progress.css";

function activeTone(status: DownloadStatus): BarTone {
  if (status === "downloading") return "downloading";
  if (status === "failed") return "failed";
  if (status === "merging" || status === "done") return "done";
  return "paused";
}

function isComplete(progress: ByteProgress): boolean {
  return progress.total != null && progress.done >= progress.total;
}

function fraction(progress: ByteProgress): number | null {
  if (progress.total == null || progress.total === 0) return null;
  return progress.done / progress.total;
}

function describeBytes(progress: ByteProgress): string {
  const value = fraction(progress);
  if (value == null || progress.total == null) return `${formatBytes(progress.done)} so far`;
  return `${formatPercent(value)} · ${formatBytesOf(progress.done, progress.total)}`;
}

function mergeSuffix(download: Download): string {
  return download.mergeProgress != null ? ` · ${formatPercent(download.mergeProgress)}` : "";
}

function DoneNote({ text = "Done" }: { text?: string }) {
  return (
    <span className="detail__done">
      <CheckIcon size="sm" />
      {text}
    </span>
  );
}

function ChunksView({ download, chunks }: { download: Download; chunks: ByteProgress[] }) {
  const tone = activeTone(download.status);
  return (
    <>
      <div className="detail__head">
        <span className="num">
          {chunks.length} {chunks.length === 1 ? "connection" : "connections"}
        </span>
        <span>Each connection fetches its own byte range into one .part file</span>
      </div>
      <ol className="chunks">
        {chunks.map((chunk, index) => {
          const done = isComplete(chunk);
          const value = fraction(chunk);
          const label = `Part ${index + 1}`;
          return (
            <li key={index} className="chunk">
              <div className="chunk__labels">
                <span className="num">
                  {label}
                  {value != null && ` · ${formatPercent(value)}`}
                </span>
                {done ? (
                  <DoneNote />
                ) : (
                  <span className="num">
                    {chunk.total != null
                      ? formatBytesOf(chunk.done, chunk.total)
                      : formatBytes(chunk.done)}
                  </span>
                )}
              </div>
              <ProgressBar value={value} tone={done ? "done" : tone} label={label} />
            </li>
          );
        })}
      </ol>
    </>
  );
}

type SegmentsDetail = Extract<Detail, { type: "segments" }>;

function SegmentsView({ download, detail }: { download: Download; detail: SegmentsDetail }) {
  const { cells, perCell } = groupSegments(detail.states);
  const waiting = Math.max(0, detail.total - detail.done - detail.inFlight);
  const summary = `${detail.done} of ${detail.total} segments done, ${detail.inFlight} downloading, ${waiting} waiting`;
  return (
    <>
      <div className="detail__head">
        <span className="num">
          Segments · {detail.done} of {detail.total} done
          {detail.inFlight > 0 && ` · ${detail.inFlight} in parallel`}
        </span>
        <span>
          {download.status === "merging"
            ? `Joining with ffmpeg${mergeSuffix(download)}`
            : "Then: join with ffmpeg (copy, no re-encode)"}
        </span>
      </div>
      <div className="segments" role="img" aria-label={summary}>
        {cells.map((cell, index) => (
          <span key={index} className={`segment segment--${cell}`} />
        ))}
      </div>
      <div className="legend">
        <span className="legend__key">
          <span className="segment segment--done" />
          <span className="num">Done {detail.done}</span>
        </span>
        <span className="legend__key">
          <span className="segment segment--inFlight" />
          <span className="num">Downloading {detail.inFlight}</span>
        </span>
        <span className="legend__key">
          <span className="segment segment--waiting" />
          <span className="num">Waiting {waiting}</span>
        </span>
        {perCell > 1 && (
          <span className="legend__note num">One cell = up to {perCell} segments</span>
        )}
      </div>
      {detail.encrypted && (
        <div className="detail__note">
          <LockIcon size="sm" />
          AES-128 · segments are decrypted on the fly
        </div>
      )}
    </>
  );
}

type StreamsDetail = Extract<Detail, { type: "streams" }>;

function StreamBlock({
  title,
  label,
  progress,
  tone,
}: {
  title: string;
  label: string;
  progress: ByteProgress;
  tone: BarTone;
}) {
  const done = isComplete(progress);
  return (
    <div className="stream-block">
      <div className="stream-block__labels">
        <span className="stream-block__title">{title}</span>
        {done ? (
          <DoneNote text={`Done · ${formatBytes(progress.done)}`} />
        ) : (
          <span className="num">{describeBytes(progress)}</span>
        )}
      </div>
      <ProgressBar value={fraction(progress)} tone={done ? "done" : tone} label={title} />
      <div className="detail__sub">{label}</div>
    </div>
  );
}

function StreamsView({ download, detail }: { download: Download; detail: StreamsDetail }) {
  const tone = activeTone(download.status);
  const videoDone = isComplete(detail.video);
  const audioDone = isComplete(detail.audio);
  const merging = download.status === "merging";

  function partStep(name: string, done: boolean) {
    if (done) {
      return (
        <li className="step-line step-line--done">
          <CheckCircleIcon />
          {name} downloaded
        </li>
      );
    }
    const running = download.status === "downloading";
    return (
      <li className={running ? "step-line step-line--active" : "step-line"}>
        {running ? <DownloadIcon /> : <PauseIcon />}
        {name} {running ? "downloading" : "waiting"}
      </li>
    );
  }

  return (
    <>
      <div className="streams">
        <StreamBlock title="Video" label={detail.videoLabel} progress={detail.video} tone={tone} />
        <StreamBlock title="Audio" label={detail.audioLabel} progress={detail.audio} tone={tone} />
      </div>
      <ol className="step-lines">
        {partStep("Video", videoDone)}
        {partStep("Audio", audioDone)}
        {download.status === "done" ? (
          <li className="step-line step-line--done">
            <CheckCircleIcon />
            Merged with ffmpeg
          </li>
        ) : (
          <li className={merging ? "step-line step-line--merging" : "step-line"}>
            <LayersIcon />
            {merging
              ? `Merging with ffmpeg${mergeSuffix(download)}`
              : "Then: merge with ffmpeg (copy, no re-encode)"}
          </li>
        )}
      </ol>
    </>
  );
}

export function ProgressDetail({ download, id }: { download: Download; id: string }) {
  const { detail } = download;
  return (
    <div className="detail" id={id}>
      {detail.type === "chunks" && <ChunksView download={download} chunks={detail.chunks} />}
      {detail.type === "segments" && <SegmentsView download={download} detail={detail} />}
      {detail.type === "streams" && <StreamsView download={download} detail={detail} />}
      {download.status === "merging" && detail.type === "chunks" && (
        <div className="merge-line num">
          <LayersIcon size="sm" />
          Finishing with ffmpeg{mergeSuffix(download)}
        </div>
      )}
    </div>
  );
}
