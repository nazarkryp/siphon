import type { Quality, Stream } from "../../api";
import { KindBadge } from "../../components/KindBadge";
import { formatBitrate, formatDuration, formatSize } from "../../lib/format";

function streamHint(stream: Stream): string {
  const parts: string[] = [];
  const { qualities } = stream;
  if (qualities.length > 1) parts.push(`${qualities.length} qualities`);
  else if (qualities[0]?.resolution) parts.push(qualities[0].resolution);
  if (stream.durationSeconds != null) parts.push(formatDuration(stream.durationSeconds));
  if (stream.encrypted) parts.push("AES-128");
  if (qualities.length === 1 && qualities[0].note) parts.push(qualities[0].note);
  return parts.join(" · ");
}

function QualityTable({
  stream,
  selected,
  onSelect,
}: {
  stream: Stream;
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  return (
    <fieldset className="quality-table">
      <legend className="sr-only">Quality of {stream.title}</legend>
      <div className="quality-row quality-row--head" aria-hidden="true">
        <span />
        <span>Quality</span>
        <span>Resolution</span>
        <span>Codecs</span>
        <span>Bitrate</span>
        <span className="quality-row__end">Est. size</span>
      </div>
      {stream.qualities.map((quality: Quality, index) => (
        <label
          key={quality.id}
          className={quality.id === selected ? "quality-row quality-row--selected" : "quality-row"}
        >
          <input
            type="radio"
            name={`quality-${stream.id}`}
            checked={quality.id === selected}
            onChange={() => onSelect(quality.id)}
          />
          <span className="quality-row__label">
            <span className="quality-row__name">{quality.label}</span>
            {index === 0 && <span className="best-badge">Best</span>}
            {quality.separateAudio && <span className="kind-badge">video + audio</span>}
            {quality.note && stream.qualities.length > 1 && (
              <span className="quality-row__note">{quality.note}</span>
            )}
          </span>
          <span className="num">{quality.resolution ?? "—"}</span>
          <span className="quality-row__codecs" title={quality.codecs ?? undefined}>
            {quality.codecs ?? "—"}
          </span>
          <span className="num">{quality.bitrate != null ? formatBitrate(quality.bitrate) : "—"}</span>
          <span className="num quality-row__end">
            {quality.sizeBytes != null ? formatSize(quality.sizeBytes, quality.sizeIsEstimate) : "—"}
          </span>
        </label>
      ))}
    </fieldset>
  );
}

interface StreamListProps {
  streams: Stream[];
  streamId: string | null;
  qualityId: string | null;
  onSelectStream: (stream: Stream) => void;
  onSelectQuality: (id: string) => void;
}

export function StreamList({
  streams,
  streamId,
  qualityId,
  onSelectStream,
  onSelectQuality,
}: StreamListProps) {
  return (
    <fieldset className="stream-list">
      <legend className="stream-list__legend">
        <span className="field-label">Choose a stream</span>
        <span className="field-hint">Best quality is selected</span>
      </legend>
      <div className="stream-list__items">
        {streams.map((stream) => {
          const selected = stream.id === streamId;
          const best = stream.qualities[0];
          return (
            <div key={stream.id} className={selected ? "stream stream--selected" : "stream"}>
              <label className="stream__main">
                <input
                  type="radio"
                  name="stream"
                  checked={selected}
                  onChange={() => onSelectStream(stream)}
                />
                <span className="stream__text">
                  <span className="stream__title selectable" title={stream.url}>
                    {stream.title}
                  </span>
                  <span className="stream__hint">{streamHint(stream)}</span>
                </span>
                {!selected && best?.sizeBytes != null && (
                  <span className="stream__size num">
                    {formatSize(best.sizeBytes, best.sizeIsEstimate)}
                  </span>
                )}
                <KindBadge kind={stream.kind} />
              </label>
              {selected && (
                <QualityTable stream={stream} selected={qualityId} onSelect={onSelectQuality} />
              )}
            </div>
          );
        })}
      </div>
    </fieldset>
  );
}
