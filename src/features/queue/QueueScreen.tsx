import { WindowBar } from "../../components/WindowBar";
import { useStore } from "../../state/store";
import { EmptyQueue } from "./EmptyQueue";
import { FilterBar } from "./FilterBar";
import { LinkForm, LinkMessage, SettingsButton, useLinkBar } from "./LinkBar";
import { QueueList } from "./QueueList";
import { StatusLine } from "./StatusLine";
import { ToolBanners } from "./ToolBanners";
import "./queue.css";

export function QueueScreen() {
  const { state } = useStore();
  const link = useLinkBar();
  return (
    <>
      <WindowBar end={<SettingsButton link={link} />}>
        <LinkForm link={link} />
      </WindowBar>
      <main className="queue-screen">
        <LinkMessage link={link} />
        <ToolBanners />
        <FilterBar />
        {state.downloads.length === 0 ? <EmptyQueue /> : <QueueList />}
        <StatusLine />
      </main>
    </>
  );
}
