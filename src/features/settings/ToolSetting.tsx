import type { RefObject } from "react";
import type { ToolId, ToolStatus, ToolUpdate } from "../../api";
import { Button } from "../../components/Button";
import { Pill } from "../../components/StatusPill";
import { TextField } from "../../components/TextField";
import { AlertCircleIcon, CheckIcon, MinusCircleIcon, SpinnerIcon } from "../../components/icons";
import { InstallProgress } from "../tools/InstallProgress";
import { TOOL_INFO, useToolInstall } from "../tools/useToolInstall";
import { isUpToDate } from "../tools/useToolUpdates";

function ToolPill({ status, required, upToDate }: { status: ToolStatus | null; required: boolean; upToDate: boolean }) {
  if (!status) {
    return (
      <Pill tone="queued" icon={<SpinnerIcon size="sm" spin />}>
        Checking…
      </Pill>
    );
  }
  if (status.found) {
    return (
      <Pill tone="done" icon={<CheckIcon size="sm" />}>
        {upToDate ? "Up to date" : "Ready"}
        {status.version ? ` · ${status.version}` : ""}
      </Pill>
    );
  }
  return required ? (
    <Pill tone="failed" icon={<AlertCircleIcon size="sm" />}>
      Not found
    </Pill>
  ) : (
    <Pill tone="needs-ffmpeg" icon={<MinusCircleIcon size="sm" />}>
      Not found
    </Pill>
  );
}

interface ToolSettingProps {
  tool: ToolId;
  /** Null until the first tool detection ends. */
  status: ToolStatus | null;
  /** Null while Siphon checks for a newer release. */
  update: ToolUpdate | null;
  required: boolean;
  value: string;
  onChange: (value: string) => void;
  inputRef: RefObject<HTMLInputElement | null>;
  hint: string;
}

/** Path field, status pill and Install or Update button of one tool. */
export function ToolSetting({ tool, status, update, required, value, onChange, inputRef, hint }: ToolSettingProps) {
  const { install, start } = useToolInstall(tool);
  const installing = install?.status === "running";
  const failed = install?.status === "failed";
  const { name, size } = TOOL_INFO[tool];
  const found = status?.found ?? false;
  const verb = found ? "Update" : "Install";
  const upToDate = isUpToDate(status, update);
  // A found tool waits for the update check, so that the button does not show and then go away.
  const checking = found && !update && !installing && !failed;
  const showButton = !upToDate && !checking;

  let label = `${verb} ${name} (${size})`;
  if (failed) label = "Try again";
  else if (found && update?.latest) label = `Update to ${update.latest} (${size})`;

  return (
    <div className="settings__item settings__item--stacked">
      <TextField
        ref={inputRef}
        label={
          <span className="settings__label">
            {name} <span className="settings__label-note">· {required ? "required" : "optional"}</span>
          </span>
        }
        labelAside={<ToolPill status={status} required={required} upToDate={upToDate} />}
        value={value}
        placeholder={status?.path ?? `Empty: find ${name} on PATH`}
        disabled={installing}
        onChange={(event) => onChange(event.target.value)}
        hint={hint}
      />
      {(showButton || install) && (
        <div className="install-row">
          {showButton && (
            <Button
              loading={installing}
              aria-label={failed ? `Try again: ${verb.toLowerCase()} ${name}` : undefined}
              onClick={() => void start()}
            >
              {label}
            </Button>
          )}
          <InstallProgress tool={tool} install={install} />
        </div>
      )}
      {checking && <p className="field-hint">Checking for a newer version…</p>}
      {found && update?.error && !installing && <p className="field-hint">{update.error}</p>}
      {status?.found && status.devBuild && !installing && (
        <div className="install-error" role="note">
          <AlertCircleIcon size="sm" />
          <span>
            This is a development build of {name}. Some development builds make broken video files. Click{" "}
            {verb} to install the release build.
          </span>
        </div>
      )}
    </div>
  );
}
