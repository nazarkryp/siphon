import type { ReactNode } from "react";
import type { ResolveStep, ResolveStepId, ResolveStepState } from "../../api";
import {
  CheckCircleIcon,
  MinusCircleIcon,
  SpinnerIcon,
  XCircleIcon,
} from "../../components/icons";

export const STEP_ORDER: ResolveStepId[] = ["direct", "hls", "ytdlp", "page"];

export const STEP_TITLES: Record<ResolveStepId, string> = {
  direct: "Direct file",
  hls: "HLS playlist",
  ytdlp: "yt-dlp",
  page: "Page scan",
};

export const STATE_LABELS: Record<ResolveStepState, string> = {
  found: "Found",
  notFound: "Not found",
  skipped: "Skipped",
  error: "Error",
};

type Look = "checking" | "found" | "neutral" | "skipped" | "failed";

const ICONS: Record<Look, ReactNode> = {
  checking: <SpinnerIcon spin />,
  found: <CheckCircleIcon />,
  neutral: <MinusCircleIcon />,
  skipped: <MinusCircleIcon />,
  failed: <XCircleIcon />,
};

function lookOf(step: ResolveStep, nothingFound: boolean): Look {
  if (step.state === "found") return "found";
  if (step.state === "error") return "failed";
  if (step.state === "skipped") return "skipped";
  // When no step found anything, the page scan was the last chance. Mark it like the design does.
  return nothingFound && step.id === "page" ? "failed" : "neutral";
}

interface ResolveStepsProps {
  /** null while Siphon checks the link. */
  steps: ResolveStep[] | null;
  nothingFound?: boolean;
}

export function ResolveSteps({ steps, nothingFound = false }: ResolveStepsProps) {
  return (
    <section aria-labelledby="resolve-steps-title">
      <h3 className="field-label add-link__section-title" id="resolve-steps-title">
        How Siphon looked for a download
      </h3>
      <ol className="resolve-steps" aria-busy={steps == null}>
        {STEP_ORDER.map((id, index) => {
          const step = steps?.find((s) => s.id === id);
          const look: Look = steps == null ? "checking" : step ? lookOf(step, nothingFound) : "skipped";
          const stateLabel = steps == null ? "Checking" : step ? STATE_LABELS[step.state] : "Not checked";
          const message = steps == null ? "Checking…" : (step?.message ?? "Not checked");
          return (
            <li key={id} className={`resolve-step resolve-step--${look}`}>
              <div className="resolve-step__title">
                {ICONS[look]}
                {index + 1} · {STEP_TITLES[id]}
              </div>
              <div className="resolve-step__message selectable">
                <span className="sr-only">{stateLabel}: </span>
                {message}
              </div>
            </li>
          );
        })}
      </ol>
    </section>
  );
}

export function scanLog(url: string, steps: ResolveStep[]): string {
  const lines = [`Siphon scan log`, `Link: ${url}`];
  for (const [index, id] of STEP_ORDER.entries()) {
    const step = steps.find((s) => s.id === id);
    const state = step ? STATE_LABELS[step.state] : "Not checked";
    lines.push(`${index + 1} · ${STEP_TITLES[id]} — ${state}${step?.message ? `: ${step.message}` : ""}`);
  }
  return lines.join("\n");
}
