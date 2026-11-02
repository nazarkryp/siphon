import { useEffect } from "react";
import type { Notice } from "../state/reducer";
import { useStore } from "../state/store";
import { IconButton } from "./IconButton";
import { AlertCircleIcon, CheckCircleIcon, CloseIcon } from "./icons";

const INFO_TIMEOUT_MS = 4000;

function NoticeItem({ notice }: { notice: Notice }) {
  const { dispatch } = useStore();
  const { id, kind } = notice;
  const onDismiss = () => dispatch({ type: "noticeDismissed", id });

  useEffect(() => {
    if (kind !== "info") return;
    const timer = window.setTimeout(
      () => dispatch({ type: "noticeDismissed", id }),
      INFO_TIMEOUT_MS,
    );
    return () => window.clearTimeout(timer);
  }, [id, kind, dispatch]);

  return (
    <div className={`notice notice--${notice.kind}`} role={notice.kind === "error" ? "alert" : "status"}>
      {notice.kind === "error" ? (
        <AlertCircleIcon className="notice__icon" />
      ) : (
        <CheckCircleIcon className="notice__icon" />
      )}
      <div className="notice__text selectable">{notice.message}</div>
      <IconButton label="Dismiss" size="sm" icon={<CloseIcon />} onClick={onDismiss} />
    </div>
  );
}

/** Messages from failed actions. Errors stay until dismissed. */
export function Notices() {
  const { state } = useStore();
  if (state.notices.length === 0) return null;
  return (
    <div className="notices">
      {state.notices.map((notice) => (
        <NoticeItem key={notice.id} notice={notice} />
      ))}
    </div>
  );
}
