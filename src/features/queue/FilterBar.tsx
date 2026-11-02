import { Tabs } from "../../components/Tabs";
import { DownloadIcon } from "../../components/icons";
import { formatSpeed } from "../../lib/format";
import { useAppData, useStore } from "../../state/store";
import { countByFilter, FILTER_LABELS, FILTERS } from "./filters";

export const QUEUE_PANEL_ID = "queue-panel";

export function FilterBar() {
  const { state, dispatch } = useStore();
  const { settings } = useAppData();
  const { downloads, filter } = state;
  const counts = countByFilter(downloads);
  const busy = downloads.filter((d) => d.status === "downloading" || d.status === "merging").length;
  const totalSpeed = downloads.reduce((sum, d) => sum + d.speed, 0);

  return (
    <div className="filter-bar">
      <Tabs
        label="Filter by status"
        controls={QUEUE_PANEL_ID}
        selected={filter}
        onSelect={(next) => dispatch({ type: "filterChanged", filter: next })}
        items={FILTERS.map((id) => ({ id, label: FILTER_LABELS[id], count: counts[id] }))}
      />
      <div className="filter-bar__stats">
        {downloads.length > 0 && (
          <span className="num">
            {busy} of {settings.maxDownloads} slots busy
          </span>
        )}
        <span className="filter-bar__speed">
          <DownloadIcon />
          <span className="num">{formatSpeed(totalSpeed)}</span>
          <span className="sr-only">total download speed</span>
        </span>
      </div>
    </div>
  );
}
