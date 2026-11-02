import { SearchIcon } from "../../components/icons";

export function NoStreamPanel() {
  return (
    <div className="no-stream">
      <div className="no-stream__icon">
        <SearchIcon size="lg" />
      </div>
      <div className="no-stream__body">
        <h3 className="no-stream__title">No video or file found on this page</h3>
        <p className="no-stream__text">
          Siphon checked all four ways and got nothing it can download. Common reasons:
        </p>
        <ul className="no-stream__reasons">
          <li>
            <strong>The page needs a login.</strong> The video loads only after sign-in. Cookies are
            not supported yet.
          </li>
          <li>
            <strong>The player loads the video later.</strong> Start the video in your browser, copy
            the .m3u8 or .mp4 link from it, and paste that link here.
          </li>
          <li>
            <strong>The server needs a Referer.</strong> Set the Referer below to the page that plays
            the video, then press Fetch again.
          </li>
          <li>
            <strong>The stream is DRM protected.</strong> Siphon doesn’t download protected streams.
          </li>
        </ul>
      </div>
    </div>
  );
}

export function ResolveErrorPanel({ message }: { message: string }) {
  return (
    <div className="no-stream no-stream--error" role="alert">
      <div className="no-stream__body">
        <h3 className="no-stream__title">Siphon could not check this link</h3>
        <p className="no-stream__text selectable">{message}</p>
      </div>
    </div>
  );
}
