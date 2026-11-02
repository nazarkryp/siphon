export type BarTone = "downloading" | "paused" | "done" | "failed" | "merging";

interface ProgressBarProps {
  /** 0..1, or null when the total is unknown. */
  value: number | null;
  tone: BarTone;
  label: string;
  striped?: boolean;
}

export function ProgressBar({ value, tone, label, striped = false }: ProgressBarProps) {
  const known = value != null;
  const percent = known ? Math.round(Math.min(1, Math.max(0, value)) * 100) : 100;
  const classes = ["bar__fill", `bar__fill--${tone}`];
  if (striped || !known) classes.push("bar__fill--striped");
  return (
    <div
      className="bar"
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={known ? percent : undefined}
    >
      <div className={classes.join(" ")} style={{ width: `${percent}%` }} />
    </div>
  );
}
