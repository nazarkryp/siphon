import type { Download, DownloadStatus } from "../../api";
import type { Filter } from "../../state/reducer";

const FILTER_STATUSES: Record<Exclude<Filter, "all">, DownloadStatus[]> = {
  active: ["downloading", "merging"],
  queued: ["queued", "needsFfmpeg"],
  paused: ["paused"],
  done: ["done"],
  failed: ["failed"],
};

export const FILTER_LABELS: Record<Filter, string> = {
  all: "All",
  active: "Active",
  queued: "Queued",
  paused: "Paused",
  done: "Done",
  failed: "Failed",
};

export const FILTERS = Object.keys(FILTER_LABELS) as Filter[];

export function matchesFilter(download: Download, filter: Filter): boolean {
  return filter === "all" || FILTER_STATUSES[filter].includes(download.status);
}

export function countByFilter(downloads: Download[]): Record<Filter, number> {
  const counts = Object.fromEntries(FILTERS.map((f) => [f, 0])) as Record<Filter, number>;
  for (const download of downloads) {
    for (const filter of FILTERS) {
      if (matchesFilter(download, filter)) counts[filter]++;
    }
  }
  return counts;
}

/** 1-based place in the queue for each queued download, in list order. */
export function queuePositions(downloads: Download[]): Map<string, number> {
  const positions = new Map<string, number>();
  for (const download of downloads) {
    if (download.status === "queued") positions.set(download.id, positions.size + 1);
  }
  return positions;
}
