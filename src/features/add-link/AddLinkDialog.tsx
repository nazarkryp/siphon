import { useEffect, useRef, useState } from "react";
import { api, type ResolveResult, type Stream } from "../../api";
import { Button } from "../../components/Button";
import { Dialog } from "../../components/Dialog";
import { TextField } from "../../components/TextField";
import { AlertCircleIcon, DownloadIcon } from "../../components/icons";
import { errorMessage } from "../../lib/errors";
import { formatBytes, formatSize } from "../../lib/format";
import { useAppData, useStore } from "../../state/store";
import { NoStreamPanel, ResolveErrorPanel } from "./NoStreamPanel";
import { ResolveSteps, scanLog } from "./ResolveSteps";
import { StreamList } from "./StreamList";
import "./add-link.css";

type Phase =
  | { kind: "idle" }
  | { kind: "resolving" }
  | { kind: "failed"; message: string }
  | { kind: "resolved"; result: ResolveResult };

interface AddLinkDialogProps {
  url: string;
  onClose: () => void;
  onAdded?: () => void;
}

function orNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

export function AddLinkDialog({ url, onClose, onAdded }: AddLinkDialogProps) {
  const { dispatch } = useStore();
  const { settings, tools } = useAppData();
  const linkRef = useRef<HTMLInputElement>(null);
  const requestRef = useRef(0);

  const [link, setLink] = useState(url);
  const [phase, setPhase] = useState<Phase>({ kind: "resolving" });
  const [referer, setReferer] = useState("");
  const [userAgent, setUserAgent] = useState("");
  const [streamId, setStreamId] = useState<string | null>(null);
  const [qualityId, setQualityId] = useState<string | null>(null);
  const [fileName, setFileName] = useState("");
  const [folder, setFolder] = useState(settings.downloadFolder);
  const [freeSpace, setFreeSpace] = useState<number | null>(null);
  const [folderError, setFolderError] = useState<string | null>(null);
  const [browsing, setBrowsing] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const result = phase.kind === "resolved" ? phase.result : null;
  const streams = result?.streams ?? [];
  const stream = streams.find((s) => s.id === streamId) ?? null;
  const quality = stream?.qualities.find((q) => q.id === qualityId) ?? null;

  function selectStream(next: Stream) {
    setStreamId(next.id);
    setQualityId(next.qualities[0]?.id ?? null);
    setFileName(next.suggestedFileName);
  }

  async function resolve(target: string, refererValue: string, userAgentValue: string) {
    const request = ++requestRef.current;
    setPhase({ kind: "resolving" });
    setSubmitError(null);
    setCopied(false);
    try {
      const next = await api.resolveLink(target, orNull(refererValue), orNull(userAgentValue));
      if (request !== requestRef.current) return;
      setPhase({ kind: "resolved", result: next });
      if (next.streams.length > 0) selectStream(next.streams[0]);
      if (folder === settings.downloadFolder) setFreeSpace(next.freeSpaceBytes);
      const foundOnPage = next.steps.some((s) => s.id === "page" && s.state === "found");
      if (foundOnPage && refererValue.trim() === "") setReferer(next.url);
    } catch (error) {
      if (request === requestRef.current) setPhase({ kind: "failed", message: errorMessage(error) });
    }
  }

  useEffect(() => {
    void resolve(url, "", "");
    // Resolve once for the link that opened the dialog. "Fetch again" resolves later links.
  }, [url]);

  async function refreshFreeSpace(path: string) {
    setFolderError(null);
    try {
      setFreeSpace(await api.freeSpace(path));
    } catch (error) {
      setFreeSpace(null);
      setFolderError(`Could not read the free space: ${errorMessage(error)}`);
    }
  }

  async function browse() {
    setBrowsing(true);
    try {
      const picked = await api.pickFolder(folder || null);
      if (picked) {
        setFolder(picked);
        await refreshFreeSpace(picked);
      }
    } catch (error) {
      setFolderError(errorMessage(error));
    } finally {
      setBrowsing(false);
    }
  }

  function tryAnotherLink() {
    requestRef.current++;
    setPhase({ kind: "idle" });
    setLink("");
    setStreamId(null);
    setQualityId(null);
    linkRef.current?.focus();
  }

  async function copyScanLog() {
    if (!result) return;
    try {
      await navigator.clipboard.writeText(scanLog(result.url, result.steps));
      setCopied(true);
    } catch (error) {
      setSubmitError(`Could not copy the scan log: ${errorMessage(error)}`);
    }
  }

  async function download() {
    if (!result || !stream || !quality) return;
    setSubmitting(true);
    setSubmitError(null);
    try {
      const added = await api.addDownload({
        resolveId: result.resolveId,
        streamId: stream.id,
        qualityId: quality.id,
        fileName: fileName.trim(),
        folder: folder.trim(),
        referer: orNull(referer),
        userAgent: orNull(userAgent),
      });
      dispatch({ type: "downloadUpserted", download: added });
      onAdded?.();
      onClose();
    } catch (error) {
      setSubmitError(errorMessage(error));
      setSubmitting(false);
    }
  }

  const resolving = phase.kind === "resolving";
  const noStreams = result != null && streams.length === 0;
  const needsFfmpeg = stream != null && (stream.kind === "hls" || quality?.separateAudio === true);
  const canDownload =
    stream != null && quality != null && fileName.trim() !== "" && folder.trim() !== "";

  const requestFields = (
    <>
      <TextField
        label="Referer"
        value={referer}
        placeholder="None"
        onChange={(event) => setReferer(event.target.value)}
        hint={
          result && referer === result.url
            ? "Filled from the page. Many HLS servers need it."
            : "The page that plays the video. Many HLS servers need it."
        }
      />
      <TextField
        label="User-Agent"
        value={userAgent}
        placeholder="Siphon default (desktop browser)"
        onChange={(event) => setUserAgent(event.target.value)}
        hint="Change only if the server refuses the request."
      />
    </>
  );

  const footerMessage = submitError ? (
    <div className="dialog__footer-error selectable" role="alert">
      <AlertCircleIcon size="sm" />
      {submitError}
    </div>
  ) : null;

  const footer = noStreams ? (
    <>
      {footerMessage ?? (
        <div className="dialog__footer-info">
          <Button variant="link" onClick={() => void copyScanLog()}>
            {copied ? "Scan log copied" : "Copy the scan log"}
          </Button>
          <span className="sr-only" role="status">
            {copied ? "Scan log copied" : ""}
          </span>
        </div>
      )}
      <Button onClick={onClose}>Close</Button>
      <Button variant="primary" onClick={tryAnotherLink}>
        Try another link
      </Button>
    </>
  ) : (
    <>
      {footerMessage ?? (
        <div className="dialog__footer-info">
          {quality && (
            <>
              <strong className="num">
                {quality.sizeBytes != null
                  ? formatSize(quality.sizeBytes, quality.sizeIsEstimate)
                  : "Size unknown"}
              </strong>
              <span className="add-link__meta num">
                {freeSpace != null && ` · ${formatBytes(freeSpace)} free`}
                {needsFfmpeg && tools?.ffmpeg.found && " · joined with ffmpeg after download"}
              </span>
              {needsFfmpeg && tools && !tools.ffmpeg.found && (
                <div className="add-link__warning">
                  <AlertCircleIcon size="sm" />
                  ffmpeg not found. The download waits in the queue until you set it.
                </div>
              )}
            </>
          )}
        </div>
      )}
      <Button onClick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        icon={<DownloadIcon />}
        loading={submitting}
        disabled={!canDownload}
        onClick={() => void download()}
      >
        Download
      </Button>
    </>
  );

  return (
    <Dialog title="Add link" size="lg" onClose={onClose} footer={footer} initialFocusRef={linkRef}>
      <div className="add-link">
        <TextField
          ref={linkRef}
          label="Link"
          value={link}
          onChange={(event) => setLink(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && link.trim() !== "") void resolve(link.trim(), referer, userAgent);
          }}
          trailing={
            <Button
              loading={resolving}
              disabled={link.trim() === ""}
              onClick={() => void resolve(link.trim(), referer, userAgent)}
            >
              {phase.kind === "idle" ? "Fetch" : "Fetch again"}
            </Button>
          }
        />

        {phase.kind === "idle" && (
          <p className="field-hint">Paste a link and press Fetch.</p>
        )}

        {(resolving || result) && (
          <ResolveSteps steps={result?.steps ?? null} nothingFound={noStreams} />
        )}

        {phase.kind === "failed" && <ResolveErrorPanel message={phase.message} />}
        {noStreams && <NoStreamPanel />}

        {stream && (
          <div className="add-link__fields">
            <TextField
              label="File name"
              value={fileName}
              onChange={(event) => setFileName(event.target.value)}
            />
            <TextField
              label="Folder"
              value={folder}
              onChange={(event) => setFolder(event.target.value)}
              onBlur={() => folder.trim() !== "" && void refreshFreeSpace(folder.trim())}
              error={folderError}
              trailing={
                <Button loading={browsing} onClick={() => void browse()}>
                  Browse
                </Button>
              }
            />
          </div>
        )}

        {result && streams.length > 0 && (
          <StreamList
            streams={streams}
            streamId={streamId}
            qualityId={qualityId}
            onSelectStream={selectStream}
            onSelectQuality={setQualityId}
          />
        )}

        {!resolving && phase.kind !== "idle" && (
          <div className="add-link__fields">{requestFields}</div>
        )}
      </div>
    </Dialog>
  );
}
