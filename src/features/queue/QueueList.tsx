import { useCallback, useEffect, useState } from "react";
import { useStore } from "../../state/store";
import { FILTER_LABELS, matchesFilter, queuePositions } from "./filters";
import { QUEUE_PANEL_ID } from "./FilterBar";
import { QueueRow } from "./QueueRow";
import { SelectBox } from "./SelectBox";
import { SelectionBar } from "./SelectionBar";

const CLOCK_INTERVAL_MS = 30_000;

function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), CLOCK_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, []);
  return now;
}

export function QueueList() {
  const { state, dispatch } = useStore();
  const { downloads, filter, expanded, selected } = state;
  const now = useNow();
  const positions = queuePositions(downloads);
  const visible = downloads.filter((d) => matchesFilter(d, filter));
  // A selected row that leaves the filter (for example when it finishes) is not acted on.
  const selectedVisible = visible.filter((d) => selected[d.id]);
  const allSelected = visible.length > 0 && selectedVisible.length === visible.length;
  const onToggle = useCallback((id: string) => dispatch({ type: "expandToggled", id }), [dispatch]);
  const onSelect = useCallback((id: string) => dispatch({ type: "selectionToggled", id }), [dispatch]);

  function onSelectAll() {
    dispatch({ type: "selectionSet", ids: allSelected ? [] : visible.map((d) => d.id) });
  }

  return (
    <div className="queue" id={QUEUE_PANEL_ID} role="tabpanel" aria-label={`${FILTER_LABELS[filter]} downloads`}>
      <div className="queue-grid queue-header">
        <SelectBox
          label={allSelected ? "Clear the selection" : "Select all"}
          checked={allSelected}
          mixed={selectedVisible.length > 0 && !allSelected}
          onChange={onSelectAll}
        />
        {selectedVisible.length > 0 ? (
          <SelectionBar selected={selectedVisible} />
        ) : (
          <div className="queue-header__labels" aria-hidden="true">
            <div>Name</div>
            <div>Type</div>
            <div>Status</div>
            <div>Progress</div>
            <div className="queue-col-speed">Speed</div>
            <div className="queue-col-eta">Time left</div>
            <div />
          </div>
        )}
      </div>
      {visible.length === 0 ? (
        <p className="queue__empty-filter">No downloads in “{FILTER_LABELS[filter]}”.</p>
      ) : (
        <ul className="queue__rows">
          {visible.map((download) => (
            <QueueRow
              key={download.id}
              download={download}
              position={positions.get(download.id)}
              expanded={Boolean(expanded[download.id])}
              onToggle={onToggle}
              selected={Boolean(selected[download.id])}
              onSelect={onSelect}
              now={now}
            />
          ))}
        </ul>
      )}
    </div>
  );
}
