export type CellState = "waiting" | "inFlight" | "done";

export const MAX_CELLS = 240;

export interface SegmentCells {
  cells: CellState[];
  /** Segments per cell. 1 when every segment has its own cell. */
  perCell: number;
}

/**
 * Groups the segment states ("0" waiting, "1" in flight, "2" done) into at most MAX_CELLS cells.
 * A cell is done when all its segments are done, in flight when any segment is in flight.
 */
export function groupSegments(states: string): SegmentCells {
  const perCell = Math.max(1, Math.ceil(states.length / MAX_CELLS));
  const cells: CellState[] = [];
  for (let start = 0; start < states.length; start += perCell) {
    const slice = states.slice(start, start + perCell);
    if (slice.includes("1")) cells.push("inFlight");
    else if (/^2+$/.test(slice)) cells.push("done");
    else cells.push("waiting");
  }
  return { cells, perCell };
}
