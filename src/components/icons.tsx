import type { ReactNode } from "react";

export interface IconProps {
  size?: "sm" | "md" | "lg";
  className?: string;
  spin?: boolean;
}

function makeIcon(paths: ReactNode) {
  return function Icon({ size = "md", className, spin }: IconProps) {
    const classes = ["icon"];
    if (size !== "md") classes.push(`icon--${size}`);
    if (spin) classes.push("icon--spin");
    if (className) classes.push(className);
    return (
      <svg className={classes.join(" ")} viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        {paths}
      </svg>
    );
  };
}

export const LinkIcon = makeIcon(
  <>
    <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1" />
    <path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" />
  </>,
);
export const SettingsIcon = makeIcon(
  <>
    <path d="M4 7h10M18 7h2M4 17h4M12 17h8" />
    <circle cx="16" cy="7" r="2" />
    <circle cx="10" cy="17" r="2" />
  </>,
);
export const DownloadIcon = makeIcon(<path d="M12 4v13M6 11l6 6 6-6" />);
export const DownloadTrayIcon = makeIcon(<path d="M12 4v13M6 11l6 6 6-6M5 20h14" />);
export const PauseIcon = makeIcon(<path d="M9 6v12M15 6v12" />);
export const PlayIcon = makeIcon(<path d="M8 5.5v13l10-6.5z" />);
export const RetryIcon = makeIcon(<path d="M19 12a7 7 0 1 1-2.05-4.95M19 5v4h-4" />);
export const OpenIcon = makeIcon(
  <path d="M14 5h5v5M19 5l-8 8M18 14v4a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h4" />,
);
export const FolderIcon = makeIcon(
  <path d="M4 7a2 2 0 0 1 2-2h3.5l2 2H18a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z" />,
);
export const MoreIcon = makeIcon(<path d="M6 12h.01M12 12h.01M18 12h.01" strokeWidth="2.5" />);
export const ChevronDownIcon = makeIcon(<path d="M7 10l5 5 5-5" />);
export const ChevronUpIcon = makeIcon(<path d="M7 14l5-5 5 5" />);
export const CloseIcon = makeIcon(<path d="M7 7l10 10M17 7L7 17" />);
export const MinimizeIcon = makeIcon(<path d="M6 12h12" />);
export const MaximizeIcon = makeIcon(<rect x="6.5" y="6.5" width="11" height="11" rx="1.5" />);
export const RestoreIcon = makeIcon(
  <>
    <rect x="5.5" y="8.5" width="10" height="10" rx="1.5" />
    <path d="M9 6h7.5A1.5 1.5 0 0 1 18 7.5V15" />
  </>,
);
export const CheckIcon = makeIcon(<path d="M5 12.5l4.5 4.5L19 7.5" />);
export const CheckCircleIcon = makeIcon(
  <>
    <circle cx="12" cy="12" r="8" />
    <path d="M8.5 12.5l2.5 2.5 4.5-5" />
  </>,
);
export const AlertCircleIcon = makeIcon(
  <>
    <circle cx="12" cy="12" r="8" />
    <path d="M12 8v4.5M12 15.8v.2" />
  </>,
);
export const XCircleIcon = makeIcon(
  <>
    <circle cx="12" cy="12" r="8" />
    <path d="M9.5 9.5l5 5M14.5 9.5l-5 5" />
  </>,
);
export const MinusCircleIcon = makeIcon(
  <>
    <circle cx="12" cy="12" r="8" />
    <path d="M9 12h6" />
  </>,
);
export const InfoIcon = makeIcon(
  <>
    <circle cx="12" cy="12" r="8" />
    <path d="M12 11v5M12 8v.2" />
  </>,
);
export const ClockIcon = makeIcon(
  <>
    <circle cx="12" cy="12" r="8" />
    <path d="M12 8v4l3 2" />
  </>,
);
export const LockIcon = makeIcon(
  <>
    <rect x="5" y="10" width="14" height="10" rx="2" />
    <path d="M8 10V7a4 4 0 0 1 8 0v3" />
  </>,
);
export const LayersIcon = makeIcon(
  <>
    <path d="M12 4l8 4.5-8 4.5-8-4.5z" />
    <path d="M4 15.5l8 4.5 8-4.5" />
  </>,
);
export const ClipboardIcon = makeIcon(
  <>
    <rect x="7" y="4" width="10" height="4" rx="1" />
    <path d="M7 6H6a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V7a1 1 0 0 0-1-1h-1" />
  </>,
);
export const SearchIcon = makeIcon(
  <>
    <circle cx="11" cy="11" r="6" />
    <path d="M20 20l-4.5-4.5M9 11h4" />
  </>,
);
export const PlusIcon = makeIcon(<path d="M6 12h12M12 6v12" />);
export const MinusIcon = makeIcon(<path d="M6 12h12" />);
export const CopyIcon = makeIcon(
  <>
    <rect x="9" y="9" width="10" height="10" rx="1.5" />
    <path d="M15 9V6.5A1.5 1.5 0 0 0 13.5 5h-7A1.5 1.5 0 0 0 5 6.5v7A1.5 1.5 0 0 0 6.5 15H9" />
  </>,
);
export const TrashIcon = makeIcon(
  <path d="M5 7h14M10 7V5h4v2M7 7l1 12h8l1-12M10.5 10.5v5M13.5 10.5v5" />,
);
export const StopIcon = makeIcon(<rect x="7" y="7" width="10" height="10" rx="1.5" />);
export const OptionsIcon = makeIcon(
  <>
    <circle cx="12" cy="12" r="2.5" />
    <path d="M12 4v3M12 17v3M4 12h3M17 12h3M6.3 6.3l2.1 2.1M15.6 15.6l2.1 2.1M6.3 17.7l2.1-2.1M15.6 8.4l2.1-2.1" />
  </>,
);
export const SpinnerIcon = makeIcon(<path d="M12 4a8 8 0 1 0 8 8" />);

export function Logo() {
  return (
    // The same mark as siphon/app-icon.svg, the source of the app icon files.
    <svg width="20" height="20" viewBox="64 64 896 896" aria-hidden="true" focusable="false">
      <defs>
        <linearGradient id="siphon-logo-bg" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="var(--color-brand-start)" />
          <stop offset="1" stopColor="var(--color-brand-end)" />
        </linearGradient>
      </defs>
      <rect x="64" y="64" width="896" height="896" rx="208" fill="url(#siphon-logo-bg)" />
      <g
        fill="none"
        stroke="var(--color-on-accent)"
        strokeWidth="92"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M330 560V430a140 140 0 0 1 280 0v260" />
        <path d="M500 590l110 110 110-110" />
        <path d="M330 790h364" />
      </g>
    </svg>
  );
}
