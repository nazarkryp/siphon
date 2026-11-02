import type { ReactNode } from "react";
import { IconButton } from "./IconButton";
import { AlertCircleIcon, CloseIcon, InfoIcon } from "./icons";

interface BannerProps {
  tone: "warning" | "neutral";
  title?: string;
  children: ReactNode;
  actions?: ReactNode;
  onDismiss?: () => void;
  /** False while the content changes often, for example during a progress display. */
  live?: boolean;
}

export function Banner({ tone, title, children, actions, onDismiss, live = true }: BannerProps) {
  const role = tone === "warning" ? "alert" : "status";
  return (
    <div className={`banner banner--${tone}`} role={live ? role : undefined}>
      {tone === "warning" ? (
        <AlertCircleIcon size="lg" className="banner__icon" />
      ) : (
        <InfoIcon size="lg" className="banner__icon" />
      )}
      <div className="banner__body">
        {title && <div className="banner__title">{title}</div>}
        <div className="banner__text">{children}</div>
      </div>
      {actions}
      {onDismiss && <IconButton label="Dismiss" icon={<CloseIcon />} onClick={onDismiss} />}
    </div>
  );
}
