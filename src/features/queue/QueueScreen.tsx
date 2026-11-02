import { useStore } from "../../state/store";
import { EmptyQueue } from "./EmptyQueue";
import { FilterBar } from "./FilterBar";
import { LinkBar } from "./LinkBar";
import { QueueList } from "./QueueList";
import { StatusLine } from "./StatusLine";
import { ToolBanners } from "./ToolBanners";
import "./queue.css";

export function QueueScreen() {
  const { state } = useStore();
  return (
    <main className="queue-screen">
      <LinkBar />
      <ToolBanners />
      <FilterBar />
      {state.downloads.length === 0 ? <EmptyQueue /> : <QueueList />}
      <StatusLine />
    </main>
  );
}
